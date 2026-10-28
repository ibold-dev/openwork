use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::error::AuroraError;
use crate::model::{Quote, QuoteRequest, RoutingMode, TransferStatus};
use crate::RoutingProvider;

/// Deterministic, offline routing adapter for development, demos and tests.
///
/// Properties that make it safe to use in production code paths:
///
/// - every quote is marked `simulated: true`,
/// - the deposit address is a pure function of `(invoice, amount)`, so repeated
///   requests are idempotent,
/// - status can only progress along a fixed, time-based curve and stops at
///   `escrow_funding_pending`,
/// - it never reports `escrow_funded` or `arrived` for real value, because no
///   value ever moves.
#[derive(Debug)]
pub struct MockIntentAdapter {
    quotes: Mutex<HashMap<String, MockQuote>>,
    quote_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
struct MockQuote {
    quote: Quote,
    created: Instant,
}

impl MockIntentAdapter {
    /// Default quote lifetime, matching the production-ish window a client has
    /// to pay a deposit address.
    pub const DEFAULT_TTL_SECONDS: u64 = 120;

    pub fn new() -> Self {
        Self::with_ttl(Self::DEFAULT_TTL_SECONDS)
    }

    pub fn with_ttl(quote_ttl_seconds: u64) -> Self {
        MockIntentAdapter {
            quotes: Mutex::new(HashMap::new()),
            quote_ttl_seconds: quote_ttl_seconds.max(1),
        }
    }

    /// Deterministic deposit address for an invoice and amount.
    pub fn deposit_address_for(invoice: &str, amount: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"openwork:aurora:mock-deposit:v1|");
        hasher.update(invoice.as_bytes());
        hasher.update(b"|");
        hasher.update(amount.as_bytes());
        bs58::encode(hasher.finalize()).into_string()
    }

    /// Status curve, in seconds since the quote was created.
    ///
    /// The curve deliberately stops before `escrow_funded`: the mock describes
    /// a transfer in flight, never a completed deposit into an escrow.
    pub fn status_for_elapsed(elapsed_seconds: u64) -> TransferStatus {
        match elapsed_seconds {
            0..=4 => TransferStatus::Requested,
            5..=14 => TransferStatus::Quoted,
            15..=44 => TransferStatus::SourceTransactionDetected,
            45..=89 => TransferStatus::Routing,
            90..=179 => TransferStatus::ArrivedInClientWallet,
            _ => TransferStatus::EscrowFundingPending,
        }
    }
}

