use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::constants::{AGREEMENT_SEED, MILESTONE_SEED, VAULT_AUTHORITY_SEED};
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementState, Milestone, MilestoneReleased, MilestoneState};

#[derive(Accounts)]
pub struct ApproveAndRelease<'info> {
    pub client: Signer<'info>,

    #[account(
        mut,
        has_one = client @ EscrowError::UnauthorizedClient,
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

    /// CHECK: The agreed payee. Constrained to the freelancer recorded at creation.
    #[account(address = agreement.freelancer @ EscrowError::UnauthorizedFreelancer)]
    pub freelancer: UncheckedAccount<'info>,

    #[account(address = agreement.mint @ EscrowError::InvalidMint)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = agreement.mint,
        token::authority = freelancer,
    )]
    pub freelancer_token_account: Account<'info, TokenAccount>,

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

pub fn handler(ctx: Context<ApproveAndRelease>) -> Result<()> {
    let agreement = &mut ctx.accounts.agreement;
    let milestone = &mut ctx.accounts.milestone;

    require!(
        agreement.state == AgreementState::Active,
        EscrowError::AgreementNotActive
    );
    require!(
        milestone.state == MilestoneState::Submitted,
        EscrowError::MilestoneNotSubmitted
    );
    require!(
        milestone.agreement == agreement.key(),
        EscrowError::InvalidVault
    );

    let amount = milestone.amount;
    let index = milestone.index;
    let freelancer = agreement.freelancer;

    // State changes before the CPI: the milestone can never be released twice,
    // even if the token transfer reverts the transaction afterwards.
    milestone.state = MilestoneState::Released;
    milestone.settled_at = Clock::get()?.unix_timestamp;
    ctx.accounts.milestone.exit(ctx.program_id)?;

    agreement.released_amount = agreement
        .released_amount
        .checked_add(amount)
        .ok_or(EscrowError::Overflow)?;
    agreement.settled_count = agreement
        .settled_count
        .checked_add(1)
        .ok_or(EscrowError::Overflow)?;
    if agreement.settled_count == agreement.milestone_count {
        agreement.state = AgreementState::Completed;
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
                to: ctx.accounts.freelancer_token_account.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

    emit!(MilestoneReleased {
        agreement: agreement_key,
        index,
        amount,
        freelancer,
    });

    Ok(())
}
