//! JSON request and response shapes for the public API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use openwork_aurora::Quote;
use openwork_domain::{Amount, Deliverable};

use crate::store::{Contract, Milestone, QuoteRecord, Settlement, User};

/// Settlement kinds mirror the program's instructions.
pub const SETTLEMENT_KINDS: &[&str] = &[
    "create_agreement",
    "create_milestone",
    "accept_agreement",
    "cancel_draft_agreement",
    "fund_milestone",
    "submit_milestone",
    "approve_and_release",
    "raise_dispute",
    "mutual_cancel_and_refund",
];

/// Kinds that must reference a milestone index.
pub const MILESTONE_SETTLEMENT_KINDS: &[&str] = &[
    "create_milestone",
    "fund_milestone",
    "submit_milestone",
    "approve_and_release",
    "raise_dispute",
    "mutual_cancel_and_refund",
];

// ---------------------------------------------------------------- auth -----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeRequest {
    pub address: String,
    #[serde(default)]
    pub cluster: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub message: String,
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub challenge_id: Uuid,
    pub signature: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    pub id: Uuid,
    pub wallet: String,
    pub created_at: DateTime<Utc>,
}

impl From<&User> for UserView {
    fn from(user: &User) -> Self {
        UserView {
            id: user.id,
            wallet: user.wallet_address.clone(),
            created_at: user.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResponse {
    pub user: UserView,
    pub csrf_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    pub user: UserView,
    pub csrf_token: String,
    pub cluster: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutResponse {
    pub logged_out: bool,
}

// ----------------------------------------------------------- contracts -----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MilestoneInput {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub amount: Amount,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateContractRequest {
    pub freelancer: String,
    pub mint: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub milestones: Vec<MilestoneInput>,
    /// Set when the browser already created the agreement on chain. The index
    /// must then store that exact PDA instead of allocating its own nonce.
    #[serde(default)]
    pub agreement_nonce: Option<String>,
    /// Hex `sha256` of the canonical terms the browser committed on chain.
    /// When supplied it must equal the hash the server recomputes.
    #[serde(default)]
    pub terms_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMilestoneRequest {
    #[serde(default)]
    pub index: Option<u16>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub amount: Amount,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitMilestoneRequest {
    pub uri: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MilestoneView {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub chain: String,
    pub index: u16,
    pub title: String,
    pub description: String,
    pub amount: String,
    pub state: String,
    pub chain_state: Option<String>,
    pub milestone_pda: String,
    pub deliverable_uri: Option<String>,
    pub deliverable_note: Option<String>,
    pub deliverable_hash: Option<String>,
    pub funded_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
    pub chain_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Milestone> for MilestoneView {
    fn from(milestone: &Milestone) -> Self {
        MilestoneView {
            id: milestone.id,
            contract_id: milestone.contract_id,
            chain: milestone.chain.clone(),
            index: u16::try_from(milestone.index).unwrap_or(0),
            title: milestone.title.clone(),
            description: milestone.description.clone(),
            amount: milestone.amount.clone(),
            state: milestone.effective_state().to_string(),
            chain_state: milestone.chain_state.clone(),
            milestone_pda: milestone.milestone_pda.clone(),
            deliverable_uri: milestone.deliverable_uri.clone(),
            deliverable_note: milestone.deliverable_note.clone(),
            deliverable_hash: milestone.deliverable_hash.clone(),
            funded_at: milestone.chain_funded_at.or(milestone.funded_at),
            submitted_at: milestone.chain_submitted_at.or(milestone.submitted_at),
            settled_at: milestone.chain_settled_at.or(milestone.settled_at),
            chain_synced_at: milestone.chain_synced_at,
            created_at: milestone.created_at,
            updated_at: milestone.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractView {
    pub id: Uuid,
    pub chain: String,
    pub role: String,
    pub client: String,
    pub freelancer: String,
    pub mint: String,
    pub title: String,
    pub description: String,
    pub terms_hash: String,
    /// 32 numbers, never base58, so SDKs can build the `[u8; 32]` directly.
    pub terms_hash_bytes: Vec<u8>,
    pub agreement_pda: String,
    pub agreement_nonce: String,
    pub state: String,
    pub draft_state: String,
    pub total_amount: String,
    pub milestone_count: usize,
    pub milestones: Vec<MilestoneView>,
    pub accepted_intent_at: Option<DateTime<Utc>>,
    pub chain_state: Option<String>,
    pub chain_settled_count: Option<i32>,
    pub chain_refunded_count: Option<i32>,
    pub chain_released_amount: Option<String>,
    pub chain_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ContractView {
    pub fn new(contract: &Contract, milestones: &[Milestone], wallet: &str) -> Self {
        ContractView {
            id: contract.id,
            chain: contract.chain.clone(),
            role: contract.role_of(wallet).unwrap_or("observer").to_string(),
            client: contract.client_address.clone(),
            freelancer: contract.freelancer_address.clone(),
            mint: contract.mint.clone(),
            title: contract.title.clone(),
            description: contract.description.clone(),
            terms_hash: contract.terms_hash.clone(),
            terms_hash_bytes: hex::decode(&contract.terms_hash).unwrap_or_default(),
            agreement_pda: contract.agreement_pda.clone(),
            agreement_nonce: contract.agreement_nonce.clone(),
            state: contract.effective_state().to_string(),
            draft_state: contract.state.clone(),
            total_amount: contract.total_amount.clone(),
            milestone_count: milestones.len(),
            milestones: milestones.iter().map(MilestoneView::from).collect(),
            accepted_intent_at: contract.accepted_intent_at,
            chain_state: contract.chain_state.clone(),
            chain_settled_count: contract.chain_settled_count,
            chain_refunded_count: contract.chain_refunded_count,
            chain_released_amount: contract.chain_released_amount.clone(),
            chain_synced_at: contract.chain_synced_at,
            created_at: contract.created_at,
            updated_at: contract.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractsResponse {
    pub contracts: Vec<ContractView>,
}

// --------------------------------------------------------- settlements -----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementRequest {
    pub contract_id: Uuid,
    #[serde(default)]
    pub milestone_index: Option<u16>,
    pub signature: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementView {
    pub id: Uuid,
    pub chain: String,
    pub signature: String,
    pub kind: String,
    pub contract_id: Uuid,
    pub milestone_id: Option<Uuid>,
    pub milestone_index: Option<u16>,
    pub claimed_by: String,
    pub verification_status: String,
    pub slot: Option<i64>,
    pub block_time: Option<DateTime<Utc>>,
    pub error: Option<serde_json::Value>,
    pub checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<&Settlement> for SettlementView {
    fn from(settlement: &Settlement) -> Self {
        SettlementView {
            id: settlement.id,
            chain: settlement.chain.clone(),
            signature: settlement.tx_signature.clone(),
            kind: settlement.kind.clone(),
            contract_id: settlement.contract_id,
            milestone_id: settlement.milestone_id,
            milestone_index: settlement
                .milestone_index
                .and_then(|i| u16::try_from(i).ok()),
            claimed_by: settlement.claimed_by_address.clone(),
            verification_status: settlement.verification_status.clone(),
            slot: settlement.slot,
            block_time: settlement.block_time,
            error: settlement.error.clone(),
            checked_at: settlement.checked_at,
            created_at: settlement.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementResponse {
    pub settlement: SettlementView,
    /// False when the signature was already recorded (idempotent replay).
    pub created: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsResponse {
    pub transactions: Vec<SettlementView>,
}

// -------------------------------------------------------------- quotes -----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteCreateRequest {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub contract_id: Option<Uuid>,
    pub invoice: String,
    pub amount: String,
    pub origin_asset: String,
    pub destination_asset: String,
    pub recipient: String,
    pub refund_to: String,
    #[serde(default)]
    pub deadline_seconds: Option<u64>,
    #[serde(default)]
    pub slippage_bps: Option<u16>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteView {
    pub id: Uuid,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&QuoteRecord> for QuoteView {
    fn from(quote: &QuoteRecord) -> Self {
        QuoteView {
            id: quote.id,
            contract_id: quote.contract_id,
            mode: quote.mode.clone(),
            provider: quote.provider.clone(),
            external_id: quote.external_id.clone(),
            invoice: quote.invoice.clone(),
            amount_in: quote.amount_in.clone(),
            amount_out: quote.amount_out.clone(),
            origin_asset: quote.origin_asset.clone(),
            destination_asset: quote.destination_asset.clone(),
            recipient: quote.recipient.clone(),
            refund_to: quote.refund_to.clone(),
            deposit_address: quote.deposit_address.clone(),
            deposit_memo: quote.deposit_memo.clone(),
            status: quote.status.clone(),
            simulated: quote.simulated,
            deadline: quote.deadline,
            expires_at: quote.expires_at,
            created_at: quote.created_at,
            updated_at: quote.updated_at,
        }
    }
}

/// Validates and converts a provider quote into a persistence record.
pub fn quote_to_new_quote(
    quote: &Quote,
    id: Uuid,
    user_id: Uuid,
    contract_id: Option<Uuid>,
    invoice: String,
) -> crate::store::NewQuote {
    let deadline = parse_rfc3339(&quote.deadline).unwrap_or_else(Utc::now);
    let expires_at = parse_rfc3339(&quote.expires_at).unwrap_or(deadline);
    crate::store::NewQuote {
        id,
        user_id,
        contract_id,
        mode: quote.mode.as_str().to_string(),
        provider: quote.provider.clone(),
        external_id: quote.id.clone(),
        invoice,
        amount_in: quote.amount_in.clone(),
        amount_out: quote.amount_out.clone(),
        origin_asset: quote.origin_asset.clone(),
        destination_asset: quote.destination_asset.clone(),
        recipient: quote.recipient.clone(),
        refund_to: quote.refund_to.clone(),
        deposit_address: quote.deposit_address.clone(),
        deposit_memo: quote.deposit_memo.clone(),
        status: quote.status.as_str().to_string(),
        simulated: quote.simulated,
        deadline,
        expires_at,
        raw_response: quote.raw.clone(),
    }
}

pub fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

/// Builds the canonical deliverable hash for a submission.
pub fn deliverable_for(index: u16, uri: &str, note: &str) -> Deliverable {
    Deliverable::new(index, uri, note)
}

// -------------------------------------------------------------- health -----

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyResponse {
    pub status: &'static str,
    pub database: &'static str,
    pub rpc: &'static str,
    pub slot: Option<u64>,
}
