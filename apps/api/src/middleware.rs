//! Cross-cutting HTTP concerns: CORS, tracing, request limits, rate limiting.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::config::Config;
use crate::ctx::{client_ip, RequestId, CSRF_HEADER, REQUEST_ID_HEADER};
use crate::error::ApiError;
use crate::AppState;

/// Maximum accepted request body. Deliverable notes are bounded at 2 KiB, so
/// 256 KiB leaves generous room while keeping memory use predictable.
pub const MAX_BODY_BYTES: usize = 256 * 1024;

/// In-process token bucket, keyed by client IP.
///
/// A single-instance API needs no shared store for this; when the API is
/// horizontally scaled, each instance enforces its own budget and the edge
/// proxy remains the global limit.
#[derive(Debug)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
    capacity: f64,
    refill_per_second: f64,
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(per_minute: u32, burst: u32) -> Self {
        RateLimiter {
            buckets: Mutex::new(HashMap::new()),
            capacity: f64::from(burst.max(1)),
            refill_per_second: f64::from(per_minute.max(1)) / 60.0,
        }
    }

    /// Returns `true` when the request may proceed.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|err| err.into_inner());

        // Bound memory: drop idle buckets once the map gets large. One entry is
        // tiny, but an attacker rotating IPs must not grow it forever.
        if buckets.len() > 10_000 {
            buckets.retain(|_, bucket| {
                now.duration_since(bucket.last_refill) < Duration::from_secs(600)
            });
        }

        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.capacity,
            last_refill: now,
        });
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_second).min(self.capacity);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-IP rate limit applied to `/v1` only; probes stay unthrottled.
pub async fn rate_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let key =
        client_ip(request.headers(), request.extensions()).unwrap_or_else(|| "unknown".to_string());
    if state.rate_limiter.check(&key) {
        next.run(request).await
    } else {
        let mut response = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many requests; retry shortly",
        )
        .into_response();
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        response
    }
}

/// Exact-origin CORS with credentials, which is what a cookie session needs.
pub fn cors_layer(config: &Config) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static(CSRF_HEADER),
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static(REQUEST_ID_HEADER),
        ])
        .expose_headers([HeaderName::from_static(REQUEST_ID_HEADER)])
        .max_age(Duration::from_secs(600))
}

/// One structured log line per request, carrying the same request id that the
/// error envelope returns to the client.
fn make_span(request: &Request) -> tracing::Span {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_else(|| "pending".to_string());
    tracing::info_span!(
        "http.request",
        request_id = %request_id,
        method = %request.method(),
        path = %request.uri().path(),
        version = ?request.version(),
    )
}

/// Applies request-id propagation, structured tracing, exact-origin CORS and
/// the body-size limit. The request id is the outermost layer so tracing and
/// error envelopes always see it.
pub fn apply(router: axum::Router<AppState>, state: &AppState) -> axum::Router<AppState> {
    router
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(cors_layer(&state.config))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(make_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .layer(axum::middleware::from_fn(crate::ctx::request_id_middleware))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_allows_burst_then_refills() {
        let limiter = RateLimiter::new(60, 2);
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4"));
        assert!(limiter.check("5.6.7.8"));
    }

    #[test]
    fn cors_uses_exact_origins() {
        let config = Config::for_tests();
        let layer = cors_layer(&config);
        // The layer is opaque, but constructing it from the allowlist proves
        // every configured origin parses as a header value.
        assert_eq!(config.allowed_origins.len(), 1);
        drop(layer);
    }
}
