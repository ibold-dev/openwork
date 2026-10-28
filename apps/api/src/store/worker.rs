//! SQL used only by the reconciliation worker.
//!
//! Kept outside the [`Store`](super::Store) trait because the worker leases
//! jobs and writes chain snapshots; the HTTP API must never be able to do
//! either.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::{Contract, Settlement};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub attempts: i32,
}

/// Enqueues a job unless an identical one is already pending or running.
///
/// Returns `false` when the dedupe predicate suppressed the insert.
pub async fn enqueue(
    pool: &PgPool,
    kind: &str,
    payload: &Value,
    dedupe_key: Option<&str>,
    run_after: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO outbox_jobs (id, kind, payload, dedupe_key, run_after)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (dedupe_key)
            WHERE dedupe_key IS NOT NULL AND status IN ('pending', 'running')
        DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(kind)
    .bind(payload)
    .bind(dedupe_key)
    .bind(run_after)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Leases up to `limit` ready jobs with `SKIP LOCKED`, so multiple workers can
/// run without coordination.
pub async fn lease(pool: &PgPool, worker_id: &str, limit: i64) -> Result<Vec<Job>, sqlx::Error> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE outbox_jobs
           SET status = 'running',
               locked_at = now(),
               locked_by = $1,
               attempts = attempts + 1,
               updated_at = now()
         WHERE id IN (
             SELECT id
               FROM outbox_jobs
              WHERE status = 'pending'
                AND run_after <= now()
              ORDER BY run_after ASC
              LIMIT $2
              FOR UPDATE SKIP LOCKED
         )
        RETURNING id, kind, payload, attempts
        "#,
    )
    .bind(worker_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn complete(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE outbox_jobs
           SET status = 'done',
               last_error = NULL,
               locked_at = NULL,
               locked_by = NULL,
               updated_at = now()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Records a failure with capped exponential-ish backoff. After five attempts
/// the job is parked as `failed` and requires operator attention.
pub async fn fail(pool: &PgPool, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE outbox_jobs
           SET status = CASE WHEN attempts >= 5 THEN 'failed' ELSE 'pending' END,
               run_after = CASE
                   WHEN attempts >= 5 THEN run_after
                   ELSE now() + make_interval(secs => LEAST(attempts, 20) * 5)
               END,
               last_error = $2,
               locked_at = NULL,
               locked_by = NULL,
               updated_at = now()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error.chars().take(2_000).collect::<String>())
    .execute(pool)
    .await?;
    Ok(())
}

/// Puts a job back without counting it as a failure (e.g. a signature is not
/// confirmed yet).
pub async fn defer(pool: &PgPool, id: Uuid, seconds: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE outbox_jobs
           SET status = 'pending',
               run_after = now() + make_interval(secs => $2),
               locked_at = NULL,
               locked_by = NULL,
               updated_at = now()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(seconds.max(1))
    .execute(pool)
    .await?;
    Ok(())
}

/// Recovers jobs whose worker died mid-flight.
pub async fn requeue_stale(pool: &PgPool, older_than_seconds: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE outbox_jobs
           SET status = 'pending',
               locked_at = NULL,
               locked_by = NULL,
               updated_at = now()
         WHERE status = 'running'
           AND locked_at < now() - make_interval(secs => $1)
        "#,
    )
    .bind(older_than_seconds.max(1))
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn set_checkpoint(
    pool: &PgPool,
    worker: &str,
    key: &str,
    value: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO worker_checkpoints (worker, key, value, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (worker, key)
        DO UPDATE SET value = EXCLUDED.value, updated_at = now()
        "#,
    )
    .bind(worker)
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_checkpoint(
    pool: &PgPool,
    worker: &str,
    key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT value FROM worker_checkpoints WHERE worker = $1 AND key = $2")
        .bind(worker)
        .bind(key)
        .fetch_optional(pool)
        .await
}

pub async fn contract(pool: &PgPool, id: Uuid) -> Result<Option<Contract>, sqlx::Error> {
    sqlx::query_as::<_, Contract>("SELECT * FROM contracts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Contracts whose chain snapshot has not been refreshed recently.
pub async fn stale_contracts(
    pool: &PgPool,
    chain: &str,
    stale_seconds: i64,
    limit: i64,
) -> Result<Vec<Contract>, sqlx::Error> {
    sqlx::query_as::<_, Contract>(
        r#"
        SELECT * FROM contracts
         WHERE chain = $1
           AND (
               chain_synced_at IS NULL
               OR chain_synced_at < now() - make_interval(secs => $2)
           )
         ORDER BY chain_synced_at ASC NULLS FIRST
         LIMIT $3
        "#,
    )
    .bind(chain)
    .bind(stale_seconds.max(0))
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn settlement(pool: &PgPool, id: Uuid) -> Result<Option<Settlement>, sqlx::Error> {
    sqlx::query_as::<_, Settlement>("SELECT * FROM onchain_transactions WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn unverified_settlements(
    pool: &PgPool,
    chain: &str,
    limit: i64,
) -> Result<Vec<Settlement>, sqlx::Error> {
    sqlx::query_as::<_, Settlement>(
        r#"
        SELECT * FROM onchain_transactions
         WHERE chain = $1 AND verification_status = 'unverified'
         ORDER BY created_at ASC
         LIMIT $2
        "#,
    )
    .bind(chain)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn mark_settlement(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    slot: Option<i64>,
    block_time: Option<DateTime<Utc>>,
    error: Option<&Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE onchain_transactions
           SET verification_status = $2,
               slot = COALESCE($3, slot),
               block_time = COALESCE($4, block_time),
               error = $5,
               checked_at = now(),
               updated_at = now()
         WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(slot)
    .bind(block_time)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub struct ContractSnapshot {
    pub state: String,
    pub milestone_count: i32,
    pub settled_count: i32,
    pub refunded_count: i32,
    pub total_amount: String,
    pub released_amount: String,
    pub slot: i64,
}

/// Writes the chain truth columns. The off-chain `state`/`total_amount` are
/// intentionally untouched.
pub async fn update_contract_snapshot(
    pool: &PgPool,
    chain: &str,
    agreement_pda: &str,
    snapshot: &ContractSnapshot,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE contracts
           SET chain_state = $3,
               chain_milestone_count = $4,
               chain_settled_count = $5,
               chain_refunded_count = $6,
               chain_total_amount = $7,
               chain_released_amount = $8,
               chain_slot = $9,
               chain_synced_at = now(),
               updated_at = now()
         WHERE chain = $1 AND agreement_pda = $2
        "#,
    )
    .bind(chain)
    .bind(agreement_pda)
    .bind(&snapshot.state)
    .bind(snapshot.milestone_count)
    .bind(snapshot.settled_count)
    .bind(snapshot.refunded_count)
    .bind(&snapshot.total_amount)
    .bind(&snapshot.released_amount)
    .bind(snapshot.slot)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub struct MilestoneSnapshot {
    pub index: i32,
    pub state: String,
    pub deliverable_hash: Option<String>,
    pub funded_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
    pub slot: i64,
}

pub async fn update_milestone_snapshot(
    pool: &PgPool,
    chain: &str,
    milestone_pda: &str,
    snapshot: &MilestoneSnapshot,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE milestones
           SET chain_state = $4,
               chain_deliverable_hash = $5,
               chain_funded_at = $6,
               chain_submitted_at = $7,
               chain_settled_at = $8,
               chain_slot = $9,
               chain_synced_at = now(),
               updated_at = now()
         WHERE chain = $1 AND milestone_pda = $2 AND "index" = $3
        "#,
    )
    .bind(chain)
    .bind(milestone_pda)
    .bind(snapshot.index)
    .bind(&snapshot.state)
    .bind(&snapshot.deliverable_hash)
    .bind(snapshot.funded_at)
    .bind(snapshot.submitted_at)
    .bind(snapshot.settled_at)
    .bind(snapshot.slot)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub struct NewChainEvent<'a> {
    pub chain: &'a str,
    pub tx_signature: &'a str,
    pub event_index: i32,
    pub event_name: &'a str,
    pub contract_id: Option<Uuid>,
    pub agreement_pda: &'a str,
    pub milestone_pda: Option<&'a str>,
    pub slot: Option<i64>,
    pub block_time: Option<DateTime<Utc>>,
    pub payload: &'a Value,
}

/// Idempotent on `(chain, tx_signature, event_index)`.
pub async fn insert_chain_event(
    pool: &PgPool,
    event: NewChainEvent<'_>,
) -> Result<bool, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        INSERT INTO chain_events
            (chain, tx_signature, event_index, event_name, contract_id,
             agreement_pda, milestone_pda, slot, block_time, payload)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (chain, tx_signature, event_index) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(event.chain)
    .bind(event.tx_signature)
    .bind(event.event_index)
    .bind(event.event_name)
    .bind(event.contract_id)
    .bind(event.agreement_pda)
    .bind(event.milestone_pda)
    .bind(event.slot)
    .bind(event.block_time)
    .bind(event.payload)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn has_events_for_signature(
    pool: &PgPool,
    chain: &str,
    tx_signature: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM chain_events WHERE chain = $1 AND tx_signature = $2)",
    )
    .bind(chain)
    .bind(tx_signature)
    .fetch_one(pool)
    .await
}
