use super::{events::*, protocol::*};
use crate::{
    domain::session::{LanguagePair, SessionDirection, SessionMode},
    providers::*,
};
use prost::Message;
use std::time::Duration;
use uuid::Uuid;

fn config() -> ProviderSessionConfig {
    ProviderSessionConfig {
        session_id: Uuid::new_v4(),
        direction: SessionDirection::TargetToSource,
        mode: SessionMode::Live,
        languages: LanguagePair::new("en", "zh").unwrap(),
        target_audio: NORMALIZED_OUTPUT_AUDIO,
        provider_config_id: Uuid::new_v4(),
    }
}
fn response(event: i32, start: i32, end: i32, text: &str, data: Vec<u8>) -> Vec<u8> {
    TranslateResponse {
        event,
        start_time: start,
        end_time: end,
        text: text.into(),
        data,
        ..Default::default()
    }
    .encode_to_vec()
}
fn floats(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

#[test]
fn maps_native_subtitle_boundaries_and_correlates_tts_by_real_timestamp() {
    let cfg = config();
    let mut mapper = VolcEventMapper::new(cfg.clone());
    let mut translation_id = None;
    for (native, finality, translated) in [
        (650, false, false),
        (651, false, false),
        (652, true, false),
        (653, false, true),
        (654, false, true),
        (655, true, true),
    ] {
        let events = mapper
            .map(&response(
                native,
                120,
                if finality { 420 } else { 0 },
                "words",
                vec![],
            ))
            .unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.metadata().session_id, cfg.session_id);
        assert_eq!(event.metadata().direction, SessionDirection::TargetToSource);
        assert_eq!(
            event.metadata().provider_event_type.as_deref(),
            Some(native.to_string().as_str())
        );
        assert_eq!(event.metadata().sentence_id, None);
        assert_eq!(event.metadata().timestamp, Some(Duration::from_millis(120)));
        let text = match event {
            ProviderEvent::SourceFinal { text, .. } if finality && !translated => text,
            ProviderEvent::SourceInterim { text, .. } if !finality && !translated => text,
            ProviderEvent::TranslationFinal { text, .. } if finality && translated => text,
            ProviderEvent::TranslationInterim { text, .. } if !finality && translated => text,
            _ => panic!("wrong subtitle event"),
        };
        assert_eq!(
            text.translation.language,
            if translated { "zh" } else { "en" }
        );
        assert_eq!(
            text.end,
            if finality {
                Some(Duration::from_millis(420))
            } else {
                None
            }
        );
        if translated {
            translation_id = event.metadata().local_correlation_id;
        }
    }
    assert!(translation_id.is_some());
    assert!(
        mapper
            .map(&response(350, 120, 0, "", vec![]))
            .unwrap()
            .is_empty()
    );
    let data = mapper
        .map(&response(352, 0, 0, "", floats(&[0.5, -0.5, 1.5, -1.5])))
        .unwrap();
    let tail = mapper
        .map(&response(351, 120, 420, "", floats(&[0.25, -0.25])))
        .unwrap();
    for (event, expected, sequence) in [
        (&data[0], vec![16384, -16384, 32767, -32768], 0),
        (&tail[0], vec![8192, -8192], 1),
    ] {
        assert_eq!(event.metadata().local_correlation_id, translation_id);
        assert_eq!(event.metadata().sentence_id, None);
        let ProviderEvent::Audio { frame, .. } = event else {
            panic!("missing audio")
        };
        assert_eq!(frame.samples(), expected);
        assert_eq!(frame.sequence(), sequence);
        assert_eq!(frame.spec(), NORMALIZED_OUTPUT_AUDIO);
    }
    mapper.map(&response(653, 600, 0, "", vec![])).unwrap();
    mapper.map(&response(350, 600, 0, "", vec![])).unwrap();
    let next = mapper
        .map(&response(352, 0, 0, "", floats(&[0.1])))
        .unwrap();
    assert_ne!(next[0].metadata().local_correlation_id, translation_id);
}

#[test]
fn rejects_malformed_oversize_nonfinite_audio_and_invalid_times_without_payload_errors() {
    let mut mapper = VolcEventMapper::new(config());
    for bytes in [
        vec![0xff],
        vec![0; MAX_MESSAGE_BYTES + 1],
        response(652, -1, 10, "secret", vec![]),
        response(655, 40, 20, "secret", vec![]),
    ] {
        assert_eq!(mapper.map(&bytes), Err(ProviderError::Protocol));
    }
    for native in [351, 352] {
        for data in [
            vec![0; 3],
            vec![0; MAX_AUDIO_BYTES + 4],
            floats(&[f32::NAN]),
            floats(&[f32::INFINITY]),
        ] {
            assert_eq!(
                mapper.map(&response(native, 0, 0, "secret", data)),
                Err(ProviderError::InvalidAudioFrame)
            );
        }
    }
}

