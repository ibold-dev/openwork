//! In-process [`Store`] used by the API test suite.
//!
//! It implements the same invariants as `PgStore` (draft-only milestone adds,
//! deduplicated settlements, single-use challenges) so route tests exercise
//! real authorization and state logic without a database.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::StoreError;

use super::{
    Challenge, Contract, IdempotentRecord, Milestone, NewAuditEvent, NewContract, NewMilestone,
    NewQuote, NewSettlement, QuoteRecord, Session, Settlement, Store, User,
};

#[derive(Default)]
struct Data {
    users: Vec<User>,
    challenges: HashMap<Uuid, Challenge>,
    sessions: HashMap<Uuid, Session>,
    contracts: HashMap<Uuid, Contract>,
    milestones: HashMap<Uuid, Milestone>,
    settlements: Vec<Settlement>,
    quotes: HashMap<Uuid, QuoteRecord>,
    idempotency: Vec<IdempotentRecord>,
    audit: Vec<NewAuditEvent>,
}

#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Data>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Data> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// Test helper: all audit events recorded so far.
    pub fn audit_events(&self) -> Vec<NewAuditEvent> {
        self.lock().audit.clone()
    }

    /// Test helper: number of sessions for a wallet.
    pub fn session_count(&self, wallet: &str) -> usize {
        let data = self.lock();
        let user_ids: Vec<Uuid> = data
            .users
            .iter()
            .filter(|user| user.wallet_address == wallet)
            .map(|user| user.id)
            .collect();
        data.sessions
            .values()
            .filter(|session| user_ids.contains(&session.user_id))
            .count()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn ping(&self) -> Result<(), StoreError> {
        Ok(())
    }

    async fn insert_challenge(&self, challenge: &Challenge) -> Result<(), StoreError> {
        let mut data = self.lock();
        if data
            .challenges
            .values()
            .any(|existing| existing.nonce == challenge.nonce)
        {
            return Err(StoreError::Conflict("duplicate nonce".into()));
        }
        data.challenges.insert(challenge.id, challenge.clone());
        Ok(())
    }

    async fn get_challenge(&self, id: Uuid) -> Result<Option<Challenge>, StoreError> {
        Ok(self.lock().challenges.get(&id).cloned())
    }

    async fn consume_challenge(&self, id: Uuid, now: DateTime<Utc>) -> Result<bool, StoreError> {
        let mut data = self.lock();
        match data.challenges.get_mut(&id) {
            Some(challenge) if challenge.consumed_at.is_none() && challenge.expires_at > now => {
                challenge.consumed_at = Some(now);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn upsert_user(
        &self,
        id: Uuid,
        address: &str,
        now: DateTime<Utc>,
    ) -> Result<User, StoreError> {
        let mut data = self.lock();
        if let Some(existing) = data
            .users
            .iter_mut()
            .find(|user| user.wallet_address == address)
        {
            return Ok(existing.clone());
        }
        let user = User {
            id,
            wallet_address: address.to_string(),
            created_at: now,
        };
        data.users.push(user.clone());
        Ok(user)
    }

    async fn insert_session(&self, session: &Session) -> Result<(), StoreError> {
        let mut data = self.lock();
        if data
            .sessions
            .values()
            .any(|existing| existing.token_hash == session.token_hash)
        {
            return Err(StoreError::Conflict("duplicate session token".into()));
        }
        data.sessions.insert(session.id, session.clone());
        Ok(())
    }

    async fn find_session(&self, token_hash: &str) -> Result<Option<(Session, User)>, StoreError> {
        let data = self.lock();
        let Some(session) = data
            .sessions
            .values()
            .find(|session| session.token_hash == token_hash)
            .cloned()
        else {
            return Ok(None);
        };
        let user = data
            .users
            .iter()
            .find(|user| user.id == session.user_id)
            .cloned();
        Ok(user.map(|user| (session, user)))
    }

    async fn revoke_session(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), StoreError> {
        let mut data = self.lock();
        if let Some(session) = data.sessions.get_mut(&id) {
            if session.revoked_at.is_none() {
                session.revoked_at = Some(now);
            }
        }
        Ok(())
    }

    async fn touch_session(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), StoreError> {
        let mut data = self.lock();
        if let Some(session) = data.sessions.get_mut(&id) {
            session.last_seen_at = now;
        }
        Ok(())
    }

    async fn insert_contract(&self, contract: NewContract) -> Result<Contract, StoreError> {
        let mut data = self.lock();
        if data.contracts.values().any(|existing| {
            existing.chain == contract.chain && existing.agreement_pda == contract.agreement_pda
        }) {
            return Err(StoreError::Conflict(
                "an agreement with this nonce already exists".into(),
            ));
        }
        let now = Utc::now();
        let row = Contract {
            id: contract.id,
            chain: contract.chain.clone(),
            client_address: contract.client_address,
            freelancer_address: contract.freelancer_address,
            mint: contract.mint,
            title: contract.title,
            description: contract.description,
            terms_hash: contract.terms_hash,
            agreement_pda: contract.agreement_pda,
            agreement_nonce: contract.agreement_nonce,
            state: "draft".to_string(),
            total_amount: contract.total_amount,
            accepted_intent_at: None,
            chain_state: None,
            chain_milestone_count: None,
            chain_settled_count: None,
            chain_refunded_count: None,
            chain_total_amount: None,
            chain_released_amount: None,
            chain_slot: None,
            chain_synced_at: None,
            created_at: now,
            updated_at: now,
        };
        data.contracts.insert(row.id, row.clone());
        for milestone in contract.milestones {
            let inserted = milestone_from_new(&row, milestone, now);
            data.milestones.insert(inserted.id, inserted);
        }
        Ok(row)
    }

    async fn get_contract(&self, id: Uuid) -> Result<Option<Contract>, StoreError> {
        Ok(self.lock().contracts.get(&id).cloned())
    }

    async fn list_contracts(&self, chain: &str, wallet: &str) -> Result<Vec<Contract>, StoreError> {
        let mut rows: Vec<Contract> = self
            .lock()
            .contracts
            .values()
            .filter(|contract| {
                contract.chain == chain
                    && (contract.client_address == wallet || contract.freelancer_address == wallet)
            })
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(rows)
    }

    async fn add_milestone(
        &self,
        contract_id: Uuid,
        expected_index: i32,
        milestone: NewMilestone,
        terms_hash: &str,
    ) -> Result<(Contract, Milestone), StoreError> {
        let mut data = self.lock();
        let contract = data
            .contracts
            .get(&contract_id)
            .cloned()
            .ok_or(StoreError::NotFound)?;

        if contract.state != "draft" || contract.chain_state.as_deref() == Some("active") {
            return Err(StoreError::Conflict(
                "milestones can only be added while the agreement is a draft".into(),
            ));
        }
        if contract.accepted_intent_at.is_some() {
            return Err(StoreError::Conflict(
                "the freelancer already accepted these terms".into(),
            ));
        }

        let count = data
            .milestones
            .values()
            .filter(|existing| existing.contract_id == contract_id)
            .count();
        if count != expected_index as usize {
            return Err(StoreError::Conflict(
                "milestone index is no longer contiguous; reload the contract".into(),
            ));
        }

        let previous_total: u64 = contract
            .total_amount
            .parse()
            .map_err(|_| StoreError::Internal("stored total is not an integer".into()))?;
        let amount: u64 = milestone
            .amount
            .parse()
            .map_err(|_| StoreError::Internal("milestone amount is not an integer".into()))?;
        let total = previous_total
            .checked_add(amount)
            .ok_or_else(|| StoreError::Conflict("total amount overflow".into()))?;

        let now = Utc::now();
        let inserted = milestone_from_new(&contract, milestone, now);
        data.milestones.insert(inserted.id, inserted.clone());

        let contract = data
            .contracts
            .get_mut(&contract_id)
            .expect("contract exists");
        contract.total_amount = total.to_string();
        contract.terms_hash = terms_hash.to_string();
        contract.updated_at = now;
        Ok((contract.clone(), inserted))
    }

    async fn list_milestones(&self, contract_id: Uuid) -> Result<Vec<Milestone>, StoreError> {
        let mut rows: Vec<Milestone> = self
            .lock()
            .milestones
            .values()
            .filter(|milestone| milestone.contract_id == contract_id)
            .cloned()
            .collect();
        rows.sort_by_key(|milestone| milestone.index);
        Ok(rows)
    }

    async fn get_milestone(&self, id: Uuid) -> Result<Option<Milestone>, StoreError> {
        Ok(self.lock().milestones.get(&id).cloned())
    }

    async fn set_contract_accepted_intent(
        &self,
        contract_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Contract, StoreError> {
        let mut data = self.lock();
        let contract = data
            .contracts
            .get_mut(&contract_id)
            .ok_or(StoreError::NotFound)?;
        if contract.accepted_intent_at.is_none() {
            contract.accepted_intent_at = Some(now);
            contract.updated_at = now;
        }
        Ok(contract.clone())
    }

    async fn set_milestone_deliverable(
        &self,
        milestone_id: Uuid,
        uri: &str,
        note: &str,
        hash: &str,
        now: DateTime<Utc>,
    ) -> Result<Milestone, StoreError> {
        let mut data = self.lock();
        let milestone = data
            .milestones
            .get_mut(&milestone_id)
            .ok_or(StoreError::NotFound)?;
        milestone.deliverable_uri = Some(uri.to_string());
        milestone.deliverable_note = Some(note.to_string());
        milestone.deliverable_hash = Some(hash.to_string());
        milestone.submitted_at = Some(now);
        milestone.updated_at = now;
        Ok(milestone.clone())
    }

    async fn insert_settlement(
        &self,
        settlement: NewSettlement,
    ) -> Result<(Settlement, bool), StoreError> {
        let mut data = self.lock();
        if let Some(existing) = data.settlements.iter().find(|existing| {
            existing.chain == settlement.chain
                && existing.tx_signature == settlement.tx_signature
                && existing.contract_id == settlement.contract_id
                && existing.kind == settlement.kind
        }) {
            return Ok((existing.clone(), false));
        }
        let now = Utc::now();
        let row = Settlement {
            id: settlement.id,
            chain: settlement.chain,
            tx_signature: settlement.tx_signature,
            kind: settlement.kind,
            contract_id: settlement.contract_id,
            milestone_id: settlement.milestone_id,
            milestone_index: settlement.milestone_index,
            claimed_by_address: settlement.claimed_by_address,
            verification_status: "unverified".into(),
            slot: None,
            block_time: None,
            error: None,
            checked_at: None,
            created_at: now,
            updated_at: now,
        };
        data.settlements.push(row.clone());
        Ok((row, true))
    }

    async fn list_settlements_for_milestone(
        &self,
        milestone_id: Uuid,
    ) -> Result<Vec<Settlement>, StoreError> {
        let mut rows: Vec<Settlement> = self
            .lock()
            .settlements
            .iter()
            .filter(|settlement| settlement.milestone_id == Some(milestone_id))
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(rows)
    }

    async fn insert_quote(&self, quote: NewQuote) -> Result<QuoteRecord, StoreError> {
        let now = Utc::now();
        let row = QuoteRecord {
            id: quote.id,
            user_id: quote.user_id,
            contract_id: quote.contract_id,
            mode: quote.mode,
            provider: quote.provider,
            external_id: quote.external_id,
            invoice: quote.invoice,
            amount_in: quote.amount_in,
            amount_out: quote.amount_out,
            origin_asset: quote.origin_asset,
            destination_asset: quote.destination_asset,
            recipient: quote.recipient,
            refund_to: quote.refund_to,
            deposit_address: quote.deposit_address,
            deposit_memo: quote.deposit_memo,
            status: quote.status,
            simulated: quote.simulated,
            deadline: quote.deadline,
            expires_at: quote.expires_at,
            raw_response: quote.raw_response,
            created_at: now,
            updated_at: now,
        };
        self.lock().quotes.insert(row.id, row.clone());
        Ok(row)
    }

    async fn get_quote(&self, id: Uuid) -> Result<Option<QuoteRecord>, StoreError> {
        Ok(self.lock().quotes.get(&id).cloned())
    }

    async fn update_quote_status(
        &self,
        id: Uuid,
        status: &str,
        raw: Option<Value>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let mut data = self.lock();
        if let Some(quote) = data.quotes.get_mut(&id) {
            quote.status = status.to_string();
            if raw.is_some() {
                quote.raw_response = raw;
            }
            quote.updated_at = now;
        }
        Ok(())
    }

    async fn get_idempotency(
        &self,
        user_id: Uuid,
        endpoint: &str,
        key: &str,
    ) -> Result<Option<IdempotentRecord>, StoreError> {
        let now = Utc::now();
        Ok(self
            .lock()
            .idempotency
            .iter()
            .find(|record| {
                record.user_id == user_id
                    && record.endpoint == endpoint
                    && record.key == key
                    && record.expires_at > now
            })
            .cloned())
    }

    async fn put_idempotency(&self, record: IdempotentRecord) -> Result<(), StoreError> {
        let mut data = self.lock();
        if !data.idempotency.iter().any(|existing| {
            existing.user_id == record.user_id
                && existing.endpoint == record.endpoint
                && existing.key == record.key
        }) {
            data.idempotency.push(record);
        }
        Ok(())
    }

    async fn insert_audit(&self, event: NewAuditEvent) -> Result<(), StoreError> {
        self.lock().audit.push(event);
        Ok(())
    }
}

fn milestone_from_new(
    contract: &Contract,
    milestone: NewMilestone,
    now: DateTime<Utc>,
) -> Milestone {
    Milestone {
        id: milestone.id,
        contract_id: contract.id,
        chain: contract.chain.clone(),
        index: milestone.index,
        title: milestone.title,
        description: milestone.description,
        amount: milestone.amount,
        state: "pending".to_string(),
        milestone_pda: milestone.milestone_pda,
        deliverable_uri: None,
        deliverable_note: None,
        deliverable_hash: None,
        funded_at: None,
        submitted_at: None,
        settled_at: None,
        chain_state: None,
        chain_deliverable_hash: None,
        chain_funded_at: None,
        chain_submitted_at: None,
        chain_settled_at: None,
        chain_slot: None,
        chain_synced_at: None,
        created_at: now,
        updated_at: now,
    }
}
