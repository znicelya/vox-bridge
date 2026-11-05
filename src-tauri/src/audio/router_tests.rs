use std::sync::{Arc, Mutex};

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
    assert!(error.to_string().contains("mic"));
    assert!(error.to_string().contains("m1"));
    assert!(matches!(
        events.recv().await.unwrap(),
        AudioRouterEvent::Backpressure {
            target: RouteTarget::M1,
            dropped_frames: 1
        }
    ));
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