#[test]
fn maps_terminal_mute_and_usage_without_exposing_remote_message() {
    let mut mapper = VolcEventMapper::new(config());
    assert!(mapper.map(&response(150, 0, 0, "", vec![])).unwrap()[0].is_session_started());
    let muted = TranslateResponse {
        event: 250,
        muted_duration_ms: 2345,
        ..Default::default()
    }
    .encode_to_vec();
    assert!(
        matches!(mapper.map(&muted).unwrap().as_slice(), [ProviderEvent::Muted{duration:Some(d),..}] if *d==Duration::from_millis(2345))
    );
    assert!(
        mapper
            .map(&response(154, 0, 0, "usage secret", vec![]))
            .unwrap()
            .is_empty()
    );
    assert!(
        mapper
            .map(&response(9999, 0, 0, "unknown secret", vec![]))
            .unwrap()
            .is_empty()
    );
    let failed = TranslateResponse {
        event: 153,
        response_meta: Some(ResponseMeta {
            status_code: 45000001,
            message: "secret-key raw transcript".into(),
            ..Default::default()
        }),
        ..Default::default()
    }
    .encode_to_vec();
    let events = mapper.map(&failed).unwrap();
    assert!(matches!(
        events.as_slice(),
        [ProviderEvent::Failed {
            error: ProviderError::RemoteFailure { code: 45000001 },
            ..
        }]
    ));
    assert!(!format!("{events:?}").contains("secret"));
    assert!(matches!(
        mapper
            .map(&response(152, 0, 0, "", vec![]))
            .unwrap()
            .as_slice(),
        [ProviderEvent::Finished { .. }]
    ));
}

#[test]
fn does_not_guess_tts_correlation_without_matching_boundary() {
    let mut mapper = VolcEventMapper::new(config());
    mapper.map(&response(653, 100, 0, "", vec![])).unwrap();
    let data = mapper
        .map(&response(352, 0, 0, "", floats(&[0.5])))
        .unwrap();
    assert_eq!(data[0].metadata().local_correlation_id, None);
    let text = mapper
        .map(&response(655, 100, 300, "hello", vec![]))
        .unwrap();
    mapper.map(&response(350, 500, 0, "", vec![])).unwrap();
    let data = mapper
        .map(&response(352, 0, 0, "", floats(&[0.5])))
        .unwrap();
    assert_ne!(
        data[0].metadata().local_correlation_id,
        text[0].metadata().local_correlation_id
    );
}

#[test]
fn rejects_foreign_session_and_negative_mute_duration() {
    let cfg = config();
    let mut mapper = VolcEventMapper::new(cfg);
    let foreign = TranslateResponse {
        event: 150,
        response_meta: Some(ResponseMeta {
            session_id: Uuid::new_v4().to_string(),
            ..Default::default()
        }),
        ..Default::default()
    }
    .encode_to_vec();
    assert_eq!(mapper.map(&foreign), Err(ProviderError::Protocol));
    let muted = TranslateResponse {
        event: 250,
        muted_duration_ms: -1,
        ..Default::default()
    }
    .encode_to_vec();
    assert_eq!(mapper.map(&muted), Err(ProviderError::Protocol));
}

#[test]
fn preserves_interim_start_time_from_actual_start_boundary() {
    let mut mapper = VolcEventMapper::new(config());
    let start = mapper.map(&response(653, 640, 0, "", vec![])).unwrap();
    let interim = mapper.map(&response(654, 0, 0, "hello", vec![])).unwrap();
    assert_eq!(
        interim[0].metadata().timestamp,
        Some(Duration::from_millis(640))
    );
    assert_eq!(
        interim[0].metadata().local_correlation_id,
        start[0].metadata().local_correlation_id
    );
}

use super::client::*;
use crate::{audio::frame::AudioFrame, config::SecretStore, domain::audio::AudioSource};
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{net::TcpListener, sync::oneshot, time::timeout};
use tokio_tungstenite::{accept_hdr_async, tungstenite::Message as WsMessage};

