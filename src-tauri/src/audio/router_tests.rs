use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};

use super::platform::{AudioDirection, AudioEndpoint};
use crate::{
    audio::{
        AudioFrame, PcmSpec,
        graph::AudioGraph,
        router::{
            AudioRouter, AudioRouterEvent, AudioRouterStatus, Direction, EndpointHealthCheck,
            RouteTarget,
        },
    },
    domain::audio::AudioSource,
    error::AppError,
    events::{AudioSessionState, EventBus, SessionEvent},
};

fn endpoint(id: &str, direction: AudioDirection) -> AudioEndpoint {
    AudioEndpoint {
        id: id.into(),
        display_name: id.into(),
        direction,
        sample_rates: vec![16_000, 24_000],
        channels: 1,
        ready: true,
    }
}

fn frame(sequence: u64) -> AudioFrame {
    AudioFrame::new(
        AudioSource::System,
        sequence,
        PcmSpec::pcm16_mono(24_000),
        vec![sequence as i16],
    )
    .unwrap()
}

#[derive(Clone, Default)]
struct Health(Arc<Mutex<Vec<String>>>);

impl EndpointHealthCheck for Health {
    fn is_ready(&self, endpoint: &AudioEndpoint) -> bool {
        self.0.lock().unwrap().contains(&endpoint.id)
    }
}

fn health_for(ids: &[&str]) -> Health {
    Health(Arc::new(Mutex::new(
        ids.iter().map(|id| (*id).into()).collect(),
    )))
}

#[derive(Clone)]
struct BlockingHealth {
    ready: Health,
    blocking_id: &'static str,
    entered: Arc<std_mpsc::Sender<()>>,
    release: Arc<Mutex<std_mpsc::Receiver<()>>>,
}

impl EndpointHealthCheck for BlockingHealth {
    fn is_ready(&self, endpoint: &AudioEndpoint) -> bool {
        if endpoint.id == self.blocking_id {
            let _ = self.entered.send(());
            let _ = self.release.lock().unwrap().recv();
        }
        self.ready.is_ready(endpoint)
    }
}

#[test]
fn live_graph_routes_provider_audio_to_m1_only() {
    let graph = AudioGraph::for_live(
        endpoint("mic", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
    )
    .unwrap();
    assert_eq!(graph.directions(), vec![Direction::Live]);
    assert_eq!(graph.target(Direction::Live), Some(RouteTarget::M1));
}

#[test]
fn call_graph_has_two_independent_translation_directions() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    assert_eq!(graph.directions(), vec![Direction::AtoB, Direction::BtoA]);
    assert_eq!(graph.target(Direction::AtoB), Some(RouteTarget::M1));
    assert_eq!(
        graph.target(Direction::BtoA),
        Some(RouteTarget::PhysicalOutput)
    );
}

#[tokio::test]
async fn call_directions_deliver_to_independent_bounded_queues() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (m1_tx, mut m1_rx) = mpsc::channel(2);
    let (speaker_tx, mut speaker_rx) = mpsc::channel(2);
    let router = AudioRouter::for_call(
        graph,
        m1_tx,
        speaker_tx,
        health_for(&["a", "m2", "m1", "speaker"]),
    );

    router.route_direction(frame(1), Direction::AtoB).unwrap();
    router.route_direction(frame(2), Direction::BtoA).unwrap();

    assert_eq!(m1_rx.recv().await.unwrap().sequence(), 1);
    assert_eq!(speaker_rx.recv().await.unwrap().sequence(), 2);
    assert!(m1_rx.try_recv().is_err());
    assert!(speaker_rx.try_recv().is_err());
}

