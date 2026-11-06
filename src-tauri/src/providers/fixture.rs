//! Deterministic in-process protocol fixture; no networking or audio devices.

use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use tokio::{
    sync::{Mutex as AsyncMutex, mpsc, watch},
    task::JoinHandle,
};
use uuid::Uuid;

use super::*;
use crate::{
    audio::frame::{AudioFrame, PcmSpec},
    domain::{
        session::{LanguagePair, ProviderId, SessionDirection},
        translation::{TimedTranslation, Translation},
    },
};

pub const FIXTURE_CAPACITY: usize = 64;
const MAX_EVENT_BYTES: usize = 65_536;
const INPUT_SAMPLES: usize = 1280;

#[derive(Clone, Debug)]
pub enum FixtureStep {
    Emit(ProviderEvent),
    /// Repetition retains one bounded template, never a pre-expanded event list.
    EmitRepeated {
        event: ProviderEvent,
        count: usize,
    },
    ReceiveAudio {
        count: usize,
    },
    WaitForFinish,
    Delay(Duration),
    Disconnect,
}

#[derive(Clone, Debug)]
pub struct FixtureScript {
    output_audio: PcmSpec,
    steps: Vec<FixtureStep>,
}

impl FixtureScript {
    /// At most 64 instructions, each retaining at most 64 KiB of payload.
    /// Stops pulling from the iterator at instruction 65, including infinite ones.
    pub fn new(steps: impl IntoIterator<Item = FixtureStep>) -> ProviderResult<Self> {
        Self::with_output_audio(PcmSpec::pcm16_mono(16_000), steps)
    }

    pub fn with_output_audio(
        output_audio: PcmSpec,
        steps: impl IntoIterator<Item = FixtureStep>,
    ) -> ProviderResult<Self> {
        if !matches!(output_audio.sample_rate(), 16_000 | 24_000)
            || output_audio != PcmSpec::pcm16_mono(output_audio.sample_rate())
        {
            return Err(ProviderError::InvalidScript);
        }
        let mut bounded = Vec::new();
        for step in steps {
            if bounded.len() == FIXTURE_CAPACITY {
                return Err(ProviderError::InvalidScript);
            }
            if let FixtureStep::Emit(event) | FixtureStep::EmitRepeated { event, .. } = &step {
                validate_event_size(event, output_audio)?;
            }
            bounded.push(step);
        }
        Ok(Self {
            output_audio,
            steps: bounded,
        })
    }