async fn fixture() -> (
    TcpListener,
    VolcengineProvider,
    ProviderSessionConfig,
    tempfile::TempDir,
) {
    fixture_for_scheme("ws").await
}
async fn fixture_for_scheme(
    scheme: &str,
) -> (
    TcpListener,
    VolcengineProvider,
    ProviderSessionConfig,
    tempfile::TempDir,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let cfg = config();
    store
        .set(
            SECRET_SERVICE,
            &secret_key(cfg.provider_config_id),
            b"fixture-api-key",
        )
        .unwrap();
    let provider = VolcengineProvider::for_endpoint(
        store,
        format!("{scheme}://{}/ast", listener.local_addr().unwrap()),
    )
    .unwrap();
    (listener, provider, cfg, dir)
}
fn frame(value: i16) -> AudioFrame {
    AudioFrame::new(AudioSource::Microphone, 0, INPUT_AUDIO, vec![value; 1280]).unwrap()
}
async fn event(stream: &ProviderStream) -> Option<ProviderEvent> {
    timeout(Duration::from_secs(3), stream.next_event())
        .await
        .unwrap()
        .unwrap()
}
async fn accept(
    listener: &TcpListener,
) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    accept_socket(listener.accept().await.unwrap().0).await
}
async fn accept_socket<S>(socket: S) -> tokio_tungstenite::WebSocketStream<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    accept_hdr_async(
        socket,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         mut resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
            assert_eq!(req.headers()["X-Api-Key"], "fixture-api-key");
            assert_eq!(
                req.headers()["X-Api-Resource-Id"],
                "volc.service_type.10053"
            );
            resp.headers_mut()
                .insert("X-Tt-Logid", "fixture-log-123".parse().unwrap());
            Ok(resp)
        },
    )
    .await
    .unwrap()
}
async fn wire_event<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> TranslateRequest
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let message = timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    decode_request(&message.into_data()).unwrap()
}
async fn send_response<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>, native: i32)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    ws.send(WsMessage::Binary(response(native, 0, 0, "", vec![]).into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn client_gates_audio_on_150_and_finishes_once_after_upgrade() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let (seen_tx, seen_rx) = oneshot::channel();
    let (ack_tx, ack_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut ws = accept(&listener).await;
        let start = wire_event(&mut ws).await;
        assert_eq!(start.event, 100);
        assert_eq!(start.target_audio.unwrap().bits, 32);
        seen_tx.send(()).unwrap();
        ack_rx.await.unwrap();
        assert!(timeout(Duration::from_millis(25), ws.next()).await.is_err());
        send_response(&mut ws, 150).await;
        let audio = wire_event(&mut ws).await;
        assert_eq!(audio.event, 200);
        assert_eq!(
            &audio.source_audio.unwrap().binary_data[..4],
            &[42, 0, 42, 0]
        );
        assert_eq!(wire_event(&mut ws).await.event, 102);
        send_response(&mut ws, 152).await;
        if let Ok(Some(Ok(message))) = timeout(Duration::from_millis(250), ws.next()).await {
            assert!(message.is_close());
        }
    });
    let stream = provider.connect(cfg).await.unwrap();
    seen_rx.await.unwrap();
    assert_eq!(
        stream.send_audio(frame(1)).await,
        Err(ProviderError::NotStarted)
    );
    ack_tx.send(()).unwrap();
    let started = event(&stream).await.unwrap();
    assert!(started.is_session_started());
    assert_eq!(
        started.metadata().log_id.as_deref(),
        Some("fixture-log-123")
    );
    stream.send_audio(frame(42)).await.unwrap();
    // Finish drains already admitted input before sending event102.
    stream.finish().await.unwrap();
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Finished { .. })
    ));
    assert!(event(&stream).await.is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn reconnect_emits_gap_and_never_replays_sent_or_stale_queued_audio() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        assert_eq!(wire_event(&mut first).await.event, 100);
        send_response(&mut first, 150).await;
        assert_eq!(wire_event(&mut first).await.event, 200);
        first.close(None).await.unwrap();
        drop(first);
        let mut second = accept(&listener).await;
        assert_eq!(wire_event(&mut second).await.event, 100);
        assert!(
            timeout(Duration::from_millis(25), second.next())
                .await
                .is_err()
        );
        send_response(&mut second, 150).await;
        let new_audio = wire_event(&mut second).await;
        assert_eq!(new_audio.event, 200);
        assert_eq!(&new_audio.source_audio.unwrap().binary_data[..2], &[7, 0]);
        assert_eq!(wire_event(&mut second).await.event, 102);
        send_response(&mut second, 152).await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    assert!(event(&stream).await.unwrap().is_session_started());
    for _ in 0..64 {
        stream.send_audio(frame(9)).await.unwrap();
    }
    assert_eq!(
        stream.send_audio(frame(9)).await,
        Err(ProviderError::QueueFull)
    );
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Gap { .. })
    ));
    assert_eq!(
        stream.send_audio(frame(9)).await,
        Err(ProviderError::NotStarted)
    );
    assert!(event(&stream).await.unwrap().is_session_started());
    stream.send_audio(frame(7)).await.unwrap();
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Finished { .. })
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn reconnect_exhausts_three_new_attempts_and_preserves_typed_timeout() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        wire_event(&mut first).await;
        send_response(&mut first, 150).await;
        // Give the stream a turn to observe its first SessionStarted.
        tokio::time::sleep(Duration::from_millis(10)).await;
        first.close(None).await.unwrap();
        drop(first);
        let mut opened = 0;
        for _ in 0..3 {
            let accepted = timeout(Duration::from_secs(1), accept(&listener)).await;
            let Ok(mut socket) = accepted else {
                return opened;
            };
            assert_eq!(wire_event(&mut socket).await.event, 100);
            opened += 1;
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
        assert!(
            timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
        opened
    });
    let stream = provider.with_test_timeouts().connect(cfg).await.unwrap();
    assert!(event(&stream).await.unwrap().is_session_started());
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Gap { .. })
    ));
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Timeout,
            ..
        })
    ));
    assert_eq!(server.await.unwrap(), 3);
}