#[tokio::test]
async fn route_uses_bounded_sender_and_reports_target_backpressure() {
    let graph = AudioGraph::for_live(
        endpoint("mic", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
    )
    .unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let router = AudioRouter::for_live(graph, tx, health_for(&["mic", "m1"]));
    let mut events = router.subscribe();

    router.route(frame(1), RouteTarget::M1).unwrap();
    assert_eq!(rx.recv().await.unwrap().sequence(), 1);
    router.route(frame(2), RouteTarget::M1).unwrap();
    router.route(frame(3), RouteTarget::M1).unwrap();
    let error = router
        .route(frame(4), RouteTarget::PhysicalOutput)
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidRequest(_)));
    assert!(error.to_string().contains("PhysicalOutput"));
    assert!(!error.to_string().contains("mic"));
    assert!(!error.to_string().contains("m1"));
    let error = router
        .route_direction(frame(5), Direction::BtoA)
        .unwrap_err();
    assert!(error.to_string().contains("BtoA"));
    assert!(!error.to_string().contains("mic"));
    assert!(matches!(
        events.recv().await.unwrap(),
        AudioRouterEvent::Backpressure {
            target: RouteTarget::M1,
            dropped_frames: 1
        }
    ));
}

#[tokio::test]
async fn pause_and_resume_publish_typed_session_audio_state() {
    let graph = AudioGraph::for_live(
        endpoint("mic", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
    )
    .unwrap();
    let (tx, _rx) = mpsc::channel(2);
    let bus = EventBus::new();
    let session_id = uuid::Uuid::new_v4();
    bus.open_session(session_id);
    let mut events = bus.subscribe(session_id);
    let router = AudioRouter::for_live_with_events(
        graph,
        tx,
        health_for(&["mic", "m1"]),
        session_id,
        Arc::new(bus),
    );

    router.pause("device disconnected");
    assert!(matches!(
        events.recv().await.unwrap(),
        SessionEvent::AudioState { state: AudioSessionState::Paused { reason }, .. } if reason == "device disconnected"
    ));
    router.resume().unwrap();
    assert!(matches!(
        events.recv().await.unwrap(),
        SessionEvent::AudioState {
            state: AudioSessionState::Running,
            ..
        }
    ));
}

#[test]
fn pause_waits_for_in_flight_route_and_blocks_later_delivery() {
    let graph = AudioGraph::for_live(
        endpoint("mic", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
    )
    .unwrap();
    let (tx, mut rx) = mpsc::channel(2);
    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let (pause_started_tx, pause_started_rx) = std_mpsc::channel();
    let (pause_done_tx, pause_done_rx) = std_mpsc::channel();
    let health = BlockingHealth {
        ready: health_for(&["mic", "m1"]),
        blocking_id: "mic",
        entered: Arc::new(entered_tx),
        release: Arc::new(Mutex::new(release_rx)),
    };
    let router = Arc::new(AudioRouter::for_live(graph, tx, health));
    let route_router = router.clone();
    let route_thread = thread::spawn(move || route_router.route(frame(1), RouteTarget::M1));
    entered_rx.recv().unwrap();
    let pause_router = router.clone();
    let pause_thread = thread::spawn(move || {
        pause_started_tx.send(()).unwrap();
        pause_router.pause("device lost");
        pause_done_tx.send(()).unwrap();
    });
    pause_started_rx.recv().unwrap();
    assert!(matches!(
        pause_done_rx.recv_timeout(Duration::from_millis(50)),
        Err(std_mpsc::RecvTimeoutError::Timeout)
    ));
    release_tx.send(()).unwrap();
    route_thread.join().unwrap().unwrap();
    pause_done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    pause_thread.join().unwrap();

    assert_eq!(rx.try_recv().unwrap().sequence(), 1);
    assert!(router.route(frame(2), RouteTarget::M1).is_err());
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn pause_resume_requires_fresh_health_for_every_endpoint() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (m1_tx, _m1_rx) = mpsc::channel(2);
    let (speaker_tx, _speaker_rx) = mpsc::channel(2);
    let health = health_for(&["a", "m2", "m1"]);
    let router = AudioRouter::for_call(graph, m1_tx, speaker_tx, health.clone());
    let mut events = router.subscribe();

    router.pause("driver disconnected");
    assert!(matches!(
        router.route(frame(1), RouteTarget::M1),
        Err(AppError::InvalidRequest(_))
    ));
    assert_eq!(
        router.resume(),
        Err(AudioRouterStatus::Paused {
            reason: "endpoint speaker is unavailable".into()
        })
    );
    assert!(matches!(
        events.recv().await.unwrap(),
        AudioRouterEvent::StatusChanged(AudioRouterStatus::Paused { .. })
    ));
    health.0.lock().unwrap().push("speaker".into());
    router.resume().unwrap();
    assert_eq!(router.status(), AudioRouterStatus::Running);
}

#[tokio::test]
async fn endpoint_loss_pauses_routes_and_requires_explicit_resume() {
    let graph = AudioGraph::for_live(
        endpoint("mic", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
    )
    .unwrap();
    let (tx, _rx) = mpsc::channel(2);
    let health = health_for(&["mic", "m1"]);
    let router = AudioRouter::for_live(graph, tx, health.clone());
    let mut events: broadcast::Receiver<AudioRouterEvent> = router.subscribe();
    health.0.lock().unwrap().retain(|id| id != "m1");

    assert!(matches!(
        router.route(frame(1), RouteTarget::M1),
        Err(AppError::InvalidRequest(_))
    ));
    assert!(matches!(router.status(), AudioRouterStatus::Paused { .. }));
    assert!(matches!(
        events.recv().await.unwrap(),
        AudioRouterEvent::StatusChanged(AudioRouterStatus::Paused { .. })
    ));
}

#[tokio::test]
async fn one_lost_route_stays_paused_while_peer_runs_and_resume_is_direction_scoped() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (m1_tx, mut m1_rx) = mpsc::channel(2);
    let (speaker_tx, mut speaker_rx) = mpsc::channel(2);
    let health = health_for(&["a", "m2", "m1", "speaker"]);
    let bus = EventBus::new();
    let id = uuid::Uuid::new_v4();
    bus.open_session(id);
    let mut states = bus.subscribe(id);
    let router = AudioRouter::for_call_with_events(
        graph,
        m1_tx,
        speaker_tx,
        health.clone(),
        id,
        Arc::new(bus),
    );
    let mut events = router.subscribe();
    health.0.lock().unwrap().retain(|id| id != "m1");
    assert!(router.route_direction(frame(1), Direction::AtoB).is_err());
    assert!(matches!(
        router.direction_status(Direction::AtoB).unwrap(),
        AudioRouterStatus::Paused { .. }
    ));
    assert_eq!(router.status(), AudioRouterStatus::Running);
    assert!(states.try_recv().is_err());
    assert!(matches!(
        events.recv().await.unwrap(),
        AudioRouterEvent::DirectionStatusChanged {
            direction: Direction::AtoB,
            status: AudioRouterStatus::Paused { .. }
        }
    ));
    router.route_direction(frame(2), Direction::BtoA).unwrap();
    assert_eq!(speaker_rx.recv().await.unwrap().sequence(), 2);
    health.0.lock().unwrap().push("m1".into());
    assert!(router.route_direction(frame(3), Direction::AtoB).is_err());
    assert!(m1_rx.try_recv().is_err());
    health.0.lock().unwrap().retain(|id| id != "speaker");
    router.resume_direction(Direction::AtoB).unwrap();
    router.route_direction(frame(4), Direction::AtoB).unwrap();
    assert_eq!(m1_rx.recv().await.unwrap().sequence(), 4);
    assert!(router.resume_direction(Direction::Live).is_err());
    assert!(
        router
            .pause_direction(Direction::Live, "absent".into())
            .is_err()
    );
    assert!(router.direction_status(Direction::Live).is_err());
}

#[tokio::test]
async fn closed_output_requires_replacement_and_only_resumes_explicitly() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (m1_tx, m1_rx) = mpsc::channel(2);
    let (speaker_tx, mut speaker_rx) = mpsc::channel(2);
    let router = AudioRouter::for_call(
        graph,
        m1_tx,
        speaker_tx,
        health_for(&["a", "m2", "m1", "speaker"]),
    );
    drop(m1_rx);
    assert!(router.route_direction(frame(1), Direction::AtoB).is_err());
    assert_eq!(router.status(), AudioRouterStatus::Running);
    router.route_direction(frame(2), Direction::BtoA).unwrap();
    assert_eq!(speaker_rx.recv().await.unwrap().sequence(), 2);
    assert!(router.resume_direction(Direction::AtoB).is_err());
    let (replacement, mut fresh_rx) = mpsc::channel(2);
    assert!(
        router
            .replace_direction_output(Direction::BtoA, replacement.clone())
            .is_err()
    );
    assert!(
        router
            .replace_direction_output(Direction::Live, replacement.clone())
            .is_err()
    );
    let (closed, receiver) = mpsc::channel(1);
    drop(receiver);
    assert!(
        router
            .replace_direction_output(Direction::AtoB, closed)
            .is_err()
    );
    router
        .replace_direction_output(Direction::AtoB, replacement)
        .unwrap();
    assert!(router.route_direction(frame(3), Direction::AtoB).is_err());
    assert!(fresh_rx.try_recv().is_err());
    router.route_direction(frame(4), Direction::BtoA).unwrap();
    assert_eq!(speaker_rx.recv().await.unwrap().sequence(), 4);
    router.resume_direction(Direction::AtoB).unwrap();
    router.route_direction(frame(5), Direction::AtoB).unwrap();
    assert_eq!(fresh_rx.recv().await.unwrap().sequence(), 5);
    assert!(speaker_rx.try_recv().is_err());
}

#[tokio::test]
async fn whole_graph_pause_requires_whole_graph_resume_and_checks_all_outputs() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (m1_tx, m1_rx) = mpsc::channel(2);
    let (speaker_tx, mut speaker_rx) = mpsc::channel(2);
    let router = AudioRouter::for_call(
        graph,
        m1_tx,
        speaker_tx,
        health_for(&["a", "m2", "m1", "speaker"]),
    );
    router.pause("user pause");
    let (fresh_tx, _fresh_rx) = mpsc::channel(2);
    assert!(
        router
            .replace_direction_output(Direction::BtoA, fresh_tx)
            .is_err()
    );
    assert!(router.resume_direction(Direction::BtoA).is_err());
    assert!(router.route_direction(frame(1), Direction::BtoA).is_err());
    assert!(speaker_rx.try_recv().is_err());
    router.resume().unwrap();
    router.route_direction(frame(2), Direction::BtoA).unwrap();
    assert_eq!(speaker_rx.recv().await.unwrap().sequence(), 2);
    router
        .pause_direction(Direction::AtoB, "A paused".into())
        .unwrap();
    router
        .pause_direction(Direction::BtoA, "B paused".into())
        .unwrap();
    assert!(matches!(router.status(), AudioRouterStatus::Paused { .. }));
    router.resume_direction(Direction::BtoA).unwrap();
    assert_eq!(router.status(), AudioRouterStatus::Running);
    drop(m1_rx);
    router.pause("pause before reopen");
    assert!(router.resume().is_err());
    assert!(matches!(router.status(), AudioRouterStatus::Paused { .. }));
    assert!(router.route_direction(frame(3), Direction::BtoA).is_err());
}

