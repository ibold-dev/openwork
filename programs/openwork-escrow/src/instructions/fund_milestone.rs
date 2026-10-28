use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::constants::{AGREEMENT_SEED, MILESTONE_SEED, VAULT_AUTHORITY_SEED};
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementState, Milestone, MilestoneFunded, MilestoneState};

#[derive(Accounts)]
pub struct FundMilestone<'info> {
    #[account(mut)]
    pub client: Signer<'info>,

    #[account(
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

    #[account(address = agreement.mint @ EscrowError::InvalidMint)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = agreement.mint,
        token::authority = client,
    )]
    pub client_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = client,
        associated_token::mint = mint,
        associated_token::authority = vault_authority,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// CHECK: PDA authority for the vault; validated by seeds. It owns no data.
    #[account(
        seeds = [VAULT_AUTHORITY_SEED, agreement.key().as_ref()],
        bump,
    )]
    pub vault_authority: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<FundMilestone>) -> Result<()> {
    let agreement = &ctx.accounts.agreement;
    let milestone = &mut ctx.accounts.milestone;

    require!(
        agreement.state == AgreementState::Active,
        EscrowError::AgreementNotActive
    );
    require!(
        milestone.state == MilestoneState::Pending,
        EscrowError::MilestoneNotPending
    );
    require!(
        milestone.agreement == agreement.key(),
        EscrowError::InvalidVault
    );

    let amount = milestone.amount;
    let client_balance_before = ctx.accounts.client_token_account.amount;
    let vault_balance_before = ctx.accounts.vault.amount;
    let mint_before = ctx.accounts.vault.mint;
    let vault_owner_before = ctx.accounts.vault.owner;

    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.client_token_account.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.client.to_account_info(),
            },
        ),
        amount,
    )?;

    ctx.accounts.client_token_account.reload()?;
    ctx.accounts.vault.reload()?;

    // Funding is accounted from observed balances, never from user claims.
    require!(
        ctx.accounts.client_token_account.amount.checked_add(amount) == Some(client_balance_before),
        EscrowError::FundingAmountMismatch
    );

    let vault_delta = ctx
        .accounts
        .vault
        .amount
        .checked_sub(vault_balance_before)
        .ok_or(EscrowError::FundingAmountMismatch)?;
    require!(vault_delta == amount, EscrowError::FundingAmountMismatch);

    require!(
        ctx.accounts.vault.mint == mint_before
            && ctx.accounts.vault.owner == vault_owner_before
            && ctx.accounts.vault.mint == agreement.mint,
        EscrowError::InvalidVault
    );

    milestone.state = MilestoneState::Funded;
    milestone.funded_at = Clock::get()?.unix_timestamp;

    emit!(MilestoneFunded {
        agreement: agreement.key(),
        index: milestone.index,
        amount,
        vault_balance: ctx.accounts.vault.amount,
    });

    Ok(())
}
