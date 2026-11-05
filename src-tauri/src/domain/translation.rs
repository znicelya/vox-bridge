use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    pub text: String,
    pub language: String,
}

/// Provider-relative timing, without changing the existing text-only contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedTranslation {
    pub translation: Translation,
    pub start: Option<Duration>,
    pub end: Option<Duration>,
}
