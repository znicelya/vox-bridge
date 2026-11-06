//! One bounded transport actor per directed provider stream.
use super::{
    errors::{safe_log_id, transport_error},
    events::{MAX_MESSAGE_BYTES, VolcEventMapper},
    protocol::*,
};
use crate::{
    audio::frame::AudioFrame,
    config::SecretStore,
    domain::session::{LanguagePair, ProviderId},
    error::AppResult,
    providers::*,
};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{Mutex as AsyncMutex, Semaphore, mpsc, watch},
    task::JoinHandle,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
    tungstenite::{
        Message, client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig,
    },
};
use uuid::Uuid;

pub const ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v4/ast/v2/translate";
pub const SECRET_SERVICE: &str = "com.voxbridge.providers.volcengine";
/// Backend configuration UUID selects its own encrypted API-key entry.
pub fn secret_key(id: Uuid) -> String {
    format!("{id}/api-key")
}
const CAPACITY: usize = 64;
const ATTEMPTS: usize = 3;
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type SecretLookup =
    Arc<dyn Fn(Arc<SecretStore>, String) -> AppResult<Option<Vec<u8>>> + Send + Sync>;

fn credential_gate() -> Arc<Semaphore> {
    static GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    GATE.get_or_init(|| Arc::new(Semaphore::new(2))).clone()
}

#[derive(Clone)]
struct Timing {
    connect: Duration,
    handshake: Duration,
    write: Duration,
    finish: Duration,
    retry: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            handshake: Duration::from_secs(10),
            write: Duration::from_secs(5),
            finish: Duration::from_secs(10),
            retry: Duration::from_millis(250),
        }
    }
}

