//! Converts directed provider subtitles into durable final rows and UI updates.
use crate::{
    domain::{
        session::{ProviderId, SessionDirection},
        transcript::{TranscriptDetails, TranscriptKind, TranscriptUpdate, invalid_transcript},
    },
    error::AppResult,
    events::{EventBus, SessionEvent},
    providers::{ProviderEvent, ProviderSessionConfig},
    storage::LocalStore,
};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const IDENTITY_CAPACITY: usize = 64;
#[derive(Clone, PartialEq, Eq)]
enum Correlation {
    Native(String),
    Local(u64),
    Unassociated,
}
#[derive(Clone, PartialEq, Eq)]
struct Key {
    kind: TranscriptKind,
    correlation: Correlation,
}
struct Identity {
    key: Key,
    id: Uuid,
    sequence: u64,
}

pub struct TranscriptSink {
    store: Arc<LocalStore>,
    bus: EventBus,
    config: ProviderSessionConfig,
    provider: ProviderId,
    epoch: u64,
    next_sequence: u64,
    active: VecDeque<Identity>,
    finalized: VecDeque<Key>,
    dropped_interims: u64,
}
impl TranscriptSink {
    pub fn new(
        store: Arc<LocalStore>,
        bus: EventBus,
        config: ProviderSessionConfig,
        provider: ProviderId,
    ) -> AppResult<Self> {
        if config.direction == SessionDirection::Bidirectional {
            return Err(invalid_transcript());
        }
        let next_sequence = store.next_transcript_sequence(config.session_id, config.direction)?;
        Ok(Self {
            store,
            bus,
            config,
            provider,
            epoch: 0,
            next_sequence,
            active: VecDeque::new(),
            finalized: VecDeque::new(),
            dropped_interims: 0,
        })
    }
    pub fn accept(&mut self, event: &ProviderEvent) -> AppResult<Option<TranscriptUpdate>> {
        let metadata = event.metadata();
        if metadata.session_id != self.config.session_id
            || metadata.direction != self.config.direction
        {
            return Err(invalid_transcript());
        }
        let (kind, is_final, text) = match event {
            ProviderEvent::SourceInterim { text, .. } => (TranscriptKind::Source, false, text),
            ProviderEvent::SourceFinal { text, .. } => (TranscriptKind::Source, true, text),
            ProviderEvent::TranslationInterim { text, .. } => {
                (TranscriptKind::Translation, false, text)
            }
            ProviderEvent::TranslationFinal { text, .. } => {
                (TranscriptKind::Translation, true, text)
            }
            _ => return Ok(None),
        };
        let expected_language = match kind {
            TranscriptKind::Source => self.config.languages.source(),
            TranscriptKind::Translation => self.config.languages.target(),
        };
        if text.translation.language != expected_language {
            return Err(invalid_transcript());
        }
        // Validate borrowed payload sizes before cloning any provider-controlled text.
        let bytes = text
            .translation
            .text
            .len()
            .saturating_add(text.translation.language.len())
            .saturating_add(metadata.provider_event_type.as_ref().map_or(0, String::len))
            .saturating_add(metadata.sentence_id.as_ref().map_or(0, String::len))
            .saturating_add(metadata.log_id.as_ref().map_or(0, String::len));
        if bytes > crate::domain::transcript::MAX_TRANSCRIPT_BYTES {
            return Err(invalid_transcript());
        }
        let key = Key {
            kind: kind.clone(),
            correlation: if let Some(native) = &metadata.sentence_id {
                Correlation::Native(native.clone())
            } else if let Some(local) = metadata.local_correlation_id {
                Correlation::Local(local)
            } else {
                Correlation::Unassociated
            },
        };
        let active_index = self.active.iter().position(|entry| entry.key == key);
        let (id, sequence) = active_index
            .map(|i| (self.active[i].id, self.active[i].sequence))
            .unwrap_or_else(|| (Uuid::new_v4(), self.next_sequence));
        let update = TranscriptUpdate {
            id,
            session_id: self.config.session_id,
            direction: self.config.direction,
            sequence,
            occurred_at_ms: i64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| invalid_transcript())?
                    .as_millis(),
            )
            .map_err(|_| invalid_transcript())?,
            language: text.translation.language.clone(),
            text: text.translation.text.clone(),
            is_final,
            details: Some(TranscriptDetails {
                kind,
                provider: self.provider.clone(),
                provider_config_id: self.config.provider_config_id,
                stream_epoch: self.epoch,
                provider_event_type: metadata.provider_event_type.clone(),
                provider_timestamp_ms: millis(metadata.timestamp)?,
                sentence_id: metadata.sentence_id.clone(),
                local_correlation_id: metadata.local_correlation_id,
                log_id: metadata.log_id.clone(),
                start_ms: millis(text.start)?,
                end_ms: millis(text.end)?,
            }),
        };
        update.validate()?;
        if key.correlation != Correlation::Unassociated && self.finalized.contains(&key) {
            return Ok(None);
        }
        let next_sequence = if active_index.is_none() {
            Some(
                self.next_sequence
                    .checked_add(1)
                    .ok_or_else(invalid_transcript)?,
            )
        } else {
            None
        };
        // Do not close an interim identity, consume a sequence or publish a final
        // unless SQLite accepted the row. Retrying a failed write keeps its ID.
        if is_final {
            self.store.append_transcript(&update)?;
        }
        if let Some(next) = next_sequence {
            self.next_sequence = next;
        }
        if is_final {
            if let Some(i) = active_index {
                self.active.remove(i);
            }
            if key.correlation != Correlation::Unassociated {
                if self.finalized.len() == IDENTITY_CAPACITY {
                    self.finalized.pop_front();
                }
                self.finalized.push_back(key);
            }
        } else if active_index.is_none() {
            if self.active.len() == IDENTITY_CAPACITY {
                self.active.pop_front();
                self.dropped_interims = self.dropped_interims.saturating_add(1);
            }
            self.active.push_back(Identity { key, id, sequence });
        }
        let _ = self.bus.publish(
            update.session_id,
            SessionEvent::Transcript {
                update: update.clone(),
            },
        );
        Ok(Some(update))
    }
    pub fn advance_epoch(&mut self) -> AppResult<u64> {
        self.epoch = self.epoch.checked_add(1).ok_or_else(invalid_transcript)?;
        self.clear_interims();
        self.finalized.clear();
        Ok(self.epoch)
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn clear_interims(&mut self) {
        self.active.clear();
    }
    pub fn take_dropped_interims(&mut self) -> u64 {
        std::mem::take(&mut self.dropped_interims)
    }
}
fn millis(duration: Option<Duration>) -> AppResult<Option<u64>> {
    duration
        .map(|value| u64::try_from(value.as_millis()).map_err(|_| invalid_transcript()))
        .transpose()
}
