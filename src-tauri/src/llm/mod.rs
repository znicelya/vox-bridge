use crate::intelligence::{
    jobs::OperationLease,
    types::{IntelligenceErrorCode, ModelProfile},
};
use tokio::sync::watch;
mod openai_compatible;
pub(crate) use openai_compatible::OpenAiCompatibleLlm;
#[cfg(test)]
mod tests;

pub struct LlmRequest {
    pub system: String,
    pub user: String,
    pub max_output_tokens: u32,
}
pub struct LlmResponse {
    pub content: String,
    pub response_hash: String,
}
pub struct LlmExecution {
    owner: OperationLease,
    cancel: watch::Receiver<bool>,
}
impl LlmExecution {
    pub(crate) fn new(owner: OperationLease, cancel: watch::Receiver<bool>) -> Self {
        Self { owner, cancel }
    }
}
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(
        &self,
        profile: &ModelProfile,
        request: LlmRequest,
        execution: LlmExecution,
    ) -> Result<LlmResponse, IntelligenceErrorCode>;
}

pub(crate) fn escaped_string_bytes(raw: &str) -> Option<usize> {
    raw.bytes().try_fold(2usize, |total, b| {
        total.checked_add(match b {
            b'"' | b'\\' | 8 | 12 | b'\n' | b'\r' | b'\t' => 2,
            0..=31 => 6,
            _ => 1,
        })
    })
}
pub(crate) fn validate_request_budget(
    profile: &ModelProfile,
    request: &LlmRequest,
) -> Result<(), IntelligenceErrorCode> {
    use IntelligenceErrorCode as Code;
    crate::intelligence::config::validate_profile(profile).map_err(|_| Code::Configuration)?;
    if profile.kind != crate::intelligence::types::ServiceKind::Llm
        || profile.max_output_tokens != Some(request.max_output_tokens)
    {
        return Err(Code::Configuration);
    }
    let bound = profile
        .max_context_tokens
        .and_then(|c| c.checked_sub(request.max_output_tokens))
        .ok_or(Code::Configuration)? as usize;
    let actual = escaped_string_bytes(&request.system)
        .and_then(|s| escaped_string_bytes(&request.user).and_then(|u| s.checked_add(u)))
        .and_then(|n| n.checked_add(1024))
        .ok_or(Code::ContextTooLarge)?;
    if actual > bound {
        return Err(Code::ContextTooLarge);
    }
    Ok(())
}
