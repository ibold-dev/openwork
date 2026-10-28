//! PostgreSQL implementation of [`Store`].
//!
//! All statements are runtime queries (`sqlx::query` / `query_as`), never the
//! compile-time macros, so the crate builds without a database present.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::StoreError;

use super::{
    Challenge, Contract, IdempotentRecord, Milestone, NewAuditEvent, NewContract, NewMilestone,
    NewQuote, NewSettlement, QuoteRecord, Session, Settlement, Store, User,
};

/// Opens a pool and verifies connectivity.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(database_url)
        .await
}

/// Applies the embedded migrations from `openwork/migrations`.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

#[derive(Debug, Clone)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        PgStore { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl Store for PgStore {
    async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }

    async fn insert_challenge(&self, challenge: &Challenge) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO login_challenges
                (id, address, cluster, nonce, domain, origin, statement, message,
                 issued_at, expires_at, consumed_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(challenge.id)
        .bind(&challenge.address)
        .bind(&challenge.cluster)
        .bind(&challenge.nonce)
        .bind(&challenge.domain)
        .bind(&challenge.origin)
        .bind(&challenge.statement)
        .bind(&challenge.message)
        .bind(challenge.issued_at)
        .bind(challenge.expires_at)
        .bind(challenge.consumed_at)
        .bind(challenge.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_challenge(&self, id: Uuid) -> Result<Option<Challenge>, StoreError> {
        let row = sqlx::query_as::<_, Challenge>("SELECT * FROM login_challenges WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    async fn consume_challenge(&self, id: Uuid, now: DateTime<Utc>) -> Result<bool, StoreError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            UPDATE login_challenges
               SET consumed_at = $2
             WHERE id = $1
               AND consumed_at IS NULL
               AND expires_at > $2
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    async fn upsert_user(
        &self,
        id: Uuid,
        address: &str,
        now: DateTime<Utc>,
    ) -> Result<User, StoreError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, wallet_address, created_at, updated_at)
            VALUES ($1, $2, $3, $3)
            ON CONFLICT (wallet_address)
            DO UPDATE SET updated_at = EXCLUDED.updated_at
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(address)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(user)
    }

    async fn insert_session(&self, session: &Session) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO wallet_sessions
                (id, user_id, token_hash, csrf_token, cluster, created_at,
                 last_seen_at, expires_at, revoked_at, ip_address, user_agent)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(session.id)
        .bind(session.user_id)
        .bind(&session.token_hash)
        .bind(&session.csrf_token)
        .bind(&session.cluster)
        .bind(session.created_at)
        .bind(session.last_seen_at)
        .bind(session.expires_at)
        .bind(session.revoked_at)
        .bind(&session.ip_address)
        .bind(&session.user_agent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_session(&self, token_hash: &str) -> Result<Option<(Session, User)>, StoreError> {
        let session =
            sqlx::query_as::<_, Session>("SELECT * FROM wallet_sessions WHERE token_hash = $1")
                .bind(token_hash)
                .fetch_optional(&self.pool)
                .await?;
        let Some(session) = session else {
            return Ok(None);
        };
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(session.user_id)
            .fetch_optional(&self.pool)
            .await?;
        match user {
            Some(user) => Ok(Some((session, user))),
            None => Ok(None),
        }
    }

    async fn revoke_session(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE wallet_sessions SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn touch_session(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), StoreError> {
        sqlx::query("UPDATE wallet_sessions SET last_seen_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn insert_contract(&self, contract: NewContract) -> Result<Contract, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = insert_contract_row(&mut tx, &contract).await?;
        for milestone in &contract.milestones {
            insert_milestone_row(&mut tx, contract.id, &contract.chain, milestone).await?;
        }
        tx.commit().await?;
        Ok(row)
    }

    async fn get_contract(&self, id: Uuid) -> Result<Option<Contract>, StoreError> {
        let row = sqlx::query_as::<_, Contract>("SELECT * FROM contracts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    async fn list_contracts(&self, chain: &str, wallet: &str) -> Result<Vec<Contract>, StoreError> {
        let rows = sqlx::query_as::<_, Contract>(
            r#"
            SELECT * FROM contracts
             WHERE chain = $1
               AND (client_address = $2 OR freelancer_address = $2)
             ORDER BY created_at DESC
            "#,
        )
        .bind(chain)
        .bind(wallet)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn add_milestone(
        &self,
        contract_id: Uuid,
        expected_index: i32,
        milestone: NewMilestone,
        terms_hash: &str,
    ) -> Result<(Contract, Milestone), StoreError> {
        let mut tx = self.pool.begin().await?;

        let contract =
            sqlx::query_as::<_, Contract>("SELECT * FROM contracts WHERE id = $1 FOR UPDATE")
                .bind(contract_id)
                .fetch_optional(&mut *tx)
                .await?
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

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM milestones WHERE contract_id = $1")
                .bind(contract_id)
                .fetch_one(&mut *tx)
                .await?;
        if count != i64::from(expected_index) {
            return Err(StoreError::Conflict(
                "milestone index is no longer contiguous; reload the contract".into(),
            ));
        }

        let inserted =
            insert_milestone_row(&mut tx, contract_id, &contract.chain, &milestone).await?;

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

        let updated = sqlx::query_as::<_, Contract>(
            r#"
            UPDATE contracts
               SET total_amount = $2, terms_hash = $3, updated_at = now()
             WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(contract_id)
        .bind(total.to_string())
        .bind(terms_hash)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((updated, inserted))
    }

    async fn list_milestones(&self, contract_id: Uuid) -> Result<Vec<Milestone>, StoreError> {
        let rows = sqlx::query_as::<_, Milestone>(
            r#"SELECT * FROM milestones WHERE contract_id = $1 ORDER BY "index" ASC"#,
        )
        .bind(contract_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn get_milestone(&self, id: Uuid) -> Result<Option<Milestone>, StoreError> {
        let row = sqlx::query_as::<_, Milestone>("SELECT * FROM milestones WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    async fn set_contract_accepted_intent(
        &self,
        contract_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Contract, StoreError> {
        let row = sqlx::query_as::<_, Contract>(
            r#"
            UPDATE contracts
               SET accepted_intent_at = COALESCE(accepted_intent_at, $2),
                   updated_at = now()
             WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(contract_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(row)
    }

    async fn set_milestone_deliverable(
        &self,
        milestone_id: Uuid,
        uri: &str,
        note: &str,
        hash: &str,
        _now: DateTime<Utc>,
    ) -> Result<Milestone, StoreError> {
        let row = sqlx::query_as::<_, Milestone>(
            r#"
            UPDATE milestones
               SET deliverable_uri = $2,
                   deliverable_note = $3,
                   deliverable_hash = $4,
                   updated_at = now()
             WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(milestone_id)
        .bind(uri)
        .bind(note)
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(row)
    }

    async fn insert_settlement(
        &self,
        settlement: NewSettlement,
    ) -> Result<(Settlement, bool), StoreError> {
        let inserted = sqlx::query_as::<_, Settlement>(
            r#"
            INSERT INTO onchain_transactions
                (id, chain, tx_signature, kind, contract_id, milestone_id,
                 milestone_index, claimed_by_address)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (chain, tx_signature, contract_id, kind) DO NOTHING
            RETURNING *
            "#,
        )
        .bind(settlement.id)
        .bind(&settlement.chain)
        .bind(&settlement.tx_signature)
        .bind(&settlement.kind)
        .bind(settlement.contract_id)
        .bind(settlement.milestone_id)
        .bind(settlement.milestone_index)
        .bind(&settlement.claimed_by_address)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = inserted {
            return Ok((row, true));
        }

        let existing = sqlx::query_as::<_, Settlement>(
            r#"
            SELECT * FROM onchain_transactions
             WHERE chain = $1 AND tx_signature = $2 AND contract_id = $3 AND kind = $4
            "#,
        )
        .bind(&settlement.chain)
        .bind(&settlement.tx_signature)
        .bind(settlement.contract_id)
        .bind(&settlement.kind)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::Internal("conflicting settlement disappeared".into()))?;
        Ok((existing, false))
    }

    async fn list_settlements_for_milestone(
        &self,
        milestone_id: Uuid,
    ) -> Result<Vec<Settlement>, StoreError> {
        let rows = sqlx::query_as::<_, Settlement>(
            r#"
            SELECT * FROM onchain_transactions
             WHERE milestone_id = $1
             ORDER BY created_at DESC
            "#,
        )
        .bind(milestone_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn insert_quote(&self, quote: NewQuote) -> Result<QuoteRecord, StoreError> {
        let row = sqlx::query_as::<_, QuoteRecord>(
            r#"
            INSERT INTO intent_quotes
                (id, user_id, contract_id, mode, provider, external_id, invoice,
                 amount_in, amount_out, origin_asset, destination_asset, recipient,
                 refund_to, deposit_address, deposit_memo, status, simulated,
                 deadline, expires_at, raw_response)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                    $14, $15, $16, $17, $18, $19, $20)
            RETURNING *
            "#,
        )
        .bind(quote.id)
        .bind(quote.user_id)
        .bind(quote.contract_id)
        .bind(&quote.mode)
        .bind(&quote.provider)
        .bind(&quote.external_id)
        .bind(&quote.invoice)
        .bind(&quote.amount_in)
        .bind(&quote.amount_out)
        .bind(&quote.origin_asset)
        .bind(&quote.destination_asset)
        .bind(&quote.recipient)
        .bind(&quote.refund_to)
        .bind(&quote.deposit_address)
        .bind(&quote.deposit_memo)
        .bind(&quote.status)
        .bind(quote.simulated)
        .bind(quote.deadline)
        .bind(quote.expires_at)
        .bind(&quote.raw_response)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn get_quote(&self, id: Uuid) -> Result<Option<QuoteRecord>, StoreError> {
        let row = sqlx::query_as::<_, QuoteRecord>("SELECT * FROM intent_quotes WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    async fn update_quote_status(
        &self,
        id: Uuid,
        status: &str,
        raw: Option<Value>,
        _now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            UPDATE intent_quotes
               SET status = $2,
                   raw_response = COALESCE($3, raw_response),
                   updated_at = now()
             WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(raw)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_idempotency(
        &self,
        user_id: Uuid,
        endpoint: &str,
        key: &str,
    ) -> Result<Option<IdempotentRecord>, StoreError> {
        let row = sqlx::query_as::<_, IdempotentRecord>(
            r#"
            SELECT * FROM idempotency_keys
             WHERE user_id = $1 AND endpoint = $2 AND key = $3 AND expires_at > now()
            "#,
        )
        .bind(user_id)
        .bind(endpoint)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn put_idempotency(&self, record: IdempotentRecord) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO idempotency_keys
                (id, user_id, endpoint, key, request_hash, response_status,
                 response_body, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, endpoint, key) DO NOTHING
            "#,
        )
        .bind(record.id)
        .bind(record.user_id)
        .bind(&record.endpoint)
        .bind(&record.key)
        .bind(&record.request_hash)
        .bind(record.response_status)
        .bind(&record.response_body)
        .bind(record.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_audit(&self, event: NewAuditEvent) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO audit_events
                (actor_address, action, object_type, object_id, request_id,
                 ip_address, user_agent, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.actor_address)
        .bind(event.action)
        .bind(event.object_type)
        .bind(event.object_id)
        .bind(event.request_id)
        .bind(event.ip_address)
        .bind(event.user_agent)
        .bind(event.metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

async fn insert_contract_row(
    tx: &mut Transaction<'_, Postgres>,
    contract: &NewContract,
) -> Result<Contract, StoreError> {
    let row = sqlx::query_as::<_, Contract>(
        r#"
        INSERT INTO contracts
            (id, chain, client_address, freelancer_address, mint, title,
             description, terms_hash, agreement_pda, agreement_nonce,
             total_amount)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING *
        "#,
    )
    .bind(contract.id)
    .bind(&contract.chain)
    .bind(&contract.client_address)
    .bind(&contract.freelancer_address)
    .bind(&contract.mint)
    .bind(&contract.title)
    .bind(&contract.description)
    .bind(&contract.terms_hash)
    .bind(&contract.agreement_pda)
    .bind(&contract.agreement_nonce)
    .bind(&contract.total_amount)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

async fn insert_milestone_row(
    tx: &mut Transaction<'_, Postgres>,
    contract_id: Uuid,
    chain: &str,
    milestone: &NewMilestone,
) -> Result<Milestone, StoreError> {
    let row = sqlx::query_as::<_, Milestone>(
        r#"
        INSERT INTO milestones
            (id, contract_id, chain, "index", title, description, amount, milestone_pda)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(milestone.id)
    .bind(contract_id)
    .bind(chain)
    .bind(milestone.index)
    .bind(&milestone.title)
    .bind(&milestone.description)
    .bind(&milestone.amount)
    .bind(&milestone.milestone_pda)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// Used by the worker to resolve a contract from a chain address.
pub async fn contract_id_by_pda(
    pool: &PgPool,
    chain: &str,
    agreement_pda: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM contracts WHERE chain = $1 AND agreement_pda = $2")
        .bind(chain)
        .bind(agreement_pda)
        .fetch_optional(pool)
        .await
}
