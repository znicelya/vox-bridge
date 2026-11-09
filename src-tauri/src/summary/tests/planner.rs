use crate::{
    intelligence::types::*,
    summary::{
        content::SummaryContent,
        planner::*,
        snapshot::{FinalSnapshot, SnapshotIdentity, SnapshotRow},
    },
};
use uuid::Uuid;
fn snapshot(texts: &[String]) -> FinalSnapshot {
    FinalSnapshot {
        session_id: Uuid::from_u128(1),
        last_rowid: Some(texts.len() as i64),
        rows: texts
            .iter()
            .enumerate()
            .map(|(i, text)| SnapshotRow {
                rowid: (i + 1) as i64,
                id: Uuid::from_u128(i as u128 + 2),
                direction: "source_to_target".into(),
                sequence: i as i64,
                occurred_at_seconds: -1,
                occurred_at_ms: None,
                kind: None,
                language: "zh".into(),
                text: text.clone(),
                identity: SnapshotIdentity::default(),
            })
            .collect(),
        input_final_count: texts.len() as u32,
        normalized_bytes: 0,
        input_hash: "a".repeat(64),
    }
}
fn profile() -> ModelProfile {
    crate::intelligence::test_support::test_llm_profile()
}
fn node(n: usize, len: usize) -> SummaryNode {
    let content = SummaryContent {
        summary: "x".repeat(len),
        participants: vec![],
        companies: vec![],
        requirements: vec![],
        action_items: vec![],
        risks: vec![],
        terms: vec![],
    };
    SummaryNode {
        content,
        coverage: n..n + 1,
    }
}
#[test]
fn summary_planner_unicode_parts_reconstruct_exact_bytes_and_all_rendered_escaping() {
    let original = "客户泵😀e\u{301}\"\\\n".repeat(1000);
    let input = snapshot(&[original.clone(), "".into(), "尾部".into()]);
    let p = profile();
    let maps = plan_maps(&input, &p).unwrap();
    let mut actual = String::new();
    for part in maps.manifest.parts.iter().filter(|p| p.snapshot_row == 0) {
        assert!(input.rows[0].text.is_char_boundary(part.byte_range.start));
        assert!(input.rows[0].text.is_char_boundary(part.byte_range.end));
        actual.push_str(&input.rows[0].text[part.byte_range.clone()]);
    }
    assert_eq!(actual, original);
    assert!(maps.manifest.parts.len() > 3);
    assert!(
        maps.manifest
            .parts
            .iter()
            .any(|p| p.snapshot_row == 1 && p.byte_range.is_empty())
    );
    for plan in &maps.requests {
        let request = render_map(&input, &maps.manifest, plan, &p).unwrap();
        let bytes = serde_json::to_string(&request.system).unwrap().len()
            + serde_json::to_string(&request.user).unwrap().len()
            + 1024;
        assert!(bytes <= 3584, "escaped request used {bytes} bytes");
        let value: serde_json::Value = serde_json::from_str(&request.user).unwrap();
        assert!(value["inputs"].is_array());
    }
}
#[test]
fn summary_planner_reserves_complete_maps_and_rejects_64_maps_before_dispatch() {
    let input = snapshot(&vec!["x".repeat(1400); 64]);
    assert!(
        matches!(
            plan_maps(&input, &profile()),
            Err(IntelligenceErrorCode::ContextTooLarge)
        ),
        "64 maps require an illegal additional root call"
    );
    let input = snapshot(&["x".repeat(2 * 1024 * 1024)]);
    assert!(
        matches!(
            plan_maps(&input, &profile()),
            Err(IntelligenceErrorCode::ContextTooLarge)
        ),
        "whole manifest must fail before request65"
    );
}
#[test]
fn summary_planner_no_pair_and_prospective_call65_level9_fail_without_counter_reset() {
    let p = profile();
    let frontier = vec![node(0, 1500), node(1, 1500)];
    let mut counters = PlannerCounters {
        calls_reserved: 2,
        level: 1,
    };
    assert!(matches!(
        plan_reduce(&frontier, &p, &mut counters),
        Err(IntelligenceErrorCode::ContextTooLarge)
    ));
    assert_eq!(counters.calls_reserved, 2);
    let frontier = vec![node(0, 1), node(1, 1)];
    let mut counters = PlannerCounters {
        calls_reserved: 64,
        level: 1,
    };
    assert!(matches!(
        plan_reduce(&frontier, &p, &mut counters),
        Err(IntelligenceErrorCode::ContextTooLarge)
    ));
    assert_eq!(counters.calls_reserved, 64);
    let mut counters = PlannerCounters {
        calls_reserved: 1,
        level: 8,
    };
    assert!(matches!(
        plan_reduce(&frontier, &p, &mut counters),
        Err(IntelligenceErrorCode::ContextTooLarge)
    ));
    assert_eq!(counters.level, 8);
}
#[test]
fn summary_planner_reductions_carry_singletons_preserve_adjacent_coverage_and_progress() {
    let frontier = vec![node(0, 800), node(1, 800), node(2, 800)];
    let p = profile();
    let mut counters = PlannerCounters {
        calls_reserved: 3,
        level: 1,
    };
    let level = plan_reduce(&frontier, &p, &mut counters).unwrap();
    assert_eq!(level.len(), 2);
    assert_eq!(level[0].input_range, 0..2);
    assert_eq!(level[0].coverage, 0..2);
    assert!(level[0].request.is_some());
    assert_eq!(level[1].input_range, 2..3);
    assert_eq!(level[1].coverage, 2..3);
    assert!(level[1].request.is_none());
    assert_eq!(counters.calls_reserved, 4);
    let mut bad = frontier;
    bad[1].coverage = 3..4;
    assert!(
        matches!(
            plan_reduce(&bad, &p, &mut counters),
            Err(IntelligenceErrorCode::ContextTooLarge)
        ),
        "foreign/gapped coverage cannot enter a reducing request"
    );
}
