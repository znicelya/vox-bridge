//! One coalesced local SQL owner per scheduler generation.
use super::*;
use crate::storage::SqlSlot;

#[derive(Default)]
struct Requests {
    queued: bool,
    queued_complete: bool,
    interrupt: u64,
    interrupted: u64,
    finish: bool,
    finished: bool,
    failed: bool,
}

#[derive(Default)]
pub(super) struct Maintenance {
    requests: Mutex<Requests>,
    changed: Notify,
    sql: SqlSlot,
}

enum Work {
    Queued,
    Interrupt(u64),
    Finish,
    Wait,
}

impl Maintenance {
    pub(super) fn request_queued(&self) {
        let mut requests = self.requests.lock().unwrap();
        if !requests.finished && !requests.queued_complete {
            requests.queued = true;
        }
        drop(requests);
        self.changed.notify_waiters();
    }

    pub(super) fn request_interrupt(&self) -> AppResult<u64> {
        let mut requests = self.requests.lock().unwrap();
        if requests.failed {
            return Err(error(IntelligenceErrorCode::Interrupted));
        }
        if !requests.finished && requests.interrupt == requests.interrupted {
            requests.interrupt = requests
                .interrupt
                .checked_add(1)
                .ok_or_else(|| error(IntelligenceErrorCode::ResourceLimit))?;
        }
        let round = requests.interrupt;
        drop(requests);
        self.changed.notify_waiters();
        Ok(round)
    }

    pub(super) async fn observe_interrupt(&self, round: u64) -> AppResult<()> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let requests = self.requests.lock().unwrap();
                if requests.failed {
                    return Err(error(IntelligenceErrorCode::Interrupted));
                }
                if requests.interrupted >= round {
                    return Ok(());
                }
            }
            changed.await;
        }
    }

    pub(super) fn finish(&self) {
        let mut requests = self.requests.lock().unwrap();
        requests.finish = true;
        if !requests.queued_complete {
            requests.queued = true;
        }
        drop(requests);
        self.changed.notify_waiters();
    }

    pub(super) async fn drain(&self) -> AppResult<()> {
        self.sql.drain().await
    }

    pub(super) fn start(
        self: &Arc<Self>,
        store: Arc<LocalStore>,
        handlers: Arc<Handlers>,
        events: broadcast::Sender<IntelligenceEvent>,
        owner: OperationLease,
    ) -> tokio::task::JoinHandle<AppResult<()>> {
        let maintenance = self.clone();
        tokio::spawn(async move {
            loop {
                let changed = maintenance.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let work = {
                    let requests = maintenance.requests.lock().unwrap();
                    if requests.queued {
                        Work::Queued
                    } else if requests.interrupt > requests.interrupted {
                        Work::Interrupt(requests.interrupt)
                    } else if requests.finish {
                        Work::Finish
                    } else {
                        Work::Wait
                    }
                };
                let round = match work {
                    Work::Wait => {
                        changed.await;
                        continue;
                    }
                    Work::Finish => {
                        maintenance.sql.drain().await?;
                        let mut requests = maintenance.requests.lock().unwrap();
                        // A timeout observer may request interruption while
                        // drain yields. Retire only after that exact round.
                        if requests.queued || requests.interrupt > requests.interrupted {
                            continue;
                        }
                        requests.finished = true;
                        drop(requests);
                        maintenance.changed.notify_waiters();
                        return Ok(());
                    }
                    Work::Queued => None,
                    Work::Interrupt(round) => Some(round),
                };
                let storage = store.clone();
                let callbacks = handlers.clone();
                let result = maintenance
                    .sql
                    .run(owner.clone(), move || {
                        if round.is_some() {
                            interrupt(&storage, &callbacks)
                        } else {
                            cancel_queued(&storage, &callbacks)
                        }
                    })
                    .await;
                match result {
                    Ok(jobs) => {
                        for job in jobs {
                            let _ = events.send(IntelligenceEvent::JobChanged { job });
                        }
                        let mut requests = maintenance.requests.lock().unwrap();
                        if let Some(round) = round {
                            requests.interrupted = round;
                        } else {
                            requests.queued = false;
                            requests.queued_complete = true;
                        }
                        drop(requests);
                        maintenance.changed.notify_waiters();
                    }
                    Err(error) => {
                        if maintenance.sql.failed() {
                            maintenance.requests.lock().unwrap().failed = true;
                            maintenance.changed.notify_waiters();
                            return Err(error);
                        }
                        // Local retry retains this exact owner and one actual
                        // slot. An observer deadline does not submit a new task.
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }
            }
        })
    }
}

fn cancel_queued(store: &LocalStore, handlers: &Handlers) -> AppResult<Vec<JobView>> {
    store
        .transaction(|tx| {
            let ids = tx
                .prepare("SELECT id FROM intelligence_jobs WHERE status='queued' LIMIT 33")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            if ids.len() > store::MAX_UNFINISHED {
                return Err(error(IntelligenceErrorCode::ResourceLimit));
            }
            let mut changed = Vec::with_capacity(ids.len());
            for raw in ids {
                let id =
                    Uuid::parse_str(&raw).map_err(|_| error(IntelligenceErrorCode::Storage))?;
                transition_tx(
                    tx,
                    id,
                    "queued",
                    JobStatus::Cancelled,
                    Some(IntelligenceErrorCode::Cancelled),
                    handlers,
                    false,
                )?;
                if let Some(job) = store::job_in_transaction(tx, id)? {
                    changed.push(job);
                }
            }
            Ok(changed)
        })
        .map_err(store::safe_store_error)
}
