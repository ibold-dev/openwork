use anchor_lang::prelude::*;

use crate::constants::{AGREEMENT_SEED, SCHEMA_VERSION};
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementCreated, AgreementState};

#[derive(Accounts)]
#[instruction(agreement_nonce: u64)]
pub struct CreateAgreement<'info> {
    #[account(mut)]
    pub client: Signer<'info>,

    #[account(
        init,
        payer = client,
        space = Agreement::LEN,
        seeds = [AGREEMENT_SEED, client.key().as_ref(), &agreement_nonce.to_le_bytes()],
        bump,
    )]
    pub agreement: Account<'info, Agreement>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<CreateAgreement>,
    agreement_nonce: u64,
    freelancer: Pubkey,
    mint: Pubkey,
    terms_hash: [u8; 32],
) -> Result<()> {
    let client = ctx.accounts.client.key();

    require_keys_neq!(client, freelancer, EscrowError::InvalidCounterparty);
    require!(
        freelancer != Pubkey::default(),
        EscrowError::InvalidCounterparty
    );
    require!(mint != Pubkey::default(), EscrowError::InvalidMint);

    let agreement = &mut ctx.accounts.agreement;
    agreement.schema_version = SCHEMA_VERSION;
    agreement.bump = ctx.bumps.agreement;
    agreement.state = AgreementState::Draft;
    agreement.client = client;
    agreement.freelancer = freelancer;
    agreement.mint = mint;
    agreement.agreement_nonce = agreement_nonce;
    agreement.terms_hash = terms_hash;
    agreement.milestone_count = 0;
    agreement.settled_count = 0;
    agreement.refunded_count = 0;
    agreement.total_amount = 0;
    agreement.released_amount = 0;
    agreement.created_at = Clock::get()?.unix_timestamp;
    agreement.accepted_at = 0;

    emit!(AgreementCreated {
        agreement: agreement.key(),
        client,
        freelancer,
        mint,
        agreement_nonce,
        terms_hash,
    });

    Ok(())
}
