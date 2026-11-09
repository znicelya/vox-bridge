use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct Server {
    url: url::Url,
    calls: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server(responses: Vec<String>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!("http://{}/test", listener.local_addr().unwrap())).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let task = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 8192];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            seen.fetch_add(1, Ordering::SeqCst);
            socket.write_all(response.as_bytes()).await.unwrap();
            if response.ends_with("BLOCK") {
                let mut byte = [0];
                let _ = socket.read(&mut byte).await;
            }
        }
    });
    Server { url, calls, task }
}
fn response(status: u16, extra: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
}
fn request(url: url::Url) -> JsonRequest {
    JsonRequest {
        endpoint: url,
        bearer: SecretString::from_store_bytes(b"synthetic-http".to_vec()).unwrap(),
        body: serde_json::json!({"test":true}),
        timeout: Duration::from_secs(2),
        max_response_bytes: 128,
        retry_policy: HttpRetryPolicy::Never,
        error_policy: JsonErrorPolicy::Generic,
    }
}
#[tokio::test]
async fn intelligence_http_compatible_context_errors_require_explicit_exact_bounded_code() {
    for (policy, body, want) in [
        (
            JsonErrorPolicy::CompatibleLlmContextErrors,
            r#"{"error":{"code":"context_length_exceeded","message":"raw-error-sentinel"}}"#
                .to_owned(),
            IntelligenceErrorCode::ContextTooLarge,
        ),
        (
            JsonErrorPolicy::Generic,
            r#"{"error":{"code":"context_length_exceeded"}}"#.to_owned(),
            IntelligenceErrorCode::Network,
        ),
        (
            JsonErrorPolicy::CompatibleLlmContextErrors,
            r#"{"error":{"message":"context_length_exceeded"}}"#.to_owned(),
            IntelligenceErrorCode::Network,
        ),
        (
            JsonErrorPolicy::CompatibleLlmContextErrors,
            r#"{"error":{"code":"CONTEXT_LENGTH_EXCEEDED"}}"#.to_owned(),
            IntelligenceErrorCode::Network,
        ),
        (
            JsonErrorPolicy::CompatibleLlmContextErrors,
            r#"{"code":"context_length_exceeded"}"#.to_owned(),
            IntelligenceErrorCode::Network,
        ),
        (
            JsonErrorPolicy::CompatibleLlmContextErrors,
            format!(
                "{{\"error\":{{\"code\":\"context_length_exceeded\",\"message\":\"{}\"}}}}",
                "x".repeat(64 * 1024)
            ),
            IntelligenceErrorCode::Network,
        ),
    ] {
        let s = server(vec![response(400, "", &body)]).await;
        let mut r = request(s.url.clone());
        r.error_policy = policy;
        assert_eq!(post(r).await.err(), Some(want));
        assert_eq!(s.calls.load(Ordering::SeqCst), 1);
    }
}
async fn post(r: JsonRequest) -> Result<JsonResponse, IntelligenceErrorCode> {
    let (_keep, c) = watch::channel(false);
    ReqwestJsonHttp::new().unwrap().post(r, c).await
}
#[tokio::test]
async fn intelligence_http_valid_json_and_status_categories_are_safe() {
    for (status, body, want) in [
        (200, "{}", None),
        (
            401,
            "secret error",
            Some(IntelligenceErrorCode::Authentication),
        ),
        (429, "bad", Some(IntelligenceErrorCode::RateLimited)),
        (503, "bad", Some(IntelligenceErrorCode::Network)),
        (302, "bad", Some(IntelligenceErrorCode::Network)),
        (
            200,
            "<html>secret</html>",
            Some(IntelligenceErrorCode::InvalidResponse),
        ),
        (200, "{", Some(IntelligenceErrorCode::InvalidResponse)),
    ] {
        let s = server(vec![response(status, "", body)]).await;
        let result = post(request(s.url.clone())).await;
        match want {
            None => assert_eq!(result.unwrap().body, b"{}"),
            Some(want) => assert_eq!(result.err(), Some(want)),
        }
    }
}
#[tokio::test]
async fn intelligence_http_stream_cap_ignores_content_length() {
    let s=server(vec!["HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n8\r\n12345678\r\n8\r\n12345678\r\n0\r\n\r\n".into()]).await;
    let mut r = request(s.url.clone());
    r.max_response_bytes = 10;
    assert_eq!(
        post(r).await.err(),
        Some(IntelligenceErrorCode::ResponseTooLarge)
    );
}
#[tokio::test]
async fn intelligence_http_cancel_drops_blocked_body_and_pre_cancel_makes_no_request() {
    let s = server(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nBLOCK".into(),
    ])
    .await;
    let (cancel, c) = watch::channel(false);
    let mut r = request(s.url.clone());
    r.max_response_bytes = 2000;
    let task = tokio::spawn(async move { ReqwestJsonHttp::new().unwrap().post(r, c).await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while s.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    cancel.send_replace(true);
    assert_eq!(
        task.await.unwrap().err(),
        Some(IntelligenceErrorCode::Cancelled)
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        while !s.task.is_finished() {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    let (cancel, c) = watch::channel(true);
    assert_eq!(
        ReqwestJsonHttp::new()
            .unwrap()
            .post(request(s.url.clone()), c)
            .await
            .err(),
        Some(IntelligenceErrorCode::Cancelled)
    );
    drop(cancel);
}
#[tokio::test]
async fn intelligence_http_retry_is_explicit_once_and_validated() {
    for (policy, header, want, calls) in [
        (HttpRetryPolicy::Never, "Retry-After: 0\r\n", false, 1),
        (
            HttpRetryPolicy::OnceTransient,
            "Retry-After: 0\r\n",
            true,
            2,
        ),
        (
            HttpRetryPolicy::OnceTransient,
            "Retry-After: 6\r\n",
            false,
            1,
        ),
        (
            HttpRetryPolicy::OnceTransient,
            "Retry-After: -1\r\n",
            false,
            1,
        ),
        (
            HttpRetryPolicy::OnceTransient,
            "Retry-After: nope\r\n",
            false,
            1,
        ),
        (HttpRetryPolicy::OnceTransient, "", false, 1),
    ] {
        let s = server(vec![response(429, header, "{}"), response(200, "", "{}")]).await;
        let mut r = request(s.url.clone());
        r.retry_policy = policy;
        assert_eq!(post(r).await.is_ok(), want);
        assert_eq!(s.calls.load(Ordering::SeqCst), calls);
    }
    let s = server(vec![
        response(503, "Retry-After: 0\r\n", "{}"),
        response(503, "Retry-After: 0\r\n", "{}"),
    ])
    .await;
    let mut r = request(s.url.clone());
    r.retry_policy = HttpRetryPolicy::OnceTransient;
    assert_eq!(post(r).await.err(), Some(IntelligenceErrorCode::Network));
    assert_eq!(s.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn intelligence_http_original_deadline_and_retry_sleep_cancellation() {
    let s = server(vec![response(429, "Retry-After: 1\r\n", "{}")]).await;
    let mut r = request(s.url.clone());
    r.retry_policy = HttpRetryPolicy::OnceTransient;
    r.timeout = Duration::from_millis(50);
    assert_eq!(
        post(r).await.err(),
        Some(IntelligenceErrorCode::RateLimited)
    );
    assert_eq!(s.calls.load(Ordering::SeqCst), 1);
    let s = server(vec![response(429, "Retry-After: 1\r\n", "{}")]).await;
    let mut r = request(s.url.clone());
    r.retry_policy = HttpRetryPolicy::OnceTransient;
    let (cancel, c) = watch::channel(false);
    let task = tokio::spawn(async move { ReqwestJsonHttp::new().unwrap().post(r, c).await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while s.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    cancel.send_replace(true);
    assert_eq!(
        task.await.unwrap().err(),
        Some(IntelligenceErrorCode::Cancelled)
    );
    assert_eq!(s.calls.load(Ordering::SeqCst), 1);
    let s = server(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nBLOCK".into(),
    ])
    .await;
    let mut r = request(s.url.clone());
    r.timeout = Duration::from_millis(30);
    r.max_response_bytes = 2000;
    assert_eq!(post(r).await.err(), Some(IntelligenceErrorCode::Timeout));
}
#[tokio::test]
async fn intelligence_http_rejects_remote_cleartext_and_userinfo() {
    for url in [
        "http://example.com",
        "http://127.0.0.2",
        "http://key@localhost",
        "https://example.com?api_key=secret",
    ] {
        assert_eq!(
            post(request(url::Url::parse(url).unwrap())).await.err(),
            Some(IntelligenceErrorCode::Configuration)
        );
    }
}
#[test]
fn intelligence_http_constructor_needs_no_global_crypto_provider() {
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    assert!(ReqwestJsonHttp::new().is_ok());
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
}
#[tokio::test]
async fn intelligence_http_tls_trust_and_hostname_are_enforced() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let der = cert.cert.der().clone();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![der.clone()], key.into())
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        for _ in 0..3 {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(mut stream) = acceptor.accept(socket).await {
                let mut buf = [0; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(response(200, "", "{}").as_bytes()).await;
            }
        }
    });
    let mut roots = rustls::RootCertStore::empty();
    roots.add(der).unwrap();
    let trusted = ReqwestJsonHttp::with_roots(roots).unwrap();
    let (_keep, c) = watch::channel(false);
    assert!(
        trusted
            .post(
                request(url::Url::parse(&format!("https://localhost:{port}")).unwrap()),
                c.clone()
            )
            .await
            .is_ok()
    );
    assert_eq!(
        trusted
            .post(
                request(url::Url::parse(&format!("https://127.0.0.1:{port}")).unwrap()),
                c.clone()
            )
            .await
            .err(),
        Some(IntelligenceErrorCode::Network)
    );
    assert_eq!(
        ReqwestJsonHttp::new()
            .unwrap()
            .post(
                request(url::Url::parse(&format!("https://localhost:{port}")).unwrap()),
                c
            )
            .await
            .err(),
        Some(IntelligenceErrorCode::Network)
    );
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn intelligence_http_actual_authorization_is_isolated_for_two_profiles() {
    use crate::{
        app::CredentialChange,
        intelligence::{test_support::*, types::SaveModelProfile},
    };
    let h = IntelligenceHarness::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        for expected in ["Bearer synthetic-one", "Bearer synthetic-two"] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = [0; 8192];
            let n = socket.read(&mut data).await.unwrap();
            let request = String::from_utf8_lossy(&data[..n]);
            let authorization = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap()
                .split_once(':')
                .unwrap()
                .1
                .trim();
            assert_eq!(authorization, expected);
            socket
                .write_all(response(200, "", "{}").as_bytes())
                .await
                .unwrap();
        }
    });
    for key in ["synthetic-one", "synthetic-two"] {
        let p = test_llm_profile();
        h.config
            .save(SaveModelProfile {
                profile: p.clone(),
                credential: CredentialChange::Replace {
                    api_key: key.into(),
                },
            })
            .await
            .unwrap();
        let mut r = request(url.clone());
        r.bearer = h.config.resolve_credentials(p.id).await.unwrap();
        assert!(post(r).await.is_ok());
    }
    server.await.unwrap();
}

#[tokio::test]
async fn intelligence_http_valid_retry_after_dates_obey_same_bound() {
    for (header, success) in [
        ("Retry-After: Sun, 06 Nov 1994 08:49:37 GMT\r\n", true),
        ("Retry-After: Fri, 06 Nov 2099 08:49:37 GMT\r\n", false),
    ] {
        let s = server(vec![response(429, header, "{}"), response(200, "", "{}")]).await;
        let mut r = request(s.url.clone());
        r.retry_policy = HttpRetryPolicy::OnceTransient;
        assert_eq!(post(r).await.is_ok(), success);
    }
}
