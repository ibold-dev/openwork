use anchor_lang::prelude::*;

use crate::constants::{AGREEMENT_SEED, MILESTONE_SEED};
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementState, Milestone, MilestoneState, MilestoneSubmitted};

#[derive(Accounts)]
pub struct SubmitMilestone<'info> {
    pub freelancer: Signer<'info>,

    #[account(
        has_one = freelancer @ EscrowError::UnauthorizedFreelancer,
        seeds = [AGREEMENT_SEED, agreement.client.as_ref(), &agreement.agreement_nonce.to_le_bytes()],
        bump = agreement.bump,
    )]
    pub agreement: Account<'info, Agreement>,

    #[account(
        mut,
        has_one = agreement @ EscrowError::InvalidVault,
        seeds = [MILESTONE_SEED, agreement.key().as_ref(), &milestone.index.to_le_bytes()],
        bump = milestone.bump,
    )]
    pub milestone: Account<'info, Milestone>,
}

pub fn handler(ctx: Context<SubmitMilestone>, deliverable_hash: [u8; 32]) -> Result<()> {
    let agreement = &ctx.accounts.agreement;
    let milestone = &mut ctx.accounts.milestone;

    require!(
        agreement.state == AgreementState::Active,
        EscrowError::AgreementNotActive
    );
    require!(
        milestone.state == MilestoneState::Funded,
        EscrowError::MilestoneNotFunded
    );
    require!(
        deliverable_hash != [0u8; 32],
        EscrowError::InvalidDeliverableHash
    );

    milestone.deliverable_hash = deliverable_hash;
    milestone.submitted_at = Clock::get()?.unix_timestamp;
    milestone.state = MilestoneState::Submitted;

    emit!(MilestoneSubmitted {
        agreement: agreement.key(),
        index: milestone.index,
        deliverable_hash,
    });

    Ok(())
}
