use anchor_lang::prelude::*;

/// Lifecycle of an agreement.
///
/// `Completed` means every milestone is settled (released or refunded).
/// `Cancelled` means every milestone was refunded after being funded, or the
/// agreement was cancelled while still a draft.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum AgreementState {
    Draft,
    Active,
    Completed,
    Cancelled,
}

/// Lifecycle of a single milestone.
///
/// Funds can only leave the vault through `Released` (client approval) or
/// `Refunded` (mutual cancellation). `Disputed` freezes both.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum MilestoneState {
    Pending,
    Funded,
    Submitted,
    Released,
    Refunded,
    Disputed,
}

#[account]
#[derive(InitSpace, Debug)]
pub struct Agreement {
    pub schema_version: u8,
    pub bump: u8,
    pub state: AgreementState,
    pub client: Pubkey,
    pub freelancer: Pubkey,
    pub mint: Pubkey,
    pub agreement_nonce: u64,
    pub terms_hash: [u8; 32],
    pub milestone_count: u16,
    pub settled_count: u16,
    pub refunded_count: u16,
    pub total_amount: u64,
    pub released_amount: u64,
    pub created_at: i64,
    pub accepted_at: i64,
}

impl Agreement {
    pub const LEN: usize = 8 + Self::INIT_SPACE;
}

#[account]
#[derive(InitSpace, Debug)]
pub struct Milestone {
    pub index: u16,
    pub bump: u8,
    pub state: MilestoneState,
    pub agreement: Pubkey,
    pub amount: u64,
    pub deliverable_hash: [u8; 32],
    pub funded_at: i64,
    pub submitted_at: i64,
    pub settled_at: i64,
}

impl Milestone {
    pub const LEN: usize = 8 + Self::INIT_SPACE;
}

#[event]
pub struct AgreementCreated {
    pub agreement: Pubkey,
    pub client: Pubkey,
    pub freelancer: Pubkey,
    pub mint: Pubkey,
    pub agreement_nonce: u64,
    pub terms_hash: [u8; 32],
}

#[event]
pub struct MilestoneCreated {
    pub agreement: Pubkey,
    pub index: u16,
    pub amount: u64,
}

#[event]
pub struct AgreementAccepted {
    pub agreement: Pubkey,
    pub freelancer: Pubkey,
    pub terms_hash: [u8; 32],
    pub accepted_at: i64,
}

#[event]
pub struct MilestoneFunded {
    pub agreement: Pubkey,
    pub index: u16,
    pub amount: u64,
    pub vault_balance: u64,
}

#[event]
pub struct MilestoneSubmitted {
    pub agreement: Pubkey,
    pub index: u16,
    pub deliverable_hash: [u8; 32],
}

#[event]
pub struct MilestoneReleased {
    pub agreement: Pubkey,
    pub index: u16,
    pub amount: u64,
    pub freelancer: Pubkey,
}

#[event]
pub struct MilestoneRefunded {
    pub agreement: Pubkey,
    pub index: u16,
    pub amount: u64,
    pub client: Pubkey,
}

#[event]
pub struct MilestoneDisputed {
    pub agreement: Pubkey,
    pub index: u16,
    pub raised_by: Pubkey,
}

#[event]
pub struct AgreementCancelled {
    pub agreement: Pubkey,
    pub client: Pubkey,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Account sizes are part of the on-chain ABI and rent calculation.
    /// These assertions must change deliberately, never accidentally.
    #[test]
    fn agreement_and_milestone_sizes_are_stable() {
        assert_eq!(Agreement::LEN, 185);
        assert_eq!(Milestone::LEN, 108);
    }

    #[test]
    fn states_serialize_to_one_byte() {
        assert_eq!(borsh::to_vec(&AgreementState::Active).unwrap().len(), 1);
        assert_eq!(borsh::to_vec(&MilestoneState::Disputed).unwrap().len(), 1);
    }
}