#[test]
fn checking_one_direction_does_not_hold_the_other_direction_gate() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (m1_tx, _m1_rx) = mpsc::channel(2);
    let (speaker_tx, mut speaker_rx) = mpsc::channel(2);
    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let router = Arc::new(AudioRouter::for_call(
        graph,
        m1_tx,
        speaker_tx,
        BlockingHealth {
            ready: health_for(&["a", "m2", "m1", "speaker"]),
            blocking_id: "a",
            entered: Arc::new(entered_tx),
            release: Arc::new(Mutex::new(release_rx)),
        },
    ));
    let forward = router.clone();
    let blocked = thread::spawn(move || forward.route_direction(frame(1), Direction::AtoB));
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let (done_tx, done_rx) = std_mpsc::channel();
    let reverse = router.clone();
    let peer = thread::spawn(move || {
        done_tx
            .send(reverse.route_direction(frame(2), Direction::BtoA))
            .unwrap();
    });
    let peer_result = done_rx.recv_timeout(Duration::from_secs(1));
    release_tx.send(()).unwrap();
    blocked.join().unwrap().unwrap();
    peer.join().unwrap();
    peer_result
        .expect("peer was blocked by another direction")
        .unwrap();
    assert_eq!(speaker_rx.try_recv().unwrap().sequence(), 2);
}

