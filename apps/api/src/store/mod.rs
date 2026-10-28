//! Persistence boundary.
//!
//! The HTTP layer only talks to this trait. `PgStore` is the production
//! implementation; `MemoryStore` is a faithful in-process implementation used
//! by the API tests so they never need a live Postgres.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::StoreError;

pub mod memory;
pub mod pg;
pub mod worker;

pub use memory::MemoryStore;
pub use pg::PgStore;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub wallet_address: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Challenge {
    pub id: Uuid,
    pub address: String,
    pub cluster: String,
    pub nonce: String,
    pub domain: String,
    pub origin: String,
    pub statement: String,
    pub message: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub csrf_token: String,
    pub cluster: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Contract {
    pub id: Uuid,
    pub chain: String,
    pub client_address: String,
    pub freelancer_address: String,
    pub mint: String,
    pub title: String,
    pub description: String,
    pub terms_hash: String,
    pub agreement_pda: String,
    pub agreement_nonce: String,
    pub state: String,
    pub total_amount: String,
    pub accepted_intent_at: Option<DateTime<Utc>>,
    pub chain_state: Option<String>,
    pub chain_milestone_count: Option<i32>,
    pub chain_settled_count: Option<i32>,
    pub chain_refunded_count: Option<i32>,
    pub chain_total_amount: Option<String>,
    pub chain_released_amount: Option<String>,
    pub chain_slot: Option<i64>,
    pub chain_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Contract {
    pub fn is_party(&self, wallet: &str) -> bool {
        self.client_address == wallet || self.freelancer_address == wallet
    }

    pub fn role_of(&self, wallet: &str) -> Option<&'static str> {
        if self.client_address == wallet {
            Some("client")
        } else if self.freelancer_address == wallet {
            Some("freelancer")
        } else {
            None
        }
    }

    /// Chain truth wins over the off-chain draft state.
    pub fn effective_state(&self) -> &str {
        self.chain_state.as_deref().unwrap_or(&self.state)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Milestone {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub chain: String,
    #[sqlx(rename = "index")]
    pub index: i32,
    pub title: String,
    pub description: String,
    pub amount: String,
    pub state: String,
    pub milestone_pda: String,
    pub deliverable_uri: Option<String>,
    pub deliverable_note: Option<String>,
    pub deliverable_hash: Option<String>,
    pub funded_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
    pub chain_state: Option<String>,
    pub chain_deliverable_hash: Option<String>,
    pub chain_funded_at: Option<DateTime<Utc>>,
    pub chain_submitted_at: Option<DateTime<Utc>>,
    pub chain_settled_at: Option<DateTime<Utc>>,
    pub chain_slot: Option<i64>,
    pub chain_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Milestone {
    pub fn effective_state(&self) -> &str {
        self.chain_state.as_deref().unwrap_or(&self.state)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Settlement {
    pub id: Uuid,
    pub chain: String,
    pub tx_signature: String,
    pub kind: String,
    pub contract_id: Uuid,
    pub milestone_id: Option<Uuid>,
    pub milestone_index: Option<i32>,
    pub claimed_by_address: String,
    pub verification_status: String,
    pub slot: Option<i64>,
    pub block_time: Option<DateTime<Utc>>,
    pub error: Option<Value>,
    pub checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QuoteRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub contract_id: Option<Uuid>,
    pub mode: String,
    pub provider: String,
    pub external_id: String,
    pub invoice: String,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub origin_asset: String,
    pub destination_asset: String,
    pub recipient: String,
    pub refund_to: String,
    pub deposit_address: String,
    pub deposit_memo: Option<String>,
    pub status: String,
    pub simulated: bool,
    pub deadline: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub raw_response: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMilestone {
    pub id: Uuid,
    pub index: i32,
    pub title: String,
    pub description: String,
    pub amount: String,
    pub milestone_pda: String,
}

#[derive(Debug, Clone)]
pub struct NewContract {
    pub id: Uuid,
    pub chain: String,
    pub client_address: String,
    pub freelancer_address: String,
    pub mint: String,
    pub title: String,
    pub description: String,
    pub terms_hash: String,
    pub agreement_pda: String,
    pub agreement_nonce: String,
    pub total_amount: String,
    pub milestones: Vec<NewMilestone>,
}

#[derive(Debug, Clone)]
pub struct NewSettlement {
    pub id: Uuid,
    pub chain: String,
    pub tx_signature: String,
    pub kind: String,
    pub contract_id: Uuid,
    pub milestone_id: Option<Uuid>,
    pub milestone_index: Option<i32>,
    pub claimed_by_address: String,
}

#[derive(Debug, Clone)]
pub struct NewQuote {
    pub id: Uuid,
    pub user_id: Uuid,
    pub contract_id: Option<Uuid>,
    pub mode: String,
    pub provider: String,
    pub external_id: String,
    pub invoice: String,
    pub amount_in: String,
    pub amount_out: Option<String>,
    pub origin_asset: String,
    pub destination_asset: String,
    pub recipient: String,
    pub refund_to: String,
    pub deposit_address: String,
    pub deposit_memo: Option<String>,
    pub status: String,
    pub simulated: bool,
    pub deadline: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub raw_response: Option<Value>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdempotentRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub endpoint: String,
    pub key: String,
    pub request_hash: String,
    pub response_status: i32,
    pub response_body: Value,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewAuditEvent {
    pub actor_address: Option<String>,
    pub action: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Value,
}

#[async_trait]
pub trait Store: Send + Sync + 'static {
    async fn ping(&self) -> Result<(), StoreError>;

    // Auth --------------------------------------------------------------
    async fn insert_challenge(&self, challenge: &Challenge) -> Result<(), StoreError>;
    async fn get_challenge(&self, id: Uuid) -> Result<Option<Challenge>, StoreError>;
    /// Atomically consumes a challenge if it is unused and unexpired.
    async fn consume_challenge(&self, id: Uuid, now: DateTime<Utc>) -> Result<bool, StoreError>;
    async fn upsert_user(
        &self,
        id: Uuid,
        address: &str,
        now: DateTime<Utc>,
    ) -> Result<User, StoreError>;
    async fn insert_session(&self, session: &Session) -> Result<(), StoreError>;
    async fn find_session(&self, token_hash: &str) -> Result<Option<(Session, User)>, StoreError>;
    async fn revoke_session(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), StoreError>;
    async fn touch_session(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), StoreError>;

    // Contracts ---------------------------------------------------------
    async fn insert_contract(&self, contract: NewContract) -> Result<Contract, StoreError>;
    async fn get_contract(&self, id: Uuid) -> Result<Option<Contract>, StoreError>;
    async fn list_contracts(&self, chain: &str, wallet: &str) -> Result<Vec<Contract>, StoreError>;
    /// Adds a milestone at `expected_index`, re-hashes the terms, and bumps the
    /// contract total atomically. Fails with `Conflict` if another writer got
    /// there first.
    async fn add_milestone(
        &self,
        contract_id: Uuid,
        expected_index: i32,
        milestone: NewMilestone,
        terms_hash: &str,
    ) -> Result<(Contract, Milestone), StoreError>;
    async fn list_milestones(&self, contract_id: Uuid) -> Result<Vec<Milestone>, StoreError>;
    async fn get_milestone(&self, id: Uuid) -> Result<Option<Milestone>, StoreError>;
    async fn set_contract_accepted_intent(
        &self,
        contract_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Contract, StoreError>;
    async fn set_milestone_deliverable(
        &self,
        milestone_id: Uuid,
        uri: &str,
        note: &str,
        hash: &str,
        now: DateTime<Utc>,
    ) -> Result<Milestone, StoreError>;

    // Settlements -------------------------------------------------------
    /// Idempotent on `(chain, tx_signature, contract_id, kind)`. The second
    /// return value is `true` when the row was created by this call.
    async fn insert_settlement(
        &self,
        settlement: NewSettlement,
    ) -> Result<(Settlement, bool), StoreError>;
    async fn list_settlements_for_milestone(
        &self,
        milestone_id: Uuid,
    ) -> Result<Vec<Settlement>, StoreError>;

    // Quotes ------------------------------------------------------------
    async fn insert_quote(&self, quote: NewQuote) -> Result<QuoteRecord, StoreError>;
    async fn get_quote(&self, id: Uuid) -> Result<Option<QuoteRecord>, StoreError>;
    async fn update_quote_status(
        &self,
        id: Uuid,
        status: &str,
        raw: Option<Value>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError>;

    // Idempotency / audit ----------------------------------------------
    async fn get_idempotency(
        &self,
        user_id: Uuid,
        endpoint: &str,
        key: &str,
    ) -> Result<Option<IdempotentRecord>, StoreError>;
    async fn put_idempotency(&self, record: IdempotentRecord) -> Result<(), StoreError>;
    async fn insert_audit(&self, event: NewAuditEvent) -> Result<(), StoreError>;
}
