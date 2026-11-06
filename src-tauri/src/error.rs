//! Errors shared by application services and command handlers.

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("session not found: {0}")]
    SessionNotFound(uuid::Uuid),
    #[error("audio error ({code:?}): {message}")]
    Audio {
        code: crate::domain::transcript::RuntimeFailureCode,
        message: &'static str,
    },
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "INVALID_REQUEST",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::Configuration(_) => "CONFIGURATION_ERROR",
            Self::SessionNotFound(_) => "SESSION_NOT_FOUND",
            Self::Audio { .. } => "AUDIO_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}
