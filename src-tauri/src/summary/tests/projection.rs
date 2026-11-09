use super::harness::SummaryHarness;
use crate::{
    intelligence::types::IntelligenceErrorCode,
    storage::SummaryVersion,
    summary::types::{SummaryCursor, SummaryView},
};
use uuid::Uuid;

#[test]
fn summary_cursor_rejects_noncanonical_and_out_of_range_wire_values() {
    for raw in [
        "",
        "00",
        "01",
        "+1",
        "-0",
        "-01",
        " 1",
        "1 ",
        "1.0",
        "9223372036854775808",
        "-9223372036854775809",
        "000000000000000000001",
    ] {
        let wire = serde_json::to_string(raw).unwrap();
        assert!(
            serde_json::from_str::<SummaryCursor>(&wire).is_err(),
            "noncanonical cursor accepted: {raw}"
        );
    }
    for raw in [
        "-9223372036854775808",
        "-1",
        "0",
        "4294967296",
        "9007199254740993",
        "9223372036854775807",
    ] {
        let wire = serde_json::to_string(raw).unwrap();
        assert_eq!(
            serde_json::to_string(&serde_json::from_str::<SummaryCursor>(&wire).unwrap()).unwrap(),
            wire
        );
    }
    assert!(serde_json::from_str::<SummaryCursor>("1").is_err());
}

fn legacy(h: &SummaryHarness, id: Uuid, version: i64, content: &str) {
    h.store
        .create_summary_version(&SummaryVersion {
            id: Uuid::new_v4(),
            session_id: id,
            version,
            created_at: -9,
            status: "complete".into(),
            structured_json: content.into(),
        })
        .unwrap();
}

