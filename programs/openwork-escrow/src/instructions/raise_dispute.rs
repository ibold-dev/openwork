use anchor_lang::prelude::*;

use crate::constants::{AGREEMENT_SEED, MILESTONE_SEED};
use crate::error::EscrowError;
use crate::state::{Agreement, Milestone, MilestoneDisputed, MilestoneState};

#[derive(Accounts)]
pub struct RaiseDispute<'info> {
    pub participant: Signer<'info>,

    #[account(
        constraint = participant.key() == agreement.client
            || participant.key() == agreement.freelancer
            @ EscrowError::UnauthorizedParticipant,
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

pub fn handler(ctx: Context<RaiseDispute>) -> Result<()> {
    let agreement_key = ctx.accounts.agreement.key();
    let milestone = &mut ctx.accounts.milestone;

    require!(
        matches!(
            milestone.state,
            MilestoneState::Funded | MilestoneState::Submitted
        ),
        EscrowError::InvalidDisputeState
    );

    milestone.state = MilestoneState::Disputed;

    emit!(MilestoneDisputed {
        agreement: agreement_key,
        index: milestone.index,
        raised_by: ctx.accounts.participant.key(),
    });

    Ok(())
}
