use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::{AGREEMENT_SEED, MILESTONE_SEED, VAULT_AUTHORITY_SEED};
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementState, Milestone, MilestoneRefunded, MilestoneState};

/// Mutual cancellation is the only way funds leave the vault without client
/// approval. Both parties must sign, and only the unspent milestone amount is
/// returned to the client.
#[derive(Accounts)]
pub struct MutualCancelAndRefund<'info> {
    pub client: Signer<'info>,
    pub freelancer: Signer<'info>,

    #[account(
        mut,
        has_one = client @ EscrowError::UnauthorizedClient,
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

    #[account(
        mut,
        token::mint = agreement.mint,
        token::authority = client,
    )]
    pub client_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = agreement.mint,
        associated_token::authority = vault_authority,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// CHECK: PDA authority for the vault; validated by seeds.
    #[account(
        seeds = [VAULT_AUTHORITY_SEED, agreement.key().as_ref()],
        bump,
    )]
    pub vault_authority: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<MutualCancelAndRefund>) -> Result<()> {
    let agreement = &mut ctx.accounts.agreement;
    let milestone = &mut ctx.accounts.milestone;

    require!(
        matches!(
            milestone.state,
            MilestoneState::Funded | MilestoneState::Submitted | MilestoneState::Disputed
        ),
        EscrowError::MilestoneSettled
    );
    require!(
        milestone.agreement == agreement.key(),
        EscrowError::InvalidVault
    );

    let amount = milestone.amount;
    let index = milestone.index;
    let client = agreement.client;

    milestone.state = MilestoneState::Refunded;
    milestone.settled_at = Clock::get()?.unix_timestamp;
    ctx.accounts.milestone.exit(ctx.program_id)?;

    agreement.settled_count = agreement
        .settled_count
        .checked_add(1)
        .ok_or(EscrowError::Overflow)?;
    agreement.refunded_count = agreement
        .refunded_count
        .checked_add(1)
        .ok_or(EscrowError::Overflow)?;
    if agreement.settled_count == agreement.milestone_count {
        agreement.state = if agreement.refunded_count == agreement.milestone_count {
            AgreementState::Cancelled
        } else {
            AgreementState::Completed
        };
    }

    let agreement_key = agreement.key();
    let vault_authority_bump = ctx.bumps.vault_authority;
    let signer_seeds: &[&[u8]] = &[
        VAULT_AUTHORITY_SEED,
        agreement_key.as_ref(),
        &[vault_authority_bump],
    ];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.client_token_account.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

    emit!(MilestoneRefunded {
        agreement: agreement_key,
        index,
        amount,
        client,
    });

    Ok(())
}
