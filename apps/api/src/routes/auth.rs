use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, Utc};
use uuid::Uuid;

use openwork_domain::pda::parse_pubkey;

use crate::auth::{self, Auth};
use crate::ctx::ReqCtx;
use crate::error::{ApiError, ApiJson};
use crate::routes::dto::{
    ChallengeRequest, ChallengeResponse, LogoutResponse, MeResponse, UserView, VerifyRequest,
    VerifyResponse,
};
use crate::routes::json_with_cookie;
use crate::store::{Challenge, NewAuditEvent, Session};
use crate::AppState;

/// `POST /v1/auth/challenge`
pub async fn challenge(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ChallengeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let address = body.address.trim().to_string();
    parse_pubkey(&address)
        .map_err(|_| ApiError::validation("address must be a base58 Solana public key"))?;

    let cluster = body
        .cluster
        .unwrap_or_else(|| state.config.default_cluster.clone());
    if !state.config.cluster_allowed(&cluster) {
        return Err(ApiError::bad_request(
            "unsupported_cluster",
            format!("cluster `{cluster}` is not enabled on this deployment"),
        ));
    }

    let now = Utc::now();
    let expires_at = now + Duration::seconds(state.config.challenge_ttl_seconds);
    let nonce = auth::new_nonce();
    let message = auth::build_message(
        &state.config.domain,
        &address,
        auth::STATEMENT,
        &state.config.public_origin,
        &cluster,
        &nonce,
        now,
        expires_at,
    );

    let challenge = Challenge {
        id: Uuid::new_v4(),
        address,
        cluster,
        nonce,
        domain: state.config.domain.clone(),
        origin: state.config.public_origin.clone(),
        statement: auth::STATEMENT.to_string(),
        message,
        issued_at: now,
        expires_at,
        consumed_at: None,
        created_at: now,
    };
    let response = ChallengeResponse {
        challenge_id: challenge.id,
        message: challenge.message.clone(),
        nonce: challenge.nonce.clone(),
        expires_at: challenge.expires_at,
    };
    state.store.insert_challenge(&challenge).await?;

    Ok((StatusCode::OK, Json(response)))
}

/// `POST /v1/auth/verify`
pub async fn verify(
    State(state): State<AppState>,
    ctx: ReqCtx,
    headers: HeaderMap,
    ApiJson(body): ApiJson<VerifyRequest>,
) -> Result<Response, ApiError> {
    let challenge = state
        .store
        .get_challenge(body.challenge_id)
        .await?
        .ok_or_else(|| {
            ApiError::bad_request(
                "unknown_challenge",
                "challenge does not exist or was pruned",
            )
        })?;

    let now = Utc::now();
    if challenge.consumed_at.is_some() {
        return Err(ApiError::bad_request(
            "nonce_replayed",
            "this challenge was already used",
        ));
    }
    if challenge.expires_at <= now {
        return Err(ApiError::bad_request(
            "challenge_expired",
            "this challenge has expired; request a new one",
        ));
    }

    // Reconstruct the message from stored fields and compare: a tampered row
    // cannot be used to obtain a session for different terms.
    let expected = auth::build_message(
        &challenge.domain,
        &challenge.address,
        &challenge.statement,
        &challenge.origin,
        &challenge.cluster,
        &challenge.nonce,
        challenge.issued_at,
        challenge.expires_at,
    );
    if expected != challenge.message {
        return Err(ApiError::bad_request(
            "challenge_mismatch",
            "challenge record is inconsistent",
        ));
    }

    auth::siws::verify_signature(&challenge.address, &challenge.message, &body.signature).map_err(
        |err| {
            tracing::info!(error = %err, address = %challenge.address, "signature rejected");
            ApiError::unauthorized("signature does not match the wallet and message")
        },
    )?;

    if !state.store.consume_challenge(challenge.id, now).await? {
        return Err(ApiError::bad_request(
            "nonce_replayed",
            "this challenge was already used or expired",
        ));
    }

    let user = state
        .store
        .upsert_user(Uuid::new_v4(), &challenge.address, now)
        .await?;

    // Rotate: any session already present on this browser is revoked.
    if let Some(existing) = auth::session::token_from_headers(&headers) {
        if let Some((old_session, _)) = state
            .store
            .find_session(&auth::hash_token(&existing))
            .await?
        {
            state.store.revoke_session(old_session.id, now).await?;
        }
    }

    let token = auth::generate_token();
    let csrf_token = auth::generate_token();
    let session = Session {
        id: Uuid::new_v4(),
        user_id: user.id,
        token_hash: auth::hash_token(&token),
        csrf_token: csrf_token.clone(),
        cluster: challenge.cluster.clone(),
        created_at: now,
        last_seen_at: now,
        expires_at: now + Duration::seconds(state.config.session_ttl_seconds),
        revoked_at: None,
        ip_address: ctx.client_ip.clone(),
        user_agent: ctx.user_agent.clone(),
    };
    state.store.insert_session(&session).await?;

    state
        .store
        .insert_audit(NewAuditEvent {
            actor_address: Some(user.wallet_address.clone()),
            action: "auth.login".to_string(),
            object_type: "user".to_string(),
            object_id: Some(user.id.to_string()),
            request_id: Some(ctx.request_id.clone()),
            ip_address: ctx.client_ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({ "cluster": challenge.cluster }),
        })
        .await?;

    let cookie = auth::session_cookie(
        &token,
        state.config.session_ttl_seconds,
        state.config.cookie_secure,
    );
    Ok(json_with_cookie(
        StatusCode::OK,
        VerifyResponse {
            user: UserView::from(&user),
            csrf_token,
        },
        cookie,
    ))
}

/// `POST /v1/auth/logout`
pub async fn logout(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
) -> Result<impl IntoResponse, ApiError> {
    state
        .store
        .revoke_session(auth.session.id, Utc::now())
        .await?;
    state
        .store
        .insert_audit(NewAuditEvent {
            actor_address: Some(auth.user.wallet_address.clone()),
            action: "auth.logout".to_string(),
            object_type: "session".to_string(),
            object_id: Some(auth.session.id.to_string()),
            request_id: Some(ctx.request_id),
            ip_address: ctx.client_ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::Value::Null,
        })
        .await?;

    Ok(json_with_cookie(
        StatusCode::OK,
        LogoutResponse { logged_out: true },
        auth::clear_session_cookie(state.config.cookie_secure),
    ))
}

/// `GET /v1/me`
pub async fn me(State(_state): State<AppState>, auth: Auth) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::OK,
        Json(MeResponse {
            user: UserView::from(&auth.user),
            csrf_token: auth.session.csrf_token.clone(),
            cluster: auth.session.cluster.clone(),
        }),
    ))
}
