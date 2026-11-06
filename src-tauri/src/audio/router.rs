use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::{
    audio::{AudioFrame, graph::AudioGraph, platform::AudioEndpoint},
    error::{AppError, AppResult},
    events::{AudioSessionState, EventBus},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Live,
    AtoB,
    BtoA,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteTarget {
    M1,
    PhysicalOutput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AudioRouterStatus {
    Running,
    Paused { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioRouterEvent {
    StatusChanged(AudioRouterStatus),
    DirectionStatusChanged {
        direction: Direction,
        status: AudioRouterStatus,
    },
    Backpressure {
        target: RouteTarget,
        dropped_frames: u64,
    },
}

pub trait EndpointHealthCheck: Send + Sync {
    fn is_ready(&self, endpoint: &AudioEndpoint) -> bool;
}

pub trait AudioStateSink: Send + Sync {
    fn publish_audio_state(&self, session_id: Uuid, state: AudioSessionState);
}

impl AudioStateSink for EventBus {
    fn publish_audio_state(&self, session_id: Uuid, state: AudioSessionState) {
        let _ = self.publish(
            session_id,
            crate::events::SessionEvent::audio_state(session_id, state),
        );
    }
}

struct RouteState {
    status: AudioRouterStatus,
    output: Option<mpsc::Sender<AudioFrame>>,
}
struct RouterState {
    whole_pause: Option<AudioRouterStatus>,
    routes: [Option<RouteState>; 3],
    dropped: [u64; 2],
}
impl RouterState {
    fn aggregate(&self) -> AudioRouterStatus {
        if let Some(status) = &self.whole_pause {
            return status.clone();
        }
        if self
            .routes
            .iter()
            .flatten()
            .any(|route| route.status == AudioRouterStatus::Running)
        {
            return AudioRouterStatus::Running;
        }
        self.routes
            .iter()
            .flatten()
            .next()
            .expect("audio graph has a route")
            .status
            .clone()
    }
    fn effective(&self, direction: Direction) -> AudioRouterStatus {
        self.whole_pause.clone().unwrap_or_else(|| {
            self.routes[index(direction)]
                .as_ref()
                .expect("validated graph direction")
                .status
                .clone()
        })
    }
}

/// Direction gates serialize only the chosen route. Whole-graph controls acquire
/// all gates in a fixed order; no route/state lock spans an async operation.
pub struct AudioRouter<H> {
    graph: AudioGraph,
    health: H,
    state: Mutex<RouterState>,
    route_gates: [Mutex<()>; 3],
    // Orders state notifications across independent direction transitions without
    // holding the state mutex while invoking an external event sink.
    status_gate: Mutex<()>,
    events: broadcast::Sender<AudioRouterEvent>,
    session_id: Option<Uuid>,
    state_sink: Option<Arc<dyn AudioStateSink>>,
}

impl<H: EndpointHealthCheck> AudioRouter<H> {
    pub fn for_live(graph: AudioGraph, m1: mpsc::Sender<AudioFrame>, health: H) -> Self {
        Self::new(graph, m1, None, health, None, None)
    }
    pub fn for_live_with_events(
        graph: AudioGraph,
        m1: mpsc::Sender<AudioFrame>,
        health: H,
        session_id: Uuid,
        state_sink: Arc<dyn AudioStateSink>,
    ) -> Self {
        Self::new(graph, m1, None, health, Some(session_id), Some(state_sink))
    }
    pub fn for_call(
        graph: AudioGraph,
        m1: mpsc::Sender<AudioFrame>,
        physical: mpsc::Sender<AudioFrame>,
        health: H,
    ) -> Self {
        Self::new(graph, m1, Some(physical), health, None, None)
    }
    pub fn for_call_with_events(
        graph: AudioGraph,
        m1: mpsc::Sender<AudioFrame>,
        physical: mpsc::Sender<AudioFrame>,
        health: H,
        session_id: Uuid,
        state_sink: Arc<dyn AudioStateSink>,
    ) -> Self {
        Self::new(
            graph,
            m1,
            Some(physical),
            health,
            Some(session_id),
            Some(state_sink),
        )
    }
    fn new(
        graph: AudioGraph,
        m1: mpsc::Sender<AudioFrame>,
        physical: Option<mpsc::Sender<AudioFrame>>,
        health: H,
        session_id: Option<Uuid>,
        state_sink: Option<Arc<dyn AudioStateSink>>,
    ) -> Self {
        let mut routes = std::array::from_fn(|_| None);
        for direction in graph.directions() {
            routes[index(direction)] = Some(RouteState {
                status: AudioRouterStatus::Running,
                output: match direction {
                    Direction::Live | Direction::AtoB => Some(m1.clone()),
                    Direction::BtoA => physical.clone(),
                },
            });
        }
        let (events, _) = broadcast::channel(32);
        Self {
            graph,
            health,
            state: Mutex::new(RouterState {
                whole_pause: None,
                routes,
                dropped: [0; 2],
            }),
            route_gates: std::array::from_fn(|_| Mutex::new(())),
            status_gate: Mutex::new(()),
            events,
            session_id,
            state_sink,
        }
    }
    pub fn route(&self, frame: AudioFrame, target: RouteTarget) -> AppResult<()> {
        let route = self.graph.route_for_target(target).ok_or_else(|| {
            AppError::InvalidRequest(format!("no audio route for target {target:?}"))
        })?;
        self.route_direction(frame, route.direction())
    }
    pub fn route_direction(&self, frame: AudioFrame, direction: Direction) -> AppResult<()> {
        self.graph
            .route(direction)
            .ok_or_else(|| missing_direction(direction))?;
        let _gate = self.route_gates[index(direction)]
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (status, output) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            (
                state.effective(direction),
                state.routes[index(direction)]
                    .as_ref()
                    .unwrap()
                    .output
                    .clone(),
            )
        };
        if status != AudioRouterStatus::Running {
            return Err(AppError::InvalidRequest(format!(
                "audio direction {direction:?} is paused: {status:?}"
            )));
        }
        if let Err(status) = self.validate_route(direction) {
            self.set_direction_status_locked(direction, status.clone());
            return Err(AppError::InvalidRequest(format!(
                "audio direction {direction:?} unavailable: {status:?}"
            )));
        }
        let output = output.expect("validated output exists");
        match output.try_send(frame) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                let dropped_frames = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let target = if direction == Direction::BtoA { 1 } else { 0 };
                    state.dropped[target] = state.dropped[target].saturating_add(1);
                    state.dropped[target]
                };
                let _ = self.events.send(AudioRouterEvent::Backpressure {
                    target: self.graph.target(direction).unwrap(),
                    dropped_frames,
                });
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                let status = AudioRouterStatus::Paused {
                    reason: format!("audio direction {direction:?} destination queue closed"),
                };
                self.set_direction_status_locked(direction, status);
                Err(AppError::Internal(format!(
                    "audio direction {direction:?} destination queue is closed"
                )))
            }
        }
    }
    fn validate_route(&self, direction: Direction) -> Result<(), AudioRouterStatus> {
        let route = self
            .graph
            .route(direction)
            .ok_or_else(|| paused_missing(direction))?;
        for endpoint in [route.source(), route.destination()] {
            self.validate_endpoint(endpoint)?;
        }
        self.validate_output(direction)
    }
    fn validate_endpoint(&self, endpoint: &AudioEndpoint) -> Result<(), AudioRouterStatus> {
        if self.health.is_ready(endpoint) {
            return Ok(());
        }
        Err(AudioRouterStatus::Paused {
            reason: format!("endpoint {} is unavailable", endpoint.id),
        })
    }
    fn validate_output(&self, direction: Direction) -> Result<(), AudioRouterStatus> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.routes[index(direction)]
            .as_ref()
            .and_then(|route| route.output.as_ref())
            .is_none_or(mpsc::Sender::is_closed)
        {
            return Err(AudioRouterStatus::Paused {
                reason: format!("audio direction {direction:?} destination queue is closed"),
            });
        }
        Ok(())
    }
    pub fn pause_direction(&self, direction: Direction, reason: String) -> AppResult<()> {
        if self.graph.route(direction).is_none() {
            return Err(missing_direction(direction));
        }
        let _gate = self.route_gates[index(direction)]
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.set_direction_status_locked(direction, AudioRouterStatus::Paused { reason });
        Ok(())
    }
    pub fn resume_direction(&self, direction: Direction) -> Result<(), AudioRouterStatus> {
        if self.graph.route(direction).is_none() {
            return Err(paused_missing(direction));
        }
        let _gate = self.route_gates[index(direction)]
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(status) = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .whole_pause
            .clone()
        {
            return Err(status);
        }
        if let Err(status) = self.validate_route(direction) {
            self.set_direction_status_locked(direction, status.clone());
            return Err(status);
        }
        self.set_direction_status_locked(direction, AudioRouterStatus::Running);
        Ok(())
    }
    pub fn direction_status(&self, direction: Direction) -> AppResult<AudioRouterStatus> {
        if self.graph.route(direction).is_none() {
            return Err(missing_direction(direction));
        }
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .effective(direction))
    }
    /// The caller closes/discards old ports before installing a fresh sender.
    /// Replacement never resumes delivery or changes the other route.
    pub fn replace_direction_output(
        &self,
        direction: Direction,
        output: mpsc::Sender<AudioFrame>,
    ) -> AppResult<()> {
        if self.graph.route(direction).is_none() {
            return Err(missing_direction(direction));
        }
        let _gate = self.route_gates[index(direction)]
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.routes[index(direction)].as_ref().unwrap().status == AudioRouterStatus::Running
            || output.is_closed()
        {
            return Err(AppError::InvalidRequest(
                "output replacement requires a paused route and open queue".into(),
            ));
        }
        state.routes[index(direction)].as_mut().unwrap().output = Some(output);
        Ok(())
    }
    fn set_direction_status_locked(&self, direction: Direction, status: AudioRouterStatus) {
        let _notifications = self
            .status_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (before, after) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.routes[index(direction)].as_ref().unwrap().status == status {
                return;
            }
            let before = state.aggregate();
            state.routes[index(direction)].as_mut().unwrap().status = status.clone();
            (before, state.aggregate())
        };
        self.publish_aggregate_change(before, after);
        let _ = self
            .events
            .send(AudioRouterEvent::DirectionStatusChanged { direction, status });
    }
    pub fn pause(&self, reason: impl Into<String>) {
        let _gates = self.lock_all_routes();
        self.set_whole_pause_locked(AudioRouterStatus::Paused {
            reason: reason.into(),
        });
    }
    fn set_whole_pause_locked(&self, status: AudioRouterStatus) {
        let _notifications = self
            .status_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (before, after) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let before = state.aggregate();
            state.whole_pause = Some(status);
            (before, state.aggregate())
        };
        self.publish_aggregate_change(before, after);
    }
    pub fn resume(&self) -> Result<(), AudioRouterStatus> {
        let _gates = self.lock_all_routes();
        for result in self
            .graph
            .endpoints()
            .iter()
            .map(|endpoint| self.validate_endpoint(endpoint))
            .chain(
                self.graph
                    .directions()
                    .into_iter()
                    .map(|direction| self.validate_output(direction)),
            )
        {
            if let Err(status) = result {
                self.set_whole_pause_locked(status.clone());
                return Err(status);
            }
        }
        let _notifications = self
            .status_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (before, after, changed) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let before = state.aggregate();
            state.whole_pause = None;
            let mut changed = Vec::new();
            for direction in self.graph.directions() {
                let route = state.routes[index(direction)].as_mut().unwrap();
                if route.status != AudioRouterStatus::Running {
                    route.status = AudioRouterStatus::Running;
                    changed.push(direction);
                }
            }
            (before, state.aggregate(), changed)
        };
        self.publish_aggregate_change(before, after);
        for direction in changed {
            let _ = self.events.send(AudioRouterEvent::DirectionStatusChanged {
                direction,
                status: AudioRouterStatus::Running,
            });
        }
        Ok(())
    }
    fn lock_all_routes(&self) -> [std::sync::MutexGuard<'_, ()>; 3] {
        std::array::from_fn(|i| {
            self.route_gates[i]
                .lock()
                .unwrap_or_else(|error| error.into_inner())
        })
    }
    pub fn status(&self) -> AudioRouterStatus {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .aggregate()
    }
    fn publish_aggregate_change(&self, before: AudioRouterStatus, after: AudioRouterStatus) {
        if before == after {
            return;
        }
        let _ = self
            .events
            .send(AudioRouterEvent::StatusChanged(after.clone()));
        if let (Some(session_id), Some(sink)) = (self.session_id, self.state_sink.as_ref()) {
            sink.publish_audio_state(
                session_id,
                match after {
                    AudioRouterStatus::Running => AudioSessionState::Running,
                    AudioRouterStatus::Paused { reason } => AudioSessionState::Paused { reason },
                },
            );
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<AudioRouterEvent> {
        self.events.subscribe()
    }
}
fn index(direction: Direction) -> usize {
    match direction {
        Direction::Live => 0,
        Direction::AtoB => 1,
        Direction::BtoA => 2,
    }
}
fn missing_direction(direction: Direction) -> AppError {
    AppError::InvalidRequest(format!("no audio route for direction {direction:?}"))
}
fn paused_missing(direction: Direction) -> AudioRouterStatus {
    AudioRouterStatus::Paused {
        reason: format!("no audio route for direction {direction:?}"),
    }
}
