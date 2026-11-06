//! Opaque transport boundary. Concrete bounded HTTP execution is added by Task 3B.
use super::types::IntelligenceErrorCode;
use crate::error::{AppError, AppResult};
use std::time::Duration;
use tokio::sync::watch;

pub struct SecretString(String);
pub(crate) fn validate_secret(value: &[u8]) -> AppResult<()> {
    if !(1..=4096).contains(&value.len()) || !value.iter().all(u8::is_ascii_graphic) {
        return Err(AppError::Intelligence(IntelligenceErrorCode::Configuration));
    }
    Ok(())
}
impl SecretString {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for Task 3B service and scheduler composition"
        )
    )]
    pub(crate) fn from_store_bytes(value: Vec<u8>) -> AppResult<Self> {
        validate_secret(&value)?;
        String::from_utf8(value)
            .map(Self)
            .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Configuration))
    }
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Concrete HTTP adapter is composed in Task 3B")
    )]
    fn authorization_header(&self) -> Result<reqwest::header::HeaderValue, IntelligenceErrorCode> {
        let mut header = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", self.0))
            .map_err(|_| IntelligenceErrorCode::Configuration)?;
        header.set_sensitive(true);
        Ok(header)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HttpRetryPolicy {
    Never,
    OnceTransient,
}
pub struct JsonRequest {
    pub endpoint: url::Url,
    pub bearer: SecretString,
    pub body: serde_json::Value,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub retry_policy: HttpRetryPolicy,
}
pub struct JsonResponse {
    pub status: u16,
    pub body: Vec<u8>,
}
#[async_trait::async_trait]
pub trait JsonHttp: Send + Sync {
    async fn post(
        &self,
        request: JsonRequest,
        cancel: watch::Receiver<bool>,
    ) -> Result<JsonResponse, IntelligenceErrorCode>;
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CredentialChange;
    use crate::intelligence::{test_support::*, types::SaveModelProfile};
    #[tokio::test]
    async fn resolved_credentials_are_isolated_sensitive_headers() {
        let h = IntelligenceHarness::new();
        let first = test_llm_profile();
        let second = test_llm_profile();
        for (profile, key) in [
            (first.clone(), "synthetic-first"),
            (second.clone(), "synthetic-second"),
        ] {
            h.config
                .save(SaveModelProfile {
                    profile,
                    credential: CredentialChange::Replace {
                        api_key: key.into(),
                    },
                })
                .await
                .unwrap();
        }
        for (id, expected) in [
            (first.id, "Bearer synthetic-first"),
            (second.id, "Bearer synthetic-second"),
        ] {
            let secret = h.config.resolve_credentials(id).await.unwrap();
            let header = secret.authorization_header().unwrap();
            assert!(header.is_sensitive());
            assert_eq!(header.to_str().unwrap(), expected);
        }
    }
    #[test]
    fn bearer_rejects_empty_whitespace_controls_non_ascii_and_oversize() {
        for value in [
            vec![],
            b"has space".to_vec(),
            b"tab\t".to_vec(),
            b"line\r\n".to_vec(),
            vec![127],
            vec![255],
            vec![b'a'; 4097],
        ] {
            assert!(SecretString::from_store_bytes(value).is_err());
        }
        assert!(SecretString::from_store_bytes(vec![b'!'; 4096]).is_ok());
    }
}
