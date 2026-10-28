//! Reconciliation worker.
//!
//! The worker is the only component that reads chain state. It:
//!
//! 1. leases jobs from `outbox_jobs` with `FOR UPDATE SKIP LOCKED`,
//! 2. confirms claimed signatures with `getSignatureStatuses`,
//! 3. decodes `Agreement` and `Milestone` accounts exactly as the program lays
//!    them out,
//! 4. records Anchor events in `chain_events`, deduplicated per transaction,
//! 5. updates the `chain_*` snapshot columns that the API exposes.
//!
//! It never signs, never holds keys, and never trusts a client claim without a
//! confirmed transaction.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use openwork_domain::limits::MAX_MILESTONES;
use openwork_domain::pda::{milestone_pda, parse_pubkey};
use serde_json::{json, Value};
use solana_pubkey::Pubkey;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::routes::dto::SETTLEMENT_KINDS;
use crate::solana::decode::{decode_agreement, decode_milestone};
use crate::solana::events::{parse_logs, ChainEvent};
use crate::solana::rpc::SolanaRpc;
use crate::store::{pg, worker as jobs, Contract};

/// How long a leased job may be silent before another worker may reclaim it.
const LEASE_TIMEOUT_SECONDS: i64 = 300;

/// Cap on event rows written per job, so one noisy transaction cannot pin a
/// worker.
const MAX_EVENTS_PER_JOB: usize = 50;

/// Recent signatures inspected per contract sync.
const SIGNATURE_SCAN_LIMIT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Done,
    /// Try again later without counting a failure.
    Deferred(i64),
}

pub struct Worker {
    pool: PgPool,
    rpc: SolanaRpc,
    program_id: Pubkey,
    chain: String,
    worker_id: String,
    batch_size: i64,
    stale_seconds: i64,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("chain", &self.chain)
            .field("worker_id", &self.worker_id)
            .field("program_id", &self.program_id)
            .finish()
    }
}

impl Worker {
    pub fn new(config: &Config, pool: PgPool) -> anyhow::Result<Self> {
        let rpc = SolanaRpc::new(&config.solana_rpc_url, config.rpc_timeout)?;
        let program_id = parse_pubkey(&config.program_id)
            .map_err(|err| anyhow::anyhow!("PROGRAM_ID is invalid: {err}"))?;
        Ok(Worker {
            pool,
            rpc,
            program_id,
            chain: config.default_cluster.clone(),
            worker_id: format!("reconciler-{}", Uuid::new_v4()),
            batch_size: config.worker_batch_size,
            stale_seconds: config.sync_stale_seconds,
        })
    }

