use serde::{Deserialize, Deserializer, Serialize};

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LanguagePair {
    source: String,
    target: String,
}

impl<'de> Deserialize<'de> for LanguagePair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawLanguagePair {
            source: String,
            target: String,
        }
        let raw = RawLanguagePair::deserialize(deserializer)?;
        Self::new(raw.source, raw.target).map_err(serde::de::Error::custom)
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionConfig {
    mode: SessionMode,
    direction: SessionDirection,
    languages: LanguagePair,
}

impl<'de> Deserialize<'de> for SessionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawSessionConfig {
            mode: SessionMode,
            direction: SessionDirection,
            languages: LanguagePair,
        }
        let raw = RawSessionConfig::deserialize(deserializer)?;
        match raw.mode {
            SessionMode::Live => Self::live(raw.languages, raw.direction),
            SessionMode::Call if raw.direction == SessionDirection::Bidirectional => {
                Self::call(raw.languages)
            }
            SessionMode::Call => Err(AppError::InvalidRequest(
                "call sessions require bidirectional direction".to_string(),
            )),
        }
        .map_err(serde::de::Error::custom)
    }
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

    pub fn mode(&self) -> SessionMode {
        self.mode
    }

    pub fn direction(&self) -> SessionDirection {
        self.direction
    }

    pub fn languages(&self) -> &LanguagePair {
        &self.languages
    }
}
