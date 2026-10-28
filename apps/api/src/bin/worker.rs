//! Reconciliation worker binary.

use std::sync::Arc;

use anyhow::Context;

use openwork_api::{connect_database, init_tracing, worker, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Arc::new(Config::from_env().context("invalid configuration")?);
    let pool = connect_database().await?;
    worker::run(config, pool).await
}
