use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::{
    audio::{AudioFrame, graph::AudioGraph, platform::AudioEndpoint},
    error::{AppError, AppResult},
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
    Backpressure {
        target: RouteTarget,
        dropped_frames: u64,
    },
}

pub trait EndpointHealthCheck: Send + Sync {
    fn is_ready(&self, endpoint: &AudioEndpoint) -> bool;
}

struct RouterState {
    status: AudioRouterStatus,
    dropped: [u64; 2],
}

/// Routes provider frames through bounded channels. Each target has its own sender.
pub struct AudioRouter<H> {
    graph: AudioGraph,
    m1: mpsc::Sender<AudioFrame>,
    physical: Option<mpsc::Sender<AudioFrame>>,
    health: H,
    state: Arc<Mutex<RouterState>>,
    events: broadcast::Sender<AudioRouterEvent>,
}

impl<H: EndpointHealthCheck> AudioRouter<H> {
    pub fn for_live(graph: AudioGraph, m1: mpsc::Sender<AudioFrame>, health: H) -> Self {
        Self::new(graph, m1, None, health)
    }

    pub fn for_call(
        graph: AudioGraph,
        m1: mpsc::Sender<AudioFrame>,
        physical: mpsc::Sender<AudioFrame>,
        health: H,
    ) -> Self {
        Self::new(graph, m1, Some(physical), health)
    }

    fn new(
        graph: AudioGraph,
        m1: mpsc::Sender<AudioFrame>,
        physical: Option<mpsc::Sender<AudioFrame>>,
        health: H,
    ) -> Self {
        let (events, _) = broadcast::channel(32);
        Self {
            graph,
            m1,
            physical,
            health,
            state: Arc::new(Mutex::new(RouterState {
                status: AudioRouterStatus::Running,
                dropped: [0; 2],
            })),
            events,
        }
    }

    pub fn route(&self, frame: AudioFrame, target: RouteTarget) -> AppResult<()> {
        let Some(route) = self.graph.route_for_target(target) else {
            let (source, destination) = self
                .graph
                .first_route()
                .map(|route| (route.source().id.as_str(), route.destination().id.as_str()))
                .unwrap_or(("unknown", "unknown"));
            return Err(AppError::InvalidRequest(format!(
                "no audio route from {source} to {destination} for target {target:?}"
            )));
        };
        self.route_on(
            route.direction(),
            frame,
            route.source(),
            route.destination(),
        )
    }

    pub fn route_direction(&self, frame: AudioFrame, direction: Direction) -> AppResult<()> {
        let Some(route) = self.graph.route(direction) else {
            let (source, destination) = self
                .graph
                .first_route()
                .map(|route| (route.source().id.as_str(), route.destination().id.as_str()))
                .unwrap_or(("unknown", "unknown"));
            return Err(AppError::InvalidRequest(format!(
                "no audio route from {source} to {destination} for direction {direction:?}"
            )));
        };
        self.route_on(
            route.direction(),
            frame,
            route.source(),
            route.destination(),
        )
    }

    fn route_on(
        &self,
        direction: Direction,
        frame: AudioFrame,
        source: &AudioEndpoint,
        destination: &AudioEndpoint,
    ) -> AppResult<()> {
        let current_status = self.status();
        if current_status != AudioRouterStatus::Running {
            return Err(AppError::InvalidRequest(format!(
                "audio route {} -> {} is paused: {current_status:?}",
                source.id, destination.id
            )));
        }
        for endpoint in [source, destination] {
            if !self.health.is_ready(endpoint) {
                self.pause(format!(
                    "route {} -> {} lost endpoint {}",
                    source.id, destination.id, endpoint.id
                ));
                return Err(AppError::InvalidRequest(format!(
                    "audio route {} -> {} endpoint {} is unavailable",
                    source.id, destination.id, endpoint.id
                )));
            }
        }
        let sender = match direction {
            Direction::Live | Direction::AtoB => &self.m1,
            Direction::BtoA => self.physical.as_ref().ok_or_else(|| {
                AppError::InvalidRequest(format!(
                    "audio route {} -> {} has no destination queue",
                    source.id, destination.id
                ))
            })?,
        };
        match sender.try_send(frame) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                let dropped_frames = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let index = if matches!(direction, Direction::BtoA) {
                        1
                    } else {
                        0
                    };
                    state.dropped[index] += 1;
                    state.dropped[index]
                };
                let _ = self.events.send(AudioRouterEvent::Backpressure {
                    target: self.graph.target(direction).unwrap(),
                    dropped_frames,
                });
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.pause(format!(
                    "route {} -> {} destination queue closed",
                    source.id, destination.id
                ));
                Err(AppError::Internal(format!(
                    "audio route {} -> {} destination queue is closed",
                    source.id, destination.id
                )))
            }
        }
    }

    pub fn pause(&self, reason: impl Into<String>) {
        let status = AudioRouterStatus::Paused {
            reason: reason.into(),
        };
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.status != status {
            state.status = status.clone();
            drop(state);
            let _ = self.events.send(AudioRouterEvent::StatusChanged(status));
        }
    }

    /// Checks every graph endpoint at resume time. The router stays paused on the first failure.
    pub fn resume(&self) -> Result<(), AudioRouterStatus> {
        let unavailable = self
            .graph
            .endpoints()
            .iter()
            .find(|endpoint| !self.health.is_ready(endpoint));
        if let Some(endpoint) = unavailable {
            let status = AudioRouterStatus::Paused {
                reason: format!("endpoint {} is unavailable", endpoint.id),
            };
            self.set_status(status.clone());
            return Err(status);
        }
        self.set_status(AudioRouterStatus::Running);
        Ok(())
    }

    fn set_status(&self, status: AudioRouterStatus) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.status != status {
            state.status = status.clone();
            drop(state);
            let _ = self.events.send(AudioRouterEvent::StatusChanged(status));
        }
    }

    pub fn status(&self) -> AudioRouterStatus {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .status
            .clone()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<AudioRouterEvent> {
        self.events.subscribe()
    }
}