    pub fn started_then_final_text() -> Self {
        let metadata = ProviderEventMetadata {
            session_id: Uuid::nil(),
            direction: SessionDirection::SourceToTarget,
            provider_event_type: None,
            timestamp: None,
            sentence_id: None,
            local_correlation_id: None,
            log_id: None,
        };
        Self::new([
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata.clone(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(ProviderEvent::TranslationFinal {
                metadata,
                text: TimedTranslation {
                    translation: Translation {
                        text: "你好".into(),
                        language: "zh".into(),
                    },
                    start: Some(Duration::ZERO),
                    end: Some(Duration::from_millis(80)),
                },
            }),
        ])
        .expect("built-in fixture is bounded")
    }
}

fn validate_event_size(event: &ProviderEvent, output_audio: PcmSpec) -> ProviderResult<()> {
    let metadata = event.metadata();
    let mut bytes = metadata
        .provider_event_type
        .as_ref()
        .map_or(0, String::len)
        .saturating_add(metadata.sentence_id.as_ref().map_or(0, String::len))
        .saturating_add(metadata.log_id.as_ref().map_or(0, String::len));
    match event {
        ProviderEvent::SourceInterim { text, .. }
        | ProviderEvent::SourceFinal { text, .. }
        | ProviderEvent::TranslationInterim { text, .. }
        | ProviderEvent::TranslationFinal { text, .. } => {
            bytes = bytes
                .saturating_add(text.translation.text.len())
                .saturating_add(text.translation.language.len());
        }
        ProviderEvent::Audio { frame, .. } => {
            let max_samples = usize::try_from(output_audio.sample_rate() / 1_000 * 80)
                .map_err(|_| ProviderError::InvalidScript)?;
            if frame.spec() != output_audio || frame.samples().len() > max_samples {
                return Err(ProviderError::InvalidScript);
            }
            bytes = bytes.saturating_add(frame.samples().len().saturating_mul(2));
        }
        _ => {}
    }
    if bytes > MAX_EVENT_BYTES {
        return Err(ProviderError::InvalidScript);
    }
    Ok(())
}

pub struct FixtureProvider(FixtureScript);

impl FixtureProvider {
    pub fn new(script: FixtureScript) -> Self {
        Self(script)
    }
}

#[async_trait]
impl TranslationProvider for FixtureProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("fixture").expect("static provider ID")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            language_pairs: vec![
                LanguagePair::new("en", "zh").unwrap(),
                LanguagePair::new("zh", "en").unwrap(),
            ],
            input_audio: PcmSpec::pcm16_mono(16_000),
            output_audio: self.0.output_audio,
            native_chunk_duration: Duration::from_millis(80),
            supports_tts: true,
            supports_bidirectional: true,
            ready: true,
            unavailable_reason: None,
        }
    }

    async fn connect(&self, config: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        self.capabilities().validate(&config)?;
        let (audio_tx, audio_rx) = mpsc::channel(FIXTURE_CAPACITY);
        let (events_tx, events_rx) = mpsc::channel(FIXTURE_CAPACITY);
        let (control_tx, control_rx) = watch::channel(Control::Running);
        let state = Arc::new(Mutex::new(State::default()));
        let actor = Actor {
            state: state.clone(),
            config: config.clone(),
            audio: audio_rx,
            events: events_tx,
            control: control_rx,
        };
        let script = self.0.clone();
        let task = tokio::spawn(actor.run(script));
        Ok(Box::new(FixtureConnection {
            state,
            config,
            audio: audio_tx,
            events: AsyncMutex::new(events_rx),
            control: control_tx,
            task,
        }))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Running,
    Finish,
    Cancel,
}

#[derive(Default)]
struct State {
    started: bool,
    input_closed: bool,
    ended: bool,
    terminal: Option<ProviderEvent>,
}

impl State {
    fn close(&mut self, terminal: ProviderEvent) {
        if !self.ended {
            self.input_closed = true;
            self.ended = true;
            self.terminal = Some(terminal);
        }
    }
}

struct FixtureConnection {
    state: Arc<Mutex<State>>,
    config: ProviderSessionConfig,
    audio: mpsc::Sender<AudioFrame>,
    events: AsyncMutex<mpsc::Receiver<ProviderEvent>>,
    control: watch::Sender<Control>,
    task: JoinHandle<()>,
}

impl Drop for FixtureConnection {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[async_trait]
impl ProviderConnection for FixtureConnection {
    async fn send_audio(&self, frame: AudioFrame) -> ProviderResult<()> {
        let state = self.state.lock().unwrap();
        if state.input_closed {
            return Err(ProviderError::Closed);
        }
        if !state.started {
            return Err(ProviderError::NotStarted);
        }
        if frame.spec() != PcmSpec::pcm16_mono(16_000) {
            return Err(ProviderError::UnsupportedAudioFormat);
        }
        if frame.samples().len() > INPUT_SAMPLES {
            return Err(ProviderError::InvalidAudioFrame);
        }
        self.audio.try_send(frame).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => ProviderError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => ProviderError::Closed,
        })
    }

    async fn finish(&self) -> ProviderResult<()> {
        let mut state = self.state.lock().unwrap();
        if !state.input_closed {
            state.input_closed = true;
            self.control.send_replace(Control::Finish);
        }
        Ok(())
    }

    async fn cancel(&self) -> ProviderResult<()> {
        let mut state = self.state.lock().unwrap();
        if !state.ended {
            state.close(ProviderEvent::Failed {
                metadata: local_metadata(&self.config),
                error: ProviderError::Cancelled,
            });
            self.control.send_replace(Control::Cancel);
        }
        Ok(())
    }

    async fn next_event(&self) -> ProviderResult<Option<ProviderEvent>> {
        let mut events = self.events.lock().await;
        if let Some(event) = events.recv().await {
            return Ok(Some(event));
        }
        Ok(self.state.lock().unwrap().terminal.take())
    }
}