#[tokio::test]
async fn summary_projection_unbounded_none_and_exact_signed_keyset_preserve_legacy() {
    let h = SummaryHarness::new().await;
    let id = h.completed_session(&[]).await;
    for (v, text) in [
        (-9, r#"{"title":"标题","text":"旧正文"}"#),
        (4_294_967_296, r#"{"text":"大版本"}"#),
        (9_007_199_254_740_993, r#"{"text":"JS之外"}"#),
    ] {
        legacy(&h, id, v, text);
    }
    let first = h.service.list(id, None, 2).unwrap();
    let last = h.service.list(id, first.next_cursor.clone(), 2).unwrap();
    h.close().await;
    assert_eq!(
        serde_json::to_value(&first.next_cursor).unwrap(),
        serde_json::json!("4294967296")
    );
    assert_eq!(first.rows.len(), 2);
    match &first.rows[0] {
        SummaryView::Legacy(row) => {
            assert_eq!(row.version, "-9");
            assert_eq!(row.status, "complete");
            assert_eq!(row.created_at_seconds, "-9");
            assert!(row.created_at_ms.is_none());
            assert_eq!(row.content.as_ref().unwrap()["title"], "标题");
            assert!(row.profile_id.is_none());
        }
        _ => panic!("legacy must remain tagged"),
    }
    match &last.rows[0] {
        SummaryView::Legacy(row) => assert_eq!(row.version, "9007199254740993"),
        _ => panic!("large signed legacy must remain tagged"),
    }
    assert!(last.next_cursor.is_none());
    let wire = serde_json::to_value(&first).unwrap();
    assert_eq!(wire["rows"][0]["origin"], "legacy");
}

#[tokio::test]
async fn summary_projection_invalid_legacy_is_visible_and_raw_fields_fail_safely() {
    let h = SummaryHarness::new().await;
    let id = h.completed_session(&[]).await;
    legacy(&h, id, 1, "{invalid");
    legacy(
        &h,
        id,
        2,
        r#"{"text":"keep","raw_text":"raw-response-sentinel"}"#,
    );
    let page = h.service.list(id, None, 50).unwrap();
    h.close().await;
    assert_eq!(page.rows.len(), 2);
    for view in &page.rows {
        match view {
            SummaryView::Legacy(row) => {
                assert!(row.content.is_none());
                assert_eq!(
                    row.projection_error,
                    Some(IntelligenceErrorCode::InvalidResponse)
                );
            }
            _ => panic!("invalid legacy was relabeled Native"),
        }
    }
    assert!(
        !serde_json::to_string(&page)
            .unwrap()
            .contains("raw-response-sentinel")
    );
    assert_eq!(
        h.store.list_summary_versions(id).unwrap()[0].structured_json,
        "{invalid"
    );
}

#[tokio::test]
async fn summary_projection_page_accounts_for_full_signed_cursor_framing_and_continues_without_loss()
 {
    let h = SummaryHarness::new().await;
    let id = h.completed_session(&[]).await;
    let versions = [
        i64::MAX - 4,
        i64::MAX - 3,
        i64::MAX - 2,
        i64::MAX - 1,
        i64::MAX,
    ];
    for version in versions {
        legacy(&h, id, version, r#"{"text":""}"#);
    }
    let empty = h.service.list(id, None, 100).unwrap();
    let cap = 8 * 1024 * 1024;
    let row_target = (cap - 36) / 4;
    for row in &empty.rows[..4] {
        let SummaryView::Legacy(row) = row else {
            panic!("expected Legacy");
        };
        let overhead = serde_json::to_vec(&SummaryView::Legacy(row.clone()))
            .unwrap()
            .len();
        let content = serde_json::json!({"text":"x".repeat(row_target-overhead)}).to_string();
        assert!(content.len() < 2 * 1024 * 1024);
        h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE session_summaries SET structured_json=?2 WHERE id=?1",
                    rusqlite::params![row.id.to_string(), content],
                )?;
                Ok(())
            })
            .unwrap();
    }
    let mut after = None;
    let mut seen = Vec::new();
    loop {
        let page = h.service.list(id, after, 100).unwrap();
        let bytes = serde_json::to_vec(&page).unwrap().len();
        assert!(
            bytes <= cap,
            "actual serialized page including next_cursor exceeded 8MiB: {bytes}"
        );
        assert!(
            !page.rows.is_empty(),
            "size-limited paging must make progress"
        );
        for row in page.rows {
            let SummaryView::Legacy(row) = row else {
                panic!("expected Legacy");
            };
            seen.push(row.version);
        }
        after = page.next_cursor;
        if after.is_none() {
            break;
        }
    }
    h.close().await;
    assert_eq!(seen, versions.map(|v| v.to_string()));
}

#[tokio::test]
async fn summary_projection_escaped_legacy_status_is_bounded_and_native_invalid_content_never_falls_back()
 {
    let h = SummaryHarness::new().await;
    let legacy_session = h.completed_session(&[]).await;
    legacy(&h, legacy_session, 1, r#"{"text":"legacy"}"#);
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET status=?2 WHERE session_id=?1",
                rusqlite::params![legacy_session.to_string(), "\0".repeat(64 * 1024)],
            )?;
            Ok(())
        })
        .unwrap();
    let page = h.service.list(legacy_session, None, 100).unwrap();
    let SummaryView::Legacy(row) = &page.rows[0] else {
        panic!("escaped Legacy status lost origin");
    };
    assert_eq!(row.status.len(), 64 * 1024);
    assert!(serde_json::to_vec(&page).unwrap().len() < 8 * 1024 * 1024);
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET status=?2 WHERE session_id=?1",
                rusqlite::params![legacy_session.to_string(), "x".repeat(64 * 1024 + 1)],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(h.service.list(legacy_session, None, 100).is_err());
    let native_session = h.completed_session(&["complete input"]).await;
    let job = h.generate(native_session, None).await.unwrap();
    h.wait(job.id).await;
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET structured_json='{}' WHERE id=?1",
                [job.object_id.unwrap().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        h.service.list(native_session, None, 100),
        Err(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::Storage
        ))
    ));
    h.close().await;
}

#[tokio::test]
async fn summary_projection_legacy_inline_audio_stays_out_of_wire_and_every_export() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&[]).await;
    let raw = r#"{"title":"旧标题","text":"保留旧内容","audio":"inline-audio-sentinel","audio_base64":"encoded-audio-sentinel","audio_bytes":[1,2,3],"metadata":{"audioData":"nested-audio-sentinel","api_key":"synthetic-key-sentinel"}}"#;
    legacy(&h, session, 1, raw);
    let wire = serde_json::to_string(&h.service.list(session, None, 50).unwrap()).unwrap();
    let data = h.store.get_export_data(session).unwrap().unwrap();
    let outputs = [
        wire,
        crate::export::json::render_json(&data).unwrap(),
        crate::export::markdown::render_markdown(&data).unwrap(),
        crate::export::plain_text::render_plain_text(&data).unwrap(),
    ];
    h.close().await;
    for output in outputs {
        assert!(output.contains("保留旧内容"));
        for sentinel in [
            "inline-audio-sentinel",
            "encoded-audio-sentinel",
            "nested-audio-sentinel",
            "synthetic-key-sentinel",
        ] {
            assert!(
                !output.contains(sentinel),
                "legacy projection exposed {sentinel}"
            );
        }
        assert!(!output.contains("audio_bytes"));
    }
    assert_eq!(
        h.store.list_summary_versions(session).unwrap()[0].structured_json,
        raw
    );
}
