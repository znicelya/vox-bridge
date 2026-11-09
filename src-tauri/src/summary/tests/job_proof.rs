use crate::{
    intelligence::{test_support::IntelligenceHarness, types::*},
    summary::{SummaryJobRequest, job_proof::SummaryJobProof},
};
use rusqlite::params;

fn fixture() -> (IntelligenceHarness, JobView, serde_json::Value) {
    let h = IntelligenceHarness::new();
    let session = h.session();
    let job = h
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: session,
            profile: Some(crate::intelligence::test_support::test_llm_profile()),
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    let payload = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                [job.id.to_string()],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    (h, job, serde_json::from_str(&payload).unwrap())
}
#[test]
fn summary_job_proof_anchors_trusted_id_and_rechecks_the_raw_byte_witness() {
    let (h, job, claimed) = fixture();
    let proof = h
        .store
        .transaction(|tx| SummaryJobProof::claim_in_tx(tx, job.id, &claimed))
        .unwrap();
    h.store.transaction(|tx| proof.require_in_tx(tx)).unwrap();
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET payload_json=payload_json || ' ' WHERE id=?1",
                [job.id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(
        h.store.transaction(|tx| proof.require_in_tx(tx)).is_err(),
        "carried digest cannot authorize changed durable bytes"
    );
    assert!(
        h.store
            .transaction(|tx| SummaryJobProof::claim_in_tx(tx, job.id, &claimed))
            .is_err(),
        "claim must compare the independently stored admission witness"
    );
}

#[test]
fn summary_job_proof_rejects_each_association_profile_and_claim_drift_before_entry() {
    for field in [
        "kind",
        "object",
        "session",
        "exclusion",
        "permanent",
        "input_summary",
        "input_session",
        "input_extra",
        "profile_id",
        "profile_kind",
        "profile_name",
        "profile_url",
        "profile_model",
        "profile_timeout",
        "profile_context",
        "profile_output",
        "profile_dimensions",
        "profile_count",
        "native_profile",
        "native_model",
        "witness_missing",
        "witness_bad",
    ] {
        let (h, job, claimed) = fixture();
        let id = job.id.to_string();
        let native = job.object_id.unwrap().to_string();
        let other_session = h.session();
        h.store.transaction(|tx| {
            let mut payload=claimed.clone();
            let foreign=uuid::Uuid::new_v4().to_string();
            match field {
                "kind"=>{tx.execute("UPDATE intelligence_jobs SET kind='research' WHERE id=?1",[&id])?;},
                "object"=>{tx.execute("UPDATE intelligence_jobs SET object_id=?2 WHERE id=?1",params![id,foreign])?;},
                "session"=>{let other=other_session;tx.execute("UPDATE intelligence_jobs SET session_id=?2 WHERE id=?1",params![id,other.to_string()])?;},
                "exclusion"=>{tx.execute("UPDATE intelligence_jobs SET exclusion_key='summary:foreign' WHERE id=?1",[&id])?;},
                "permanent"=>{tx.execute("UPDATE intelligence_jobs SET permanent_key='auto-summary:foreign' WHERE id=?1",[&id])?;},
                "input_summary"=>payload["input"]["summary_id"]=foreign.into(),
                "input_session"=>payload["input"]["session_id"]=foreign.into(),
                "input_extra"=>payload["input"]["extra"]="foreign".into(),
                "profile_id"=>payload["profiles"][0]["id"]=foreign.into(),
                "profile_kind"=>payload["profiles"][0]["kind"]="embedding".into(),
                "profile_name"=>payload["profiles"][0]["name"]="edited".into(),
                "profile_url"=>payload["profiles"][0]["base_url"]="http://127.0.0.1:23456/v1".into(),
                "profile_model"=>payload["profiles"][0]["model"]="edited".into(),
                "profile_timeout"=>payload["profiles"][0]["timeout_ms"]=2000.into(),
                "profile_context"=>payload["profiles"][0]["max_context_tokens"]=8192.into(),
                "profile_output"=>payload["profiles"][0]["max_output_tokens"]=1024.into(),
                "profile_dimensions"=>payload["profiles"][0]["dimensions"]=128.into(),
                "profile_count"=>{let p=payload["profiles"][0].clone();payload["profiles"].as_array_mut().unwrap().push(p);},
                "native_profile"=>{tx.execute("UPDATE session_summaries SET profile_id=?2 WHERE id=?1",params![native,foreign])?;},
                "native_model"=>{tx.execute("UPDATE session_summaries SET model='edited' WHERE id=?1",[&native])?;},
                "witness_missing"=>{tx.pragma_update(None,"ignore_check_constraints",true)?;tx.execute("UPDATE session_summaries SET job_payload_hash=NULL WHERE id=?1",[&native])?;},
                "witness_bad"=>{tx.execute("UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",params![native,"b".repeat(64)])?;},
                _=>unreachable!(),
            }
            if field.starts_with("input_") || field.starts_with("profile_") {tx.execute("UPDATE intelligence_jobs SET payload_json=?2 WHERE id=?1",params![id,payload.to_string()])?;}
            Ok(())
        }).unwrap();
        assert!(
            h.store
                .transaction(|tx| SummaryJobProof::claim_in_tx(tx, job.id, &claimed))
                .is_err(),
            "authorized admission-before-claim drift: {field}"
        );
    }
    let (h, job, mut claimed) = fixture();
    claimed["input"]["extra"] = "changed claim".into();
    assert!(
        h.store
            .transaction(|tx| SummaryJobProof::claim_in_tx(tx, job.id, &claimed))
            .is_err()
    );
}
