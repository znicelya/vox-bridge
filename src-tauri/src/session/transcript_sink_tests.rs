use super::transcript_sink::TranscriptSink;
use crate::{
    audio::frame::PcmSpec,
    domain::{
        session::{LanguagePair, ProviderId, SessionDirection, SessionMode},
        transcript::TranscriptKind,
        translation::{TimedTranslation, Translation},
    },
    events::{EventBus, SessionEvent},
    providers::{ProviderEvent, ProviderEventMetadata, ProviderSessionConfig},
    storage::{LocalStore, StoredSession},
};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[test]
fn reverse_source_keeps_its_role_and_interim_identity() {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let id = Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "call".into(),
            direction: "bidirectional".into(),
            source_language: "zh".into(),
            target_language: "en".into(),
        })
        .unwrap();
    let bus = EventBus::new();
    bus.open_session(id);
    let mut rx = bus.subscribe(id);
    let config = ProviderSessionConfig {
        session_id: id,
        direction: SessionDirection::TargetToSource,
        mode: SessionMode::Call,
        languages: LanguagePair::new("en", "zh").unwrap(),
        target_audio: PcmSpec::pcm16_mono(24_000),
        provider_config_id: Uuid::new_v4(),
    };
    let mut sink = TranscriptSink::new(
        store.clone(),
        bus,
        config,
        ProviderId::new("fixture").unwrap(),
    )
    .unwrap();
    let metadata = ProviderEventMetadata {
        session_id: id,
        direction: SessionDirection::TargetToSource,
        provider_event_type: Some("651".into()),
        timestamp: Some(Duration::from_millis(10)),
        sentence_id: Some("native-7".into()),
        local_correlation_id: None,
        log_id: None,
    };
    let text = TimedTranslation {
        translation: Translation {
            language: "en".into(),
            text: "pump".into(),
        },
        start: Some(Duration::from_millis(10)),
        end: Some(Duration::from_millis(80)),
    };
    let interim = sink
        .accept(&ProviderEvent::SourceInterim {
            metadata: metadata.clone(),
            text: text.clone(),
        })
        .unwrap()
        .unwrap();
    assert_eq!(store.count_segments(id).unwrap(), 0);
    let final_row = sink
        .accept(&ProviderEvent::SourceFinal { metadata, text })
        .unwrap()
        .unwrap();
    assert_eq!(interim.id, final_row.id);
    assert_eq!(interim.sequence, final_row.sequence);
    assert_eq!(
        store.transcript_page(id, None, 50).unwrap().rows,
        vec![final_row.clone()]
    );
    let details = final_row.details.as_ref().unwrap();
    assert_eq!(details.kind, TranscriptKind::Source);
    assert_eq!(details.sentence_id.as_deref(), Some("native-7"));
    assert_eq!(details.start_ms, Some(10));
    assert!(
        matches!(rx.try_recv().unwrap(), SessionEvent::Transcript { update }
        if !update.is_final)
    );
    assert!(
        matches!(rx.try_recv().unwrap(), SessionEvent::Transcript { update }
        if update == final_row)
    );
    let data = store.get_export_data(id).unwrap().unwrap();
    assert_eq!(
        crate::export::segment_role(&data, &data.segments[0]),
        "source"
    );
    let exported: serde_json::Value =
        serde_json::from_str(&crate::export::json::render_json(&data).unwrap()).unwrap();
    assert_eq!(exported["segments"][0]["origin"], "provider");
    assert_eq!(
        exported["segments"][0]["details"]["sentence_id"],
        "native-7"
    );
    assert_eq!(exported["segments"][0]["details"]["start_ms"], 10);
    assert_eq!(exported["segments"][0]["role"], "source");
}

