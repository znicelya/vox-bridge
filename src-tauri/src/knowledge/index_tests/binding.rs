use super::*;
#[tokio::test]
async fn knowledge_binding_none_to_same_dimension_and_profile_endpoint_model_edits() {
    let h = KnowledgeHarness::new().await;
    let mut profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    profile.dimensions = None;
    h.h.config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Keep,
        })
        .await
        .unwrap();
    let id = h.import_text("binding", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let first = h.search("24V").await.unwrap().identity;
    profile.dimensions = Some(128);
    h.h.config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Keep,
        })
        .await
        .unwrap();
    assert_eq!(h.search("24V").await.unwrap().identity, first);
    profile.name = "Renamed".into();
    profile.timeout_ms = 2000;
    h.h.config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Keep,
        })
        .await
        .unwrap();
    assert_eq!(h.search("24V").await.unwrap().identity, first);
    for change in ["endpoint", "model", "profile"] {
        match change {
            "endpoint" => profile.base_url = "http://127.0.0.1:12346/v1".into(),
            "model" => profile.model = Some("changed-model".into()),
            "profile" => profile.id = Uuid::new_v4(),
            _ => unreachable!(),
        }
        h.h.config
            .save(SaveModelProfile {
                profile: profile.clone(),
                credential: if change == "profile" {
                    CredentialChange::Replace {
                        api_key: "synthetic".into(),
                    }
                } else {
                    CredentialChange::Keep
                },
            })
            .await
            .unwrap();
        let mut settings = h.h.config.settings().await.unwrap();
        settings.embedding_profile_id = Some(profile.id);
        h.h.config.save_settings(settings).await.unwrap();
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        assert!(h.search("24V").await.is_err(), "{change}");
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        assert_ne!(h.search("24V").await.unwrap().identity, first, "{change}");
    }
    h.close().await;
}
#[tokio::test]
async fn knowledge_binding_concurrent_first_index_conflicting_actual_dimensions_has_one_winner() {
    let h = IntelligenceHarness::new();
    let profile = ModelProfile {
        kind: ServiceKind::Embedding,
        max_context_tokens: None,
        max_output_tokens: None,
        dimensions: None,
        ..test_llm_profile()
    };
    h.config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic".into(),
            },
        })
        .await
        .unwrap();
    let mut settings = h.config.settings().await.unwrap();
    settings.embedding_profile_id = Some(profile.id);
    h.config.save_settings(settings).await.unwrap();
    let index = crate::knowledge::index::IndexOwner::new(
        h.temp_dir.path().join("indexes"),
        h.store.clone(),
    );
    index.start_owned(Arc::new(())).await.unwrap();
    let second_jobs = Arc::new(crate::intelligence::jobs::JobScheduler::new(
        h.store.clone(),
    ));
    let first_vectors = Arc::new(Axes::default());
    let second_vectors = Arc::new(Axes::default());
    let mut v = vec![0.; 256];
    v[0] = 1.;
    second_vectors.vectors.lock().unwrap().push_back(v);
    first_vectors.block.store(true, Ordering::SeqCst);
    second_vectors.block.store(true, Ordering::SeqCst);
    let first = KnowledgeService::new(
        h.store.clone(),
        h.config.clone(),
        h.jobs.clone(),
        first_vectors.clone(),
        index.clone(),
        h.temp_dir.path().into(),
    );
    let second = KnowledgeService::new(
        h.store.clone(),
        h.config.clone(),
        second_jobs.clone(),
        second_vectors.clone(),
        index.clone(),
        h.temp_dir.path().into(),
    );
    first.register_handlers().unwrap();
    second.register_handlers().unwrap();
    h.jobs.start().unwrap();
    let fixture = KnowledgeHarness {
        h,
        service: first.clone(),
        index: index.clone(),
        embeddings: first_vectors.clone(),
    };
    let one = fixture.import_text("one", "24V");
    let two = fixture.import_text("two", "48V");
    let spare = fixture.import_text("occupied first worker", "12V");
    let job1 = first.reindex_owned(one, Arc::new(())).await.unwrap();
    let entered = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        first_vectors.entered.notified().await;
        // Both schedulers claim from the same store. Keep both genuine first
        // workers occupied before starting the second scheduler, so its job
        // must enter its independent Core instead of waiting on the first serial.
        let occupied = first.reindex_owned(spare, Arc::new(())).await.unwrap();
        fixture.wait_running(occupied.id).await;
        second_jobs.start().unwrap();
        let job2 = second.reindex_owned(two, Arc::new(())).await.unwrap();
        second_vectors.entered.notified().await;
        fixture.h.jobs.cancel(occupied.id).await.unwrap();
        (job2, occupied)
    })
    .await;
    first_vectors.block.store(false, Ordering::SeqCst);
    second_vectors.block.store(false, Ordering::SeqCst);
    first_vectors.release.notify_waiters();
    second_vectors.release.notify_waiters();
    assert!(
        entered.is_ok(),
        "both independent cores must enter before either publishes"
    );
    let (job2, occupied) = entered.unwrap();
    assert_eq!(
        (
            first_vectors.calls.load(Ordering::SeqCst),
            second_vectors.calls.load(Ordering::SeqCst)
        ),
        (1, 1)
    );
    let statuses = [
        fixture.wait(job1.id).await.status,
        fixture.wait_on(&second_jobs, job2.id).await.status,
    ];
    assert_eq!(fixture.wait(occupied.id).await.status, JobStatus::Cancelled);
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == JobStatus::Succeeded)
            .count(),
        1,
        "conflicting actual dimensions must not both bind"
    );
    let (active, ready): (u32, u32) = fixture
        .h
        .store
        .transaction(|tx| {
            Ok((
                tx.query_row(
                    "SELECT count(*) FROM \
                    knowledge_index_generations WHERE \
                    state='active' AND semantic_key IS NOT NULL",
                    [],
                    |r| r.get(0),
                )?,
                tx.query_row(
                    "SELECT count(*) FROM documents WHERE status='ready'",
                    [],
                    |r| r.get(0),
                )?,
            ))
        })
        .unwrap();
    assert_eq!((active, ready), (1, 1));
    second_jobs.shutdown().await.unwrap();
    fixture.close().await;
}
