//! HTTP API binary.

use std::sync::Arc;

use anyhow::Context;

use openwork_api::{
    connect_database, init_tracing, serve, AppState, Config, PgStore, SolanaRpc, Store,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env().context("invalid configuration")?;
    let pool = connect_database().await?;
    let store: Arc<dyn Store> = Arc::new(PgStore::new(pool));
    let rpc = Arc::new(
        SolanaRpc::new(&config.solana_rpc_url, config.rpc_timeout).context("invalid Solana RPC")?,
    );
    let state = AppState::new(config, store, rpc)?;

    serve(state).await
}
