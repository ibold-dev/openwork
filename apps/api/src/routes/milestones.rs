//! Milestone submissions and their claimed transactions.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::auth::Auth;
use crate::ctx::ReqCtx;
use crate::error::{ApiError, ApiJson};
use crate::routes::contracts::{ensure_party, load_contract, require_role};
use crate::routes::dto::{
    deliverable_for, MilestoneView, SubmitMilestoneRequest, TransactionsResponse,
};
use crate::store::{Milestone, NewAuditEvent};
use crate::AppState;

/// Loads a milestone together with its contract, enforcing cluster binding.
async fn load_milestone(
    state: &AppState,
    id: Uuid,
) -> Result<(Milestone, crate::store::Contract), ApiError> {
    let milestone = state
        .store
        .get_milestone(id)
        .await?
        .ok_or_else(|| ApiError::not_found("milestone not found"))?;
    let (contract, _) = load_contract(state, milestone.contract_id).await?;
    Ok((milestone, contract))
}

/// `POST /v1/milestones/:id/submit`
///
/// Records the off-chain deliverable document and its canonical hash. The
/// freelancer still has to send `submit_milestone` on chain with that hash; the
/// API never signs or sends transactions.
pub async fn submit_milestone(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<SubmitMilestoneRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (milestone, contract) = load_milestone(&state, id).await?;
    ensure_party(&contract, auth.wallet())?;
    if contract.chain != auth.cluster() {
        return Err(ApiError::forbidden(
            "forbidden_cluster",
            "milestone belongs to a different cluster",
        ));
    }
    require_role(&contract, auth.wallet(), "freelancer")?;

    let current = milestone.effective_state();
    if current == "released" || current == "refunded" {
        return Err(ApiError::conflict(
            "milestone_settled",
            "this milestone is already settled",
        ));
    }
    if current == "disputed" {
        return Err(ApiError::conflict(
            "milestone_disputed",
            "a disputed milestone cannot be submitted",
        ));
    }
    if current == "submitted" {
        return Err(ApiError::conflict(
            "milestone_already_submitted",
            "this milestone was already submitted",
        ));
    }
    if current != "funded" {
        return Err(ApiError::conflict(
            "milestone_not_funded",
            "the milestone must be funded before work can be submitted",
        ));
    }

    let deliverable = deliverable_for(
        u16::try_from(milestone.index)
            .map_err(|_| ApiError::internal("stored milestone index out of range"))?,
        body.uri.trim(),
        &body.note,
    );
    deliverable.validate()?;
    let hash = deliverable.hash_hex();

    let milestone = state
        .store
        .set_milestone_deliverable(
            milestone.id,
            &deliverable.uri,
            &deliverable.note,
            &hash,
            Utc::now(),
        )
        .await?;

    state
        .store
        .insert_audit(NewAuditEvent {
            actor_address: Some(auth.wallet().to_string()),
            action: "milestone.submit".to_string(),
            object_type: "milestone".to_string(),
            object_id: Some(milestone.id.to_string()),
            request_id: Some(ctx.request_id),
            ip_address: ctx.client_ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({
                "contractId": contract.id,
                "index": milestone.index,
                "deliverableHash": hash,
            }),
        })
        .await?;

    Ok((StatusCode::OK, Json(MilestoneView::from(&milestone))))
}

/// `GET /v1/milestones/:id/transactions`
///
/// Party-scoped view of the signatures the API has observed for a milestone.
pub async fn list_transactions(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let (milestone, contract) = load_milestone(&state, id).await?;
    ensure_party(&contract, auth.wallet())?;
    if contract.chain != auth.cluster() {
        return Err(ApiError::forbidden(
            "forbidden_cluster",
            "milestone belongs to a different cluster",
        ));
    }

    let settlements = state
        .store
        .list_settlements_for_milestone(milestone.id)
        .await?;

    Ok((
        StatusCode::OK,
        Json(TransactionsResponse {
            transactions: settlements.iter().map(Into::into).collect(),
        }),
    ))
}
