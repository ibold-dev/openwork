use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("Only the agreement client may perform this action")]
    UnauthorizedClient,
    #[msg("Only the agreement freelancer may perform this action")]
    UnauthorizedFreelancer,
    #[msg("Signer is not a participant of this agreement")]
    UnauthorizedParticipant,
    #[msg("Agreement is not in draft state")]
    AgreementNotDraft,
    #[msg("Agreement is not active")]
    AgreementNotActive,
    #[msg("Agreement is already closed")]
    AgreementClosed,
    #[msg("Milestone is not pending funding")]
    MilestoneNotPending,
    #[msg("Milestone has not been funded")]
    MilestoneNotFunded,
    #[msg("Milestone work has not been submitted")]
    MilestoneNotSubmitted,
    #[msg("Milestone is already settled")]
    MilestoneSettled,
    #[msg("Milestone is disputed and release is frozen")]
    MilestoneDisputed,
    #[msg("Milestone cannot be disputed in its current state")]
    InvalidDisputeState,
    #[msg("Client and freelancer must be distinct, non-default wallets")]
    InvalidCounterparty,
    #[msg("Token mint is invalid for this agreement")]
    InvalidMint,
    #[msg("Vault token account does not match the agreement")]
    InvalidVault,
    #[msg("Milestone amount must be greater than zero")]
    InvalidAmount,
    #[msg("Too many milestones for this agreement")]
    TooManyMilestones,
    #[msg("Milestone index must be the next contiguous index")]
    NonContiguousMilestone,
    #[msg("Terms hash does not match the committed agreement terms")]
    TermsHashMismatch,
    #[msg("Agreement must have at least one milestone before acceptance")]
    NoMilestones,
    #[msg("Funding amount did not match the milestone amount")]
    FundingAmountMismatch,
    #[msg("Deliverable hash must not be empty")]
    InvalidDeliverableHash,
    #[msg("Arithmetic overflow")]
    Overflow,
}
