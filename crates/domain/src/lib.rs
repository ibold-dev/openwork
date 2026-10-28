//! Domain rules shared by the OpenWork API, worker and (via generated clients)
//! the SDK.
//!
//! Nothing in this crate talks to the network, the database or the chain. It
//! only encodes the rules that both the on-chain program and the off-chain
//! service must agree on:
//!
//! - escrow state machines,
//! - integer amount handling,
//! - canonical terms serialization and hashing,
//! - milestone contiguity,
//! - PDA derivation,
//! - bounded deliverable metadata.

pub mod amount;
pub mod deliverable;
pub mod error;
pub mod limits;
pub mod pda;
pub mod state;
pub mod terms;

pub use amount::Amount;
pub use deliverable::Deliverable;
pub use error::DomainError;
pub use state::{AgreementState, MilestoneState};
pub use terms::{Terms, TermsMilestone};
