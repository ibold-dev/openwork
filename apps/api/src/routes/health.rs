//! Liveness and readiness.
//!
//! Liveness answers "is this process alive" and never touches the network.
//! Readiness answers "can this process serve traffic" and checks both the
//! database and the Solana RPC endpoint, so a rolling deploy does not route
//! users to an instance that cannot reconcile.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::error::not_ready;
use crate::routes::dto::{LiveResponse, ReadyResponse};
use crate::AppState;

/// `GET /health/live`
pub async fn live() -> impl IntoResponse {
    (StatusCode::OK, Json(LiveResponse { status: "ok" }))
}

/// `GET /health/ready`
pub async fn ready(State(state): State<AppState>) -> Response {
    let mut reasons = Vec::new();

    let database = match state.store.ping().await {
        Ok(()) => "ok",
        Err(err) => {
            tracing::warn!(error = %err, "readiness: database unavailable");
            reasons.push(format!("database: {err}"));
            "unavailable"
        }
    };

    let (rpc, slot) = match state.rpc.slot().await {
        Ok(slot) => ("ok", Some(slot)),
        Err(err) => {
            tracing::warn!(error = %err, "readiness: rpc unavailable");
            reasons.push(format!("rpc: {err}"));
            ("unavailable", None)
        }
    };

    if !reasons.is_empty() {
        return not_ready(reasons).into_response();
    }

    (
        StatusCode::OK,
        Json(ReadyResponse {
            status: "ready",
            database,
            rpc,
            slot,
        }),
    )
        .into_response()
}