fn fixture() -> (
    Arc<LocalStore>,
    EventBus,
    TranscriptSink,
    ProviderEventMetadata,
) {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let id = Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "call".into(),
            direction: "bidirectional".into(),
            source_language: "zh".into(),
            target_language: "en".into(),
        })
        .unwrap();
    let bus = EventBus::new();
    bus.open_session(id);
    let sink = TranscriptSink::new(
        store.clone(),
        bus.clone(),
        ProviderSessionConfig {
            session_id: id,
            direction: SessionDirection::SourceToTarget,
            mode: SessionMode::Call,
            languages: LanguagePair::new("zh", "en").unwrap(),
            target_audio: PcmSpec::pcm16_mono(24_000),
            provider_config_id: Uuid::new_v4(),
        },
        ProviderId::new("fixture").unwrap(),
    )
    .unwrap();
    let metadata = ProviderEventMetadata {
        session_id: id,
        direction: SessionDirection::SourceToTarget,
        provider_event_type: Some("655".into()),
        timestamp: None,
        sentence_id: None,
        local_correlation_id: None,
        log_id: Some("log-1".into()),
    };
    (store, bus, sink, metadata)
}
fn text(language: &str) -> TimedTranslation {
    TimedTranslation {
        translation: Translation {
            language: language.into(),
            text: "pump".into(),
        },
        start: Some(Duration::from_millis(10)),
        end: Some(Duration::from_millis(80)),
    }
}
#[test]
fn translation_before_source_preserves_separate_roles_and_native_correlation() {
    let (store, _, mut sink, mut metadata) = fixture();
    metadata.sentence_id = Some("native".into());
    metadata.local_correlation_id = Some(9);
    let translation = sink
        .accept(&ProviderEvent::TranslationFinal {
            metadata: metadata.clone(),
            text: text("en"),
        })
        .unwrap()
        .unwrap();
    let source = sink
        .accept(&ProviderEvent::SourceFinal {
            metadata,
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    assert_ne!(translation.id, source.id);
    assert_eq!(source.sequence, translation.sequence + 1);
    assert_eq!(
        translation.details.as_ref().unwrap().kind,
        TranscriptKind::Translation
    );
    assert_eq!(
        source.details.as_ref().unwrap().local_correlation_id,
        Some(9)
    );
    assert_eq!(store.count_segments(source.session_id).unwrap(), 2);
}
#[test]
fn absent_correlation_does_not_pair_final_sentences() {
    let (store, _, mut sink, metadata) = fixture();
    let event = ProviderEvent::TranslationFinal {
        metadata,
        text: text("en"),
    };
    let a = sink.accept(&event).unwrap().unwrap();
    let b = sink.accept(&event).unwrap().unwrap();
    assert_ne!(a.id, b.id);
    assert!(a.details.as_ref().unwrap().sentence_id.is_none());
    assert!(a.details.as_ref().unwrap().local_correlation_id.is_none());
    assert_eq!(store.count_segments(a.session_id).unwrap(), 2);
}
#[test]
fn identified_duplicates_are_suppressed_but_new_epochs_allocate_new_ids() {
    let (_, _, mut sink, mut metadata) = fixture();
    metadata.local_correlation_id = Some(1);
    let event = ProviderEvent::SourceFinal {
        metadata: metadata.clone(),
        text: text("zh"),
    };
    let a = sink.accept(&event).unwrap().unwrap();
    assert!(sink.accept(&event).unwrap().is_none());
    assert!(
        sink.accept(&ProviderEvent::Gap { metadata })
            .unwrap()
            .is_none()
    );
    assert_eq!(sink.epoch(), 0);
    assert_eq!(sink.advance_epoch().unwrap(), 1);
    let b = sink.accept(&event).unwrap().unwrap();
    assert_ne!(a.id, b.id);
    assert!(b.sequence > a.sequence);
    assert_eq!(b.details.unwrap().stream_epoch, 1);
}
#[test]
fn no_subscriber_does_not_prevent_final_storage_and_audio_is_ignored() {
    use crate::{audio::frame::AudioFrame, domain::audio::AudioSource};
    let (store, _, mut sink, metadata) = fixture();
    assert!(
        sink.accept(&ProviderEvent::Audio {
            metadata: metadata.clone(),
            frame: AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(24_000),
                vec![5; 1920]
            )
            .unwrap()
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(store.count_segments(metadata.session_id).unwrap(), 0);
    let row = sink
        .accept(&ProviderEvent::SourceFinal {
            metadata,
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .transcript_page(row.session_id, None, 50)
            .unwrap()
            .rows,
        vec![row]
    );
}
#[test]
fn failed_final_insert_never_publishes_and_can_retry_same_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, bus, _, metadata) = fixture();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    store
        .create_session(&fixture.get_session(metadata.session_id).unwrap().unwrap())
        .unwrap();
    let mut sink = TranscriptSink::new(
        store.clone(),
        bus.clone(),
        ProviderSessionConfig {
            session_id: metadata.session_id,
            direction: metadata.direction,
            mode: SessionMode::Call,
            languages: LanguagePair::new("zh", "en").unwrap(),
            target_audio: PcmSpec::pcm16_mono(24_000),
            provider_config_id: Uuid::new_v4(),
        },
        ProviderId::new("fixture").unwrap(),
    )
    .unwrap();
    let mut rx = bus.subscribe(metadata.session_id);
    let interim = sink
        .accept(&ProviderEvent::SourceInterim {
            metadata: metadata.clone(),
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    rx.try_recv().unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_final BEFORE INSERT ON translation_segments BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    let event = ProviderEvent::SourceFinal {
        metadata,
        text: text("zh"),
    };
    assert!(sink.accept(&event).is_err());
    assert!(rx.try_recv().is_err());
    assert_eq!(store.count_segments(interim.session_id).unwrap(), 0);
    connection
        .execute_batch("DROP TRIGGER fail_final;")
        .unwrap();
    assert_eq!(sink.accept(&event).unwrap().unwrap().id, interim.id);
}
#[test]
fn bounded_interims_drop_oldest_and_report_resettable_count() {
    let (_, _, mut sink, mut metadata) = fixture();
    metadata.sentence_id = Some("0".into());
    let first = sink
        .accept(&ProviderEvent::SourceInterim {
            metadata: metadata.clone(),
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    for n in 1..66 {
        metadata.sentence_id = Some(n.to_string());
        sink.accept(&ProviderEvent::SourceInterim {
            metadata: metadata.clone(),
            text: text("zh"),
        })
        .unwrap();
    }
    assert_eq!(sink.take_dropped_interims(), 2);
    assert_eq!(sink.take_dropped_interims(), 0);
    metadata.sentence_id = Some("0".into());
    let final_row = sink
        .accept(&ProviderEvent::SourceFinal {
            metadata,
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    assert_ne!(first.id, final_row.id);
    sink.clear_interims();
    assert_eq!(sink.take_dropped_interims(), 0);
}
#[test]
fn invalid_direction_size_or_times_never_publish_or_persist() {
    let (store, bus, mut sink, metadata) = fixture();
    let mut rx = bus.subscribe(metadata.session_id);
    let mut wrong = metadata.clone();
    wrong.direction = SessionDirection::TargetToSource;
    assert!(
        sink.accept(&ProviderEvent::SourceFinal {
            metadata: wrong,
            text: text("zh")
        })
        .is_err()
    );
    let mut huge = text("zh");
    huge.translation.text = "x".repeat(262_145);
    assert!(
        sink.accept(&ProviderEvent::SourceFinal {
            metadata: metadata.clone(),
            text: huge
        })
        .is_err()
    );
    let mut invalid_time = text("zh");
    invalid_time.end = Some(Duration::MAX);
    assert!(
        sink.accept(&ProviderEvent::SourceFinal {
            metadata: metadata.clone(),
            text: invalid_time
        })
        .is_err()
    );
    assert_eq!(store.count_segments(metadata.session_id).unwrap(), 0);
    assert!(rx.try_recv().is_err());
}
#[test]
fn frontend_transcript_and_diagnostic_shapes_are_audio_free() {
    use crate::domain::transcript::{DiagnosticPayload, RuntimeDiagnostic, RuntimeFailureCode};
    let (_, _, mut sink, metadata) = fixture();
    let update = sink
        .accept(&ProviderEvent::SourceFinal {
            metadata: metadata.clone(),
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    let value = serde_json::to_value(SessionEvent::Transcript { update }).unwrap();
    assert_eq!(value["type"], "transcript");
    assert_eq!(value["update"]["direction"], "source_to_target");
    assert_eq!(value["update"]["details"]["kind"], "source");
    assert!(value["update"].get("samples").is_none());
    let value = serde_json::to_value(SessionEvent::Diagnostic {
        diagnostic: RuntimeDiagnostic {
            id: Uuid::new_v4(),
            session_id: metadata.session_id,
            direction: None,
            stream_epoch: None,
            occurred_at_ms: 80,
            payload: DiagnosticPayload::Failure {
                code: RuntimeFailureCode::Storage,
                provider_code: None,
                log_id: None,
            },
        },
    })
    .unwrap();
    assert_eq!(value["type"], "diagnostic");
    assert_eq!(value["diagnostic"]["payload"]["failure"]["code"], "storage");
    assert!(value["diagnostic"].get("samples").is_none());
}

#[test]
fn unsigned_provider_metadata_is_preserved_without_narrowing_to_sqlite_integers() {
    let (store, _, mut sink, mut metadata) = fixture();
    metadata.local_correlation_id = Some(u64::MAX);
    metadata.timestamp = Some(Duration::from_millis(u64::MAX));
    let row = sink
        .accept(&ProviderEvent::SourceFinal {
            metadata,
            text: text("zh"),
        })
        .unwrap()
        .unwrap();
    let stored = store
        .transcript_page(row.session_id, None, 20)
        .unwrap()
        .rows
        .remove(0);
    let details = stored.details.unwrap();
    assert_eq!(details.local_correlation_id, Some(u64::MAX));
    assert_eq!(details.provider_timestamp_ms, Some(u64::MAX));
}
#[test]
fn finalized_identity_window_expires_without_suppressing_later_unassociated_finals() {
    let (_, _, mut sink, mut metadata) = fixture();
    metadata.sentence_id = Some("0".into());
    let first_event = ProviderEvent::SourceFinal {
        metadata: metadata.clone(),
        text: text("zh"),
    };
    let first = sink.accept(&first_event).unwrap().unwrap();
    for n in 1..=64 {
        metadata.sentence_id = Some(n.to_string());
        sink.accept(&ProviderEvent::SourceFinal {
            metadata: metadata.clone(),
            text: text("zh"),
        })
        .unwrap();
    }
    let later = sink.accept(&first_event).unwrap().unwrap();
    assert_ne!(later.id, first.id);
    assert_eq!(later.sequence, 65);
}
