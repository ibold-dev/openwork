//! Protocol limits mirrored from the on-chain program.
//!
//! These values are part of the product contract: the API rejects anything the
//! program would reject, so a user never signs a transaction that is guaranteed
//! to fail.

/// Maximum milestones per agreement (`MAX_MILESTONES` in the program).
pub const MAX_MILESTONES: u16 = 12;

/// On-chain schema version for `Agreement` and `Milestone`.
pub const SCHEMA_VERSION: u8 = 1;

/// Version of the canonical terms document.
pub const TERMS_VERSION: u8 = 1;

/// Maximum length of an agreement title, in bytes.
pub const MAX_TITLE_LEN: usize = 200;

/// Maximum length of an agreement description, in bytes.
pub const MAX_DESCRIPTION_LEN: usize = 5_000;

/// Maximum length of a milestone title, in bytes.
pub const MAX_MILESTONE_TITLE_LEN: usize = 200;

/// Maximum length of a milestone description, in bytes.
pub const MAX_MILESTONE_DESCRIPTION_LEN: usize = 2_000;

/// Maximum length of a deliverable URI, in bytes.
pub const MAX_DELIVERABLE_URI_LEN: usize = 512;

/// Maximum length of a deliverable note, in bytes.
pub const MAX_DELIVERABLE_NOTE_LEN: usize = 2_000;

/// Maximum length of a transaction signature (base58, 64 bytes encoded).
pub const MAX_TX_SIGNATURE_LEN: usize = 88;

/// Maximum length of a wallet address (base58, 32 bytes encoded).
pub const MAX_ADDRESS_LEN: usize = 44;

/// Allowed escrow clusters.
pub const ALLOWED_CLUSTERS: &[&str] = &["localnet", "devnet", "testnet", "mainnet-beta"];
