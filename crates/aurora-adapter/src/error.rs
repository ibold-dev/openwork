use thiserror::Error;

/// Failures from a routing provider.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuroraError {
    /// The adapter refuses to run in this configuration (missing credentials,
    /// wrong mode). This is a configuration error, never a transient one.
    #[error("adapter disabled: {0}")]
    Disabled(String),
    #[error("invalid quote request: {0}")]
    InvalidRequest(String),
    #[error("unknown quote: {0}")]
    NotFound(String),
    #[error("provider error: {0}")]
    Provider(String),
}

impl AuroraError {
    /// Stable machine code for API error responses.
    pub fn code(&self) -> &'static str {
        match self {
            AuroraError::Disabled(_) => "intents_unavailable",
            AuroraError::InvalidRequest(_) => "validation_error",
            AuroraError::NotFound(_) => "not_found",
            AuroraError::Provider(_) => "provider_error",
        }
    }
}