#[tokio::test]
async fn unbound_routes_require_real_output_and_explicit_independent_resume() {
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let router = AudioRouter::paused_unbound(graph, health_for(&["a", "m2", "m1", "speaker"]));
    assert!(router.resume_direction(Direction::AtoB).is_err());
    let (sender, mut receiver) = mpsc::channel(2);
    router
        .replace_direction_output(Direction::AtoB, sender)
        .unwrap();
    assert!(router.route_direction(frame(1), Direction::AtoB).is_err());
    router.resume_direction(Direction::AtoB).unwrap();
    router.route_direction(frame(2), Direction::AtoB).unwrap();
    assert_eq!(receiver.recv().await.unwrap().sequence(), 2);
    assert!(router.route_direction(frame(3), Direction::BtoA).is_err());
    assert!(matches!(
        router.direction_status(Direction::BtoA).unwrap(),
        AudioRouterStatus::Paused { .. }
    ));
}

#[tokio::test]
async fn exact_admission_feedback_survives_peer_event_lag_and_preserves_public_drop_contract() {
    use super::router::RouteAdmission;
    let graph = AudioGraph::for_call(
        endpoint("a", AudioDirection::Input),
        endpoint("m2", AudioDirection::Input),
        endpoint("m1", AudioDirection::Output),
        endpoint("speaker", AudioDirection::Output),
    )
    .unwrap();
    let (a_output, mut a) = mpsc::channel(1);
    let (b_output, mut b) = mpsc::channel(1);
    let router = AudioRouter::for_call(
        graph,
        a_output,
        b_output,
        health_for(&["a", "m2", "m1", "speaker"]),
    );
    let mut events = router.subscribe();
    assert_eq!(
        router
            .route_direction_admission(frame(1), Direction::AtoB)
            .unwrap(),
        RouteAdmission::Enqueued
    );
    for index in 0..40 {
        router
            .pause_direction(Direction::BtoA, format!("peer {index}"))
            .unwrap();
        router.resume_direction(Direction::BtoA).unwrap();
    }
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Lagged(_))
    ));
    let mut current_events = router.subscribe();
    assert_eq!(
        router
            .route_direction_admission(frame(2), Direction::AtoB)
            .unwrap(),
        RouteAdmission::Dropped
    );
    // The existing public API still drops/counts/notifies and returns Ok.
    router.route_direction(frame(3), Direction::AtoB).unwrap();
    assert_eq!(
        current_events.try_recv().unwrap(),
        AudioRouterEvent::Backpressure {
            target: RouteTarget::M1,
            dropped_frames: 1
        }
    );
    assert_eq!(
        current_events.try_recv().unwrap(),
        AudioRouterEvent::Backpressure {
            target: RouteTarget::M1,
            dropped_frames: 2
        }
    );
    assert_eq!(
        router
            .route_direction_admission(frame(4), Direction::BtoA)
            .unwrap(),
        RouteAdmission::Enqueued
    );
    assert_eq!(a.recv().await.unwrap().sequence(), 1);
    assert_eq!(b.recv().await.unwrap().sequence(), 4);
    assert!(a.try_recv().is_err());
    assert_eq!(
        router
            .route_direction_admission(frame(5), Direction::AtoB)
            .unwrap(),
        RouteAdmission::Enqueued
    );
    assert_eq!(a.recv().await.unwrap().sequence(), 5);
}