impl Default for MockIntentAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RoutingProvider for MockIntentAdapter {
    fn mode(&self) -> RoutingMode {
        RoutingMode::Mock
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    async fn quote(&self, request: QuoteRequest) -> Result<Quote, AuroraError> {
        request.validate()?;

        let id = {
            let mut hasher = Sha256::new();
            hasher.update(b"openwork:aurora:mock-quote:v1|");
            hasher.update(request.invoice.as_bytes());
            hasher.update(b"|");
            hasher.update(request.amount.as_bytes());
            crate::model::hex_lower(&hasher.finalize())[..32].to_string()
        };

        // A quote that expires before its deposit deadline would tell the caller
        // they can pay after the quote is already unusable, so the simulated
        // deadline is capped by the simulated lifetime.
        let deadline_seconds = request
            .deadline_seconds_or_default()
            .min(self.quote_ttl_seconds);
        let now = Quote::now_rfc3339();

        let quote = Quote {
            id: id.clone(),
            mode: RoutingMode::Mock,
            provider: "mock".to_string(),
            deposit_address: Self::deposit_address_for(&request.invoice, &request.amount),
            deposit_memo: Some(format!("OW-{}", &id[..8])),
            amount_in: request.amount.clone(),
            amount_out: Some(request.amount.clone()),
            origin_asset: request.origin_asset.clone(),
            destination_asset: request.destination_asset.clone(),
            recipient: request.recipient.clone(),
            refund_to: request.refund_to.clone(),
            deadline: Quote::in_rfc3339(deadline_seconds as i64),
            expires_at: Quote::in_rfc3339(self.quote_ttl_seconds as i64),
            status: TransferStatus::Requested,
            simulated: true,
            created_at: now,
            raw: None,
        };

        let mut quotes = self.quotes.lock().expect("mock quote lock");
        let entry = quotes.entry(id).or_insert_with(|| MockQuote {
            quote: quote.clone(),
            created: Instant::now(),
        });
        // Repeated requests return the original quote (stable expiry), but the
        // newest request parameters are what the caller sees.
        let mut stable = entry.quote.clone();
        stable.amount_in = quote.amount_in;
        stable.recipient = quote.recipient;
        stable.refund_to = quote.refund_to;
        stable.deadline = quote.deadline;
        Ok(stable)
    }

    async fn status(&self, id: &str) -> Result<TransferStatus, AuroraError> {
        let quotes = self.quotes.lock().expect("mock quote lock");
        let entry = quotes
            .get(id)
            .ok_or_else(|| AuroraError::NotFound(id.to_string()))?;
        let elapsed = entry.created.elapsed().as_secs();
        Ok(Self::status_for_elapsed(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> QuoteRequest {
        QuoteRequest {
            invoice: "invoice-1".into(),
            amount: "1000000".into(),
            origin_asset: "nep141:usdt.tether-token.near".into(),
            destination_asset: "solana:mainnet/spl:USDC".into(),
            recipient: "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".into(),
            refund_to: "client.near".into(),
            deadline_seconds: 600,
            slippage_bps: 100,
        }
    }

    #[tokio::test]
    async fn quote_is_deterministic_and_simulated() {
        let adapter = MockIntentAdapter::new();
        let first = adapter.quote(request()).await.unwrap();
        let second = adapter.quote(request()).await.unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.deposit_address, second.deposit_address);
        assert!(first.simulated);
        assert_eq!(first.mode, RoutingMode::Mock);
        assert_eq!(first.provider, "mock");
        assert_eq!(
            first.deposit_address,
            MockIntentAdapter::deposit_address_for("invoice-1", "1000000")
        );
        assert_ne!(
            first.deposit_address,
            MockIntentAdapter::deposit_address_for("invoice-1", "1000001")
        );
        assert_ne!(
            first.deposit_address,
            MockIntentAdapter::deposit_address_for("invoice-2", "1000000")
        );
    }

    #[tokio::test]
    async fn quote_expires_after_the_ttl() {
        let adapter = MockIntentAdapter::with_ttl(120);
        let quote = adapter.quote(request()).await.unwrap();
        let created = chrono::DateTime::parse_from_rfc3339(&quote.created_at).unwrap();
        let expires = chrono::DateTime::parse_from_rfc3339(&quote.expires_at).unwrap();
        assert_eq!((expires - created).num_seconds(), 120);
    }

    #[tokio::test]
    async fn deadline_never_exceeds_the_quote_expiry() {
        let adapter = MockIntentAdapter::with_ttl(30);
        let quote = adapter.quote(request()).await.unwrap();
        let deadline = chrono::DateTime::parse_from_rfc3339(&quote.deadline).unwrap();
        let expires = chrono::DateTime::parse_from_rfc3339(&quote.expires_at).unwrap();
        assert!(
            deadline <= expires,
            "deposit deadline {deadline} must not outlive the quote {expires}"
        );
    }

    #[tokio::test]
    async fn status_progresses_and_never_claims_escrow_funding() {
        let adapter = MockIntentAdapter::new();
        let quote = adapter.quote(request()).await.unwrap();
        assert_eq!(
            adapter.status(&quote.id).await.unwrap(),
            TransferStatus::Requested
        );

        assert_eq!(
            MockIntentAdapter::status_for_elapsed(5),
            TransferStatus::Quoted
        );
        assert_eq!(
            MockIntentAdapter::status_for_elapsed(15),
            TransferStatus::SourceTransactionDetected
        );
        assert_eq!(
            MockIntentAdapter::status_for_elapsed(45),
            TransferStatus::Routing
        );
        assert_eq!(
            MockIntentAdapter::status_for_elapsed(90),
            TransferStatus::ArrivedInClientWallet
        );
        assert_eq!(
            MockIntentAdapter::status_for_elapsed(180),
            TransferStatus::EscrowFundingPending
        );

        for elapsed in [0_u64, 5, 15, 45, 90, 180, 3_600, 86_400, u64::MAX / 2] {
            let status = MockIntentAdapter::status_for_elapsed(elapsed);
            assert_ne!(status, TransferStatus::EscrowFunded);
            assert_ne!(status, TransferStatus::Failed);
        }
    }

    #[tokio::test]
    async fn unknown_quote_is_not_found() {
        let adapter = MockIntentAdapter::new();
        assert_eq!(
            adapter.status("missing").await,
            Err(AuroraError::NotFound("missing".into()))
        );
    }

    #[tokio::test]
    async fn invalid_requests_are_rejected_before_quoting() {
        let adapter = MockIntentAdapter::new();

        let mut bad = request();
        bad.amount = "0".into();
        assert!(matches!(
            adapter.quote(bad).await,
            Err(AuroraError::InvalidRequest(_))
        ));

        let mut bad = request();
        bad.amount = "1.5".into();
        assert!(matches!(
            adapter.quote(bad).await,
            Err(AuroraError::InvalidRequest(_))
        ));

        let mut bad = request();
        bad.recipient = String::new();
        assert!(matches!(
            adapter.quote(bad).await,
            Err(AuroraError::InvalidRequest(_))
        ));
    }
}
