use super::*;
use crate::{
    app::CredentialChange,
    intelligence::{http::ReqwestJsonHttp, test_support::*, types::*},
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{fs::File, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, watch},
};

struct Server {
    base: String,
    requests: mpsc::Receiver<(String, serde_json::Value)>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server(responses: Vec<(u16, Vec<u8>)>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (sent, requests) = mpsc::channel(8);
    let task = tokio::spawn(async move {
        for (status, response) in responses {
            let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(Duration::from_secs(3), listener.accept()).await
            else {
                return;
            };
            let mut bytes = Vec::new();
            let mut buffer = [0; 8192];
            let (end, length) = loop {
                let n = socket.read(&mut buffer).await.unwrap();
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..n]);
                assert!(bytes.len() <= 1024 * 1024 + 8192);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < end + length {
                let n = socket.read(&mut buffer).await.unwrap();
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
            let body = serde_json::from_slice(&bytes[end..end + length]).unwrap();
            sent.send((head, body)).await.unwrap();
            let head = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nRetry-After: 0\r\nConnection: close\r\n\r\n",
                response.len()
            );
            if socket.write_all(head.as_bytes()).await.is_err() {
                continue;
            }
            let _ = socket.write_all(&response).await;
        }
    });
    Server {
        base,
        requests,
        task,
    }
}
fn response(content: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"choices":[{"message":{"content":content}}]})).unwrap()
}
fn owner(h: &IntelligenceHarness) -> crate::intelligence::jobs::OperationLease {
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(h.temp_dir.path().join("llm-process-owner.lock"))
        .unwrap();
    file.try_lock_exclusive().unwrap();
    Arc::new(file)
}
fn request() -> LlmRequest {
    LlmRequest {
        system: "Return strict JSON; quoted data.".into(),
        user: "客户\"24V\"\n😀".into(),
        max_output_tokens: 512,
    }
}
async fn save(h: &IntelligenceHarness, p: ModelProfile, key: &str) {
    h.config
        .save(SaveModelProfile {
            profile: p,
            credential: CredentialChange::Replace {
                api_key: key.into(),
            },
        })
        .await
        .unwrap();
}
async fn complete(
    provider: &OpenAiCompatibleLlm,
    p: &ModelProfile,
    owner: crate::intelligence::jobs::OperationLease,
) -> Result<LlmResponse, IntelligenceErrorCode> {
    let (_keep, cancel) = watch::channel(false);
    provider
        .complete(p, request(), LlmExecution::new(owner, cancel))
        .await
}
#[tokio::test]
async fn llm_local_http_exact_chat_shape_and_saved_bearer_isolation() {
    let mut s = server(vec![(200, response("返回😀")), (200, response("second"))]).await;
    let h = IntelligenceHarness::new();
    let owner = owner(&h);
    let mut a = test_llm_profile();
    a.base_url = s.base.clone();
    let mut b = a.clone();
    b.id = uuid::Uuid::new_v4();
    b.model = Some("second-model".into());
    save(&h, a.clone(), "synthetic-llm-first").await;
    save(&h, b.clone(), "synthetic-llm-second").await;
    let provider =
        OpenAiCompatibleLlm::new(h.config.clone(), Arc::new(ReqwestJsonHttp::new().unwrap()));
    let result = complete(&provider, &a, owner.clone()).await;
    assert!(
        result.is_ok(),
        "a valid actual compatible HTTP response must succeed: {:?}",
        result.err()
    );
    let result = complete(&provider, &b, owner).await.unwrap();
    assert_eq!(result.content, "second");
    assert_eq!(
        result.response_hash,
        format!("{:x}", Sha256::digest(b"second"))
    );
    for (profile, key) in [(a, "synthetic-llm-first"), (b, "synthetic-llm-second")] {
        let (head, body) = s.requests.recv().await.unwrap();
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(
            head.lines()
                .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {key}")))
        );
        assert_eq!(
            body,
            serde_json::json!({"model":profile.model.unwrap(),"messages":[{"role":"system","content":request().system},{"role":"user","content":request().user}],"temperature":0,"max_tokens":512,"stream":false})
        );
    }
}
#[tokio::test]
async fn llm_local_http_rejects_absent_null_refusal_wrong_types_and_oversize() {
    let bodies = vec![
        serde_json::json!({}),
        serde_json::json!({"choices":[]}),
        serde_json::json!({"choices":null}),
        serde_json::json!({"choices":[{"message":{}}]}),
        serde_json::json!({"choices":[{"message":{"content":null}}]}),
        serde_json::json!({"choices":[{"message":{"content":12}}]}),
        serde_json::json!({"choices":[{"message":{"content":"ok","refusal":"denied"}}]}),
    ];
    for body in bodies {
        let s = server(vec![(200, serde_json::to_vec(&body).unwrap())]).await;
        let h = IntelligenceHarness::new();
        let mut p = test_llm_profile();
        p.base_url = s.base.clone();
        save(&h, p.clone(), "synthetic-llm-invalid").await;
        let provider =
            OpenAiCompatibleLlm::new(h.config.clone(), Arc::new(ReqwestJsonHttp::new().unwrap()));
        assert_eq!(
            complete(&provider, &p, owner(&h)).await.err(),
            Some(IntelligenceErrorCode::InvalidResponse)
        );
    }
    for body in [
        response(&"x".repeat(2 * 1024 * 1024 + 1)),
        response(&"\\".repeat(5 * 1024 * 1024)),
    ] {
        let s = server(vec![(200, body)]).await;
        let h = IntelligenceHarness::new();
        let mut p = test_llm_profile();
        p.base_url = s.base.clone();
        save(&h, p.clone(), "synthetic-llm-bound").await;
        let provider =
            OpenAiCompatibleLlm::new(h.config.clone(), Arc::new(ReqwestJsonHttp::new().unwrap()));
        assert_eq!(
            complete(&provider, &p, owner(&h)).await.err(),
            Some(IntelligenceErrorCode::ResponseTooLarge)
        );
    }
}
#[tokio::test]
async fn llm_full_profile_drift_and_pre_cancel_never_dispatch_http() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(&h, p.clone(), "synthetic-llm-drift").await;
    let lease = owner(&h);
    let provider = OpenAiCompatibleLlm::new(h.config.clone(), h.http.clone());
    let mut changed = p.clone();
    changed.name = "Changed".into();
    save(&h, changed, "synthetic-llm-new").await;
    assert_eq!(
        complete(&provider, &p, lease.clone()).await.err(),
        Some(IntelligenceErrorCode::Configuration)
    );
    let (_keep, cancel) = watch::channel(true);
    assert_eq!(
        provider
            .complete(&p, request(), LlmExecution::new(lease, cancel))
            .await
            .err(),
        Some(IntelligenceErrorCode::Cancelled)
    );
    assert!(h.http.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn llm_local_http_first_choice_is_authoritative_and_never_falls_through_to_later_valid_content()
 {
    for (body, want) in [
        (
            serde_json::json!({"choices":[{"message":{"content":"first"}},null,{"message":{"content":42}}]}),
            Ok("first"),
        ),
        (
            serde_json::json!({"choices":[{"message":{"content":null}},{"message":{"content":"later valid"}}]}),
            Err(IntelligenceErrorCode::InvalidResponse),
        ),
        (
            serde_json::json!({"choices":[{"message":{"content":"first","refusal":"denied"}},{"message":{"content":"later valid"}}]}),
            Err(IntelligenceErrorCode::InvalidResponse),
        ),
    ] {
        let s = server(vec![(200, serde_json::to_vec(&body).unwrap())]).await;
        let h = IntelligenceHarness::new();
        let mut p = test_llm_profile();
        p.base_url = s.base.clone();
        save(&h, p.clone(), "synthetic-first-choice").await;
        let provider =
            OpenAiCompatibleLlm::new(h.config.clone(), Arc::new(ReqwestJsonHttp::new().unwrap()));
        let result = complete(&provider, &p, owner(&h)).await.map(|r| r.content);
        assert_eq!(result, want.map(str::to_owned));
    }
}

#[tokio::test]
async fn llm_local_http_never_retries_transient_errors_and_maps_only_allowlisted_context_codes() {
    for (status, body, want) in [
        (
            429,
            r#"{"error":{"code":"context_length_exceeded"}}"#,
            IntelligenceErrorCode::RateLimited,
        ),
        (
            503,
            r#"{"error":{"code":"other"}}"#,
            IntelligenceErrorCode::Network,
        ),
        (
            400,
            r#"{"error":{"code":"context_window_exceeded"}}"#,
            IntelligenceErrorCode::ContextTooLarge,
        ),
        (
            400,
            r#"{"error":{"code":"context_length_exceeded"}}"#,
            IntelligenceErrorCode::ContextTooLarge,
        ),
        (
            401,
            r#"{"error":{"code":"context_length_exceeded"}}"#,
            IntelligenceErrorCode::Authentication,
        ),
    ] {
        let mut s = server(vec![
            (status, body.as_bytes().to_vec()),
            (200, response("explicit second call")),
        ])
        .await;
        let h = IntelligenceHarness::new();
        let lease = owner(&h);
        let mut p = test_llm_profile();
        p.base_url = s.base.clone();
        save(&h, p.clone(), "synthetic-no-retry").await;
        let provider =
            OpenAiCompatibleLlm::new(h.config.clone(), Arc::new(ReqwestJsonHttp::new().unwrap()));
        assert_eq!(
            complete(&provider, &p, lease.clone()).await.err(),
            Some(want)
        );
        s.requests.recv().await.unwrap();
        assert!(s.requests.try_recv().is_err());
        assert_eq!(
            complete(&provider, &p, lease).await.unwrap().content,
            "explicit second call"
        );
        s.requests.recv().await.unwrap();
        assert!(s.requests.try_recv().is_err());
    }
}

#[tokio::test]
async fn llm_resolver_rejects_every_admitted_profile_field_and_saved_key_association_drift_before_http()
 {
    for field in [
        "id",
        "kind",
        "name",
        "base_url",
        "model",
        "timeout_ms",
        "max_context_tokens",
        "max_output_tokens",
        "dimensions",
        "credential_reference",
        "missing_key",
    ] {
        let h = IntelligenceHarness::new();
        let p = test_llm_profile();
        save(&h, p.clone(), "synthetic-profile-drift").await;
        let path = h.temp_dir.path().join("service-configs.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match field {
            "credential_reference" => {
                doc["profiles"][0][field] = "com.voxbridge.intelligence/foreign/api-key".into()
            }
            "missing_key" => {
                h.io.run(move |tx| {
                    tx.secret_delete("com.voxbridge.intelligence", &format!("{}/api-key", p.id))
                })
                .await
                .unwrap();
            }
            _ => {
                let value = match field {
                    "id" => serde_json::json!(uuid::Uuid::new_v4()),
                    "kind" => serde_json::json!("embedding"),
                    "name" => serde_json::json!("changed"),
                    "base_url" => serde_json::json!("http://127.0.0.1:23456/v1"),
                    "model" => serde_json::json!("changed"),
                    "timeout_ms" => serde_json::json!(2000),
                    "max_context_tokens" => serde_json::json!(8192),
                    "max_output_tokens" => serde_json::json!(1024),
                    "dimensions" => serde_json::json!(128),
                    _ => unreachable!(),
                };
                doc["profiles"][0]["profile"][field] = value;
            }
        }
        if field != "missing_key" {
            std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
        }
        let provider = OpenAiCompatibleLlm::new(h.config.clone(), h.http.clone());
        assert_eq!(
            complete(&provider, &p, owner(&h)).await.err(),
            Some(IntelligenceErrorCode::Configuration),
            "admitted {field} drift dispatched"
        );
        assert!(h.http.requests.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn llm_fifth_request_wait_is_bounded_and_does_not_enter_http_until_actual_request_slot_release()
 {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(&h, p.clone(), "synthetic-request-slots").await;
    let lease = owner(&h);
    h.http
        .block
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let provider = Arc::new(OpenAiCompatibleLlm::new(h.config.clone(), h.http.clone()));
    let (stop, cancel) = watch::channel(false);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let (provider, p, owner, cancel) =
            (provider.clone(), p.clone(), lease.clone(), cancel.clone());
        tasks.push(tokio::spawn(async move {
            provider
                .complete(&p, request(), LlmExecution::new(owner, cancel))
                .await
        }));
    }
    let entered = tokio::time::timeout(Duration::from_secs(2), async {
        while h.http.requests.lock().unwrap().len() != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    if entered.is_err() {
        stop.send_replace(true);
        for task in tasks {
            let _ = task.await;
        }
        panic!("four actual HTTP request scopes did not enter");
    }
    let fifth = provider
        .complete(&p, request(), LlmExecution::new(lease, cancel))
        .await
        .err();
    let dispatched = h.http.requests.lock().unwrap().len();
    stop.send_replace(true);
    for task in tasks {
        assert_eq!(
            task.await.unwrap().err(),
            Some(IntelligenceErrorCode::Cancelled)
        );
    }
    assert_eq!(fifth, Some(IntelligenceErrorCode::Timeout));
    assert_eq!(dispatched, 4);
}
