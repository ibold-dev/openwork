//! Minimal Solana JSON-RPC client.
//!
//! The binary deliberately does not depend on `solana-client` or the full Agave
//! SDK: the worker needs five methods, and a small `reqwest` client keeps the
//! dependency tree and the trust surface small.

use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

pub const DEFAULT_RPC_TIMEOUT_SECONDS: u64 = 15;
pub const DEFAULT_COMMITMENT: &str = "confirmed";

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("rpc request failed: {0}")]
    Transport(String),
    #[error("rpc returned error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("unexpected rpc response: {0}")]
    Unexpected(String),
    #[error("invalid base64 account data: {0}")]
    Base64(String),
}

#[derive(Clone)]
pub struct SolanaRpc {
    url: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for SolanaRpc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SolanaRpc").field("url", &self.url).finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    /// `[data, encoding]` tuple, always requested as base64.
    pub data: (String, String),
    pub owner: String,
    pub lamports: u64,
    pub executable: bool,
    pub rent_epoch: u64,
    pub space: Option<u64>,
}

impl AccountInfo {
    pub fn bytes(&self) -> Result<Vec<u8>, RpcError> {
        let (encoded, encoding) = &self.data;
        if encoding != "base64" {
            return Err(RpcError::Unexpected(format!(
                "expected base64 account data, got {encoding}"
            )));
        }
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|err| RpcError::Base64(err.to_string()))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseContext {
    pub slot: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfoResponse {
    pub context: ResponseContext,
    pub value: Option<AccountInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MultipleAccountsResponse {
    pub context: ResponseContext,
    pub value: Vec<Option<AccountInfo>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureStatus {
    pub slot: u64,
    pub confirmations: Option<u64>,
    pub err: Option<Value>,
    pub confirmation_status: Option<String>,
}

impl SignatureStatus {
    /// `err` deserializes as `Some(Null)` for successful transactions.
    pub fn error(&self) -> Option<&Value> {
        self.err.as_ref().filter(|value| !value.is_null())
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(
            self.confirmation_status.as_deref(),
            Some("confirmed") | Some("finalized")
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignatureStatusesResponse {
    pub context: ResponseContext,
    pub value: Vec<Option<SignatureStatus>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    pub signature: String,
    pub slot: u64,
    pub err: Option<Value>,
    pub memo: Option<String>,
    pub block_time: Option<i64>,
    pub confirmation_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionMeta {
    pub err: Option<Value>,
    #[serde(default)]
    pub log_messages: Option<Vec<String>>,
    #[serde(default)]
    pub fee: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionEnvelope {
    pub signatures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionResult {
    pub slot: u64,
    pub block_time: Option<i64>,
    pub meta: Option<TransactionMeta>,
    pub transaction: TransactionEnvelope,
}

impl TransactionResult {
    pub fn block_time_utc(&self) -> Option<DateTime<Utc>> {
        self.block_time
            .and_then(|secs| DateTime::from_timestamp(secs, 0))
    }

    pub fn error(&self) -> Option<&Value> {
        self.meta
            .as_ref()
            .and_then(|meta| meta.err.as_ref())
            .filter(|value| !value.is_null())
    }

    pub fn logs(&self) -> &[String] {
        self.meta
            .as_ref()
            .and_then(|meta| meta.log_messages.as_deref())
            .unwrap_or_default()
    }
}

impl SolanaRpc {
    pub fn new(url: impl Into<String>, timeout: Duration) -> anyhow::Result<Self> {
        let url = url.into();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            anyhow::bail!("SOLANA_RPC_URL must be an absolute http(s) URL");
        }
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("openwork-api/0.1")
            .build()?;
        Ok(SolanaRpc { url, client })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T, RpcError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let response = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|err| RpcError::Transport(err.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| RpcError::Transport(err.to_string()))?;
        if !status.is_success() {
            return Err(RpcError::Transport(format!(
                "{method} returned HTTP {status}: {}",
                truncate(&text, 200)
            )));
        }

        let envelope: RpcEnvelope<T> = serde_json::from_str(&text)
            .map_err(|err| RpcError::Unexpected(format!("{method}: {err}")))?;
        if let Some(error) = envelope.error {
            return Err(RpcError::Rpc {
                code: error.code,
                message: error.message,
            });
        }
        envelope
            .result
            .ok_or_else(|| RpcError::Unexpected(format!("{method} returned no result")))
    }

    pub async fn health(&self) -> Result<(), RpcError> {
        let _: String = self.call("getHealth", json!([])).await?;
        Ok(())
    }

    pub async fn slot(&self) -> Result<u64, RpcError> {
        self.call("getSlot", json!([{ "commitment": DEFAULT_COMMITMENT }]))
            .await
    }

    pub async fn account_info(&self, address: &str) -> Result<AccountInfoResponse, RpcError> {
        self.call(
            "getAccountInfo",
            json!([
                address,
                { "encoding": "base64", "commitment": DEFAULT_COMMITMENT }
            ]),
        )
        .await
    }

    pub async fn multiple_accounts(
        &self,
        addresses: &[String],
    ) -> Result<MultipleAccountsResponse, RpcError> {
        self.call(
            "getMultipleAccounts",
            json!([
                addresses,
                { "encoding": "base64", "commitment": DEFAULT_COMMITMENT }
            ]),
        )
        .await
    }

    pub async fn signature_statuses(
        &self,
        signatures: &[String],
    ) -> Result<SignatureStatusesResponse, RpcError> {
        self.call(
            "getSignatureStatuses",
            json!([
                signatures,
                { "searchTransactionHistory": true }
            ]),
        )
        .await
    }

    pub async fn signatures_for_address(
        &self,
        address: &str,
        limit: usize,
    ) -> Result<Vec<SignatureInfo>, RpcError> {
        self.call(
            "getSignaturesForAddress",
            json!([
                address,
                { "limit": limit, "commitment": DEFAULT_COMMITMENT }
            ]),
        )
        .await
    }

    pub async fn transaction(
        &self,
        signature: &str,
    ) -> Result<Option<TransactionResult>, RpcError> {
        self.call(
            "getTransaction",
            json!([
                signature,
                {
                    "encoding": "json",
                    "maxSupportedTransactionVersion": 0,
                    "commitment": DEFAULT_COMMITMENT
                }
            ]),
        )
        .await
    }
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64_account_data() {
        let info = AccountInfo {
            data: (
                base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4]),
                "base64".to_string(),
            ),
            owner: "owner".into(),
            lamports: 1,
            executable: false,
            rent_epoch: 0,
            space: Some(4),
        };
        assert_eq!(info.bytes().unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn rejects_non_base64_payloads() {
        let info = AccountInfo {
            data: ("{}".into(), "jsonParsed".into()),
            owner: "owner".into(),
            lamports: 1,
            executable: false,
            rent_epoch: 0,
            space: None,
        };
        assert!(matches!(info.bytes(), Err(RpcError::Unexpected(_))));
    }

    #[test]
    fn null_errors_are_successes() {
        let status = SignatureStatus {
            slot: 1,
            confirmations: Some(0),
            err: Some(Value::Null),
            confirmation_status: Some("confirmed".into()),
        };
        assert!(status.error().is_none());
        assert!(status.is_confirmed());
    }

    #[test]
    fn decodes_camel_case_transaction_meta() {
        // Shape copied from a real Devnet `getTransaction` response. The log
        // messages are what the reconciliation worker parses for Anchor events,
        // so a field-name mismatch here silently turns every claim into
        // `no_events_for_contract`.
        let raw = json!({
            "slot": 516_056_033u64,
            "blockTime": 1_793_190_615i64,
            "meta": {
                "err": null,
                "fee": 5_000u64,
                "logMessages": [
                    "Program GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR invoke [1]",
                    "Program data: MeFb3yKlbbUeNMj/S9rKBb19faIO35gq6Z11kJ67R9mEmCZWH3dXUAAAgLLmDgAAAAAaadMg8VR/DFYI3rClVvbbwdxXTo0mOdF3b2WMBrAOiA==",
                    "Program GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR success"
                ]
            },
            "transaction": { "signatures": ["5KnvGAW"] }
        });

        let result: TransactionResult = serde_json::from_value(raw).unwrap();
        assert_eq!(result.slot, 516_056_033);
        assert_eq!(result.block_time, Some(1_793_190_615));
        assert_eq!(result.logs().len(), 3);
        assert!(result.error().is_none());
    }

    #[test]
    fn requires_absolute_urls() {
        assert!(SolanaRpc::new("localhost:8899", Duration::from_secs(1)).is_err());
        assert!(SolanaRpc::new("http://localhost:8899", Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn truncation_is_utf8_safe() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("héllo", 2), "h…");
    }
}
