use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AuroraError;

/// Routing mode selected for a quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    /// Deterministic, local, no network. Never moves funds.
    Mock,
    /// Real NEAR Intents 1Click API.
    OneClick,
}

impl RoutingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            RoutingMode::Mock => "mock",
            RoutingMode::OneClick => "oneclick",
        }
    }

    /// `INTENTS_MODE=mock|mainnet` maps to an adapter mode.
    pub fn from_config(value: &str) -> Result<Self, AuroraError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mock" | "simulated" | "local" => Ok(RoutingMode::Mock),
            "mainnet" | "oneclick" | "near" => Ok(RoutingMode::OneClick),
            other => Err(AuroraError::Disabled(format!(
                "unknown INTENTS_MODE `{other}` (expected mock or mainnet)"
            ))),
        }
    }
}

/// Normalized transfer lifecycle shared by every provider.
///
/// `EscrowFundingPending` and `EscrowFunded` are OpenWork-only states: they are
/// derived from our own Solana reconciliation, never from a vendor response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Requested,
    Quoted,
    SourceTransactionDetected,
    Routing,
    ArrivedInClientWallet,
    EscrowFundingPending,
    EscrowFunded,
    Failed,
    Expired,
    RefundPending,
    Refunded,
}

impl TransferStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            TransferStatus::Requested => "requested",
            TransferStatus::Quoted => "quoted",
            TransferStatus::SourceTransactionDetected => "source_transaction_detected",
            TransferStatus::Routing => "routing",
            TransferStatus::ArrivedInClientWallet => "arrived_in_client_wallet",
            TransferStatus::EscrowFundingPending => "escrow_funding_pending",
            TransferStatus::EscrowFunded => "escrow_funded",
            TransferStatus::Failed => "failed",
            TransferStatus::Expired => "expired",
            TransferStatus::RefundPending => "refund_pending",
            TransferStatus::Refunded => "refunded",
        }
    }

    /// Maps a NEAR 1Click status onto our vocabulary.
    ///
    /// Unknown values map to `Requested`: we never optimistically claim that a
    /// transfer progressed.
    pub fn from_vendor_status(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "PENDING_DEPOSIT" => TransferStatus::Quoted,
            "KNOWN_DEPOSIT_TX" => TransferStatus::SourceTransactionDetected,
            "INCOMPLETE_DEPOSIT" => TransferStatus::SourceTransactionDetected,
            "PROCESSING" => TransferStatus::Routing,
            "SUCCESS" => TransferStatus::ArrivedInClientWallet,
            "REFUNDED" => TransferStatus::Refunded,
            "REFUND_PENDING" => TransferStatus::RefundPending,
            "FAILED" => TransferStatus::Failed,
            "EXPIRED" => TransferStatus::Expired,
            _ => TransferStatus::Requested,
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferStatus::EscrowFunded
                | TransferStatus::Failed
                | TransferStatus::Expired
                | TransferStatus::Refunded
        )
    }

    /// Whether the state implies the client already received routed funds.
    pub const fn implies_client_received_funds(self) -> bool {
        matches!(
            self,
            TransferStatus::ArrivedInClientWallet
                | TransferStatus::EscrowFundingPending
                | TransferStatus::EscrowFunded
        )
    }
}

/// A routing request. Amounts are integer base units as decimal strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRequest {
    /// Caller-supplied reference (invoice id, contract id, ...).
    pub invoice: String,
    /// Amount of `origin_asset` in its base units.
    pub amount: String,
    /// CAIP-2 style origin asset identifier, e.g. `nep141:usdt.tether-token.near`.
    pub origin_asset: String,
    /// Destination asset identifier, e.g. `solana:.../spl:<mint>`.
    pub destination_asset: String,
    /// Destination wallet on Solana.
    pub recipient: String,
    /// Refund address on the origin chain.
    pub refund_to: String,
    /// Requested quote lifetime in seconds.
    pub deadline_seconds: u64,
    /// Slippage tolerance in basis points.
    pub slippage_bps: u16,
}

impl QuoteRequest {
    pub const DEFAULT_DEADLINE_SECONDS: u64 = 600;
    pub const DEFAULT_SLIPPAGE_BPS: u16 = 100;

    pub fn validate(&self) -> Result<(), AuroraError> {
        if self.invoice.trim().is_empty() || self.invoice.len() > 200 {
            return Err(AuroraError::InvalidRequest(
                "invoice must be 1..=200 bytes".into(),
            ));
        }
        if self.amount.is_empty()
            || self.amount.len() > 38
            || !self.amount.bytes().all(|b| b.is_ascii_digit())
            || self.amount.bytes().all(|b| b == b'0')
        {
            return Err(AuroraError::InvalidRequest(
                "amount must be a positive integer string".into(),
            ));
        }
        for (field, value) in [
            ("originAsset", &self.origin_asset),
            ("destinationAsset", &self.destination_asset),
            ("recipient", &self.recipient),
            ("refundTo", &self.refund_to),
        ] {
            if value.trim().is_empty() || value.len() > 200 {
                return Err(AuroraError::InvalidRequest(format!(
                    "{field} must be 1..=200 bytes"
                )));
            }
        }
        if self.deadline_seconds == 0 || self.deadline_seconds > 7 * 24 * 60 * 60 {
            return Err(AuroraError::InvalidRequest(
                "deadlineSeconds must be between 1 and 604800".into(),
            ));
        }
        if self.slippage_bps > 10_000 {
            return Err(AuroraError::InvalidRequest(
                "slippageBps must be at most 10000".into(),
            ));
        }
        Ok(())
    }

    /// Effective deadline if the caller did not specify one.
    pub fn deadline_seconds_or_default(&self) -> u64 {
        if self.deadline_seconds == 0 {
            Self::DEFAULT_DEADLINE_SECONDS
        } else {
            self.deadline_seconds
        }
    }
}

/// A normalized quote returned by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    /// Provider quote id (1Click uses the deposit address as the lookup key).
    pub id: String,
    pub mode: RoutingMode,
    pub provider: String,
    pub deposit_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_memo: Option<String>,
    pub amount_in: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<String>,
    pub origin_asset: String,
    pub destination_asset: String,
    pub recipient: String,
    pub refund_to: String,
    /// RFC3339 deadline for paying the deposit address.
    pub deadline: String,
    /// RFC3339 timestamp after which the quote is no longer usable.
    pub expires_at: String,
    pub status: TransferStatus,
    /// True for adapters that simulate routing locally.
    pub simulated: bool,
    pub created_at: String,
    /// Vendor payload, persisted for debugging and audits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

impl Quote {
    pub fn now_rfc3339() -> String {
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    pub fn in_rfc3339(seconds: i64) -> String {
        (Utc::now() + chrono::Duration::seconds(seconds))
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    pub fn deadline_at(seconds: u64) -> (String, DateTime<Utc>) {
        let at = Utc::now() + chrono::Duration::seconds(seconds as i64);
        (at.to_rfc3339_opts(SecondsFormat::Millis, true), at)
    }
}

/// Lowercase hex encoding, no prefix.
pub fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).expect("nibble"));
        out.push(char::from_digit(u32::from(byte & 0x0f), 16).expect("nibble"));
    }
    out
}
