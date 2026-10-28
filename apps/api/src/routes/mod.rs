//! HTTP surface. All routes live under `/v1`.

pub mod auth;
pub mod contracts;
pub mod dto;
pub mod health;
pub mod milestones;
pub mod openapi;
pub mod quotes;
pub mod settlements;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Attaches a `Set-Cookie` header to a JSON response.
pub(crate) fn json_with_cookie<T: Serialize>(
    status: StatusCode,
    body: T,
    cookie: String,
) -> Response {
    let mut response = (status, Json(body)).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

/// Returns the `Idempotency-Key` header if present and sane.
pub(crate) fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 255)
        .map(ToString::to_string)
}
