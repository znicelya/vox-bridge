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
    InterimTranscript {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        language: String,
        text: String,
    },
    FinalTranscript {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        language: String,
        text: String,
    },
    TranslationAudio {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        language: String,
        samples: Vec<i16>,
    },
    Error {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        message: String,
    },
    Latency {
        event_id: Uuid,
        session_id: Uuid,
        occurred_at: SystemTime,
        stage: String,
        millis: u64,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> Uuid {
        match self {
            Self::Status { session_id, .. }
            | Self::InterimTranscript { session_id, .. }
            | Self::FinalTranscript { session_id, .. }
            | Self::TranslationAudio { session_id, .. }
            | Self::Error { session_id, .. }
            | Self::Latency { session_id, .. } => *session_id,
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
}
