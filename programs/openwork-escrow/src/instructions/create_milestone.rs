use anchor_lang::prelude::*;

use crate::constants::{AGREEMENT_SEED, MAX_MILESTONES, MILESTONE_SEED};
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementState, Milestone, MilestoneCreated, MilestoneState};

#[derive(Accounts)]
#[instruction(index: u16, amount: u64)]
pub struct CreateMilestone<'info> {
    #[account(mut)]
    pub client: Signer<'info>,

    #[account(
        mut,
        has_one = client @ EscrowError::UnauthorizedClient,
        seeds = [AGREEMENT_SEED, agreement.client.as_ref(), &agreement.agreement_nonce.to_le_bytes()],
        bump = agreement.bump,
    )]
    pub agreement: Account<'info, Agreement>,

    #[account(
        init,
        payer = client,
        space = Milestone::LEN,
        seeds = [MILESTONE_SEED, agreement.key().as_ref(), &index.to_le_bytes()],
        bump,
    )]
    pub milestone: Account<'info, Milestone>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CreateMilestone>, index: u16, amount: u64) -> Result<()> {
    let agreement = &mut ctx.accounts.agreement;

    require!(
        agreement.state == AgreementState::Draft,
        EscrowError::AgreementNotDraft
    );
    require!(amount > 0, EscrowError::InvalidAmount);
    require!(
        agreement.milestone_count < MAX_MILESTONES,
        EscrowError::TooManyMilestones
    );
    require!(
        index == agreement.milestone_count,
        EscrowError::NonContiguousMilestone
    );

    agreement.total_amount = agreement
        .total_amount
        .checked_add(amount)
        .ok_or(EscrowError::Overflow)?;
    agreement.milestone_count = agreement
        .milestone_count
        .checked_add(1)
        .ok_or(EscrowError::Overflow)?;

    let milestone = &mut ctx.accounts.milestone;
    milestone.index = index;
    milestone.bump = ctx.bumps.milestone;
    milestone.state = MilestoneState::Pending;
    milestone.agreement = agreement.key();
    milestone.amount = amount;
    milestone.deliverable_hash = [0u8; 32];
    milestone.funded_at = 0;
    milestone.submitted_at = 0;
    milestone.settled_at = 0;

    emit!(MilestoneCreated {
        agreement: agreement.key(),
        index,
        amount,
    });

    Ok(())
}
