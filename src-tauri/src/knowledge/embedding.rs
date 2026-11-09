//! Each metered request retains its actual-completion owner.
use crate::{
    error::{AppError, AppResult},
    intelligence::{
        config::{ServiceConfigStore, endpoint, validate_profile},
        http::{HttpRetryPolicy, JsonErrorPolicy, JsonHttp, JsonRequest},
        jobs::OperationLease,
        types::{IntelligenceErrorCode as Code, ModelProfile, ServiceKind},
    },
};
use std::{
    io::{self, Write},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Semaphore, watch};
pub(crate) struct EmbeddingExecution {
    owner: OperationLease,
    cancel: watch::Receiver<bool>,
}
impl EmbeddingExecution {
    pub(crate) fn new(owner: OperationLease, cancel: watch::Receiver<bool>) -> Self {
        Self { owner, cancel }
    }
}
#[async_trait::async_trait]
pub(crate) trait Embeddings: Send + Sync {
    async fn embed(
        &self,
        profile: &ModelProfile,
        inputs: &[String],
        execution: EmbeddingExecution,
    ) -> AppResult<Vec<Vec<f32>>>;
}
pub(crate) struct OpenAiCompatibleEmbeddings {
    config: Arc<ServiceConfigStore>,
    http: Arc<dyn JsonHttp>,
    requests: Arc<Semaphore>,
}
impl OpenAiCompatibleEmbeddings {
    pub(crate) fn new(config: Arc<ServiceConfigStore>, http: Arc<dyn JsonHttp>) -> Self {
        Self {
            config,
            http,
            requests: Arc::new(Semaphore::new(4)),
        }
    }
}
pub(crate) fn error(code: Code) -> AppError {
    AppError::Intelligence(code)
}
#[derive(Default)]
pub(crate) struct JsonCount(pub(crate) usize);
impl Write for JsonCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("JSON budget overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn json_bytes(value: &impl serde::Serialize) -> AppResult<usize> {
    let mut c = JsonCount::default();
    serde_json::to_writer(&mut c, value).map_err(|_| error(Code::ResourceLimit))?;
    Ok(c.0)
}
pub(crate) fn request_body(
    profile: &ModelProfile,
    inputs: &[String],
) -> AppResult<serde_json::Value> {
    if inputs.is_empty() || inputs.len() > 32 {
        return Err(error(Code::ResourceLimit));
    }
    let mut body = serde_json::json!({
    "model":profile.model.as_ref().ok_or_else(||error(Code::Configuration))?,
    "input":inputs,
    "encoding_format":"float"}
    );
    if let Some(d) = profile.dimensions {
        body["dimensions"] = d.into();
    }
    if json_bytes(&body)? > 128 * 1024 {
        return Err(error(Code::ResourceLimit));
    }
    Ok(body)
}
#[async_trait::async_trait]
impl Embeddings for OpenAiCompatibleEmbeddings {
    async fn embed(
        &self,
        profile: &ModelProfile,
        inputs: &[String],
        execution: EmbeddingExecution,
    ) -> AppResult<Vec<Vec<f32>>> {
        validate_profile(profile)?;
        if profile.kind != ServiceKind::Embedding {
            return Err(error(Code::Configuration));
        }
        let body = request_body(profile, inputs)?;
        let mut cancel = execution.cancel.clone();
        let _permit = tokio::select! {
        biased;
                _=crate::intelligence::http::cancelled(&mut cancel)=>return Err(error(Code::Cancelled)),
        p=self.requests.clone().acquire_owned()=>p.map_err(|_|error(Code::Interrupted))?};
        let bearer = self
            .config
            .scoped(execution.owner.clone())
            .resolve_snapshot_credentials(profile.clone())
            .await
            .map_err(|_| error(Code::Configuration))?;
        let response = self
            .http
            .post(
                JsonRequest {
                    endpoint: endpoint(profile)?,
                    bearer,
                    body,
                    timeout: Duration::from_millis(u64::from(profile.timeout_ms)),
                    max_response_bytes: 8 * 1024 * 1024,
                    retry_policy: HttpRetryPolicy::OnceTransient,
                    error_policy: JsonErrorPolicy::Generic,
                },
                execution.cancel.clone(),
            )
            .await
            .map_err(error)?;
        if response.body.len() > 8 * 1024 * 1024 {
            return Err(error(Code::ResponseTooLarge));
        }
        if !(200..300).contains(&response.status) {
            return Err(error(match response.status {
                401 | 403 => Code::Authentication,
                429 => Code::RateLimited,
                _ => Code::Network,
            }));
        }
        let invalid = || error(Code::InvalidResponse);
        let value: serde_json::Value =
            serde_json::from_slice(&response.body).map_err(|_| invalid())?;
        let data = value["data"]
            .as_array()
            .filter(|d| d.len() == inputs.len())
            .ok_or_else(invalid)?;
        let mut vectors = vec![None; inputs.len()];
        let mut dimension = None;
        for row in data {
            let index = row["index"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .filter(|i| *i < inputs.len())
                .ok_or_else(invalid)?;
            if vectors[index].is_some() {
                return Err(invalid());
            }
            let values = row["embedding"]
                .as_array()
                .filter(|v| (128..=4096).contains(&v.len()))
                .ok_or_else(invalid)?;
            if profile
                .dimensions
                .is_some_and(|d| d as usize != values.len())
                || dimension.is_some_and(|d| d != values.len())
            {
                return Err(invalid());
            }
            dimension = Some(values.len());
            let mut vector = Vec::with_capacity(values.len());
            let mut norm = 0.0f64;
            for v in values {
                let f = v.as_f64().ok_or_else(invalid)? as f32;
                if !f.is_finite() {
                    return Err(invalid());
                }
                norm += f64::from(f) * f64::from(f);
                vector.push(f);
            }
            if !norm.is_finite() || norm <= 0.0 {
                return Err(invalid());
            }
            let norm = norm.sqrt();
            for v in &mut vector {
                *v = (f64::from(*v) / norm) as f32;
            }
            if !vector.iter().all(|v| v.is_finite()) || !vector.iter().any(|v| *v != 0.0) {
                return Err(invalid());
            }
            vectors[index] = Some(vector);
        }
        vectors.into_iter().map(|v| v.ok_or_else(invalid)).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::CredentialChange,
        intelligence::{test_support::*, types::*},
    };
    fn profile() -> ModelProfile {
        ModelProfile {
            kind: ServiceKind::Embedding,
            max_context_tokens: None,
            max_output_tokens: None,
            dimensions: None,
            ..test_llm_profile()
        }
    }
    #[tokio::test]
    async fn knowledge_embeddings_restore_response_order_and_normalize_full_precision() {
        let h = IntelligenceHarness::new();
        let p = profile();
        h.config
            .save(SaveModelProfile {
                profile: p.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-embedding-key".into(),
                },
            })
            .await
            .unwrap();
        let mut a = vec![0.0; 128];
        a[0] = 3.0;
        a[1] = 4.0;
        let mut b = vec![0.0; 128];
        b[2] = 2.0;
        h.http
            .responses
            .lock()
            .unwrap()
            .push_back(Ok(serde_json::json!({
            "data":[{
            "index":1,
            "embedding":b}
            ,
            {
            "index":0,
            "embedding":a}
            ]}
            )));
        let transport = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
        let (_cancel, rx) = tokio::sync::watch::channel(false);
        let vectors = transport
            .embed(
                &p,
                &["first".into(), "second".into()],
                EmbeddingExecution::new(std::sync::Arc::new(()), rx),
            )
            .await
            .unwrap();
        assert_eq!(&vectors[0][0..2], &[0.6, 0.8]);
        assert_eq!(vectors[1][2], 1.0);
        let calls = h.http.requests.lock().unwrap();
        assert_eq!(
            calls[0].body,
            serde_json::json!({
            "model":"fixture-model",
            "input":["first",
            "second"],
            "encoding_format":"float"}
            )
        );
        assert!(calls[0].retry == crate::intelligence::http::HttpRetryPolicy::OnceTransient);
    }
    #[tokio::test]
    async fn knowledge_embeddings_reject_snapshot_edits_before_http_and_invalid_indexes() {
        let h = IntelligenceHarness::new();
        let p = profile();
        h.config
            .save(SaveModelProfile {
                profile: p.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-embedding-key".into(),
                },
            })
            .await
            .unwrap();
        let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
        let mut edited = p.clone();
        edited.timeout_ms = 2000;
        let (_c, rx) = tokio::sync::watch::channel(false);
        assert!(matches!(
            t.embed(
                &edited,
                &["x".into()],
                EmbeddingExecution::new(std::sync::Arc::new(()), rx)
            )
            .await,
            Err(crate::error::AppError::Intelligence(
                IntelligenceErrorCode::Configuration
            ))
        ));
        assert!(h.http.requests.lock().unwrap().is_empty());
        for data in [
            serde_json::json!([]),
            serde_json::json!([{
            "index":1,
            "embedding":vec![1.0;
            128]}
            ]),
            serde_json::json!([{
            "index":0,
            "embedding":vec![0.0;
            128]}
            ]),
            serde_json::json!([{
            "index":0,
            "embedding":vec![1.0;
            127]}
            ]),
            serde_json::json!([{
            "index":0,
            "embedding":vec![1.0;
            128]}
            ,
            {
            "index":0,
            "embedding":vec![1.0;
            128]}
            ]),
        ] {
            h.http
                .responses
                .lock()
                .unwrap()
                .push_back(Ok(serde_json::json!({
                "data":data}
                )));
            let (_c, rx) = tokio::sync::watch::channel(false);
            assert!(
                t.embed(
                    &p,
                    &["x".into()],
                    EmbeddingExecution::new(std::sync::Arc::new(()), rx)
                )
                .await
                .is_err()
            );
        }
    }
    #[test]
    fn knowledge_embedding_semantics_bind_endpoint_profile_model_without_actual_dimension() {
        let p = profile();
        let a = crate::knowledge::identity::EmbeddingIdentity::from_profile(&p, 128).unwrap();
        let b = crate::knowledge::identity::EmbeddingIdentity::from_profile(&p, 256).unwrap();
        assert_eq!(a.semantic_key().unwrap(), b.semantic_key().unwrap());
        assert_ne!(a.key().unwrap(), b.key().unwrap());
        let mut other = p;
        other.base_url = "http://127.0.0.1:23456/v1".into();
        assert_ne!(
            a.semantic_key().unwrap(),
            crate::knowledge::identity::EmbeddingIdentity::from_profile(&other, 128)
                .unwrap()
                .semantic_key()
                .unwrap()
        );
    }
    #[tokio::test]
    async fn knowledge_embeddings_enforce_json_bytes_and_text_count_before_http() {
        let h = IntelligenceHarness::new();
        let p = profile();
        h.config
            .save(SaveModelProfile {
                profile: p.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-embedding-key".into(),
                },
            })
            .await
            .unwrap();
        let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
        for inputs in [vec![], vec!["x".into(); 33], vec!["\t".repeat(65536)]] {
            let (_c, rx) = tokio::sync::watch::channel(false);
            assert!(matches!(
                t.embed(
                    &p,
                    &inputs,
                    EmbeddingExecution::new(std::sync::Arc::new(()), rx)
                )
                .await,
                Err(crate::error::AppError::Intelligence(
                    IntelligenceErrorCode::ResourceLimit
                ))
            ));
        }
        assert!(h.http.requests.lock().unwrap().is_empty());
    }
}
