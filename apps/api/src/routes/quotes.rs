//! Intent-based funding quotes.
//!
//! Quotes describe how a client *could* route value from another chain into
//! their own Solana wallet. They never move funds and never fund an escrow: the
//! client still signs `fund_milestone` on Solana with their own wallet.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use openwork_aurora::{QuoteRequest, RoutingMode};

use crate::auth::Auth;
use crate::ctx::ReqCtx;
use crate::error::{ApiError, ApiJson};
use crate::routes::contracts::{ensure_party, load_contract};
use crate::routes::dto::{quote_to_new_quote, QuoteCreateRequest, QuoteView};
use crate::store::NewAuditEvent;
use crate::AppState;

/// `POST /v1/quotes`
pub async fn create_quote(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
    ApiJson(body): ApiJson<QuoteCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mode = state.adapters.resolve(body.mode.as_deref())?;

    let recipient = body.recipient.trim().to_string();
    if recipient != auth.wallet() {
        return Err(ApiError::validation(
            "recipient must be the authenticated wallet; OpenWork never routes funds to a third party",
        ));
    }
    openwork_domain::pda::parse_pubkey(&recipient)
        .map_err(|_| ApiError::validation("recipient must be a base58 Solana address"))?;

    let invoice = body.invoice.trim().to_string();
    if invoice.is_empty() {
        return Err(ApiError::validation("invoice must not be empty"));
    }

    let contract_id = match body.contract_id {
        Some(id) => {
            let (contract, _) = load_contract(&state, id).await?;
            ensure_party(&contract, auth.wallet())?;
            if contract.chain != auth.cluster() {
                return Err(ApiError::forbidden(
                    "forbidden_cluster",
                    "contract belongs to a different cluster",
                ));
            }
            Some(contract.id)
        }
        None => None,
    };

    let request = QuoteRequest {
        invoice: invoice.clone(),
        amount: body.amount.trim().to_string(),
        origin_asset: body.origin_asset.trim().to_string(),
        destination_asset: body.destination_asset.trim().to_string(),
        recipient: recipient.clone(),
        refund_to: body.refund_to.trim().to_string(),
        deadline_seconds: body
            .deadline_seconds
            .unwrap_or(QuoteRequest::DEFAULT_DEADLINE_SECONDS),
        slippage_bps: body
            .slippage_bps
            .unwrap_or(QuoteRequest::DEFAULT_SLIPPAGE_BPS),
    };
    request.validate()?;

    let provider = state.adapters.provider(mode)?;
    let quote = provider.quote(request).await?;
    if quote.recipient != recipient {
        return Err(ApiError::internal(
            "routing provider returned a quote for a different recipient",
        ));
    }

    let record = state
        .store
        .insert_quote(quote_to_new_quote(
            &quote,
            Uuid::new_v4(),
            auth.user.id,
            contract_id,
            invoice,
        ))
        .await?;

    state
        .store
        .insert_audit(NewAuditEvent {
            actor_address: Some(auth.wallet().to_string()),
            action: "quote.create".to_string(),
            object_type: "intent_quote".to_string(),
            object_id: Some(record.id.to_string()),
            request_id: Some(ctx.request_id),
            ip_address: ctx.client_ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({
                "mode": record.mode,
                "provider": record.provider,
                "simulated": record.simulated,
                "contractId": record.contract_id,
            }),
        })
        .await?;

    Ok((StatusCode::CREATED, Json(QuoteView::from(&record))))
}

/// `GET /v1/quotes/:id`
///
/// Refreshes the provider status on a best-effort basis. A provider outage never
/// turns into a 500: the last persisted status is returned with `updatedAt`
/// untouched, and the reconciliation worker keeps escrow truth separate.
pub async fn get_quote(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let mut quote = state
        .store
        .get_quote(id)
        .await?
        .ok_or_else(|| ApiError::not_found("quote not found"))?;

    if quote.user_id != auth.user.id {
        return Err(ApiError::forbidden(
            "forbidden",
            "this quote belongs to another user",
        ));
    }

    if let Ok(provider) = state.adapters.provider_for_mode_str(&quote.mode) {
        match provider.status(&quote.external_id).await {
            Ok(status) => {
                let status = status.as_str();
                if status != quote.status {
                    let now = Utc::now();
                    state
                        .store
                        .update_quote_status(quote.id, status, None, now)
                        .await?;
                    quote.status = status.to_string();
                    quote.updated_at = now;
                }
            }
            Err(err) => {
                tracing::debug!(
                    quote_id = %quote.id,
                    error = %err,
                    "could not refresh quote status"
                );
            }
        }
    }

    Ok((StatusCode::OK, Json(QuoteView::from(&quote))))
}

/// Mode names accepted by the API, exposed for documentation tests.
pub fn accepted_modes() -> [&'static str; 3] {
    [
        RoutingMode::Mock.as_str(),
        RoutingMode::OneClick.as_str(),
        "mainnet",
    ]
}
