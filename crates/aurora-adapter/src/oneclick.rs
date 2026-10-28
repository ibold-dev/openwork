use std::time::Duration;

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AuroraError;
use crate::model::{Quote, QuoteRequest, RoutingMode, TransferStatus};
use crate::RoutingProvider;

/// Production 1Click endpoint.
pub const DEFAULT_BASE_URL: &str = "https://1click.chaindefuser.com";

/// NEAR Intents 1Click REST client.
///
/// This adapter refuses to exist unless the process was explicitly configured
/// for mainnet **and** a 1Click API key is present. There is no "default
/// enabled" path: a missing key is a hard error, not a silent fallback to mock.
#[derive(Debug, Clone)]
pub struct OneClickAdapter {
    http: reqwest::Client,
    config: OneClickConfig,
}

#[derive(Debug, Clone)]
pub struct OneClickConfig {
    pub base_url: String,
    pub api_key: String,
    pub mode: RoutingMode,
    /// 1Click `dry` flag: quote without reserving liquidity.
    pub dry: bool,
    pub timeout: Duration,
}

impl OneClickConfig {
    /// Reads `INTENTS_MODE`, `ONECLICK_API_KEY` and optional
    /// `ONECLICK_BASE_URL` / `ONECLICK_DRY`.
    pub fn from_env() -> Result<Self, AuroraError> {
        let mode_raw = std::env::var("INTENTS_MODE").unwrap_or_else(|_| "mock".to_string());
        let mode = RoutingMode::from_config(&mode_raw)?;
        let api_key = std::env::var("ONECLICK_API_KEY").ok();
        let base_url = std::env::var("ONECLICK_BASE_URL").ok();
        let dry = std::env::var("ONECLICK_DRY")
            .map(|value| !matches!(value.as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
        Self::new(mode, api_key, base_url, dry)
    }

    /// The single constructor gate. Both conditions are required:
    /// `mode == OneClick` (i.e. `INTENTS_MODE=mainnet`) and a non-empty API key.
    pub fn new(
        mode: RoutingMode,
        api_key: Option<String>,
        base_url: Option<String>,
        dry: bool,
    ) -> Result<Self, AuroraError> {
        if mode != RoutingMode::OneClick {
            return Err(AuroraError::Disabled(
                "1Click requires INTENTS_MODE=mainnet (it is not available in mock mode)".into(),
            ));
        }
        let api_key = api_key.unwrap_or_default();
        if api_key.trim().is_empty() {
            return Err(AuroraError::Disabled(
                "ONECLICK_API_KEY must be set to use NEAR Intents 1Click".into(),
            ));
        }
        Ok(OneClickConfig {
            base_url: base_url
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.trim().to_string(),
            mode,
            dry,
            timeout: Duration::from_secs(15),
        })
    }
}

impl OneClickAdapter {
    pub fn new(config: OneClickConfig) -> Result<Self, AuroraError> {
        // Re-validate so a hand-built config cannot bypass the gate.
        if config.mode != RoutingMode::OneClick {
            return Err(AuroraError::Disabled(
                "1Click adapter cannot run outside mainnet mode".into(),
            ));
        }
        if config.api_key.trim().is_empty() {
            return Err(AuroraError::Disabled(
                "ONECLICK_API_KEY must be set to use NEAR Intents 1Click".into(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent("openwork-aurora/0.1")
            .build()
            .map_err(|err| AuroraError::Provider(format!("http client: {err}")))?;
        Ok(OneClickAdapter { http, config })
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url, path)
    }

    /// Parses a `POST /v0/quote` body into a normalized quote. Kept separate
    /// from the HTTP call so the vendor wire format is testable offline.
    pub fn parse_quote_response(body: &str, request: &QuoteRequest) -> Result<Quote, AuroraError> {
        let parsed: VendorQuoteResponse = serde_json::from_str(body)
            .map_err(|err| AuroraError::Provider(format!("invalid quote response: {err}")))?;
        let vendor = parsed.quote;
        if vendor.deposit_address.trim().is_empty() {
            return Err(AuroraError::Provider(
                "quote response is missing depositAddress".into(),
            ));
        }
        let deadline = vendor
            .deadline
            .clone()
            .unwrap_or_else(|| Quote::in_rfc3339(request.deadline_seconds_or_default() as i64));
        let expires_at = vendor
            .time_when_inactive
            .clone()
            .unwrap_or_else(|| deadline.clone());

        Ok(Quote {
            id: vendor.deposit_address.clone(),
            mode: RoutingMode::OneClick,
            provider: "oneclick".to_string(),
            deposit_address: vendor.deposit_address,
            deposit_memo: vendor.deposit_memo.filter(|memo| !memo.is_empty()),
            amount_in: vendor.amount_in,
            amount_out: vendor.amount_out,
            origin_asset: request.origin_asset.clone(),
            destination_asset: request.destination_asset.clone(),
            recipient: request.recipient.clone(),
            refund_to: request.refund_to.clone(),
            deadline,
            expires_at,
            status: TransferStatus::Quoted,
            simulated: false,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            raw: serde_json::from_str(body).ok(),
        })
    }

    /// Parses a `GET /v0/status` body.
    pub fn parse_status_response(body: &str) -> Result<TransferStatus, AuroraError> {
        let parsed: VendorStatusResponse = serde_json::from_str(body)
            .map_err(|err| AuroraError::Provider(format!("invalid status response: {err}")))?;
        Ok(TransferStatus::from_vendor_status(&parsed.status))
    }

    fn request_body(&self, request: &QuoteRequest) -> VendorQuoteRequest {
        let deadline =
            Utc::now() + chrono::Duration::seconds(request.deadline_seconds_or_default() as i64);
        VendorQuoteRequest {
            dry: self.config.dry,
            swap_type: "EXACT_INPUT",
            slippage_tolerance: request.slippage_bps,
            origin_asset: request.origin_asset.clone(),
            deposit_type: "ORIGIN_CHAIN",
            destination_asset: request.destination_asset.clone(),
            amount: request.amount.clone(),
            recipient: request.recipient.clone(),
            recipient_type: "DESTINATION_CHAIN",
            refund_to: request.refund_to.clone(),
            refund_type: "ORIGIN_CHAIN",
            deadline: deadline.to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }

    fn auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.header("X-API-Key", &self.config.api_key)
    }

    async fn parse_error(response: reqwest::Response) -> AuroraError {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(300).collect();
        AuroraError::Provider(format!("1Click returned {status}: {snippet}"))
    }
}

#[async_trait]
impl RoutingProvider for OneClickAdapter {
    fn mode(&self) -> RoutingMode {
        RoutingMode::OneClick
    }

    fn name(&self) -> &'static str {
        "oneclick"
    }

    async fn quote(&self, request: QuoteRequest) -> Result<Quote, AuroraError> {
        request.validate()?;
        let body = self.request_body(&request);
        let response = self
            .auth(self.http.post(self.endpoint("/v0/quote")))
            .json(&body)
            .send()
            .await
            .map_err(|err| AuroraError::Provider(format!("1Click quote request: {err}")))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        let text = response
            .text()
            .await
            .map_err(|err| AuroraError::Provider(format!("1Click quote body: {err}")))?;
        Self::parse_quote_response(&text, &request)
    }

    async fn status(&self, id: &str) -> Result<TransferStatus, AuroraError> {
        if id.trim().is_empty() {
            return Err(AuroraError::NotFound(id.to_string()));
        }
        let response = self
            .auth(self.http.get(self.endpoint("/v0/status")))
            .query(&[("depositAddress", id)])
            .send()
            .await
            .map_err(|err| AuroraError::Provider(format!("1Click status request: {err}")))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AuroraError::NotFound(id.to_string()));
        }
        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        let text = response
            .text()
            .await
            .map_err(|err| AuroraError::Provider(format!("1Click status body: {err}")))?;
        Self::parse_status_response(&text)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VendorQuoteRequest {
    dry: bool,
    swap_type: &'static str,
    slippage_tolerance: u16,
    origin_asset: String,
    deposit_type: &'static str,
    destination_asset: String,
    amount: String,
    recipient: String,
    recipient_type: &'static str,
    refund_to: String,
    refund_type: &'static str,
    deadline: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorQuoteResponse {
    pub quote: VendorQuote,
    #[serde(default)]
    pub quote_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorQuote {
    pub deposit_address: String,
    #[serde(default)]
    pub deposit_memo: Option<String>,
    pub amount_in: String,
    #[serde(default)]
    pub amount_out: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub time_when_inactive: Option<String>,
    #[serde(default)]
    pub time_estimate: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorStatusResponse {
    pub status: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub quote_response: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUOTE_FIXTURE: &str = include_str!("../tests/fixtures/quote_response.json");
    const STATUS_SUCCESS: &str = include_str!("../tests/fixtures/status_success.json");
    const STATUS_PENDING: &str = include_str!("../tests/fixtures/status_pending.json");
    const STATUS_FAILED: &str = include_str!("../tests/fixtures/status_failed.json");
    const ERROR_FIXTURE: &str = include_str!("../tests/fixtures/error_response.json");

    fn request() -> QuoteRequest {
        QuoteRequest {
            invoice: "invoice-1".into(),
            amount: "1000000".into(),
            origin_asset: "nep141:usdt.tether-token.near".into(),
            destination_asset: "solana:mainnet/spl:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                .into(),
            recipient: "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".into(),
            refund_to: "client.near".into(),
            deadline_seconds: 600,
            slippage_bps: 100,
        }
    }

    #[test]
    fn refuses_to_construct_without_mainnet_mode() {
        let err =
            OneClickConfig::new(RoutingMode::Mock, Some("key".into()), None, true).unwrap_err();
        assert!(matches!(err, AuroraError::Disabled(_)));
        assert!(err.to_string().contains("mainnet"));
    }

    #[test]
    fn refuses_to_construct_without_api_key() {
        for key in [None, Some(String::new()), Some("   ".to_string())] {
            let err = OneClickConfig::new(RoutingMode::OneClick, key, None, true).unwrap_err();
            assert!(matches!(err, AuroraError::Disabled(_)));
            assert!(err.to_string().contains("ONECLICK_API_KEY"));
        }
    }

    #[test]
    fn constructs_in_mainnet_with_key() {
        let config =
            OneClickConfig::new(RoutingMode::OneClick, Some("secret".into()), None, true).unwrap();
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        let adapter = OneClickAdapter::new(config).unwrap();
        assert_eq!(adapter.mode(), RoutingMode::OneClick);
        assert_eq!(adapter.name(), "oneclick");
        assert_eq!(adapter.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn parses_vendor_quote_fixture() {
        let quote = OneClickAdapter::parse_quote_response(QUOTE_FIXTURE, &request()).unwrap();
        assert_eq!(
            quote.deposit_address,
            "0x4f3a2b1c9d8e7f6051423322110099aabbccddeeff00112233445566778899aa"
        );
        assert_eq!(quote.id, quote.deposit_address);
        assert_eq!(quote.amount_in, "1000000");
        assert_eq!(quote.amount_out.as_deref(), Some("998500"));
        assert_eq!(quote.deadline, "2026-11-01T00:10:00.000Z");
        assert_eq!(quote.expires_at, "2026-11-01T00:20:00.000Z");
        assert_eq!(quote.status, TransferStatus::Quoted);
        assert!(!quote.simulated);
        assert_eq!(quote.mode, RoutingMode::OneClick);
        assert_eq!(quote.deposit_memo, None);
        assert!(quote.raw.is_some());
    }

    #[test]
    fn quote_missing_deposit_address_is_a_provider_error() {
        let broken = r#"{"quote":{"amountIn":"1"}}"#;
        let err = OneClickAdapter::parse_quote_response(broken, &request()).unwrap_err();
        assert!(matches!(err, AuroraError::Provider(_)));
    }

    #[test]
    fn maps_vendor_statuses_conservatively() {
        assert_eq!(
            OneClickAdapter::parse_status_response(STATUS_SUCCESS).unwrap(),
            TransferStatus::ArrivedInClientWallet
        );
        assert_eq!(
            OneClickAdapter::parse_status_response(STATUS_PENDING).unwrap(),
            TransferStatus::Quoted
        );
        assert_eq!(
            OneClickAdapter::parse_status_response(STATUS_FAILED).unwrap(),
            TransferStatus::Failed
        );
        assert_eq!(
            TransferStatus::from_vendor_status("SOMETHING_NEW"),
            TransferStatus::Requested
        );
        assert_eq!(
            TransferStatus::from_vendor_status("processing"),
            TransferStatus::Routing
        );
    }

    #[test]
    fn error_fixture_is_not_mistaken_for_a_status() {
        assert!(OneClickAdapter::parse_status_response(ERROR_FIXTURE).is_err());
    }

    #[test]
    fn vendor_statuses_never_map_to_escrow_states() {
        for vendor in [
            "PENDING_DEPOSIT",
            "KNOWN_DEPOSIT_TX",
            "INCOMPLETE_DEPOSIT",
            "PROCESSING",
            "SUCCESS",
            "REFUNDED",
            "FAILED",
            "EXPIRED",
            "UNKNOWN",
        ] {
            let mapped = TransferStatus::from_vendor_status(vendor);
            assert_ne!(mapped, TransferStatus::EscrowFunded);
            assert_ne!(mapped, TransferStatus::EscrowFundingPending);
        }
    }

    /// Opt-in live check. Requires `INTENTS_MODE=mainnet` and
    /// `ONECLICK_API_KEY`; never runs in CI.
    #[tokio::test]
    #[ignore = "live network call to 1Click; requires ONECLICK_API_KEY and INTENTS_MODE=mainnet"]
    async fn live_dry_quote() {
        let config = OneClickConfig::from_env().expect("1Click env configuration");
        assert!(config.dry, "live test must use dry quotes");
        let adapter = OneClickAdapter::new(config).unwrap();

        let mut req = request();
        req.amount = std::env::var("ONECLICK_TEST_AMOUNT").unwrap_or_else(|_| "1000000".into());
        let quote = adapter.quote(req).await.expect("dry quote");
        assert!(!quote.deposit_address.is_empty());
        assert_eq!(quote.status, TransferStatus::Quoted);
    }
}