#[tokio::test]
async fn initial_upgrade_timeout_stops_after_three_attempts() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut sockets = Vec::new();
        for _ in 0..3 {
            sockets.push(
                timeout(Duration::from_secs(1), listener.accept())
                    .await
                    .unwrap()
                    .unwrap()
                    .0,
            );
        }
        assert!(
            timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err()
        );
        sockets.len()
    });
    let stream = provider.with_test_timeouts().connect(cfg).await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Timeout,
            ..
        })
    ));
    assert!(event(&stream).await.is_none());
    assert_eq!(server.await.unwrap(), 3);
}

#[tokio::test]
async fn finish_timeout_closes_without_retrying_or_sending_102_twice() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        send_response(&mut socket, 150).await;
        assert_eq!(wire_event(&mut socket).await.event, 102);
        // No event152; the client must close on its finish deadline.
        let next = timeout(Duration::from_secs(1), socket.next())
            .await
            .unwrap();
        assert!(!matches!(next, Some(Ok(WsMessage::Binary(_)))));
        assert!(
            timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let stream = provider.with_test_timeouts().connect(cfg).await.unwrap();
    event(&stream).await.unwrap();
    stream.finish().await.unwrap();
    stream.finish().await.unwrap();
    assert_eq!(
        stream.send_audio(frame(1)).await,
        Err(ProviderError::Closed)
    );
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Timeout,
            ..
        })
    ));
    assert!(event(&stream).await.is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn finish_and_cancel_bypass_a_saturated_output_queue() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let (flooded_tx, flooded_rx) = oneshot::channel();
    let (finished_tx, finished_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        send_response(&mut socket, 150).await;
        for _ in 0..96 {
            socket
                .send(WsMessage::Binary(
                    response(651, 0, 0, "bounded text", vec![]).into(),
                ))
                .await
                .unwrap();
        }
        flooded_tx.send(()).unwrap();
        assert_eq!(wire_event(&mut socket).await.event, 102);
        finished_tx.send(()).unwrap();
        let _ = socket.next().await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    event(&stream).await.unwrap();
    flooded_rx.await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    timeout(Duration::from_millis(100), stream.finish())
        .await
        .unwrap()
        .unwrap();
    timeout(Duration::from_millis(200), finished_rx)
        .await
        .unwrap()
        .unwrap();
    timeout(Duration::from_millis(100), stream.cancel())
        .await
        .unwrap()
        .unwrap();
    let mut queued = 0;
    loop {
        match event(&stream).await {
            Some(ProviderEvent::SourceInterim { .. }) => queued += 1,
            Some(ProviderEvent::Failed {
                error: ProviderError::Cancelled,
                ..
            }) => break,
            other => panic!("unexpected terminal {other:?}"),
        }
    }
    assert!(queued <= 64);
    assert!(event(&stream).await.is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn cancel_interrupts_handshake_and_finish_interrupts_retry_delay() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let (started_tx, started_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        started_tx.send(()).unwrap();
        let _ = socket.next().await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    started_rx.await.unwrap();
    stream.cancel().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Cancelled,
            ..
        })
    ));
    server.await.unwrap();

    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        send_response(&mut socket, 150).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        socket.close(None).await.unwrap();
        assert!(
            timeout(Duration::from_millis(400), listener.accept())
                .await
                .is_err()
        );
    });
    let stream = provider.connect(cfg).await.unwrap();
    event(&stream).await.unwrap();
    // The gap is an observable disconnect barrier; finish during backoff,
    // without assuming TCP close propagation fits a fixed sleep.
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Gap { .. })
    ));
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Disconnected,
            ..
        })
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn credentials_are_selected_by_backend_config_and_auth_errors_are_redacted() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let length = socket.read(&mut request).await.unwrap();
        assert!(
            std::str::from_utf8(&request[..length])
                .unwrap()
                .contains("fixture-api-key")
        );
        socket
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 24\r\n\r\nsecret-key transcript xx",
            )
            .await
            .unwrap();
        assert!(
            timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let mut wrong = cfg.clone();
    wrong.provider_config_id = Uuid::new_v4();
    assert!(matches!(
        provider.connect(wrong).await,
        Err(ProviderError::Authentication)
    ));
    let stream = provider.connect(cfg).await.unwrap();
    let terminal = event(&stream).await.unwrap();
    assert!(matches!(
        terminal,
        ProviderEvent::Failed {
            error: ProviderError::Authentication,
            ..
        }
    ));
    assert!(!format!("{terminal:?}").contains("secret-key"));
    assert!(!format!("{terminal:?}").contains("transcript"));
    server.await.unwrap();
}

