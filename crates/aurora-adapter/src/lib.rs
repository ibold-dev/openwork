//! Aurora: the intents routing abstraction used to fund OpenWork escrows.
//!
//! Two implementations exist:
//!
//! - [`mock::MockIntentAdapter`] is deterministic and entirely local. It is the
//!   default in development and CI.
//! - [`oneclick::OneClickAdapter`] calls the real NEAR Intents 1Click REST API
//!   and refuses to be constructed unless the process runs in mainnet mode with
//!   an API key.
//!
//! Neither adapter moves money and neither adapter signs anything. They only
//! describe how a client could route value to their own Solana wallet. The
//! Solana escrow program remains the sole authority over escrowed funds.

pub mod error;
pub mod mock;
pub mod model;
pub mod oneclick;

pub use error::AuroraError;
pub use mock::MockIntentAdapter;
pub use model::{Quote, QuoteRequest, RoutingMode, TransferStatus};
pub use oneclick::{OneClickAdapter, OneClickConfig};

use async_trait::async_trait;

/// A routing provider.
#[async_trait]
pub trait RoutingProvider: Send + Sync {
    /// Which mode this adapter implements.
    fn mode(&self) -> RoutingMode;

    /// Stable provider name for persistence and logs.
    fn name(&self) -> &'static str;

    /// Requests a quote. Implementations must validate the request first.
    async fn quote(&self, request: QuoteRequest) -> Result<Quote, AuroraError>;

    /// Looks up the current transfer status by provider quote id.
    async fn status(&self, id: &str) -> Result<TransferStatus, AuroraError>;
}
