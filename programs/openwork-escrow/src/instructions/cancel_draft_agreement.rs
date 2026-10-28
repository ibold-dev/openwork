use anchor_lang::prelude::*;

use crate::constants::AGREEMENT_SEED;
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementCancelled, AgreementState};

#[derive(Accounts)]
pub struct CancelDraftAgreement<'info> {
    pub client: Signer<'info>,

    #[account(
        mut,
        has_one = client @ EscrowError::UnauthorizedClient,
        seeds = [AGREEMENT_SEED, agreement.client.as_ref(), &agreement.agreement_nonce.to_le_bytes()],
        bump = agreement.bump,
    )]
    pub agreement: Account<'info, Agreement>,
}

pub fn handler(ctx: Context<CancelDraftAgreement>) -> Result<()> {
    let agreement = &mut ctx.accounts.agreement;

    require!(
        agreement.state == AgreementState::Draft,
        EscrowError::AgreementNotDraft
    );

    agreement.state = AgreementState::Cancelled;

    emit!(AgreementCancelled {
        agreement: agreement.key(),
        client: agreement.client,
    });

    Ok(())
}