#[tokio::test]
async fn capabilities_and_fixture_endpoint_fail_closed_before_network_io() {
    let (listener, provider, mut cfg, dir) = fixture().await;
    let caps = provider.capabilities();
    assert_eq!(caps.output_audio, NORMALIZED_OUTPUT_AUDIO);
    assert!(
        caps.language_pairs
            .contains(&LanguagePair::new("en", "ja").unwrap())
    );
    assert!(
        !caps
            .language_pairs
            .contains(&LanguagePair::new("de", "fr").unwrap())
    );
    cfg.languages = LanguagePair::new("de", "fr").unwrap();
    assert!(matches!(
        provider.connect(cfg.clone()).await,
        Err(ProviderError::UnsupportedLanguagePair)
    ));
    cfg.languages = LanguagePair::new("en", "zh").unwrap();
    cfg.target_audio = INPUT_AUDIO;
    assert!(matches!(
        provider.connect(cfg).await,
        Err(ProviderError::UnsupportedAudioFormat)
    ));
    assert!(
        timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
    let store = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    for url in [
        "ws://example.com/",
        "ws://192.0.2.1/",
        "ws://localhost/",
        "ws://key@127.0.0.1/",
    ] {
        assert!(matches!(
            VolcengineProvider::for_endpoint(store.clone(), url.into()),
            Err(ProviderError::Protocol)
        ));
    }
    let custom = provider.with_start_options(StartSessionOptions {
        speaker: Some(Speaker {
            id: "zh_female_vv_uranus_bigtts".into(),
            is_custom: false,
            tts_resource_id: "seed-tts-2.0".into(),
        }),
        corpus: None,
    });
    let caps = custom.capabilities();
    assert!(
        caps.language_pairs
            .contains(&LanguagePair::new("ko", "en").unwrap())
    );
    assert!(
        !caps
            .language_pairs
            .contains(&LanguagePair::new("en", "ja").unwrap())
    );
}

#[tokio::test]
async fn rejects_invalid_input_frames_without_sending_task_request() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        send_response(&mut socket, 150).await;
        assert_eq!(wire_event(&mut socket).await.event, 102);
        send_response(&mut socket, 152).await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    event(&stream).await.unwrap();
    let wrong = AudioFrame::new(
        AudioSource::Microphone,
        0,
        NORMALIZED_OUTPUT_AUDIO,
        vec![1; 80],
    )
    .unwrap();
    assert_eq!(
        stream.send_audio(wrong).await,
        Err(ProviderError::UnsupportedAudioFormat)
    );
    let huge = AudioFrame::new(AudioSource::Microphone, 0, INPUT_AUDIO, vec![1; 1281]).unwrap();
    assert_eq!(
        stream.send_audio(huge).await,
        Err(ProviderError::InvalidAudioFrame)
    );
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Finished { .. })
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn independent_directions_keep_their_own_stream_identity() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut workers = Vec::new();
        for _ in 0..2 {
            let mut socket = accept(&listener).await;
            workers.push(tokio::spawn(async move {
                wire_event(&mut socket).await;
                send_response(&mut socket, 150).await;
                assert_eq!(wire_event(&mut socket).await.event, 200);
                socket
                    .send(WsMessage::Binary(
                        response(655, 10, 20, "translation", vec![]).into(),
                    ))
                    .await
                    .unwrap();
                send_response(&mut socket, 152).await;
            }));
        }
        for worker in workers {
            worker.await.unwrap();
        }
    });
    let mut reverse = cfg.clone();
    reverse.session_id = Uuid::new_v4();
    reverse.direction = SessionDirection::SourceToTarget;
    let (first, second) = tokio::join!(
        provider.connect(cfg.clone()),
        provider.connect(reverse.clone())
    );
    let first = first.unwrap();
    let second = second.unwrap();
    event(&first).await.unwrap();
    event(&second).await.unwrap();
    first.send_audio(frame(1)).await.unwrap();
    second.send_audio(frame(2)).await.unwrap();
    for (stream, expected) in [(&first, &cfg), (&second, &reverse)] {
        let translated = event(stream).await.unwrap();
        assert_eq!(translated.metadata().session_id, expected.session_id);
        assert_eq!(translated.metadata().direction, expected.direction);
        assert!(translated.is_final_translation());
        assert!(matches!(
            event(stream).await,
            Some(ProviderEvent::Finished { .. })
        ));
    }
    server.await.unwrap();
}

