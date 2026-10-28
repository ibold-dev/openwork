//! Seed constants and protocol limits.
//!
//! Seeds are part of the public ABI. Changing any of them is a breaking change
//! that requires a new program deployment and a schema migration.

/// Agreement PDA: `[b"agreement", client, agreement_nonce_le]`.
pub const AGREEMENT_SEED: &[u8] = b"agreement";

/// Milestone PDA: `[b"milestone", agreement, milestone_index_le]`.
pub const MILESTONE_SEED: &[u8] = b"milestone";

/// Authority PDA that owns the escrow vault token account:
/// `[b"vault-authority", agreement]`.
pub const VAULT_AUTHORITY_SEED: &[u8] = b"vault-authority";

/// Maximum milestones per agreement.
///
/// Bounded so that a full lifecycle stays within account-lock and transaction
/// size limits, and so a single agreement cannot become unbounded work.
pub const MAX_MILESTONES: u16 = 12;

/// On-disk schema version for `Agreement` and `Milestone`.
pub const SCHEMA_VERSION: u8 = 1;
