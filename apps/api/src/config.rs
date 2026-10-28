//! Environment-driven configuration.
//!
//! Everything is validated at startup so a misconfigured process fails loudly
//! instead of serving with unsafe defaults. `.env.example` lists the variable
//! names; values are never logged.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use openwork_domain::limits::ALLOWED_CLUSTERS;

use crate::solana::rpc::DEFAULT_RPC_TIMEOUT_SECONDS;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// Browser origin that signs in, e.g. `https://app.openwork.xyz`.
    pub public_origin: String,
    /// SIWS domain, normally the host of `public_origin`.
    pub domain: String,
    /// Exact-match CORS and Origin allowlist.
    pub allowed_origins: Vec<String>,
    pub allowed_clusters: Vec<String>,
    pub default_cluster: String,
    pub program_id: String,
    pub solana_rpc_url: String,
    pub rpc_timeout: Duration,
    pub session_ttl_seconds: i64,
    pub challenge_ttl_seconds: i64,
    pub cookie_secure: bool,
    pub rate_limit_per_minute: u32,
    pub rate_limit_burst: u32,
    pub intents_mode: String,
    pub oneclick_api_key: Option<String>,
    pub oneclick_base_url: Option<String>,
    pub oneclick_dry: bool,
    pub mock_quote_ttl_seconds: u64,
    pub worker_poll_interval_seconds: u64,
    pub worker_batch_size: i64,
    pub sync_stale_seconds: i64,
}

impl Config {
    /// Reads and validates the process environment.
    pub fn from_env() -> Result<Self> {
        let config = Config {
            bind: env_or("API_BIND", "0.0.0.0:8080")
                .parse()
                .context("API_BIND must be a socket address such as 0.0.0.0:8080")?,
            public_origin: env_or("API_PUBLIC_ORIGIN", "http://localhost:3000"),
            domain: env_or("API_DOMAIN", "localhost"),
            allowed_origins: split_list(&env_or("CORS_ALLOWED_ORIGINS", "http://localhost:3000")),
            allowed_clusters: split_list(&env_or("ALLOWED_CLUSTERS", "devnet,localnet,testnet")),
            default_cluster: env_or("SOLANA_CLUSTER", "devnet"),
            program_id: env_or("PROGRAM_ID", "GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR"),
            solana_rpc_url: env_or("SOLANA_RPC_URL", "https://api.devnet.solana.com"),
            rpc_timeout: Duration::from_secs(env_parse(
                "SOLANA_RPC_TIMEOUT_SECONDS",
                DEFAULT_RPC_TIMEOUT_SECONDS,
            )?),
            session_ttl_seconds: env_parse("SESSION_TTL_SECONDS", 60 * 60 * 24 * 7)?,
            challenge_ttl_seconds: env_parse("CHALLENGE_TTL_SECONDS", 300)?,
            cookie_secure: env_bool("COOKIE_SECURE", true)?,
            rate_limit_per_minute: env_parse("RATE_LIMIT_PER_MINUTE", 120)?,
            rate_limit_burst: env_parse("RATE_LIMIT_BURST", 60)?,
            intents_mode: env_or("INTENTS_MODE", "mock"),
            oneclick_api_key: std::env::var("ONECLICK_API_KEY").ok(),
            oneclick_base_url: std::env::var("ONECLICK_BASE_URL").ok(),
            oneclick_dry: env_bool("ONECLICK_DRY", true)?,
            mock_quote_ttl_seconds: env_parse("MOCK_QUOTE_TTL_SECONDS", 120)?,
            worker_poll_interval_seconds: env_parse("WORKER_POLL_INTERVAL_SECONDS", 5)?,
            worker_batch_size: env_parse("WORKER_BATCH_SIZE", 10)?,
            sync_stale_seconds: env_parse("WORKER_SYNC_STALE_SECONDS", 30)?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.public_origin.trim().is_empty() {
            bail!("API_PUBLIC_ORIGIN must not be empty");
        }
        if !(self.public_origin.starts_with("https://")
            || self.public_origin.starts_with("http://"))
        {
            bail!("API_PUBLIC_ORIGIN must be an absolute http(s) origin");
        }
        if self.domain.trim().is_empty() {
            bail!("API_DOMAIN must not be empty");
        }
        if self.allowed_origins.is_empty() {
            bail!("CORS_ALLOWED_ORIGINS must list at least one origin");
        }
        if !self.allowed_origins.contains(&self.public_origin) {
            bail!("API_PUBLIC_ORIGIN must also appear in CORS_ALLOWED_ORIGINS");
        }
        if self.allowed_clusters.is_empty() {
            bail!("ALLOWED_CLUSTERS must list at least one cluster");
        }
        for cluster in &self.allowed_clusters {
            if !ALLOWED_CLUSTERS.contains(&cluster.as_str()) {
                bail!("unsupported cluster in ALLOWED_CLUSTERS: {cluster}");
            }
        }
        if !self.allowed_clusters.contains(&self.default_cluster) {
            bail!("SOLANA_CLUSTER must be one of ALLOWED_CLUSTERS");
        }
        if openwork_domain::pda::parse_pubkey(&self.program_id).is_err() {
            bail!("PROGRAM_ID must be a base58 Solana address");
        }
        if self.session_ttl_seconds <= 0 {
            bail!("SESSION_TTL_SECONDS must be positive");
        }
        if self.challenge_ttl_seconds <= 0 || self.challenge_ttl_seconds > 900 {
            bail!("CHALLENGE_TTL_SECONDS must be between 1 and 900");
        }
        if self.rate_limit_per_minute == 0 || self.rate_limit_burst == 0 {
            bail!("rate limits must be positive");
        }
        if self.worker_batch_size <= 0 || self.worker_batch_size > 100 {
            bail!("WORKER_BATCH_SIZE must be between 1 and 100");
        }
        match self.intents_mode.trim().to_ascii_lowercase().as_str() {
            "mock" | "simulated" | "local" => {}
            "mainnet" | "oneclick" | "near" => {
                if self
                    .oneclick_api_key
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
                {
                    bail!("INTENTS_MODE=mainnet requires ONECLICK_API_KEY");
                }
            }
            other => bail!("INTENTS_MODE must be mock or mainnet, got {other}"),
        }
        Ok(())
    }

    pub fn cluster_allowed(&self, cluster: &str) -> bool {
        self.allowed_clusters.iter().any(|value| value == cluster)
    }

    /// Configuration used by in-process tests. No database, no network.
    pub fn for_tests() -> Self {
        Config {
            bind: "127.0.0.1:0".parse().expect("bind"),
            public_origin: "https://app.openwork.test".to_string(),
            domain: "app.openwork.test".to_string(),
            allowed_origins: vec!["https://app.openwork.test".to_string()],
            allowed_clusters: vec!["devnet".to_string()],
            default_cluster: "devnet".to_string(),
            program_id: "GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR".to_string(),
            solana_rpc_url: "http://127.0.0.1:18899".to_string(),
            rpc_timeout: Duration::from_secs(2),
            session_ttl_seconds: 3600,
            challenge_ttl_seconds: 300,
            cookie_secure: false,
            rate_limit_per_minute: 100_000,
            rate_limit_burst: 100_000,
            intents_mode: "mock".to_string(),
            oneclick_api_key: None,
            oneclick_base_url: None,
            oneclick_dry: true,
            mock_quote_ttl_seconds: 120,
            worker_poll_interval_seconds: 5,
            worker_batch_size: 10,
            sync_stale_seconds: 30,
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_parse<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|err| anyhow::anyhow!("{key} is invalid: {err}")),
        Err(_) => Ok(default),
    }
}

fn env_bool(key: &str, default: bool) -> Result<bool> {
    match std::env::var(key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            other => bail!("{key} must be true or false, got {other}"),
        },
        Err(_) => Ok(default),
    }
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .map(|item| item.trim_end_matches('/'))
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_env<T>(vars: &[(&str, &str)], f: impl FnOnce() -> T) -> T {
        // Serialize env mutations through a mutex because tests run in parallel
        // within one process.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|err| err.into_inner());
        for (key, value) in vars {
            std::env::set_var(key, value);
        }
        let result = f();
        for (key, _) in vars {
            std::env::remove_var(key);
        }
        result
    }