pub struct VolcengineProvider {
    secrets: Arc<SecretStore>,
    credential_gate: Arc<Semaphore>,
    endpoint: String,
    options: StartSessionOptions,
    timing: Timing,
    #[cfg(test)]
    test_roots: Option<rustls::RootCertStore>,
    #[cfg(test)]
    test_secret_lookup: Option<SecretLookup>,
}
impl VolcengineProvider {
    pub fn new(secrets: Arc<SecretStore>) -> Self {
        Self {
            secrets,
            credential_gate: credential_gate(),
            endpoint: ENDPOINT.into(),
            options: StartSessionOptions::default(),
            timing: Timing::default(),
            #[cfg(test)]
            test_roots: None,
            #[cfg(test)]
            test_secret_lookup: None,
        }
    }
    pub fn with_start_options(mut self, options: StartSessionOptions) -> Self {
        self.options = options;
        self
    }
    fn tls_connector(&self) -> ProviderResult<Connector> {
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        #[cfg(test)]
        let roots = self.test_roots.clone().unwrap_or(roots);
        // Select per configuration, never through Rustls's process-wide
        // provider inference (which can panic with zero or multiple backends).
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| ProviderError::Protocol)?
        .with_root_certificates(roots)
        .with_no_client_auth();
        Ok(Connector::Rustls(Arc::new(config)))
    }
    /// Compiled only for tests; unencrypted transport is restricted to literal
    /// loopback addresses, so production credentials cannot reach remote WS.
    #[cfg(test)]
    pub(super) fn for_endpoint(
        secrets: Arc<SecretStore>,
        endpoint: String,
    ) -> ProviderResult<Self> {
        let url = url::Url::parse(&endpoint).map_err(|_| ProviderError::Protocol)?;
        let local = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if !matches!(url.scheme(), "ws" | "wss")
            || !local
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(ProviderError::Protocol);
        }
        Ok(Self {
            endpoint,
            ..Self::new(secrets)
        })
    }
    #[cfg(test)]
    pub(super) fn with_test_root(
        mut self,
        certificate: rustls::pki_types::CertificateDer<'static>,
    ) -> ProviderResult<Self> {
        // Reuse the literal-loopback guard; a test trust root cannot be added
        // to the production endpoint or any arbitrary remote host.
        Self::for_endpoint(self.secrets.clone(), self.endpoint.clone())?;
        if !self.endpoint.starts_with("wss://") {
            return Err(ProviderError::Protocol);
        }
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(certificate)
            .map_err(|_| ProviderError::Protocol)?;
        self.test_roots = Some(roots);
        Ok(self)
    }
    #[cfg(test)]
    pub(super) fn with_test_timeouts(mut self) -> Self {
        self.timing = Timing {
            connect: Duration::from_millis(100),
            handshake: Duration::from_millis(100),
            write: Duration::from_millis(100),
            finish: Duration::from_millis(150),
            retry: Duration::from_millis(10),
        };
        self
    }
    #[cfg(test)]
    pub(crate) fn with_test_credential_lookup(
        mut self,
        gate: Arc<Semaphore>,
        lookup: SecretLookup,
    ) -> Self {
        self.credential_gate = gate;
        self.test_secret_lookup = Some(lookup);
        self
    }

    fn secret_lookup(&self) -> SecretLookup {
        #[cfg(test)]
        if let Some(lookup) = &self.test_secret_lookup {
            return lookup.clone();
        }
        Arc::new(|secrets, key| secrets.get(SECRET_SERVICE, &key))
    }
}
#[async_trait]
impl TranslationProvider for VolcengineProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("volcengine").expect("static provider ID")
    }
    fn capabilities(&self) -> ProviderCapabilities {
        // Reuse the protocol validator, including the selected speaker rules,
        // so advertised pairs and the actual StartSession cannot diverge.
        let languages = [
            "zh", "en", "de", "fr", "es", "id", "ja", "pt", "ko", "tr", "ms", "nl", "ro", "pl",
            "cs", "ar", "th", "vi", "ru", "it",
        ];
        let mut language_pairs = Vec::new();
        for source in languages {
            for target in languages {
                let pair = LanguagePair::new(source, target).expect("static languages");
                let config = ProviderSessionConfig {
                    session_id: Uuid::from_u128(1),
                    provider_config_id: Uuid::nil(),
                    direction: crate::domain::session::SessionDirection::SourceToTarget,
                    mode: crate::domain::session::SessionMode::Live,
                    languages: pair.clone(),
                    target_audio: NORMALIZED_OUTPUT_AUDIO,
                };
                if encode_start_session(&config, &self.options).is_ok() {
                    language_pairs.push(pair);
                }
            }
        }
        ProviderCapabilities {
            language_pairs,
            input_audio: INPUT_AUDIO,
            output_audio: NORMALIZED_OUTPUT_AUDIO,
            native_chunk_duration: NATIVE_INPUT_CHUNK_DURATION,
            supports_tts: true,
            supports_bidirectional: true,
            ready: true,
            unavailable_reason: None,
        }
    }
    /// Returns a cancellable stream before network I/O. The stream emits150
    /// only after upgrade + StartSession; subsequent connection failures are
    /// terminal Failed events. Missing/invalid credentials fail this call.
    async fn connect(&self, config: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        self.connect_owned(config, Arc::new(())).await
    }
    fn supports_owned_cleanup(&self) -> bool {
        true
    }
    async fn connect_owned(
        &self,
        config: ProviderSessionConfig,
        owner: Arc<dyn Send + Sync>,
    ) -> ProviderResult<ProviderStream> {
        self.capabilities().validate(&config)?;
        let start = encode_start_session(&config, &self.options)?;
        let secrets = self.secrets.clone();
        let key = secret_key(config.provider_config_id);
        let gate = self.credential_gate.clone();
        let lookup = self.secret_lookup();
        let lookup_owner = owner.clone();
        let api_key = timeout(self.timing.connect, async move {
            let permit = gate
                .acquire_owned()
                .await
                .map_err(|_| ProviderError::Timeout)?;
            tokio::task::spawn_blocking(move || {
                let (_permit, _owner) = (permit, lookup_owner);
                lookup(secrets, key)
            })
            .await
            .map_err(|_| ProviderError::Authentication)?
            .map_err(|_| ProviderError::Authentication)
        })
        .await
        .map_err(|_| ProviderError::Timeout)??
        .filter(|v| !v.is_empty())
        .ok_or(ProviderError::Authentication)?;
        HeaderValue::from_bytes(&api_key).map_err(|_| ProviderError::Authentication)?;
        let (audio_tx, audio_rx) = mpsc::channel(CAPACITY);
        let (events_tx, events_rx) = mpsc::channel(CAPACITY);
        let (control_tx, control_rx) = watch::channel(Control::Running);
        let state = Arc::new(Mutex::new(State::default()));
        let mapper = VolcEventMapper::new(config.clone());
        let actor = Actor {
            state: state.clone(),
            endpoint: self.endpoint.clone(),
            connector: self.tls_connector()?,
            api_key,
            start,
            timing: self.timing.clone(),
            mapper,
            audio: audio_rx,
            events: events_tx,
            control: control_rx,
            pending: VecDeque::new(),
        };
        let task = tokio::spawn(async move {
            let _owner = owner;
            actor.run().await
        });
        let abort = task.abort_handle();
        Ok(Box::new(Connection {
            state,
            config,
            audio: audio_tx,
            events: AsyncMutex::new(events_rx),
            control: control_tx,
            task: AsyncMutex::new(Some(task)),
            abort,
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
            self.started = false;
            self.input_closed = true;
            self.ended = true;
            self.terminal = Some(terminal);
        }
    }
}
struct Connection {
    state: Arc<Mutex<State>>,
    config: ProviderSessionConfig,
    audio: mpsc::Sender<AudioFrame>,
    events: AsyncMutex<mpsc::Receiver<ProviderEvent>>,
    control: watch::Sender<Control>,
    task: AsyncMutex<Option<JoinHandle<()>>>,
    abort: tokio::task::AbortHandle,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
#[async_trait]
impl ProviderConnection for Connection {
    async fn send_audio(&self, frame: AudioFrame) -> ProviderResult<()> {
        let state = self.state.lock().unwrap();
        if state.input_closed {
            return Err(ProviderError::Closed);
        }
        if !state.started {
            return Err(ProviderError::NotStarted);
        }
        if frame.spec() != INPUT_AUDIO {
            return Err(ProviderError::UnsupportedAudioFormat);
        }
        if frame.samples().len() > MAX_INPUT_AUDIO_BYTES / 2 {
            return Err(ProviderError::InvalidAudioFrame);
        }
        self.audio.try_send(frame).map_err(|e| match e {
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
                metadata: VolcEventMapper::new(self.config.clone()).metadata(),
                error: ProviderError::Cancelled,
            });
            self.control.send_replace(Control::Cancel);
            // Abort also interrupts connect, TLS, writes and a blocked output
            // queue without waiting for any socket or actor-held async lock.
            self.abort.abort();
        }
        Ok(())
    }
    async fn wait_closed(&self) -> ProviderResult<()> {
        let mut task = self.task.lock().await;
        if let Some(handle) = task.as_mut() {
            let _ = handle.await;
        }
        *task = None;
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
struct Actor {
    state: Arc<Mutex<State>>,
    endpoint: String,
    connector: Connector,
    api_key: Vec<u8>,
    start: Vec<u8>,
    timing: Timing,
    mapper: VolcEventMapper,
    audio: mpsc::Receiver<AudioFrame>,
    events: mpsc::Sender<ProviderEvent>,
    control: watch::Receiver<Control>,
    pending: VecDeque<ProviderEvent>,
}
impl Actor {
    async fn run(mut self) {
        let mut remaining = ATTEMPTS;
        let mut attempt = 0;
        let mut gap_pending = false;
        loop {
            remaining -= 1;
            let result = self.connection().await;
            match result {
                Ok(terminal) => {
                    self.state.lock().unwrap().close(terminal);
                    return;
                }
                Err(error) => {
                    let started = {
                        let mut state = self.state.lock().unwrap();
                        let started = state.started;
                        state.started = false;
                        started
                    };
                    // AST does not guarantee per-frame acknowledgement. Every
                    // popped frame is uncertain; never put it back in a queue.
                    while self.audio.try_recv().is_ok() {}
                    if started {
                        remaining = ATTEMPTS;
                        attempt = 0;
                        gap_pending = true;
                    }
                    if !matches!(error, ProviderError::Disconnected | ProviderError::Timeout)
                        || *self.control.borrow() != Control::Running
                        || remaining == 0
                    {
                        self.fail(error);
                        return;
                    }
                    if gap_pending {
                        self.pending.push_back(ProviderEvent::Gap {
                            metadata: self.mapper.metadata(),
                        });
                        gap_pending = false;
                        // Publish the continuity break before sleeping, so a
                        // finish during backoff still observes the gap.
                        self.flush_pending();
                    }
                    self.mapper.reset_connection();
                    let delay = retry_delay(self.timing.retry, attempt);
                    attempt += 1;
                    tokio::select! {
                        biased;
                        _=self.control.changed()=>{self.fail(ProviderError::Disconnected);return;},
                        _=tokio::time::sleep(delay)=>{},
                    }
                }
            }
        }
    }
    async fn connection(&mut self) -> ProviderResult<ProviderEvent> {
        let mut request = self
            .endpoint
            .as_str()
            .into_client_request()
            .map_err(|_| ProviderError::Protocol)?;
        let mut api_key =
            HeaderValue::from_bytes(&self.api_key).map_err(|_| ProviderError::Authentication)?;
        api_key.set_sensitive(true);
        request.headers_mut().insert("X-Api-Key", api_key);
        request.headers_mut().insert(
            "X-Api-Resource-Id",
            HeaderValue::from_static("volc.service_type.10053"),
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));
        let (mut ws, response) = timeout(
            self.timing.connect,
            connect_async_tls_with_config(
                request,
                Some(config),
                true,
                Some(self.connector.clone()),
            ),
        )
        .await
        .map_err(|_| ProviderError::Timeout)?
        .map_err(transport_error)?;
        self.mapper.set_log_id(
            response
                .headers()
                .get("X-Tt-Logid")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| safe_log_id(value, &self.api_key)),
        );
        write(
            &mut ws,
            Message::Binary(self.start.clone().into()),
            self.timing.write,
        )
        .await?;
        let mut started = false;
        let mut finish_sent = false;
        let mut deadline = Some(Instant::now() + self.timing.handshake);
        loop {
            let control = *self.control.borrow_and_update();
            if control == Control::Cancel {
                return Err(ProviderError::Cancelled);
            }
            if started && control == Control::Finish && !finish_sent && self.audio.is_empty() {
                write(
                    &mut ws,
                    Message::Binary(encode_finish_session().into()),
                    self.timing.write,
                )
                .await?;
                finish_sent = true;
                deadline = Some(Instant::now() + self.timing.finish);
            }
            tokio::select! {
                biased;
                changed=self.control.changed()=>if changed.is_err(){return Err(ProviderError::Cancelled);},
                _=wait_deadline(deadline)=>return Err(ProviderError::Timeout),
                permit=self.events.reserve(), if !self.pending.is_empty()=>{
                    permit.map_err(|_|ProviderError::Cancelled)?.send(self.pending.pop_front().unwrap());
                }
                input=self.audio.recv(), if started && !finish_sent=>{
                    let frame=input.ok_or(ProviderError::Closed)?;
                    let pcm:Vec<u8>=frame.samples().iter().flat_map(|s|s.to_le_bytes()).collect();
                    write(&mut ws,Message::Binary(encode_task_request(&pcm)?.into()),self.timing.write).await?;
                }
                message=ws.next(), if self.pending.is_empty()=>{
                    match message.ok_or(ProviderError::Disconnected)?.map_err(transport_error)? {
                        Message::Binary(bytes)=>{
                            for event in self.mapper.map(&bytes)? {
                                match event {
                                    ProviderEvent::SessionStarted{..}=>{
                                        if started{return Err(ProviderError::Protocol);}
                                        started=true;deadline=None;
                                        self.state.lock().unwrap().started=true;
                                        self.pending.push_back(event);
                                    }
                                    ProviderEvent::Failed{..}=>return Ok(event),
                                    ProviderEvent::Finished{..}=>{
                                        if !started{return Err(ProviderError::Protocol);}
                                        // Closing the transport is also bounded. The peer's
                                        // event152 is the completion acknowledgement.
                                        let _=timeout(self.timing.write,ws.close(None)).await;
                                        return Ok(event);
                                    }
                                    _=>{
                                        if !started{return Err(ProviderError::Protocol);}
                                        self.pending.push_back(event);
                                    }
                                }
                            }
                        }
                        Message::Ping(bytes)=>write(&mut ws,Message::Pong(bytes),self.timing.write).await?,
                        Message::Pong(_)=>{},
                        Message::Close(_)=>return Err(ProviderError::Disconnected),
                        _=>return Err(ProviderError::Protocol),
                    }
                }
            }
        }
    }
    fn flush_pending(&mut self) {
        while let Some(event) = self.pending.pop_front() {
            match self.events.try_send(event) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(event)) => {
                    self.pending.push_front(event);
                    break;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => break,
            }
        }
    }
    fn fail(&self, error: ProviderError) {
        self.state.lock().unwrap().close(ProviderEvent::Failed {
            metadata: self.mapper.metadata(),
            error,
        });
    }
}
fn retry_delay(base: Duration, attempt: usize) -> Duration {
    base.saturating_mul(1u32.checked_shl(attempt as u32).unwrap_or(u32::MAX))
        .min(Duration::from_secs(5))
}
async fn write(ws: &mut Socket, message: Message, limit: Duration) -> ProviderResult<()> {
    timeout(limit, ws.send(message))
        .await
        .map_err(|_| ProviderError::Timeout)?
        .map_err(transport_error)
}
async fn wait_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}
