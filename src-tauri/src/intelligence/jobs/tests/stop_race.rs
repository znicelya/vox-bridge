use super::*;
use std::time::Instant;

// Regression: a requested, genuinely joined normal worker exit may win after
// stop returned Pending in the same select poll. It must not poison close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requested_stop_between_stop_poll_and_worker_join_closes_and_rearms() {
    stop_at_pending_poll(false).await;
}

// The same window cannot turn a genuinely aborted worker into a successful
// join merely because stop was requested. Actual leases still drain first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requested_stop_between_stop_poll_and_worker_join_keeps_abnormal_exit_failed() {
    stop_at_pending_poll(true).await;
}

async fn stop_at_pending_poll(abort_worker: bool) {
    let h = IntelligenceHarness::new();
    let handler = handler();
    h.jobs.register(JobKind::Import, handler.clone()).unwrap();
    let path = h.temp_dir.path().join("stop-poll-process.lock");
    let process = Arc::new(
        std::fs::File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap(),
    );
    fs2::FileExt::try_lock_exclusive(&*process).unwrap();
    let retained = Arc::downgrade(&process);
    let polls = Arc::new(AtomicUsize::new(0));
    let original_workers = Arc::new(AtomicUsize::new(0));
    let initially_open = Arc::new(AtomicBool::new(false));
    let ready_to_join = Arc::new(AtomicBool::new(false));
    let probe_finished = Arc::new(AtomicBool::new(false));
    let joined_observed = Arc::new(AtomicBool::new(false));
    let joined_normally = Arc::new(AtomicBool::new(false));
    let aborted_before_stop = Arc::new(AtomicBool::new(false));
    *h.jobs.oo1_stop_poll_probe.lock().unwrap() = Some(Arc::new({
        let jobs = Arc::downgrade(&h.jobs);
        let polls = polls.clone();
        let original_workers = original_workers.clone();
        let initially_open = initially_open.clone();
        let ready_to_join = ready_to_join.clone();
        let probe_finished = probe_finished.clone();
        let joined_observed = joined_observed.clone();
        let joined_normally = joined_normally.clone();
        let aborted_before_stop = aborted_before_stop.clone();
        move |point| {
            if let StopPollPoint::Joined { normal } = point {
                joined_normally.store(normal, Ordering::Release);
                joined_observed.store(true, Ordering::Release);
                return;
            }
            if polls.fetch_add(1, Ordering::SeqCst) != 0 {
                return;
            }
            let jobs = jobs.upgrade().unwrap();
            initially_open.store(!*jobs.signal.borrow(), Ordering::Release);
            let workers = jobs.oo1_workers.lock().unwrap().clone();
            original_workers.store(workers.len(), Ordering::Release);
            if abort_worker {
                workers[0].abort();
                // abort() is a request, and a running poll can still return
                // normally after stop. Finish abortion while stop is false;
                // this idle worker has no normal exit available in that window.
                let deadline = Instant::now() + Duration::from_secs(2);
                while !workers[0].is_finished() && Instant::now() < deadline {
                    std::thread::yield_now();
                }
                aborted_before_stop.store(
                    workers[0].is_finished() && !*jobs.signal.borrow(),
                    Ordering::Release,
                );
            }
            jobs.signal_shutdown();
            // Only this test's supervisor is paused. An original worker runs
            // on the other thread, so join_next can observe its real handle
            // before stop is repolled. The remaining worker drains normally
            // after this supervisor poll resumes.
            let deadline = Instant::now() + Duration::from_secs(2);
            while !workers.iter().any(|worker| worker.is_finished()) && Instant::now() < deadline {
                std::thread::yield_now();
            }
            ready_to_join.store(
                workers.iter().any(|worker| worker.is_finished()),
                Ordering::Release,
            );
            probe_finished.store(true, Ordering::Release);
        }
    }));
    h.jobs.start_owned(process).unwrap();
    let completion = h
        .jobs
        .state
        .lock()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .completion
        .clone();
    // The probe issues the stop. Await its actual generation completion rather
    // than racing the test's first shutdown call against the initial poll.
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        while !probe_finished.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let close = h.jobs.shutdown().await;
    let actual_process_join = tokio::time::timeout(Duration::from_secs(2), async {
        while retained.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let process_lock = std::fs::File::options()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let unlocked = fs2::FileExt::try_lock_exclusive(&process_lock).is_ok();

    // All actual task/SQL/maintenance/resource cleanup precedes assertions,
    // including the failing pre-fix run. is_finished above is an arrangement
    // guard; only the production joins release the process and publish done.
    assert!(observed.is_ok() && actual_process_join.is_ok() && unlocked);
    assert!(initially_open.load(Ordering::Acquire));
    assert_eq!(original_workers.load(Ordering::Acquire), 2);
    assert!(ready_to_join.load(Ordering::Acquire));
    assert!(joined_observed.load(Ordering::Acquire));
    assert_eq!(handler.calls.load(Ordering::SeqCst), 0);
    if abort_worker {
        assert!(aborted_before_stop.load(Ordering::Acquire));
        assert!(
            close.is_err() && !*completion.borrow(),
            "genuine original worker abortion did not fail close: {close:?}"
        );
        assert!(h.jobs.start().is_err());
        assert!(h.jobs.shutdown().await.is_err());
    } else {
        assert!(joined_normally.load(Ordering::Acquire));
        assert!(
            close.is_ok(),
            "normal requested worker exit poisoned close: {close:?}"
        );
        assert!(*completion.borrow());
        *h.jobs.oo1_stop_poll_probe.lock().unwrap() = None;
        h.jobs.start().unwrap();
        h.jobs.shutdown().await.unwrap();
    }
}
