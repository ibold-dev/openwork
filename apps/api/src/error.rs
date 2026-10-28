//! Structured, RFC 7807-ish errors.
//!
//! Every failure returns:
//!
//! ```json
//! { "error": { "code": "forbidden", "message": "...", "requestId": "..." } }
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};

use openwork_aurora::AuroraError;
use openwork_domain::DomainError;

use crate::ctx::current_request_id;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    pub fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            message,
        )
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::CONFLICT, code, message)
    }

    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    pub fn service_unavailable(code: &'static str, message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(
                code = self.code,
                message = %self.message,
                request_id = %current_request_id(),
                "request failed"
            );
        }
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code.to_string(),
                message: self.message,
                request_id: current_request_id(),
                details: self.details,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            err.to_string(),
        )
    }
}

impl From<AuroraError> for ApiError {
    fn from(err: AuroraError) -> Self {
        match &err {
            AuroraError::Disabled(reason) => ApiError::service_unavailable(
                "intents_unavailable",
                format!("intents routing is not available: {reason}"),
            ),
            AuroraError::InvalidRequest(reason) => ApiError::validation(reason.clone()),
            AuroraError::NotFound(id) => ApiError::not_found(format!("unknown quote {id}")),
            AuroraError::Provider(reason) => ApiError::service_unavailable(
                "provider_error",
                format!("routing provider failed: {reason}"),
            ),
        }
    }
}

/// Store failures. Internal driver errors are never leaked to clients.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::NotFound => ApiError::not_found("resource not found"),
            StoreError::Conflict(message) => ApiError::conflict("conflict", message),
            StoreError::Database(source) => {
                tracing::error!(error = %source, "database failure");
                ApiError::internal("internal error")
            }
            StoreError::Internal(source) => {
                tracing::error!(error = %source, "internal failure");
                ApiError::internal("internal error")
            }
        }
    }
}

/// JSON body extractor that produces our error envelope on rejection.
pub struct ApiJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(ApiJson(value)),
            Err(rejection) => {
                let mut error =
                    ApiError::new(rejection.status(), "invalid_request", rejection.body_text());
                if error.status == StatusCode::UNSUPPORTED_MEDIA_TYPE {
                    error.code = "unsupported_media_type";
                }
                Err(error)
            }
        }
    }
}

/// Helper for readiness responses.
pub fn not_ready(reasons: Vec<String>) -> ApiError {
    ApiError::service_unavailable("not_ready", "service is not ready to accept traffic")
        .with_details(json!({ "reasons": reasons }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_are_unprocessable() {
        let error: ApiError = DomainError::InvalidAmount.into();
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error.code, "validation_error");
    }

    #[test]
    fn store_conflicts_are_409() {
        let error: ApiError = StoreError::Conflict("index".into()).into();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.code, "conflict");
    }

    #[test]
    fn disabled_intents_are_503() {
        let error: ApiError = AuroraError::Disabled("no key".into()).into();
        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code, "intents_unavailable");
    }
}
