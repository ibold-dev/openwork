//! Per-request context: request id, client ip, user agent.
//!
//! The request id lives in a task-local so that error constructors deep inside
//! handlers can stamp it without threading a parameter through every call.

use axum::extract::{ConnectInfo, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use std::net::SocketAddr;
use uuid::Uuid;

use crate::AppState;

pub const REQUEST_ID_HEADER: &str = "x-request-id";
pub const CSRF_HEADER: &str = "x-openwork-csrf";

tokio::task_local! {
    static REQUEST_ID: String;
}

pub fn current_request_id() -> String {
    REQUEST_ID
        .try_with(Clone::clone)
        .unwrap_or_else(|_| "unavailable".to_string())
}

#[derive(Debug, Clone)]
pub struct ReqCtx {
    pub request_id: String,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
}

impl FromRequestParts<AppState> for ReqCtx {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(ReqCtx {
            request_id: request_id_from_extensions(&parts.extensions),
            client_ip: client_ip(&parts.headers, &parts.extensions),
            user_agent: header_string(&parts.headers, "user-agent"),
        })
    }
}

pub fn request_id_from_extensions(extensions: &axum::http::Extensions) -> String {
    extensions
        .get::<RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Middleware that assigns a request id, exposes it to handlers, stamps it on
/// the response, and makes it available to the error envelope.
pub async fn request_id_middleware(
    mut request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_valid_request_id(value))
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    request.extensions_mut().insert(RequestId(id.clone()));

    let mut response = REQUEST_ID.scope(id.clone(), next.run(request)).await;
    if let Ok(value) = axum::http::HeaderValue::from_str(&id) {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(REQUEST_ID_HEADER),
            value,
        );
    }
    response
}

fn is_valid_request_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn client_ip(headers: &HeaderMap, extensions: &axum::http::Extensions) -> Option<String> {
    // Only trust proxy headers when the deployment terminates TLS in front of
    // the API; the direct peer address always wins when available.
    if let Some(ConnectInfo(addr)) = extensions.get::<ConnectInfo<SocketAddr>>() {
        return Some(addr.ip().to_string());
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(512).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_request_ids() {
        assert!(is_valid_request_id("12345678"));
        assert!(is_valid_request_id("abc-DEF_123"));
        assert!(!is_valid_request_id("short"));
        assert!(!is_valid_request_id("bad value here"));
        assert!(!is_valid_request_id(&"a".repeat(65)));
    }
}