#[test]
fn diagnostic_logid_rejects_unbounded_values_and_echoed_credentials() {
    assert_eq!(
        super::errors::safe_log_id("fixture-log", b"fixture-api-key"),
        Some("fixture-log".into())
    );
    for value in [
        "fixture-api-key",
        "request-fixture-api-key-trace",
        "spaces and transcript",
        "<script>",
        &"a".repeat(129),
    ] {
        assert_eq!(super::errors::safe_log_id(value, b"fixture-api-key"), None);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn continuous_input_cannot_starve_server_events() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        send_response(&mut socket, 150).await;
        assert_eq!(wire_event(&mut socket).await.event, 200);
        socket
            .send(WsMessage::Binary(
                response(654, 0, 0, "live translation", vec![]).into(),
            ))
            .await
            .unwrap();
        while let Some(Ok(_)) = socket.next().await {}
    });
    let stream: Arc<dyn ProviderConnection> = Arc::from(provider.connect(cfg).await.unwrap());
    assert!(
        stream
            .next_event()
            .await
            .unwrap()
            .unwrap()
            .is_session_started()
    );
    let producer_stream = stream.clone();
    let producer = tokio::spawn(async move {
        loop {
            match producer_stream.send_audio(frame(1)).await {
                Ok(()) => {}
                Err(ProviderError::QueueFull) => tokio::task::yield_now().await,
                Err(ProviderError::Closed) => break,
                other => panic!("unexpected admission {other:?}"),
            }
        }
    });
    let observed = timeout(Duration::from_millis(300), stream.next_event()).await;
    stream.cancel().await.unwrap();
    producer.await.unwrap();
    server.await.unwrap();
    assert!(matches!(
        observed,
        Ok(Ok(Some(ProviderEvent::TranslationInterim { .. })))
    ));
}

#[test]
fn tts_tail_uses_its_native_boundary_even_without_a_matching_start_event() {
    let mut mapper = VolcEventMapper::new(config());
    let text = mapper
        .map(&response(655, 500, 900, "translated", vec![]))
        .unwrap();
    // Missing or stale event350 must not override event351's explicit time.
    for start in [None, Some(100)] {
        if let Some(start) = start {
            mapper.map(&response(350, start, 0, "", vec![])).unwrap();
        }
        let tail = mapper
            .map(&response(351, 500, 900, "", floats(&[0.25])))
            .unwrap();
        assert_eq!(
            tail[0].metadata().timestamp,
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            tail[0].metadata().local_correlation_id,
            text[0].metadata().local_correlation_id
        );
        assert_eq!(tail[0].metadata().sentence_id, None);
    }
}

