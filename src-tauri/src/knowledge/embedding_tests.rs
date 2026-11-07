use super::embedding::*;
use crate::{
    app::CredentialChange,
    intelligence::{
        http::{JsonHttp, JsonRequest, JsonResponse},
        test_support::*,
        types::*,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
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
async fn knowledge_embedding_rejects_dimension_inconsistency_duplicate_missing_nonfinite_and_extremes()
 {
    let h = IntelligenceHarness::new();
    let p = profile();
    h.config
        .save(SaveModelProfile {
            profile: p.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic".into(),
            },
        })
        .await
        .unwrap();
    let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
    for data in [
        serde_json::json!([{
        "index":0,
        "embedding":vec![1.;
        128]}
        ,
        {
        "index":0,
        "embedding":vec![1.;
        128]}
        ]),
        serde_json::json!([{
        "index":0,
        "embedding":vec![1.;
        128]}
        ]),
        serde_json::json!([{
        "index":0,
        "embedding":vec![1.;
        128]}
        ,
        {
        "index":1,
        "embedding":vec![1.;
        256]}
        ]),
        serde_json::json!([{
        "index":0,
        "embedding":vec![1e100;
        128]}
        ,
        {
        "index":1,
        "embedding":vec![1.;
        128]}
        ]),
        serde_json::json!([{
        "index":0,
        "embedding":vec![1.;
        4097]}
        ,
        {
        "index":1,
        "embedding":vec![1.;
        4097]}
        ]),
        serde_json::json!([{
        "index":0,
        "embedding":vec![0.;
        128]}
        ,
        {
        "index":1,
        "embedding":vec![1.;
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
        assert!(matches!(
            t.embed(
                &p,
                &["first".into(), "second".into()],
                EmbeddingExecution::new(Arc::new(()), rx)
            )
            .await,
            Err(crate::error::AppError::Intelligence(
                IntelligenceErrorCode::InvalidResponse
            ))
        ));
    }
    let mut configured = p.clone();
    configured.dimensions = Some(128);
    h.config
        .save(SaveModelProfile {
            profile: configured.clone(),
            credential: CredentialChange::Keep,
        })
        .await
        .unwrap();
    h.http
        .responses
        .lock()
        .unwrap()
        .push_back(Ok(serde_json::json!({
        "data":[{
        "index":0,
        "embedding":vec![1.;
        256]}
        ]}
        )));
    let (_c, rx) = tokio::sync::watch::channel(false);
    assert!(
        t.embed(
            &configured,
            &["first".into()],
            EmbeddingExecution::new(Arc::new(()), rx)
        )
        .await
        .is_err()
    );
}
struct BlockHttp {
    entered: AtomicUsize,
    active: AtomicUsize,
    max: AtomicUsize,
    notice: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
    oversize: bool,
}
#[async_trait::async_trait]
impl JsonHttp for BlockHttp {
    async fn post(
        &self,
        request: JsonRequest,
        _cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<JsonResponse, IntelligenceErrorCode> {
        assert_eq!(request.max_response_bytes, 8 * 1024 * 1024);
        assert!(matches!(
            request.retry_policy,
            crate::intelligence::http::HttpRetryPolicy::OnceTransient
        ));
        self.entered.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max.fetch_max(active, Ordering::SeqCst);
        self.notice.notify_one();
        let _release = self.release.acquire().await.unwrap();
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(JsonResponse {
            status: 200,
            body: if self.oversize {
                vec![b' '; 8 * 1024 * 1024 + 1]
            } else {
                serde_json::to_vec(&serde_json::json!({
                "data":[{
                "index":0,
                "embedding":vec![1.;
                128]}
                ]}
                ))
                .unwrap()
            },
        })
    }
}
#[tokio::test]
async fn knowledge_embedding_shared_four_request_gate_is_held_until_http_finishes() {
    let h = IntelligenceHarness::new();
    let p = profile();
    h.config
        .save(SaveModelProfile {
            profile: p.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic".into(),
            },
        })
        .await
        .unwrap();
    let http = Arc::new(BlockHttp {
        entered: AtomicUsize::new(0),
        active: AtomicUsize::new(0),
        max: AtomicUsize::new(0),
        notice: tokio::sync::Notify::new(),
        release: tokio::sync::Semaphore::new(0),
        oversize: false,
    });
    let t = Arc::new(OpenAiCompatibleEmbeddings::new(
        h.config.clone(),
        http.clone(),
    ));
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let t = t.clone();
        let p = p.clone();
        let owner = owner.clone();
        tasks.push(tokio::spawn(async move {
            let (_c, rx) = tokio::sync::watch::channel(false);
            t.embed(&p, &["x".into()], EmbeddingExecution::new(owner, rx))
                .await
        }));
    }
    drop(owner);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while http.entered.load(Ordering::SeqCst) < 4 {
            http.notice.notified().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(http.entered.load(Ordering::SeqCst), 4);
    assert!(weak.upgrade().is_some());
    http.release.add_permits(5);
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(http.max.load(Ordering::SeqCst), 4);
    assert!(weak.upgrade().is_none());
}
#[tokio::test]
async fn knowledge_embedding_eight_mib_response_limit_is_checked_even_for_a_port_ignoring_the_bound()
 {
    let h = IntelligenceHarness::new();
    let p = profile();
    h.config
        .save(SaveModelProfile {
            profile: p.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic".into(),
            },
        })
        .await
        .unwrap();
    let http = Arc::new(BlockHttp {
        entered: AtomicUsize::new(0),
        active: AtomicUsize::new(0),
        max: AtomicUsize::new(0),
        notice: tokio::sync::Notify::new(),
        release: tokio::sync::Semaphore::new(1),
        oversize: true,
    });
    let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), http);
    let (_c, rx) = tokio::sync::watch::channel(false);
    assert!(matches!(
        t.embed(&p, &["x".into()], EmbeddingExecution::new(Arc::new(()), rx))
            .await,
        Err(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::ResponseTooLarge
        ))
    ));
}
