//! Compatible completion owns its request slot through actual resolver release.
use super::{LlmExecution, LlmProvider, LlmRequest, LlmResponse, validate_request_budget};
use crate::intelligence::{
    config::{ServiceConfigStore, endpoint},
    http::{HttpRetryPolicy, JsonErrorPolicy, JsonHttp, JsonRequest, cancelled},
    types::{IntelligenceErrorCode as Code, ModelProfile},
};
use serde::{
    Deserialize,
    de::{self, SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::{self, Write},
    sync::Arc,
    time::Duration,
};
pub(crate) struct OpenAiCompatibleLlm {
    config: Arc<ServiceConfigStore>,
    http: Arc<dyn JsonHttp>,
    requests: Arc<tokio::sync::Semaphore>,
}
impl OpenAiCompatibleLlm {
    pub(crate) fn new(config: Arc<ServiceConfigStore>, http: Arc<dyn JsonHttp>) -> Self {
        Self {
            config,
            http,
            requests: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}
struct RequestScope {
    _owner: crate::intelligence::jobs::OperationLease,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
struct RequestCount(usize);
impl Write for RequestCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 1024 * 1024)
            .ok_or_else(|| io::Error::other("request size limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Deserialize)]
struct Envelope {
    choices: FirstChoice,
}
struct FirstChoice(Choice);
impl<'de> Deserialize<'de> for FirstChoice {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct First;
        impl<'de> Visitor<'de> for First {
            type Value = FirstChoice;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("nonempty choices")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let first = a
                    .next_element()?
                    .ok_or_else(|| de::Error::custom("missing choice"))?;
                while a.next_element::<de::IgnoredAny>()?.is_some() {}
                Ok(FirstChoice(first))
            }
        }
        d.deserialize_seq(First)
    }
}
#[derive(Deserialize)]
struct Choice {
    message: Message,
}
#[derive(Deserialize)]
struct Message {
    content: Content,
    #[serde(default)]
    refusal: Option<de::IgnoredAny>,
}
struct Content(Option<String>);
impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Text;
        impl Visitor<'_> for Text {
            type Value = Content;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded completion string")
            }
            fn visit_str<E: de::Error>(self, raw: &str) -> Result<Content, E> {
                if raw.len() > 2 * 1024 * 1024 {
                    return Ok(Content(None));
                }
                Ok(Content(Some(raw.to_owned())))
            }
        }
        d.deserialize_str(Text)
    }
}
fn parse(body: &[u8]) -> Result<LlmResponse, Code> {
    if body.len() > 8 * 1024 * 1024 {
        return Err(Code::ResponseTooLarge);
    }
    let envelope: Envelope = serde_json::from_slice(body).map_err(|_| Code::InvalidResponse)?;
    let message = envelope.choices.0.message;
    if message.refusal.is_some() {
        return Err(Code::InvalidResponse);
    }
    let content = message.content.0.ok_or(Code::ResponseTooLarge)?;
    let response_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    Ok(LlmResponse {
        content,
        response_hash,
    })
}
#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatibleLlm {
    async fn complete(
        &self,
        profile: &ModelProfile,
        request: LlmRequest,
        execution: LlmExecution,
    ) -> Result<LlmResponse, Code> {
        validate_request_budget(profile, &request)?;
        let body = serde_json::json!({"model":profile.model.as_ref().ok_or(Code::Configuration)?,"messages":[{"role":"system","content":request.system},{"role":"user","content":request.user}],"temperature":0,"max_tokens":request.max_output_tokens,"stream":false});
        serde_json::to_writer(&mut RequestCount(0), &body).map_err(|_| Code::ResponseTooLarge)?;
        let mut cancel = execution.cancel.clone();
        let permit = tokio::select! {biased;
            _=cancelled(&mut cancel)=>return Err(Code::Cancelled),
            result=tokio::time::timeout(Duration::from_millis(profile.timeout_ms as u64),self.requests.clone().acquire_owned())=>result.map_err(|_|Code::Timeout)?.map_err(|_|Code::Interrupted)?,
        };
        let scope: crate::intelligence::jobs::OperationLease = Arc::new(RequestScope {
            _owner: execution.owner,
            _permit: permit,
        });
        let config = self.config.scoped(scope.clone());
        let bearer = tokio::select! {biased;
            _=cancelled(&mut cancel)=>return Err(Code::Cancelled),
            result=config.resolve_snapshot_credentials(profile.clone())=>result.map_err(|_|Code::Configuration)?,
        };
        // Directly await the entered HTTP future under the actual worker owner.
        let _scope = scope;
        let response = self
            .http
            .post(
                JsonRequest {
                    endpoint: endpoint(profile).map_err(|_| Code::Configuration)?,
                    bearer,
                    body,
                    timeout: Duration::from_millis(profile.timeout_ms as u64),
                    max_response_bytes: 8 * 1024 * 1024,
                    retry_policy: HttpRetryPolicy::Never,
                    error_policy: JsonErrorPolicy::CompatibleLlmContextErrors,
                },
                cancel,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(match response.status {
                401 | 403 => Code::Authentication,
                429 => Code::RateLimited,
                _ => Code::Network,
            });
        }
        parse(&response.body)
    }
}
#[cfg(test)]
mod owned_tests;
