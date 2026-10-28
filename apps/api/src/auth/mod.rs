//! Wallet authentication, session extraction, CSRF and Origin enforcement.

pub mod session;
pub mod siws;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};
use chrono::{Duration, Utc};

use crate::ctx::{header_string, CSRF_HEADER};
use crate::error::ApiError;
use crate::store::{Session, User};
use crate::AppState;

pub use session::{
    clear_session_cookie, generate_token, hash_token, session_cookie, SESSION_COOKIE,
};
pub use siws::{build_message, new_nonce, STATEMENT};

/// An authenticated session. Extraction enforces Origin and CSRF for unsafe
/// methods, so every mutating handler is protected by construction.
#[derive(Debug, Clone)]
pub struct Auth {
    pub user: User,
    pub session: Session,
}

impl Auth {
    pub fn wallet(&self) -> &str {
        &self.user.wallet_address
    }

    pub fn cluster(&self) -> &str {
        &self.session.cluster
    }

    pub fn is_party(&self, client: &str, freelancer: &str) -> bool {
        self.wallet() == client || self.wallet() == freelancer
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = session::token_from_headers(&parts.headers)
            .ok_or_else(|| ApiError::unauthorized("no session cookie"))?;
        let token_hash = session::hash_token(&token);

        let (session, user) = state
            .store
            .find_session(&token_hash)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid session"))?;

        let now = Utc::now();
        if session.revoked_at.is_some() {
            return Err(ApiError::unauthorized("session was revoked"));
        }
        if session.expires_at <= now {
            return Err(ApiError::unauthorized("session expired"));
        }

        authorize_request(parts, state, &session)?;

        // Touch at most every five minutes: sessions are long lived and we do
        // not want a write per request.
        if now - session.last_seen_at > Duration::minutes(5) {
            state.store.touch_session(session.id, now).await?;
        }

        Ok(Auth { user, session })
    }
}

fn is_unsafe_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn authorize_request(parts: &Parts, state: &AppState, session: &Session) -> Result<(), ApiError> {
    let origin = header_string(&parts.headers, "origin");
    match origin {
        Some(origin) => {
            if !state.config.allowed_origins.contains(&origin) {
                return Err(ApiError::forbidden(
                    "origin_not_allowed",
                    "request origin is not allowed",
                ));
            }
        }
        None => {
            if is_unsafe_method(&parts.method) {
                return Err(ApiError::forbidden(
                    "origin_required",
                    "Origin header is required for state-changing requests",
                ));
            }
        }
    }

    if is_unsafe_method(&parts.method) {
        let csrf = header_string(&parts.headers, CSRF_HEADER).ok_or_else(|| {
            ApiError::forbidden(
                "csrf_missing",
                "x-openwork-csrf header is required for state-changing requests",
            )
        })?;
        if !siws::constant_time_eq(&csrf, &session.csrf_token) {
            return Err(ApiError::forbidden(
                "csrf_invalid",
                "x-openwork-csrf header does not match the session",
            ));
        }
    }

    Ok(())
}

/// Origin check for unauthenticated mutation endpoints (there are none today,
/// but login should also never be cross-site triggered).
pub fn require_allowed_origin(
    headers: &HeaderMap,
    allowed: &[String],
    method: &Method,
) -> Result<(), ApiError> {
    if !is_unsafe_method(method) {
        return Ok(());
    }
    let origin = headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError::forbidden(
                "origin_required",
                "Origin header is required for this request",
            )
        })?;
    if allowed.iter().any(|allowed| allowed == origin) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "origin_not_allowed",
            "request origin is not allowed",
        ))
    }
}