#[tokio::test]
async fn socket_backpressure_times_out_and_remains_cancellable() {
    let (listener, provider, cfg, _dir) = fixture().await;
    let (stop_tx, stop_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        wire_event(&mut socket).await;
        send_response(&mut socket, 150).await;
        // Retain the socket without reading; input eventually fills the real
        // TCP buffers. No peer close or handshake failure causes the gap.
        stop_rx.await.unwrap();
    });
    let stream: Arc<dyn ProviderConnection> =
        Arc::from(provider.with_test_timeouts().connect(cfg).await.unwrap());
    assert!(
        stream
            .next_event()
            .await
            .unwrap()
            .unwrap()
            .is_session_started()
    );
    let writer = stream.clone();
    let producer = tokio::spawn(async move {
        loop {
            match writer.send_audio(frame(123)).await {
                Ok(()) => {}
                Err(ProviderError::QueueFull) => tokio::task::yield_now().await,
                Err(ProviderError::NotStarted | ProviderError::Closed) => break,
                other => panic!("unexpected admission {other:?}"),
            }
        }
    });
    let gap = timeout(Duration::from_secs(3), stream.next_event()).await;
    // Cancellation also interrupts the retry's in-progress upgrade.
    stream.cancel().await.unwrap();
    producer.await.unwrap();
    stop_tx.send(()).unwrap();
    server.await.unwrap();
    assert!(matches!(gap, Ok(Ok(Some(ProviderEvent::Gap { .. })))));
    assert!(matches!(
        stream.next_event().await.unwrap(),
        Some(ProviderEvent::Failed {
            error: ProviderError::Cancelled,
            ..
        })
    ));
}

#[test]
fn absent_interim_timestamps_do_not_invent_or_reuse_completed_correlations() {
    let mut mapper = VolcEventMapper::new(config());
    let unbound = mapper
        .map(&response(651, 0, 0, "text only", vec![]))
        .unwrap();
    assert_eq!(unbound[0].metadata().timestamp, None);
    assert_eq!(unbound[0].metadata().local_correlation_id, None);
    mapper.map(&response(650, 100, 0, "", vec![])).unwrap();
    mapper
        .map(&response(652, 100, 300, "final", vec![]))
        .unwrap();
    let next = mapper
        .map(&response(651, 0, 0, "text only", vec![]))
        .unwrap();
    assert_eq!(next[0].metadata().local_correlation_id, None);
    mapper.map(&response(653, 640, 0, "", vec![])).unwrap();
    let explicit = mapper
        .map(&response(654, 800, 0, "explicit time", vec![]))
        .unwrap();
    assert_eq!(
        explicit[0].metadata().timestamp,
        Some(Duration::from_millis(800))
    );
}

#[tokio::test]
async fn transport_cannot_log_credentials_frames_or_remote_close_bodies() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct DetectUnsafeTransportLogging(AtomicUsize);
    impl log::Log for DetectUnsafeTransportLogging {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            // Do not retain or print the sensitive record even in the test.
            if (record.target().starts_with("tungstenite")
                || record.target().starts_with("tokio_tungstenite"))
                && record.level() > log::Level::Info
            {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        fn flush(&self) {}
    }
    static DETECTOR: DetectUnsafeTransportLogging =
        DetectUnsafeTransportLogging(AtomicUsize::new(0));
    log::set_logger(&DETECTOR).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let (listener, provider, cfg, _dir) = fixture().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        assert_eq!(wire_event(&mut socket).await.event, 100);
        send_response(&mut socket, 150).await;
        assert_eq!(wire_event(&mut socket).await.event, 200);
        assert_eq!(wire_event(&mut socket).await.event, 102);
        send_response(&mut socket, 152).await;
        let _ = socket.next().await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    assert!(event(&stream).await.unwrap().is_session_started());
    stream.send_audio(frame(42)).await.unwrap();
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Finished { .. })
    ));
    server.await.unwrap();
    assert_eq!(DETECTOR.0.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn tls_setup_failure_produces_one_terminal_event_instead_of_silent_actor_panic() {
    use tokio::io::AsyncWriteExt;
    let (listener, provider, cfg, _dir) = fixture_for_scheme("wss").await;
    let provider = provider.with_test_timeouts();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        // Exercise the real WSS connector, without a remote service or a
        // functioning TLS peer. Later retry attempts hit a closed listener.
        let _ = socket.write_all(b"not a TLS handshake").await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Disconnected,
            ..
        })
    ));
    assert!(event(&stream).await.is_none());
    assert_eq!(
        stream.send_audio(frame(1)).await,
        Err(ProviderError::Closed)
    );
    server.await.unwrap();
}

