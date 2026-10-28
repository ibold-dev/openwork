//! Which intents adapter a deployment exposes.
//!
//! The registry is deliberately conservative: the simulated adapter is only
//! reachable when the process is configured in mock mode, and the real adapter
//! is only reachable in mainnet mode with an API key. There is no runtime
//! fallback between them, because a silent fallback would mean a mainnet user
//! quietly receives a simulated deposit address.

use std::sync::Arc;

use anyhow::Context;
use openwork_aurora::{
    AuroraError, MockIntentAdapter, OneClickAdapter, OneClickConfig, RoutingMode, RoutingProvider,
};

use crate::config::Config;

#[derive(Clone)]
pub struct AdapterRegistry {
    configured: RoutingMode,
    mock: Option<Arc<dyn RoutingProvider>>,
    oneclick: Option<Arc<dyn RoutingProvider>>,
}

impl std::fmt::Debug for AdapterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterRegistry")
            .field("configured", &self.configured)
            .field("mock", &self.mock.is_some())
            .field("oneclick", &self.oneclick.is_some())
            .finish()
    }
}

impl AdapterRegistry {
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let configured = RoutingMode::from_config(&config.intents_mode)
            .map_err(|err| anyhow::anyhow!("{err}"))?;

        let mock = if configured == RoutingMode::Mock {
            Some(
                Arc::new(MockIntentAdapter::with_ttl(config.mock_quote_ttl_seconds))
                    as Arc<dyn RoutingProvider>,
            )
        } else {
            None
        };

        let oneclick = if configured == RoutingMode::OneClick {
            let adapter = OneClickAdapter::new(OneClickConfig {
                base_url: config
                    .oneclick_base_url
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| openwork_aurora::oneclick::DEFAULT_BASE_URL.to_string()),
                api_key: config.oneclick_api_key.clone().unwrap_or_default(),
                mode: RoutingMode::OneClick,
                dry: config.oneclick_dry,
                timeout: config.rpc_timeout.max(std::time::Duration::from_secs(5)),
            })
            .context("could not construct the 1Click adapter")?;
            Some(Arc::new(adapter) as Arc<dyn RoutingProvider>)
        } else {
            None
        };

        Ok(AdapterRegistry {
            configured,
            mock,
            oneclick,
        })
    }

    pub fn default_mode(&self) -> RoutingMode {
        self.configured
    }

    /// Resolves a caller-requested mode, rejecting modes this deployment does
    /// not expose.
    pub fn resolve(&self, requested: Option<&str>) -> Result<RoutingMode, AuroraError> {
        let mode = match requested.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => RoutingMode::from_config(value)?,
            None => self.configured,
        };
        if self.provider(mode).is_err() {
            return Err(AuroraError::Disabled(format!(
                "routing mode `{}` is not enabled on this deployment",
                mode.as_str()
            )));
        }
        Ok(mode)
    }

    pub fn provider(&self, mode: RoutingMode) -> Result<&Arc<dyn RoutingProvider>, AuroraError> {
        match mode {
            RoutingMode::Mock => self.mock.as_ref().ok_or_else(|| {
                AuroraError::Disabled("the simulated router is disabled".to_string())
            }),
            RoutingMode::OneClick => self.oneclick.as_ref().ok_or_else(|| {
                AuroraError::Disabled(
                    "1Click is disabled; set INTENTS_MODE=mainnet and ONECLICK_API_KEY".to_string(),
                )
            }),
        }
    }

    /// Resolves a mode back from its persisted string.
    pub fn provider_for_mode_str(
        &self,
        mode: &str,
    ) -> Result<&Arc<dyn RoutingProvider>, AuroraError> {
        self.provider(RoutingMode::from_config(mode)?)
    }
}