fn local_metadata(config: &ProviderSessionConfig) -> ProviderEventMetadata {
    ProviderEventMetadata {
        session_id: config.session_id,
        direction: config.direction,
        provider_event_type: None,
        timestamp: None,
        sentence_id: None,
        local_correlation_id: None,
        log_id: None,
    }
}

struct Actor {
    state: Arc<Mutex<State>>,
    config: ProviderSessionConfig,
    audio: mpsc::Receiver<AudioFrame>,
    events: mpsc::Sender<ProviderEvent>,
    control: watch::Receiver<Control>,
}

impl Actor {
    async fn run(mut self, script: FixtureScript) {
        for step in script.steps {
            if *self.control.borrow() == Control::Cancel {
                return;
            }
            match step {
                FixtureStep::Emit(event) => {
                    if !self.emit(event).await {
                        return;
                    }
                }
                FixtureStep::EmitRepeated { event, count } => {
                    for _ in 0..count {
                        if !self.emit(event.clone()).await {
                            return;
                        }
                    }
                }
                FixtureStep::ReceiveAudio { count } => {
                    for _ in 0..count {
                        if self.receive_audio().await.is_none() {
                            break;
                        }
                    }
                }
                FixtureStep::WaitForFinish => {
                    while *self.control.borrow_and_update() == Control::Running {
                        if self.control.changed().await.is_err() {
                            return;
                        }
                    }
                }
                FixtureStep::Delay(duration) => {
                    if unless_cancelled(&mut self.control, tokio::time::sleep(duration))
                        .await
                        .is_none()
                    {
                        return;
                    }
                }
                FixtureStep::Disconnect => {
                    self.fail(ProviderError::Disconnected);
                    return;
                }
            }
        }
        self.emit(ProviderEvent::Finished {
            metadata: local_metadata(&self.config),
        })
        .await;
    }

    async fn receive_audio(&mut self) -> Option<AudioFrame> {
        loop {
            match *self.control.borrow_and_update() {
                Control::Cancel => return None,
                Control::Finish => return self.audio.try_recv().ok(),
                Control::Running => {}
            }
            tokio::select! {
                biased;
                changed = self.control.changed() => if changed.is_err() { return None; },
                audio = self.audio.recv() => return audio,
            }
        }
    }

    async fn emit(&mut self, mut event: ProviderEvent) -> bool {
        // Scripts are templates; session/direction correlation belongs to this
        // connection. Native type, provider timing and wire sentence ID survive.
        event.metadata_mut().session_id = self.config.session_id;
        event.metadata_mut().direction = self.config.direction;
        {
            let mut state = self.state.lock().unwrap();
            if state.ended {
                return false;
            }
            if event.is_session_started() {
                if state.started {
                    drop(state);
                    self.fail(ProviderError::Protocol);
                    return false;
                }
                state.started = true;
            } else if matches!(event, ProviderEvent::Gap { .. }) {
                if !state.started {
                    drop(state);
                    self.fail(ProviderError::Protocol);
                    return false;
                }
                state.started = false;
                while self.audio.try_recv().is_ok() {}
            } else if matches!(event, ProviderEvent::Failed { .. }) {
                state.close(event);
                return false;
            } else if !state.started {
                drop(state);
                self.fail(ProviderError::Protocol);
                return false;
            } else if matches!(event, ProviderEvent::Finished { .. }) {
                state.close(event);
                return false;
            }
        }
        matches!(
            unless_cancelled(&mut self.control, self.events.send(event)).await,
            Some(Ok(()))
        )
    }

    fn fail(&self, error: ProviderError) {
        self.state.lock().unwrap().close(ProviderEvent::Failed {
            metadata: local_metadata(&self.config),
            error,
        });
    }
}

/// Cancellation interrupts delays and blocked output sends; graceful finish
/// keeps the scripted final events and never competes for an audio queue slot.
async fn unless_cancelled<T>(
    control: &mut watch::Receiver<Control>,
    future: impl Future<Output = T>,
) -> Option<T> {
    tokio::pin!(future);
    loop {
        if *control.borrow_and_update() == Control::Cancel {
            return None;
        }
        tokio::select! {
            biased;
            changed = control.changed() => if changed.is_err() { return None; },
            result = &mut future => return Some(result),
        }
    }
}
