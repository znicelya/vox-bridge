use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Live,
    Call,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionDirection {
    SourceToTarget,
    TargetToSource,
    Bidirectional,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguagePair {
    source: String,
    target: String,
}

impl LanguagePair {
    pub fn new(source: impl AsRef<str>, target: impl AsRef<str>) -> AppResult<Self> {
        fn language_code(value: &str) -> AppResult<String> {
            let code = value.trim();
            if code.len() != 2 || !code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
                return Err(AppError::InvalidRequest(format!(
                    "language code must be two ASCII letters: {code}"
                )));
            }
            Ok(code.to_ascii_lowercase())
        }

        Ok(Self {
            source: language_code(source.as_ref())?,
            target: language_code(target.as_ref())?,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> AppResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AppError::InvalidRequest(
                "provider ID must not be empty".to_string(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionConfig {
    pub mode: SessionMode,
    pub direction: SessionDirection,
    pub languages: LanguagePair,
}

impl SessionConfig {
    pub fn live(languages: LanguagePair, direction: SessionDirection) -> AppResult<Self> {
        if direction == SessionDirection::Bidirectional {
            return Err(AppError::InvalidRequest(
                "live sessions require a single direction".to_string(),
            ));
        }
        Ok(Self {
            mode: SessionMode::Live,
            direction,
            languages,
        })
    }

    pub fn call(languages: LanguagePair) -> AppResult<Self> {
        Ok(Self {
            mode: SessionMode::Call,
            direction: SessionDirection::Bidirectional,
            languages,
        })
    }
}
