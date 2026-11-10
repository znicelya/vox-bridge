//! Per-backend control before Pulse enters native work; tests retain real owners.
use std::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundary {
    OwnerReady,
    StreamGroup,
}
struct Gate {
    boundary: Boundary,
    entered: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}
#[derive(Default)]
pub(crate) struct EntryControl {
    next: Mutex<Option<Gate>>,
}
impl EntryControl {
    pub(crate) fn hold(&self, boundary: Boundary) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered, receive) = oneshot::channel();
        let (release, wait) = oneshot::channel();
        *self.next.lock().unwrap() = Some(Gate {
            boundary,
            entered,
            release: wait,
        });
        (receive, release)
    }
    pub(crate) async fn enter(&self, boundary: Boundary) {
        let gate = {
            let mut next = self.next.lock().unwrap();
            if next.as_ref().is_some_and(|gate| gate.boundary == boundary) {
                next.take()
            } else {
                None
            }
        };
        if let Some(gate) = gate {
            let _ = gate.entered.send(());
            let _ = gate.release.await;
        }
    }
}
