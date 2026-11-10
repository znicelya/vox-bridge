//! Holds the real entered ConfigIo selection closure before its SQLite fence.
use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use tokio::sync::oneshot;

struct Gate {
    entered: oneshot::Sender<()>,
    release: mpsc::Receiver<()>,
}
#[derive(Default)]
pub(super) struct SelectionControl {
    entries: AtomicUsize,
    next: Mutex<Option<Gate>>,
}
impl SelectionControl {
    pub(super) fn enter(&self) -> AppResult<()> {
        self.entries.fetch_add(1, Ordering::SeqCst);
        let gate = self.next.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.entered.send(());
            gate.release.recv().map_err(|_| configuration())?;
        }
        Ok(())
    }
}
impl ServiceConfigStore {
    pub(crate) fn hold_next_summary_selection_for_tests(
        &self,
    ) -> (oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (entered, receive) = oneshot::channel();
        let (release, wait) = mpsc::channel();
        *self.summary_selection.next.lock().unwrap() = Some(Gate {
            entered,
            release: wait,
        });
        (receive, release)
    }
    pub(crate) fn summary_selection_entries_for_tests(&self) -> usize {
        self.summary_selection.entries.load(Ordering::SeqCst)
    }
}
