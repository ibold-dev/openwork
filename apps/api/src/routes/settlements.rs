//! Claimed on-chain settlements.
//!
//! A settlement row is a *claim*: the client submits a signature it obtained by
//! signing locally with its wallet. The API never signs, never custodies keys,
//! and never trusts the claim until the reconciliation worker confirms it on
//! chain (`verification_status` starts as `unverified`).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use openwork_domain::limits::MAX_TX_SIGNATURE_LEN;

use crate::auth::Auth;
use crate::ctx::ReqCtx;
use crate::error::{ApiError, ApiJson};
use crate::routes::contracts::{ensure_party, load_contract};
use crate::routes::dto::{
    SettlementRequest, SettlementResponse, MILESTONE_SETTLEMENT_KINDS, SETTLEMENT_KINDS,
};
use crate::routes::idempotency_key;
use crate::store::{IdempotentRecord, NewAuditEvent, NewSettlement};
use crate::AppState;

const ENDPOINT: &str = "POST /v1/settlements";

/// Which party is expected to have signed the instruction.
///
/// This is a UX guard, not a security boundary: the program enforces the real
/// signer set on chain. The reconciliation worker is what makes the record
/// authoritative.
fn expected_role(kind: &str) -> Option<&'static str> {
    match kind {
        "create_agreement"
        | "create_milestone"
        | "cancel_draft_agreement"
        | "fund_milestone"
        | "approve_and_release" => Some("client"),
        "accept_agreement" | "submit_milestone" => Some("freelancer"),
        _ => None,
    }
}

fn validate_signature(signature: &str) -> Result<String, ApiError> {
    let signature = signature.trim();
    if signature.is_empty() {
        return Err(ApiError::validation("signature must not be empty"));
    }
    if signature.len() > MAX_TX_SIGNATURE_LEN {
        return Err(ApiError::validation(format!(
            "signature must be at most {MAX_TX_SIGNATURE_LEN} characters"
        )));
    }
    let decoded = bs58::decode(signature)
        .into_vec()
        .map_err(|_| ApiError::validation("signature must be base58 encoded"))?;
    if decoded.len() != 64 {
        return Err(ApiError::validation(
            "signature must decode to exactly 64 bytes",
        ));
    }
    Ok(signature.to_string())
}

/// Canonical hash of the submission, used to detect idempotency-key reuse with
/// a different payload.
fn request_hash(body: &SettlementRequest, signature: &str) -> String {
    let canonical = serde_json::json!({
        "contractId": body.contract_id,
        "milestoneIndex": body.milestone_index,
        "signature": signature,
        "kind": body.kind,
    });
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

/// `POST /v1/settlements`
pub async fn create_settlement(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SettlementRequest>,
) -> Result<Response, ApiError> {
    let kind = body.kind.trim().to_ascii_lowercase();
    if !SETTLEMENT_KINDS.contains(&kind.as_str()) {
        return Err(ApiError::validation(format!(
            "unknown settlement kind `{kind}`"
        )));
    }

    let signature = validate_signature(&body.signature)?;
    let idempotency = idempotency_key(&headers);
    let hash = request_hash(&body, &signature);

    if let Some(key) = idempotency.as_deref() {
        if let Some(record) = state
            .store
            .get_idempotency(auth.user.id, ENDPOINT, key)
            .await?
        {
            if record.request_hash != hash {
                return Err(ApiError::conflict(
                    "idempotency_key_reuse",
                    "this Idempotency-Key was already used for a different request",
                ));
            }
            let status =
                StatusCode::from_u16(record.response_status as u16).unwrap_or(StatusCode::OK);
            return Ok((status, Json(record.response_body)).into_response());
        }
    }

    let (contract, milestones) = load_contract(&state, body.contract_id).await?;
    ensure_party(&contract, auth.wallet())?;
    if contract.chain != auth.cluster() {
        return Err(ApiError::forbidden(
            "forbidden_cluster",
            "contract belongs to a different cluster",
        ));
    }

    let is_milestone_kind = MILESTONE_SETTLEMENT_KINDS.contains(&kind.as_str());
    let (milestone_id, milestone_index) = if is_milestone_kind {
        let index = body.milestone_index.ok_or_else(|| {
            ApiError::validation(format!("kind `{kind}` requires milestoneIndex"))
        })?;
        let milestone = milestones
            .iter()
            .find(|milestone| milestone.index == i32::from(index))
            .ok_or_else(|| ApiError::not_found("milestone index does not exist"))?;
        (Some(milestone.id), Some(i32::from(index)))
    } else {
        if body.milestone_index.is_some() {
            return Err(ApiError::validation(format!(
                "kind `{kind}` is agreement-level and must not set milestoneIndex"
            )));
        }
        (None, None)
    };

    if let Some(role) = expected_role(&kind) {
        if contract.role_of(auth.wallet()) != Some(role) {
            return Err(ApiError::forbidden(
                "forbidden_role",
                format!("only the {role} may claim `{kind}`"),
            ));
        }
    }

    let (settlement, created) = state
        .store
        .insert_settlement(NewSettlement {
            id: Uuid::new_v4(),
            chain: contract.chain.clone(),
            tx_signature: signature.clone(),
            kind: kind.clone(),
            contract_id: contract.id,
            milestone_id,
            milestone_index,
            claimed_by_address: auth.wallet().to_string(),
        })
        .await?;

    let response = SettlementResponse {
        settlement: (&settlement).into(),
        created,
    };
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    if created {
        state
            .store
            .insert_audit(NewAuditEvent {
                actor_address: Some(auth.wallet().to_string()),
                action: "settlement.claim".to_string(),
                object_type: "onchain_transaction".to_string(),
                object_id: Some(settlement.id.to_string()),
                request_id: Some(ctx.request_id),
                ip_address: ctx.client_ip,
                user_agent: ctx.user_agent,
                metadata: serde_json::json!({
                    "contractId": contract.id,
                    "kind": kind,
                    "signature": signature,
                    "milestoneIndex": milestone_index,
                }),
            })
            .await?;
    }

    let body = serde_json::to_value(&response)
        .map_err(|err| ApiError::internal(format!("could not serialize response: {err}")))?;

    if let Some(key) = idempotency {
        state
            .store
            .put_idempotency(IdempotentRecord {
                id: Uuid::new_v4(),
                user_id: auth.user.id,
                endpoint: ENDPOINT.to_string(),
                key,
                request_hash: hash,
                response_status: i32::from(status.as_u16()),
                response_body: body,
                expires_at: Utc::now() + Duration::hours(24),
            })
            .await?;
    }

    Ok((status, Json(response)).into_response())
}
