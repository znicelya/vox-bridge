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
    #[error("internal error: {0}")]
    Internal(String),
}
