//! Holds publication or an entered SQL owner before SQLite acquisition.
use super::*;
use crate::intelligence::types::JobView;
use std::sync::mpsc;
use tokio::sync::oneshot;

// Accepted local producer fixtures share the real mutation implementation;
// production callers must pass the completed combined delivery capability.
pub(crate) async fn generate_local_owned(
    service: &SummaryService,
    session: Uuid,
    profile: Option<Uuid>,
    owner: OperationLease,
) -> AppResult<JobView> {
    service
        .generate_with_readiness_owned(
            session,
            profile,
            owner,
            crate::summary::ManualReadiness::Local(service.core.recovery.ready()?),
        )
        .await
}

pub(crate) async fn delete_local_owned(
    service: &SummaryService,
    id: Uuid,
    owner: OperationLease,
) -> AppResult<()> {
    crate::summary::operations::delete_summary(
        service.core.clone(),
        service.jobs.clone(),
        id,
        owner,
        crate::summary::operations::DeleteFence::Ready(service.core.recovery.ready()?),
    )
    .await
}

struct Gate {
    entered: oneshot::Sender<()>,
    release: mpsc::Receiver<()>,
}
#[derive(Default)]
pub(crate) struct BoundaryControl {
    next: Mutex<Option<Gate>>,
}
impl BoundaryControl {
    fn hold(&self) -> (oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (entered, receive) = oneshot::channel();
        let (release, wait) = mpsc::channel();
        *self.next.lock().unwrap() = Some(Gate {
            entered,
            release: wait,
        });
        (receive, release)
    }
    pub(crate) fn enter(&self) {
        let gate = self.next.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.entered.send(());
            let _ = gate.release.recv();
        }
    }
}
pub(crate) fn hold_publication(
    delivery: &SummaryDeliveryOwner,
) -> (oneshot::Receiver<()>, mpsc::Sender<()>) {
    delivery.port.control.publication.hold()
}
pub(crate) fn hold_sql_entry(
    service: &SummaryService,
) -> (oneshot::Receiver<()>, mpsc::Sender<()>) {
    service.core.sql_entry.hold()
}
pub(crate) fn hold_ready_issued(
    delivery: &SummaryDeliveryOwner,
) -> (
    oneshot::Receiver<()>,
    mpsc::Sender<()>,
    oneshot::Receiver<()>,
) {
    let (sent, published) = oneshot::channel();
    *delivery.port.control.ready_published.lock().unwrap() = Some(sent);
    let (entered, release) = delivery.port.control.ready_issued.hold();
    (entered, release, published)
}
pub(super) fn published(control: &DeliveryControl) {
    if let Some(published) = control.ready_published.lock().unwrap().take() {
        let _ = published.send(());
    }
}
