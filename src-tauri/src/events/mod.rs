//! Application event delivery boundaries.

mod bus;

pub use bus::{EVENT_CAPACITY, EventBus, EventBusError};

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    Status {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        status: String,
    },
    AudioState {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        state: AudioSessionState,
    },
    Transcript {
        update: crate::domain::transcript::TranscriptUpdate,
    },
    Diagnostic {
        diagnostic: crate::domain::transcript::RuntimeDiagnostic,
    },
    ResyncNeeded {
        session_id: Uuid,
        missed_events: u64,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> Uuid {
        match self {
            Self::Status { session_id, .. }
            | Self::AudioState { session_id, .. }
            | Self::ResyncNeeded { session_id, .. } => *session_id,
            Self::Transcript { update } => update.session_id,
            Self::Diagnostic { diagnostic } => diagnostic.session_id,
        }
    }

    pub fn status(session_id: Uuid, status: impl Into<String>) -> Self {
        Self::Status {
            event_id: Uuid::new_v4(),
            session_id,
            occurred_at: SystemTime::now(),
            status: status.into(),
        }
    }

    pub fn audio_state(session_id: Uuid, state: AudioSessionState) -> Self {
        Self::AudioState {
            event_id: Uuid::new_v4(),
            session_id,
            occurred_at: SystemTime::now(),
            state,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AudioSessionState {
    Running,
    Paused { reason: String },
}
