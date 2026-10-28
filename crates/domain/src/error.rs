use thiserror::Error;

/// Everything the domain layer can reject.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("amount must be a positive integer with no leading zeros")]
    InvalidAmount,
    #[error("amount overflow")]
    AmountOverflow,
    #[error("too many milestones (maximum {0})")]
    TooManyMilestones(u16),
    #[error("at least one milestone is required")]
    NoMilestones,
    #[error("milestone index {expected} was expected, got {actual}")]
    NonContiguousMilestone { expected: u16, actual: u16 },
    #[error("unsupported terms version {0}")]
    UnsupportedTermsVersion(u8),
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains disallowed characters")]
    DisallowedCharacters { field: &'static str },
    #[error("invalid base58 public key")]
    InvalidPublicKey,
    #[error("unsupported cluster: {0}")]
    UnsupportedCluster(String),
    #[error("agreement state cannot transition from {from} to {to}")]
    InvalidAgreementTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("milestone state cannot transition from {from} to {to}")]
    InvalidMilestoneTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("unknown state ordinal {0}")]
    UnknownState(u8),
    #[error("invalid milestone index {0}")]
    InvalidMilestoneIndex(u16),
}
