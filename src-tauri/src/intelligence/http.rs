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
    pub(crate) fn from_store_bytes(value: Vec<u8>) -> AppResult<Self> {
        validate_secret(&value)?;
        String::from_utf8(value)
            .map(Self)
            .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Configuration))
    }
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
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JsonErrorPolicy {
    Generic,
    CompatibleLlmContextErrors,
}
pub struct JsonRequest {
    pub endpoint: url::Url,
    pub bearer: SecretString,
    pub body: serde_json::Value,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub retry_policy: HttpRetryPolicy,
    pub error_policy: JsonErrorPolicy,
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

pub struct ReqwestJsonHttp {
    client: reqwest::Client,
}
impl ReqwestJsonHttp {
    pub fn new() -> AppResult<Self> {
        Self::with_roots(rustls::RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        ))
    }
    fn with_roots(roots: rustls::RootCertStore) -> AppResult<Self> {
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| AppError::Configuration("HTTP TLS configuration failed".into()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let client = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| AppError::Configuration("HTTP client configuration failed".into()))?;
        Ok(Self { client })
    }
}
#[async_trait::async_trait]
impl JsonHttp for ReqwestJsonHttp {
    async fn post(
        &self,
        request: JsonRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<JsonResponse, IntelligenceErrorCode> {
        use IntelligenceErrorCode as E;
        use futures_util::StreamExt;
        let url = &request.endpoint;
        let local = match url.host() {
            Some(url::Host::Domain(name)) => name == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.octets() == [127, 0, 0, 1],
            Some(url::Host::Ipv6(ip)) => ip == std::net::Ipv6Addr::LOCALHOST,
            None => false,
        };
        if url.host().is_none()
            || !(url.scheme() == "https" || url.scheme() == "http" && local)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || request.timeout.is_zero()
            || request.timeout > Duration::from_secs(120)
            || request.max_response_bytes == 0
            || request.max_response_bytes > 8 * 1024 * 1024
        {
            return Err(E::Configuration);
        }
        let deadline = tokio::time::Instant::now() + request.timeout;
        let authorization = request.bearer.authorization_header()?;
        let operation = async {
            for attempt in 0..2 {
                let response = self
                    .client
                    .post(request.endpoint.clone())
                    .header(reqwest::header::AUTHORIZATION, authorization.clone())
                    .json(&request.body)
                    .timeout(request.timeout)
                    .send()
                    .await
                    .map_err(http_error)?;
                let status = response.status().as_u16();
                if attempt == 0
                    && request.retry_policy == HttpRetryPolicy::OnceTransient
                    && matches!(status, 429 | 503)
                {
                    let delay = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(retry_delay);
                    if let Some(delay) =
                        delay.filter(|delay| tokio::time::Instant::now() + *delay < deadline)
                    {
                        drop(response);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                }
                if !(200..300).contains(&status) {
                    let generic = match status {
                        401 | 403 => E::Authentication,
                        429 => E::RateLimited,
                        _ => E::Network,
                    };
                    if request.error_policy == JsonErrorPolicy::CompatibleLlmContextErrors
                        && !matches!(status, 401 | 403 | 429)
                    {
                        return Err(compatible_context_error(response, generic).await);
                    }
                    return Err(generic);
                }
                if response
                    .content_length()
                    .is_some_and(|n| n > request.max_response_bytes as u64)
                {
                    return Err(E::ResponseTooLarge);
                }
                let mut stream = response.bytes_stream();
                let mut body = Vec::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(http_error)?;
                    if chunk.len() > request.max_response_bytes - body.len() {
                        return Err(E::ResponseTooLarge);
                    }
                    body.extend_from_slice(&chunk);
                }
                serde_json::from_slice::<serde_json::Value>(&body)
                    .map_err(|_| E::InvalidResponse)?;
                return Ok(JsonResponse { status, body });
            }
            Err(E::Network)
        };
        tokio::select! {biased;
            _=cancelled(&mut cancel)=>Err(E::Cancelled),
            result=tokio::time::timeout_at(deadline,operation)=>result.unwrap_or(Err(E::Timeout)),
        }
    }
}
async fn compatible_context_error(
    response: reqwest::Response,
    generic: IntelligenceErrorCode,
) -> IntelligenceErrorCode {
    use futures_util::StreamExt;
    const LIMIT: usize = 64 * 1024;
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return generic;
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return generic;
        };
        if chunk.len() > LIMIT - body.len() {
            return generic;
        }
        body.extend_from_slice(&chunk);
    }
    #[derive(serde::Deserialize)]
    struct Envelope {
        error: ErrorCode,
    }
    #[derive(serde::Deserialize)]
    struct ErrorCode {
        code: ContextCode,
    }
    struct ContextCode(bool);
    impl<'de> serde::Deserialize<'de> for ContextCode {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Code;
            impl serde::de::Visitor<'_> for Code {
                type Value = ContextCode;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("compatible error code")
                }
                fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<ContextCode, E> {
                    Ok(ContextCode(matches!(
                        raw,
                        "context_length_exceeded" | "context_window_exceeded"
                    )))
                }
            }
            d.deserialize_str(Code)
        }
    }
    if serde_json::from_slice::<Envelope>(&body).is_ok_and(|e| e.error.code.0) {
        IntelligenceErrorCode::ContextTooLarge
    } else {
        generic
    }
}
fn retry_delay(value: &str) -> Option<Duration> {
    let delay = if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        Duration::from_secs(value.parse().ok()?)
    } else {
        httpdate::parse_http_date(value)
            .ok()?
            .duration_since(std::time::SystemTime::now())
            .unwrap_or_default()
    };
    (delay <= Duration::from_secs(5)).then_some(delay)
}
fn http_error(error: reqwest::Error) -> IntelligenceErrorCode {
    if error.is_timeout() {
        IntelligenceErrorCode::Timeout
    } else {
        IntelligenceErrorCode::Network
    }
}
pub(crate) async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow_and_update() {
            return;
        }
        if cancel.changed().await.is_err() {
            return;
        }
    }
}
#[cfg(test)]
#[path = "http_tests.rs"]
mod transport_tests;