    #[test]
    fn test_config_is_valid() {
        assert!(Config::for_tests().validate().is_ok());
    }

    #[test]
    fn rejects_mainnet_intents_without_key() {
        let mut config = Config::for_tests();
        config.intents_mode = "mainnet".into();
        config.oneclick_api_key = None;
        assert!(config.validate().is_err());

        config.oneclick_api_key = Some("   ".into());
        assert!(config.validate().is_err());

        config.oneclick_api_key = Some("key".into());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_origin_outside_allowlist() {
        let mut config = Config::for_tests();
        config.public_origin = "https://evil.test".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unknown_clusters() {
        let mut config = Config::for_tests();
        config.allowed_clusters = vec!["moon".into()];
        config.default_cluster = "moon".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn parses_env_lists_and_bools() {
        assert_eq!(
            split_list("https://a.test/, https://b.test"),
            vec!["https://a.test".to_string(), "https://b.test".to_string()]
        );
        with_env(&[("COOKIE_SECURE", "false")], || {
            assert!(!env_bool("COOKIE_SECURE", true).unwrap());
        });
        with_env(&[("COOKIE_SECURE", "maybe")], || {
            assert!(env_bool("COOKIE_SECURE", true).is_err());
        });
    }

    #[test]
    fn production_config_requires_matching_origins() {
        with_env(
            &[
                ("API_PUBLIC_ORIGIN", "https://app.openwork.xyz"),
                ("CORS_ALLOWED_ORIGINS", "https://app.openwork.xyz"),
                ("API_DOMAIN", "app.openwork.xyz"),
                ("COOKIE_SECURE", "true"),
                ("INTENTS_MODE", "mock"),
            ],
            || {
                let config = Config::from_env().expect("config");
                assert!(config.cookie_secure);
                assert_eq!(config.domain, "app.openwork.xyz");
            },
        );
    }
}