fn tls_peer(
    peer_name: &str,
) -> (
    rustls::pki_types::CertificateDer<'static>,
    tokio_rustls::TlsAcceptor,
) {
    // Ephemeral test-only key/certificate; no host keyring or checked-in key.
    let certified = rcgen::generate_simple_self_signed(vec![peer_name.into()]).unwrap();
    let certificate = certified.cert.der().clone();
    let private_key =
        rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![certificate.clone()], private_key.into())
    .unwrap();
    (
        certificate,
        tokio_rustls::TlsAcceptor::from(Arc::new(config)),
    )
}

#[tokio::test]
async fn tls_trusted_loopback_stream_uses_authenticated_upgrade_audio_and_finish() {
    let (listener, provider, cfg, _dir) = fixture_for_scheme("wss").await;
    let (certificate, acceptor) = tls_peer("127.0.0.1");
    let provider = provider.with_test_root(certificate).unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let tls = acceptor.accept(socket).await.unwrap();
        let mut socket = accept_socket(tls).await;
        assert_eq!(wire_event(&mut socket).await.event, 100);
        send_response(&mut socket, 150).await;
        let audio = wire_event(&mut socket).await;
        assert_eq!(audio.event, 200);
        assert_eq!(&audio.source_audio.unwrap().binary_data[..2], &[42, 0]);
        assert_eq!(wire_event(&mut socket).await.event, 102);
        send_response(&mut socket, 152).await;
        let _ = socket.next().await;
    });
    let stream = provider.connect(cfg).await.unwrap();
    assert!(event(&stream).await.unwrap().is_session_started());
    stream.send_audio(frame(42)).await.unwrap();
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&stream).await,
        Some(ProviderEvent::Finished { .. })
    ));
    assert!(event(&stream).await.is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn tls_rejects_unknown_roots_and_wrong_peer_names_with_terminal_failure() {
    for (peer_name, trust_root) in [("127.0.0.1", false), ("wrong.example", true)] {
        let (listener, mut provider, cfg, _dir) = fixture_for_scheme("wss").await;
        let (certificate, acceptor) = tls_peer(peer_name);
        if trust_root {
            provider = provider.with_test_root(certificate).unwrap();
        }
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (socket, _) = timeout(Duration::from_secs(3), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
                // Validation rejects the peer before sending upgrade headers
                // containing credentials, with or without a trusted issuer.
                assert!(acceptor.accept(socket).await.is_err());
            }
        });
        let stream = provider.connect(cfg).await.unwrap();
        assert!(matches!(
            event(&stream).await,
            Some(ProviderEvent::Failed {
                error: ProviderError::Disconnected,
                ..
            })
        ));
        assert!(event(&stream).await.is_none());
        assert_eq!(
            stream.send_audio(frame(1)).await,
            Err(ProviderError::Closed)
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn tls_fixture_root_injection_cannot_change_a_remote_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let (certificate, _) = tls_peer("127.0.0.1");
    assert!(matches!(
        VolcengineProvider::new(secrets).with_test_root(certificate),
        Err(ProviderError::Protocol)
    ));
}

#[tokio::test]
async fn blocked_credential_work_is_bounded_across_fresh_provider_instances() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let gate = Arc::new(tokio::sync::Semaphore::new(2));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let lookup = {
        let release = release.clone();
        let active = active.clone();
        let maximum = maximum.clone();
        Arc::new(move |_secrets: Arc<SecretStore>, _key: String| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(current, Ordering::SeqCst);
            let (lock, changed) = &*release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(Some(b"fixture-api-key".to_vec()))
        })
    };
    let providers = (0..3)
        .map(|_| {
            VolcengineProvider::new(secrets.clone())
                .with_test_timeouts()
                .with_test_credential_lookup(gate.clone(), lookup.clone())
        })
        .collect::<Vec<_>>();
    let first = providers[0].connect(config());
    let second = providers[1].connect(config());
    let third = providers[2].connect(config());
    let results = tokio::join!(first, second, third);
    assert!(matches!(results.0, Err(ProviderError::Timeout)));
    assert!(matches!(results.1, Err(ProviderError::Timeout)));
    assert!(matches!(results.2, Err(ProviderError::Timeout)));
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 2);
    {
        let (lock, changed) = &*release;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
