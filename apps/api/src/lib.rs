//! OpenWork API: wallet-authenticated escrow metadata and reconciliation.
//!
//! The HTTP layer never signs transactions and never custodies keys. It records
//! what users intend to do, stores the signatures they claim, and exposes the
//! chain truth that the [`worker`] confirms independently.

pub mod auth;
pub mod config;
pub mod ctx;
pub mod error;
pub mod intents;
pub mod middleware;
pub mod routes;
pub mod solana;
pub mod store;
pub mod worker;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::routing::{get, post};
use axum::Router;
use solana_pubkey::Pubkey;
use sqlx::PgPool;
use tracing_subscriber::EnvFilter;

pub use config::Config;
pub use error::ApiError;
pub use intents::AdapterRegistry;
pub use middleware::RateLimiter;
pub use solana::SolanaRpc;
pub use store::{MemoryStore, PgStore, Store};

use openwork_domain::pda::parse_pubkey;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: Arc<dyn Store>,
    pub rpc: Arc<SolanaRpc>,
    pub adapters: Arc<AdapterRegistry>,
    pub rate_limiter: Arc<RateLimiter>,
    pub program_id: Pubkey,
}

impl AppState {
    pub fn new(config: Config, store: Arc<dyn Store>, rpc: Arc<SolanaRpc>) -> anyhow::Result<Self> {
        let adapters = Arc::new(AdapterRegistry::from_config(&config)?);
        let program_id = parse_pubkey(&config.program_id)
            .map_err(|err| anyhow::anyhow!("PROGRAM_ID is invalid: {err}"))?;
        let rate_limiter = Arc::new(RateLimiter::new(
            config.rate_limit_per_minute,
            config.rate_limit_burst,
        ));
        Ok(AppState {
            config: Arc::new(config),
            store,
            rpc,
            adapters,
            rate_limiter,
            program_id,
        })
    }
}

async fn not_found() -> ApiError {
    ApiError::not_found("route not found")
}

/// Builds the full router, including `/health/*` and `/openapi.yaml`.
pub fn build_app(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/auth/challenge", post(routes::auth::challenge))
        .route("/auth/verify", post(routes::auth::verify))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/me", get(routes::auth::me))
        .route(
            "/contracts",
            get(routes::contracts::list_contracts).post(routes::contracts::create_contract),
        )
        .route("/contracts/{id}", get(routes::contracts::get_contract))
        .route(
            "/contracts/{id}/milestones",
            post(routes::contracts::add_milestone),
        )
        .route(
            "/contracts/{id}/accept-intent",
            post(routes::contracts::accept_intent),
        )
        .route(
            "/milestones/{id}/submit",
            post(routes::milestones::submit_milestone),
        )
        .route(
            "/milestones/{id}/transactions",
            get(routes::milestones::list_transactions),
        )
        .route("/settlements", post(routes::settlements::create_settlement))
        .route("/quotes", post(routes::quotes::create_quote))
        .route("/quotes/{id}", get(routes::quotes::get_quote))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit,
        ));

    let router = Router::new()
        .route("/health/live", get(routes::health::live))
        .route("/health/ready", get(routes::health::ready))
        .route("/openapi.yaml", get(routes::openapi::spec))
        .nest("/v1", v1)
        .fallback(not_found);

    middleware::apply(router, &state).with_state(state)
}

/// Connects to PostgreSQL and applies the embedded migrations.
pub async fn connect_database() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set (see apps/api/.env.example)")?;
    let pool = store::pg::connect(&url, 10)
        .await
        .context("could not connect to PostgreSQL")?;
    store::pg::migrate(&pool)
        .await
        .context("could not apply database migrations")?;
    Ok(pool)
}

/// JSON logs with request ids. Safe to call more than once.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(true)
        .with_span_list(false)
        .try_init();
}

/// Binds the configured address and serves until `Ctrl+C`.
pub async fn serve(state: AppState) -> anyhow::Result<()> {
    let bind = state.config.bind;
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("could not bind {bind}"))?;
    tracing::info!(%bind, "openwork api listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("http server failed")?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %err, "could not listen for shutdown signal");
    }
}
