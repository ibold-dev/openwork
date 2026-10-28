pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::*;
pub use state::*;

// Generated account-metadata modules must be reachable from the crate root
// because Anchor's #[program] macro resolves them there.
pub(crate) use instructions::accept_agreement::__client_accounts_accept_agreement;
pub(crate) use instructions::approve_and_release::__client_accounts_approve_and_release;
pub(crate) use instructions::cancel_draft_agreement::__client_accounts_cancel_draft_agreement;
pub(crate) use instructions::create_agreement::__client_accounts_create_agreement;
pub(crate) use instructions::create_milestone::__client_accounts_create_milestone;
pub(crate) use instructions::fund_milestone::__client_accounts_fund_milestone;
pub(crate) use instructions::mutual_cancel_and_refund::__client_accounts_mutual_cancel_and_refund;
pub(crate) use instructions::raise_dispute::__client_accounts_raise_dispute;
pub(crate) use instructions::submit_milestone::__client_accounts_submit_milestone;

use instructions::{
    accept_agreement::AcceptAgreement, approve_and_release::ApproveAndRelease,
    cancel_draft_agreement::CancelDraftAgreement, create_agreement::CreateAgreement,
    create_milestone::CreateMilestone, fund_milestone::FundMilestone,
    mutual_cancel_and_refund::MutualCancelAndRefund, raise_dispute::RaiseDispute,
    submit_milestone::SubmitMilestone,
};

declare_id!("GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR");

/// OpenWork milestone escrow.
///
/// Design rules:
/// - The program is the only authority over escrowed funds.
/// - Every milestone settles exactly once.
/// - No administrator can move funds. Disputes freeze release and are only
///   resolved by mutual cancellation signed by both parties.
/// - All amounts are `u64` base units; arithmetic uses checked operations.
#[program]
pub mod openwork_escrow {
    use super::*;

    /// Create a draft agreement. Milestones are added afterwards, before acceptance.
    pub fn create_agreement(
        ctx: Context<CreateAgreement>,
        agreement_nonce: u64,
        freelancer: Pubkey,
        mint: Pubkey,
        terms_hash: [u8; 32],
    ) -> Result<()> {
        instructions::create_agreement::handler(ctx, agreement_nonce, freelancer, mint, terms_hash)
    }

    /// Add the next contiguous milestone while the agreement is still a draft.
    pub fn create_milestone(ctx: Context<CreateMilestone>, index: u16, amount: u64) -> Result<()> {
        instructions::create_milestone::handler(ctx, index, amount)
    }

    /// Freelancer accepts the committed terms. Requires the exact terms hash.
    pub fn accept_agreement(ctx: Context<AcceptAgreement>, terms_hash: [u8; 32]) -> Result<()> {
        instructions::accept_agreement::handler(ctx, terms_hash)
    }

    /// Client cancels a draft agreement that never held funds.
    pub fn cancel_draft_agreement(ctx: Context<CancelDraftAgreement>) -> Result<()> {
        instructions::cancel_draft_agreement::handler(ctx)
    }

    /// Client funds one milestone from an SPL token account into the vault.
    pub fn fund_milestone(ctx: Context<FundMilestone>) -> Result<()> {
        instructions::fund_milestone::handler(ctx)
    }

    /// Freelancer submits a bounded deliverable hash.
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        deliverable_hash: [u8; 32],
    ) -> Result<()> {
        instructions::submit_milestone::handler(ctx, deliverable_hash)
    }

    /// Client approves submitted work and the vault pays the freelancer.
    pub fn approve_and_release(ctx: Context<ApproveAndRelease>) -> Result<()> {
        instructions::approve_and_release::handler(ctx)
    }

    /// Either party raises a dispute. Release is frozen until mutual cancellation.
    pub fn raise_dispute(ctx: Context<RaiseDispute>) -> Result<()> {
        instructions::raise_dispute::handler(ctx)
    }

    /// Both parties authorize returning this milestone's funds to the client.
    pub fn mutual_cancel_and_refund(ctx: Context<MutualCancelAndRefund>) -> Result<()> {
        instructions::mutual_cancel_and_refund::handler(ctx)
    }
}
