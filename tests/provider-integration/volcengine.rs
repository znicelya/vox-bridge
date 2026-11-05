//! Explicitly opted-in service acceptance; never run by the default test suite.
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};
use tokio::{
    sync::oneshot,
    time::{Instant, sleep_until, timeout},
};
use uuid::Uuid;
use vox_bridge_lib::{
    audio::{AudioFrame, PcmSpec},
    config::{ProviderConfig, SecretStore},
    domain::{
        audio::AudioSource,
        session::{LanguagePair, SessionConfig, SessionDirection},
    },
    providers::{
        ProviderEvent,
        factory::{ProviderFactory, volcengine_secret_reference},
        volc::client::{ENDPOINT, SECRET_SERVICE, secret_key},
    },
};

const SPEECH: &[u8] = include_bytes!("fixtures/hello-en-16k-s16le.pcm");
const INPUT: PcmSpec = PcmSpec::pcm16_mono(16_000);
const OUTPUT: PcmSpec = PcmSpec::pcm16_mono(24_000);

#[test]
#[ignore = "paid Volcengine request; explicit operator key and native keyring required"]
fn real_volcengine_translates_two_seconds_of_speech() {
    // Covers native keyring/driver stalls as well as async networking. This is
    // an isolated opt-in test target; the OS closes streams on watchdog exit.
    let (deadline, watchdog) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if matches!(
            watchdog.recv_timeout(Duration::from_secs(60)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) {
            eprintln!("Volcengine acceptance exceeded its 60-second process deadline");
            std::process::exit(124);
        }
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create acceptance runtime");
    let result = runtime.block_on(timeout(Duration::from_secs(45), accept_speech()));
    // Drop runtime before disarming watchdog: a stalled blocking native keyring
    // operation must not leave this process running forever during shutdown.
    drop(runtime);
    drop(deadline);
    match result {
        Ok(Ok(())) => {}
        Ok(Err(stage)) => panic!("Volcengine acceptance failed: {stage}"),
        Err(_) => panic!("Volcengine acceptance timed out"),
    }
}

async fn accept_speech() -> Result<(), &'static str> {
    let key = std::env::var("VOX_VOLC_API_KEY").map_err(|_| "VOX_VOLC_API_KEY is required")?;
    if key.trim().is_empty() {
        return Err("VOX_VOLC_API_KEY is empty");
    }
    if SPEECH.len() != 64_000 {
        return Err("speech fixture is not exactly two seconds");
    }
    let dir = tempfile::tempdir().map_err(|_| "temporary data directory")?;
    let secrets =
        Arc::new(SecretStore::new(dir.path()).map_err(|_| "secret-store initialization")?);
    let config_id = Uuid::new_v4();
    secrets
        .set(SECRET_SERVICE, &secret_key(config_id), key.as_bytes())
        .map_err(|_| "native encrypted credential storage")?;
    drop(key);
    let saved = ProviderConfig::new(
        config_id,
        "volcengine",
        ENDPOINT,
        None,
        None,
        vec![],
        Some(volcengine_secret_reference(config_id)),
    )
    .map_err(|_| "provider settings")?;
    let session = SessionConfig::live(
        LanguagePair::new("en", "zh").map_err(|_| "language pair")?,
        SessionDirection::SourceToTarget,
    )
    .map_err(|_| "session settings")?;
    let prepared = ProviderFactory::new(secrets)
        .prepare(&saved, Uuid::new_v4(), &session, INPUT, OUTPUT)
        .map_err(|_| "provider preflight")?;
    let mut streams = prepared.connect().await.map_err(|_| "provider opening")?;
    let directed = streams.pop().ok_or("provider stream missing")?;
    let stream = directed.stream;
    let (started_tx, started_rx) = oneshot::channel();
    let sender = async {
        started_rx.await.map_err(|_| "handshake not completed")?;
        let origin = Instant::now();
        for (index, bytes) in SPEECH.chunks_exact(2_560).enumerate() {
            sleep_until(origin + Duration::from_millis(index as u64 * 80)).await;
            let samples = bytes
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
            let frame = AudioFrame::new(AudioSource::Microphone, index as u64 + 1, INPUT, samples)
                .map_err(|_| "speech PCM framing")?;
            stream
                .send_audio(frame)
                .await
                .map_err(|_| "speech submission")?;
        }
        sleep_until(origin + Duration::from_secs(2)).await;
        stream.finish().await.map_err(|_| "finish request")
    };
    let receiver = async {
        let mut started_tx = Some(started_tx);
        let (mut source, mut translation, mut audio) = (false, false, false);
        loop {
            match stream
                .next_event()
                .await
                .map_err(|_| "provider event receive")?
            {
                Some(ProviderEvent::SessionStarted { .. }) => {
                    if let Some(tx) = started_tx.take() {
                        let _ = tx.send(());
                    }
                }
                Some(ProviderEvent::SourceFinal { text, .. }) => {
                    source |= !text.translation.text.trim().is_empty()
                }
                Some(ProviderEvent::TranslationFinal { text, .. }) => {
                    translation |= !text.translation.text.trim().is_empty()
                }
                Some(ProviderEvent::Audio { frame, .. }) => {
                    if frame.spec() != OUTPUT || frame.samples().is_empty() {
                        return Err("invalid normalized TTS audio");
                    }
                    audio = true;
                }
                Some(ProviderEvent::Finished { .. }) => {
                    return if source && translation && audio {
                        Ok(())
                    } else {
                        Err("missing final source, translation or TTS evidence")
                    };
                }
                Some(ProviderEvent::Failed { .. }) => return Err("classified provider failure"),
                Some(ProviderEvent::Gap { .. }) => return Err("audio continuity lost"),
                None => return Err("provider closed without successful completion"),
                _ => {}
            }
        }
    };
    let result = tokio::try_join!(sender, receiver).map(|_| ());
    if result.is_err() {
        let _ = stream.cancel().await;
    }
    result
}
