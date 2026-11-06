use super::*;
use crate::intelligence::types::IntelligenceErrorCode;
use crate::{audio::platform::process_io::*, error::AppResult, knowledge::types::TextLocation};
use std::{
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const READY: &[u8] = b"VBPW\x01\0\0\0";
struct Input {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicUsize>,
}
impl AsyncWrite for Input {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.bytes.lock().unwrap().extend(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.closed.store(1, Ordering::Release);
        Poll::Ready(Ok(()))
    }
}
struct Output {
    delay: Pin<Box<tokio::time::Sleep>>,
    bytes: Vec<u8>,
    offset: usize,
    blocked: bool,
}
impl AsyncRead for Output {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        use std::future::Future;
        if self.delay.as_mut().poll(cx).is_pending() {
            return Poll::Pending;
        }
        let n = (self.bytes.len() - self.offset).min(out.remaining());
        if n == 0 && self.blocked {
            return Poll::Pending;
        }
        out.put_slice(&self.bytes[self.offset..self.offset + n]);
        self.offset += n;
        Poll::Ready(Ok(()))
    }
}
struct Control {
    exit: watch::Sender<Option<ProcessExit>>,
    kills: AtomicUsize,
    automatic: bool,
}
#[async_trait::async_trait]
impl ProcessControl for Control {
    async fn wait(&self) -> AppResult<ProcessExit> {
        let mut rx = self.exit.subscribe();
        loop {
            if let Some(exit) = rx.borrow_and_update().clone() {
                return Ok(exit);
            }
            rx.changed().await.unwrap();
        }
    }
    async fn terminate(&self) -> AppResult<ProcessExit> {
        if self.kills.fetch_add(1, Ordering::AcqRel) == 0 && self.automatic {
            self.exit.send_if_modified(|exit| {
                if exit.is_some() {
                    return false;
                }
                *exit = Some(ProcessExit { code: Some(-9) });
                true
            });
        }
        self.wait().await
    }
}
struct Fixture {
    bytes: Mutex<Vec<u8>>,
    input: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicUsize>,
    control: Arc<Control>,
    calls: AtomicUsize,
    spawn_delay: Duration,
    read_delay: Duration,
    blocked: bool,
    spawn_error: bool,
}
#[async_trait::async_trait]
impl ProcessSpawner for Fixture {
    async fn spawn(&self, _: ProcessSpec) -> AppResult<ProcessPipes> {
        panic!("parser must use limited spawn");
    }
    async fn spawn_limited(
        &self,
        spec: ProcessSpec,
        limits: ProcessResourceLimits,
    ) -> AppResult<ProcessPipes> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        assert_eq!(spec.program, PathBuf::from("fixture-worker"));
        assert_eq!(spec.args[0], "--voxbridge-parse-worker");
        assert_eq!(
            limits,
            ProcessResourceLimits {
                memory_bytes: 1024 * 1024 * 1024,
                cpu_seconds: 30
            }
        );
        tokio::time::sleep(self.spawn_delay).await;
        if self.spawn_error {
            return Err(crate::error::AppError::Intelligence(
                IntelligenceErrorCode::Configuration,
            ));
        }
        Ok(ProcessPipes {
            stdin: Some(Box::pin(Input {
                bytes: self.input.clone(),
                closed: self.closed.clone(),
            })),
            stdout: Some(Box::pin(Output {
                delay: Box::pin(tokio::time::sleep(self.read_delay)),
                bytes: self.bytes.lock().unwrap().clone(),
                offset: 0,
                blocked: self.blocked,
            })),
            control: self.control.clone(),
        })
    }
}
fn fixture(bytes: Vec<u8>, exit: Option<i32>, automatic: bool) -> Fixture {
    Fixture {
        bytes: Mutex::new(bytes),
        input: Arc::default(),
        closed: Arc::default(),
        control: Arc::new(Control {
            exit: watch::channel(exit.map(|code| ProcessExit { code: Some(code) })).0,
            kills: AtomicUsize::new(0),
            automatic,
        }),
        calls: AtomicUsize::new(0),
        spawn_delay: Duration::ZERO,
        read_delay: Duration::ZERO,
        blocked: false,
        spawn_error: false,
    }
}
fn parser(fixture: Arc<Fixture>, deadline: u64) -> ProcessDocumentParser {
    ProcessDocumentParser {
        spawner: fixture,
        executable: "fixture-worker".into(),
        deadline: Duration::from_millis(deadline),
        cleanup_allowance: Duration::from_millis(5),
        #[cfg(unix)]
        allocation_bytes: 1024 * 1024 * 1024,
    }
}
fn output() -> (Vec<u8>, Vec<TextBlock>) {
    let blocks = vec![TextBlock {
        text: "pump".into(),
        location: TextLocation {
            page: None,
            paragraph: 1,
            heading: None,
            start_char: 0,
            end_char: 4,
        },
    }];
    let body = serde_json::to_vec(&serde_json::json!({"kind":"parsed","blocks":blocks})).unwrap();
    let mut bytes = READY.to_vec();
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(body);
    (bytes, blocks)
}
#[tokio::test]
async fn parser_supervisor_gates_request_on_readiness_closes_stdin_and_requires_exit() {
    let (bytes, expected) = output();
    let fixture = Arc::new(fixture(bytes, Some(0), true));
    let p = parser(fixture.clone(), 200);
    let result = run(&p, "/managed/input.txt").await;
    assert_eq!(result, Ok(expected));
    assert_eq!(fixture.closed.load(Ordering::Acquire), 1);
    let input = fixture.input.lock().unwrap();
    let len = u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;
    assert_eq!(len + 4, input.len());
    let request: serde_json::Value = serde_json::from_slice(&input[4..]).unwrap();
    assert_eq!(request["version"], 1);
    assert_eq!(request["parser_id"], crate::knowledge::PARSER_ID);
    assert_eq!(request["kind"], "text");
}
#[tokio::test]
async fn parser_supervisor_one_deadline_and_pre_ready_failures_write_zero_bytes() {
    let (bytes, _) = output();
    let mut f = fixture(bytes, None, true);
    f.spawn_delay = Duration::from_millis(20);
    f.read_delay = Duration::from_millis(20);
    let f = Arc::new(f);
    assert_eq!(
        run(&parser(f.clone(), 30), "/file").await,
        Err(IntelligenceErrorCode::Timeout)
    );
    assert!(f.input.lock().unwrap().is_empty());
    assert_eq!(f.control.kills.load(Ordering::Acquire), 1);
    for code in [86, 87] {
        let f = Arc::new(fixture(Vec::new(), Some(code), true));
        assert_eq!(
            run(&parser(f.clone(), 100), "/file").await,
            Err(if code == 86 {
                IntelligenceErrorCode::ResourceLimit
            } else {
                IntelligenceErrorCode::ParseFailed
            })
        );
        assert!(f.input.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn parser_supervisor_cancel_retains_late_spawn_and_unconfirmed_reap() {
    let mut f = fixture(Vec::new(), None, false);
    f.spawn_delay = Duration::from_millis(40);
    let f = Arc::new(f);
    let (cancel, rx) = watch::channel(false);
    let p = parser(f.clone(), 500);
    let task = tokio::spawn(async move { p.parse("/file".into(), DocumentKind::Text, rx).await });
    tokio::time::timeout(Duration::from_millis(200), async {
        while f.calls.load(Ordering::Acquire) == 0 && !task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.send_replace(true);
    tokio::time::sleep(Duration::from_millis(70)).await;
    let pending = !task.is_finished();
    let writes = f.input.lock().unwrap().len();
    f.control
        .exit
        .send_replace(Some(ProcessExit { code: Some(0) }));
    assert!(
        pending,
        "cancel cannot discard late spawn or an unreaped child"
    );
    assert_eq!(writes, 0);
    assert_eq!(task.await.unwrap(), Err(IntelligenceErrorCode::Cancelled));
    assert_eq!(f.control.kills.load(Ordering::Acquire), 1);
}
#[tokio::test]
async fn parser_supervisor_rejects_oversize_truncated_extra_frames_and_failed_exit() {
    let (valid, _) = output();
    let mut oversized = READY.to_vec();
    oversized.extend((crate::knowledge::MAX_OUTPUT_BYTES as u32 + 1).to_le_bytes());
    let mut extra = valid.clone();
    extra.push(0);
    for (bytes, code, expected) in [
        (oversized, 0, IntelligenceErrorCode::ResourceLimit),
        (valid[..10].to_vec(), 0, IntelligenceErrorCode::ParseFailed),
        (extra, 0, IntelligenceErrorCode::ParseFailed),
        (valid, 87, IntelligenceErrorCode::ParseFailed),
    ] {
        let f = Arc::new(fixture(bytes, Some(code), true));
        assert_eq!(run(&parser(f, 100), "/file").await, Err(expected));
    }
}

async fn run(
    parser: &ProcessDocumentParser,
    path: &str,
) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
    let (_sender, receiver) = watch::channel(false);
    parser
        .parse(path.into(), DocumentKind::Text, receiver)
        .await
}

#[tokio::test]
async fn parser_supervisor_maps_budget_exit_after_readiness_and_spawn_failure_safely() {
    let f = Arc::new(fixture(READY.to_vec(), Some(86), true));
    assert_eq!(
        run(&parser(f, 100), "/file").await,
        Err(Code::ResourceLimit)
    );
    let mut f = fixture(Vec::new(), None, true);
    f.spawn_error = true;
    let f = Arc::new(f);
    assert_eq!(
        run(&parser(f.clone(), 100), "/file").await,
        Err(Code::Configuration)
    );
    assert!(f.input.lock().unwrap().is_empty());
    assert_eq!(f.control.kills.load(Ordering::Acquire), 0);
}
#[tokio::test]
async fn parser_supervisor_precancel_bad_preface_and_blocked_result_have_one_cleanup_owner() {
    let f = Arc::new(fixture(vec![b'!'; 8], Some(87), true));
    assert_eq!(
        run(&parser(f.clone(), 100), "/file").await,
        Err(Code::ParseFailed)
    );
    assert!(f.input.lock().unwrap().is_empty());
    assert_eq!(f.control.kills.load(Ordering::Acquire), 1);
    let mut bytes = READY.to_vec();
    bytes.extend(100_u32.to_le_bytes());
    let mut f = fixture(bytes, None, true);
    f.blocked = true;
    let f = Arc::new(f);
    assert_eq!(
        run(&parser(f.clone(), 30), "/file").await,
        Err(Code::Timeout)
    );
    assert_eq!(f.control.kills.load(Ordering::Acquire), 1);
    let f = Arc::new(fixture(Vec::new(), None, true));
    let (_cancel, receiver) = watch::channel(true);
    assert_eq!(
        parser(f.clone(), 100)
            .parse("/file".into(), DocumentKind::Text, receiver)
            .await,
        Err(Code::Cancelled)
    );
    assert_eq!(f.calls.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn parser_supervisor_closed_output_requests_cleanup_without_waiting_for_deadline() {
    for bytes in [Vec::new(), READY.to_vec()] {
        let f = Arc::new(fixture(bytes, None, true));
        let result = run(&parser(f.clone(), 100), "/file").await;
        assert_eq!(result, Err(Code::ParseFailed));
        assert_eq!(f.control.kills.load(Ordering::Acquire), 1);
    }
}
