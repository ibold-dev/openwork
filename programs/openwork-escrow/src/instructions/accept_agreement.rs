use anchor_lang::prelude::*;

use crate::constants::AGREEMENT_SEED;
use crate::error::EscrowError;
use crate::state::{Agreement, AgreementAccepted, AgreementState};

#[derive(Accounts)]
#[instruction(terms_hash: [u8; 32])]
pub struct AcceptAgreement<'info> {
    pub freelancer: Signer<'info>,

    #[account(
        mut,
        has_one = freelancer @ EscrowError::UnauthorizedFreelancer,
        seeds = [AGREEMENT_SEED, agreement.client.as_ref(), &agreement.agreement_nonce.to_le_bytes()],
        bump = agreement.bump,
    )]
    pub agreement: Account<'info, Agreement>,
}

pub fn handler(ctx: Context<AcceptAgreement>, terms_hash: [u8; 32]) -> Result<()> {
    let agreement = &mut ctx.accounts.agreement;

    require!(
        agreement.state == AgreementState::Draft,
        EscrowError::AgreementNotDraft
    );
    require!(agreement.milestone_count > 0, EscrowError::NoMilestones);
    require!(
        terms_hash == agreement.terms_hash,
        EscrowError::TermsHashMismatch
    );

    let accepted_at = Clock::get()?.unix_timestamp;
    agreement.state = AgreementState::Active;
    agreement.accepted_at = accepted_at;

    emit!(AgreementAccepted {
        agreement: agreement.key(),
        freelancer: agreement.freelancer,
        terms_hash,
        accepted_at,
    });

    Ok(())
}
