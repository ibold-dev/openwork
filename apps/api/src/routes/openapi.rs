//! Serves the committed OpenAPI document.
//!
//! The document is embedded at compile time so the deployed binary can never
//! drift from the contract the SDK was generated against.

use axum::http::header;
use axum::response::IntoResponse;

const OPENAPI_YAML: &str = include_str!("../../openapi.yaml");

/// `GET /openapi.yaml`
pub async fn spec() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/yaml")], OPENAPI_YAML)
}