    /// Runs until `SIGINT`/`SIGTERM`.
    pub async fn run(self, poll_interval: Duration) -> anyhow::Result<()> {
        tracing::info!(
            worker_id = %self.worker_id,
            chain = %self.chain,
            poll_seconds = poll_interval.as_secs(),
            "reconciliation worker started"
        );
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!(worker_id = %self.worker_id, "reconciliation worker stopping");
                    return Ok(());
                }
                _ = tokio::time::sleep(poll_interval) => {}
            }
            if let Err(err) = self.tick().await {
                tracing::error!(error = %err, "worker tick failed");
            }
        }
    }

    /// One pass: reclaim, enqueue, lease, process, checkpoint.
    pub async fn tick(&self) -> anyhow::Result<()> {
        let reclaimed = jobs::requeue_stale(&self.pool, LEASE_TIMEOUT_SECONDS).await?;
        if reclaimed > 0 {
            tracing::warn!(reclaimed, "requeued jobs from a stopped worker");
        }

        self.enqueue_due_work().await?;

        let leased = jobs::lease(&self.pool, &self.worker_id, self.batch_size).await?;
        for job in leased {
            let job_id = job.id;
            let kind = job.kind.clone();
            let payload = job.payload.clone();
            let result = match kind.as_str() {
                "sync_contract" => self.sync_contract_from_payload(&payload).await,
                "verify_signature" => self.verify_signature_from_payload(&payload).await,
                other => Err(format!("unknown job kind `{other}`")),
            };

            match result {
                Ok(Outcome::Done) => {
                    jobs::complete(&self.pool, job_id).await?;
                }
                Ok(Outcome::Deferred(seconds)) => {
                    jobs::defer(&self.pool, job_id, seconds).await?;
                }
                Err(err) => {
                    tracing::warn!(job_id = %job_id, kind, error = %err, "job failed");
                    jobs::fail(&self.pool, job_id, &err).await?;
                }
            }
        }

        if let Ok(slot) = self.rpc.slot().await {
            jobs::set_checkpoint(
                &self.pool,
                "reconciler",
                "last_seen_slot",
                &slot.to_string(),
            )
            .await?;
        }
        jobs::set_checkpoint(
            &self.pool,
            "reconciler",
            "last_tick_at",
            &Utc::now().to_rfc3339(),
        )
        .await?;

        Ok(())
    }

    async fn enqueue_due_work(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        for contract in
            jobs::stale_contracts(&self.pool, &self.chain, self.stale_seconds, self.batch_size)
                .await?
        {
            jobs::enqueue(
                &self.pool,
                "sync_contract",
                &json!({ "contractId": contract.id }),
                Some(&format!("sync_contract:{}", contract.id)),
                now,
            )
            .await?;
        }

        for settlement in
            jobs::unverified_settlements(&self.pool, &self.chain, self.batch_size).await?
        {
            jobs::enqueue(
                &self.pool,
                "verify_signature",
                &json!({ "settlementId": settlement.id }),
                Some(&format!("verify_signature:{}", settlement.id)),
                now,
            )
            .await?;
        }

        Ok(())
    }

    async fn sync_contract_from_payload(&self, payload: &Value) -> Result<Outcome, String> {
        let contract_id = payload_uuid(payload, "contractId")?;
        self.sync_contract(contract_id).await
    }

    async fn verify_signature_from_payload(&self, payload: &Value) -> Result<Outcome, String> {
        let settlement_id = payload_uuid(payload, "settlementId")?;
        self.verify_signature(settlement_id).await
    }

    async fn sync_contract(&self, contract_id: Uuid) -> Result<Outcome, String> {
        let Some(contract) = jobs::contract(&self.pool, contract_id)
            .await
            .map_err(db_error)?
        else {
            tracing::debug!(%contract_id, "contract disappeared before sync");
            return Ok(Outcome::Done);
        };

        let agreement_address = parse_pubkey(&contract.agreement_pda)
            .map_err(|err| format!("stored agreement PDA is invalid: {err}"))?;

        let response = self
            .rpc
            .account_info(&contract.agreement_pda)
            .await
            .map_err(|err| err.to_string())?;
        let Some(info) = response.value else {
            // The draft has not been created on chain yet. Nothing to record.
            tracing::debug!(%contract_id, "agreement account does not exist yet");
            return Ok(Outcome::Done);
        };
        let bytes = info.bytes().map_err(|err| err.to_string())?;
        let agreement = decode_agreement(&bytes).map_err(|err| err.to_string())?;

        if hex::encode(agreement.terms_hash) != contract.terms_hash {
            tracing::warn!(
                %contract_id,
                on_chain = %hex::encode(agreement.terms_hash),
                stored = %contract.terms_hash,
                "term hash mismatch between chain and database"
            );
        }
        if bs58::encode(agreement.client).into_string() != contract.client_address
            || bs58::encode(agreement.freelancer).into_string() != contract.freelancer_address
        {
            tracing::warn!(%contract_id, "agreement parties differ from the stored draft");
        }

        jobs::update_contract_snapshot(
            &self.pool,
            &self.chain,
            &contract.agreement_pda,
            &jobs::ContractSnapshot {
                state: agreement.state.as_str().to_string(),
                milestone_count: i32::from(agreement.milestone_count),
                settled_count: i32::from(agreement.settled_count),
                refunded_count: i32::from(agreement.refunded_count),
                total_amount: agreement.total_amount.to_string(),
                released_amount: agreement.released_amount.to_string(),
                slot: i64::try_from(response.context.slot).unwrap_or(i64::MAX),
            },
        )
        .await
        .map_err(db_error)?;

        let count = agreement.milestone_count.min(MAX_MILESTONES);
        if count > 0 {
            let mut addresses = Vec::with_capacity(usize::from(count));
            for index in 0..count {
                let (pda, _) = milestone_pda(&self.program_id, &agreement_address, index);
                addresses.push(pda.to_string());
            }

            let accounts = self
                .rpc
                .multiple_accounts(&addresses)
                .await
                .map_err(|err| err.to_string())?;
            let slot = i64::try_from(accounts.context.slot).unwrap_or(i64::MAX);

            for (position, account) in accounts.value.iter().enumerate() {
                let Some(account) = account else {
                    continue;
                };
                let bytes = account.bytes().map_err(|err| err.to_string())?;
                let milestone = decode_milestone(&bytes).map_err(|err| err.to_string())?;
                if milestone.agreement != agreement_address.to_bytes() {
                    tracing::warn!(%contract_id, index = position, "milestone belongs to another agreement");
                    continue;
                }
                if usize::from(milestone.index) != position {
                    tracing::warn!(
                        %contract_id,
                        expected = position,
                        actual = milestone.index,
                        "milestone index does not match its PDA slot"
                    );
                    continue;
                }

                let deliverable = if milestone.deliverable_hash == [0u8; 32] {
                    None
                } else {
                    Some(hex::encode(milestone.deliverable_hash))
                };
                jobs::update_milestone_snapshot(
                    &self.pool,
                    &self.chain,
                    &addresses[position],
                    &jobs::MilestoneSnapshot {
                        index: i32::from(milestone.index),
                        state: milestone.state.as_str().to_string(),
                        deliverable_hash: deliverable,
                        funded_at: timestamp(milestone.funded_at),
                        submitted_at: timestamp(milestone.submitted_at),
                        settled_at: timestamp(milestone.settled_at),
                        slot,
                    },
                )
                .await
                .map_err(db_error)?;
            }
        }

        self.discover_events(&contract, &agreement_address).await?;
        Ok(Outcome::Done)
    }

    /// Records events for recent transactions that the API has not seen yet,
    /// so a freelancer submitting from a different device is still reconciled.
    async fn discover_events(
        &self,
        contract: &Contract,
        agreement_address: &Pubkey,
    ) -> Result<(), String> {
        let signatures = self
            .rpc
            .signatures_for_address(&contract.agreement_pda, SIGNATURE_SCAN_LIMIT)
            .await
            .map_err(|err| err.to_string())?;

        let mut written = 0usize;
        for info in signatures {
            if info.err.as_ref().is_some_and(|value| !value.is_null()) {
                continue;
            }
            if jobs::has_events_for_signature(&self.pool, &self.chain, &info.signature)
                .await
                .map_err(db_error)?
            {
                continue;
            }

            let Some(tx) = self
                .rpc
                .transaction(&info.signature)
                .await
                .map_err(|err| err.to_string())?
            else {
                continue;
            };
            if tx.error().is_some() {
                continue;
            }

            let events = parse_logs(tx.logs(), &self.program_id.to_bytes())
                .map_err(|err| err.to_string())?;
            let slot = Some(i64::try_from(tx.slot).unwrap_or(i64::MAX));
            let block_time = tx.block_time_utc();

            for (index, event) in events.iter().enumerate() {
                if event.agreement() != agreement_address.to_bytes() {
                    continue;
                }
                self.record_event(
                    &info.signature,
                    index,
                    event,
                    Some(contract.id),
                    slot,
                    block_time,
                    &contract.agreement_pda,
                )
                .await?;
                written += 1;
                if written >= MAX_EVENTS_PER_JOB {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    async fn verify_signature(&self, settlement_id: Uuid) -> Result<Outcome, String> {
        let Some(settlement) = jobs::settlement(&self.pool, settlement_id)
            .await
            .map_err(db_error)?
        else {
            return Ok(Outcome::Done);
        };
        if settlement.verification_status == "verified" {
            return Ok(Outcome::Done);
        }

        let statuses = self
            .rpc
            .signature_statuses(std::slice::from_ref(&settlement.tx_signature))
            .await
            .map_err(|err| err.to_string())?;
        let Some(status) = statuses.value.into_iter().next().flatten() else {
            // Not confirmed yet; the client may still be waiting.
            return Ok(Outcome::Deferred(10));
        };
        if !status.is_confirmed() {
            return Ok(Outcome::Deferred(5));
        }
        if let Some(error) = status.error() {
            jobs::mark_settlement(
                &self.pool,
                settlement_id,
                "failed",
                Some(i64::try_from(status.slot).unwrap_or(i64::MAX)),
                None,
                Some(error),
            )
            .await
            .map_err(db_error)?;
            return Ok(Outcome::Done);
        }

        let Some(tx) = self
            .rpc
            .transaction(&settlement.tx_signature)
            .await
            .map_err(|err| err.to_string())?
        else {
            jobs::mark_settlement(
                &self.pool,
                settlement_id,
                "not_found",
                Some(i64::try_from(status.slot).unwrap_or(i64::MAX)),
                None,
                Some(&json!({ "code": "transaction_unavailable" })),
            )
            .await
            .map_err(db_error)?;
            return Ok(Outcome::Done);
        };

        if !tx
            .transaction
            .signatures
            .iter()
            .any(|signature| signature == &settlement.tx_signature)
        {
            return self
                .fail_settlement(settlement_id, "signature_not_in_transaction")
                .await;
        }
        if let Some(error) = tx.error() {
            jobs::mark_settlement(
                &self.pool,
                settlement_id,
                "failed",
                Some(i64::try_from(tx.slot).unwrap_or(i64::MAX)),
                tx.block_time_utc(),
                Some(error),
            )
            .await
            .map_err(db_error)?;
            return Ok(Outcome::Done);
        }

        let Some(contract) = jobs::contract(&self.pool, settlement.contract_id)
            .await
            .map_err(db_error)?
        else {
            return self
                .fail_settlement(settlement_id, "contract_missing")
                .await;
        };

        let events =
            parse_logs(tx.logs(), &self.program_id.to_bytes()).map_err(|err| err.to_string())?;
        let contract_events: Vec<&ChainEvent> = events
            .iter()
            .filter(|event| bs58::encode(event.agreement()).into_string() == contract.agreement_pda)
            .collect();

        if contract_events.is_empty() {
            return self
                .fail_settlement(settlement_id, "no_events_for_contract")
                .await;
        }
        if !contract_events
            .iter()
            .any(|event| event_matches_kind(&settlement.kind, event, settlement.milestone_index))
        {
            return self
                .fail_settlement(settlement_id, "claimed_kind_mismatch")
                .await;
        }

        let slot = Some(i64::try_from(tx.slot).unwrap_or(i64::MAX));
        let block_time = tx.block_time_utc();
        for (index, event) in events.iter().enumerate() {
            self.record_event(
                &settlement.tx_signature,
                index,
                event,
                None,
                slot,
                block_time,
                &bs58::encode(event.agreement()).into_string(),
            )
            .await?;
        }

        jobs::mark_settlement(
            &self.pool,
            settlement_id,
            "verified",
            slot,
            block_time,
            None,
        )
        .await
        .map_err(db_error)?;

        // Refresh the snapshot that this transaction changed.
        jobs::enqueue(
            &self.pool,
            "sync_contract",
            &json!({ "contractId": settlement.contract_id }),
            Some(&format!("sync_contract:{}", settlement.contract_id)),
            Utc::now(),
        )
        .await
        .map_err(db_error)?;

        Ok(Outcome::Done)
    }

    async fn fail_settlement(
        &self,
        settlement_id: Uuid,
        code: &'static str,
    ) -> Result<Outcome, String> {
        jobs::mark_settlement(
            &self.pool,
            settlement_id,
            "failed",
            None,
            None,
            Some(&json!({ "code": code })),
        )
        .await
        .map_err(db_error)?;
        Ok(Outcome::Done)
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_event(
        &self,
        tx_signature: &str,
        event_index: usize,
        event: &ChainEvent,
        contract_id: Option<Uuid>,
        slot: Option<i64>,
        block_time: Option<DateTime<Utc>>,
        agreement_pda: &str,
    ) -> Result<(), String> {
        let event_index = i32::try_from(event_index).map_err(|_| "too many events".to_string())?;
        let contract_id = match contract_id {
            Some(id) => Some(id),
            None => pg::contract_id_by_pda(&self.pool, &self.chain, agreement_pda)
                .await
                .map_err(db_error)?,
        };
        let milestone_pda = event.milestone_index().map(|index| {
            let agreement = event.agreement();
            openwork_domain::pda::milestone_pda(
                &self.program_id,
                &Pubkey::new_from_array(agreement),
                index,
            )
            .0
            .to_string()
        });
        let payload = event.payload();

        jobs::insert_chain_event(
            &self.pool,
            jobs::NewChainEvent {
                chain: &self.chain,
                tx_signature,
                event_index,
                event_name: event.name(),
                contract_id,
                agreement_pda,
                milestone_pda: milestone_pda.as_deref(),
                slot,
                block_time,
                payload: &payload,
            },
        )
        .await
        .map_err(db_error)?;
        Ok(())
    }
}

/// Maps a claimed kind onto the event the program must have emitted.
fn event_matches_kind(kind: &str, event: &ChainEvent, milestone_index: Option<i32>) -> bool {
    let expected = match kind {
        "create_agreement" => "AgreementCreated",
        "create_milestone" => "MilestoneCreated",
        "accept_agreement" => "AgreementAccepted",
        "cancel_draft_agreement" => "AgreementCancelled",
        "fund_milestone" => "MilestoneFunded",
        "submit_milestone" => "MilestoneSubmitted",
        "approve_and_release" => "MilestoneReleased",
        "raise_dispute" => "MilestoneDisputed",
        "mutual_cancel_and_refund" => "MilestoneRefunded",
        _ => return false,
    };
    if !SETTLEMENT_KINDS.contains(&kind) {
        return false;
    }
    if event.name() != expected {
        return false;
    }
    match (event.milestone_index(), milestone_index) {
        (Some(emitted), Some(claimed)) => i32::from(emitted) == claimed,
        (None, None) => true,
        _ => false,
    }
}

fn payload_uuid(payload: &Value, key: &str) -> Result<Uuid, String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("job payload is missing `{key}`"))?
        .parse()
        .map_err(|err| format!("job payload `{key}` is not a UUID: {err}"))
}

fn timestamp(seconds: i64) -> Option<DateTime<Utc>> {
    if seconds <= 0 {
        None
    } else {
        DateTime::from_timestamp(seconds, 0)
    }
}

fn db_error(err: sqlx::Error) -> String {
    format!("database error: {err}")
}

/// Convenience for the binary.
pub async fn run(config: Arc<Config>, pool: PgPool) -> anyhow::Result<()> {
    let interval = Duration::from_secs(config.worker_poll_interval_seconds.max(1));
    let worker = Worker::new(&config, pool)?;
    worker.run(interval).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claimed_kinds_must_match_events() {
        let funded = ChainEvent::MilestoneFunded {
            agreement: [1u8; 32],
            index: 2,
            amount: 1,
            vault_balance: 1,
        };
        assert!(event_matches_kind("fund_milestone", &funded, Some(2)));
        assert!(!event_matches_kind("fund_milestone", &funded, Some(1)));
        assert!(!event_matches_kind("approve_and_release", &funded, Some(2)));
        assert!(!event_matches_kind("not_a_kind", &funded, Some(2)));
    }

    #[test]
    fn agreement_level_kinds_reject_milestone_indexes() {
        let accepted = ChainEvent::AgreementAccepted {
            agreement: [1u8; 32],
            freelancer: [2u8; 32],
            terms_hash: [3u8; 32],
            accepted_at: 0,
        };
        assert!(event_matches_kind("accept_agreement", &accepted, None));
        assert!(!event_matches_kind("accept_agreement", &accepted, Some(0)));
    }

    #[test]
    fn timestamps_treat_zero_as_absent() {
        assert!(timestamp(0).is_none());
        assert!(timestamp(-1).is_none());
        assert!(timestamp(1_700_000_000).is_some());
    }
}
