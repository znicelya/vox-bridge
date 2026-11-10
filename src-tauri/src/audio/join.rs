//! Opaque actual audio ownership receipts, separate from operation outcomes.
use crate::error::AppResult;
use futures_util::FutureExt;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore, watch},
    task::JoinHandle,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct AudioOwnerAck {
    _owner: Uuid,
}
#[derive(Clone)]
pub struct AudioOwnerJoin {
    completed: watch::Receiver<Option<AudioOwnerAck>>,
    reaper: Arc<Mutex<Option<JoinHandle<()>>>>,
}
impl AudioOwnerJoin {
    pub async fn wait(&self) -> AppResult<AudioOwnerAck> {
        let mut completed = self.completed.clone();
        loop {
            let ack = completed.borrow_and_update().clone();
            if let Some(ack) = ack {
                let mut reaper = self.reaper.lock().await;
                if let Some(task) = reaper.as_mut() {
                    task.await.map_err(|_| {
                        super::runtime::audio_error("actual audio join owner stopped")
                    })?;
                    reaper.take();
                }
                return Ok(ack);
            }
            completed.changed().await.map_err(|_| {
                super::runtime::audio_error("actual audio owner ended without join proof")
            })?;
        }
    }
}

pub(crate) struct AudioOwnerSource {
    owner: Uuid,
    publish: watch::Sender<Option<AudioOwnerAck>>,
    reaper: Arc<Mutex<Option<JoinHandle<()>>>>,
}
impl AudioOwnerSource {
    pub(crate) fn new() -> (Self, AudioOwnerJoin) {
        let (publish, completed) = watch::channel(None);
        let reaper = Arc::new(Mutex::new(None));
        (
            Self {
                owner: Uuid::new_v4(),
                publish,
                reaper: reaper.clone(),
            },
            AudioOwnerJoin { completed, reaper },
        )
    }
    pub(in crate::audio) fn retain_thread_parent(
        self,
        thread: std::thread::JoinHandle<()>,
        parent: Option<AudioParent>,
    ) {
        let reaper = self.reaper.clone();
        let thread_join = tokio::task::spawn_blocking(move || {
            // Even a panicked thread is actually ended; operation/health errors
            // remain separate. The native destructor and its worker joins ran
            // on that original thread before its JoinHandle can return.
            let _ = thread.join();
        });
        let task = tokio::spawn(async move {
            if thread_join.await.is_err() {
                return;
            }
            if let Some(parent) = parent {
                if parent.finish().await.is_err() {
                    return;
                }
            }
            self.publish
                .send_replace(Some(AudioOwnerAck { _owner: self.owner }));
        });
        *reaper.try_lock().expect("new actual owner reaper") = Some(task);
    }
    pub(crate) fn retain_task(self, task: JoinHandle<()>, parent: AudioParent) {
        let reaper = self.reaper.clone();
        let joined = tokio::spawn(async move {
            if task.await.is_err() {
                return;
            }
            if parent.finish().await.is_ok() {
                self.publish
                    .send_replace(Some(AudioOwnerAck { _owner: self.owner }));
            }
        });
        *reaper.try_lock().expect("new actual owner reaper") = Some(joined);
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AudioCloseKey {
    application_epoch: u64,
    delivery_id: Uuid,
    backend_epoch: u64,
}
impl AudioCloseKey {
    pub(crate) fn new(application_epoch: u64, delivery_id: Uuid, backend_epoch: u64) -> Self {
        Self {
            application_epoch,
            delivery_id,
            backend_epoch,
        }
    }
    pub(crate) fn delivery_id(&self) -> Uuid {
        self.delivery_id
    }
    pub(crate) fn backend_epoch(&self) -> u64 {
        self.backend_epoch
    }
    pub(crate) fn application_epoch(&self) -> u64 {
        self.application_epoch
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BackendStage {
    Ready,
    Closing,
}
struct RegistryState {
    epoch: u64,
    stage: BackendStage,
    closing: Option<AudioCloseKey>,
    parents: HashMap<Uuid, Arc<ParentInner>>,
}
struct RegistryCore {
    id: Uuid,
    state: StdMutex<RegistryState>,
    streams: Arc<Semaphore>,
    inventory: Arc<Semaphore>,
}
#[derive(Clone)]
pub(crate) struct AudioRegistry(Arc<RegistryCore>);
impl AudioRegistry {
    pub(crate) fn new() -> Self {
        Self(Arc::new(RegistryCore {
            id: Uuid::new_v4(),
            state: StdMutex::new(RegistryState {
                epoch: 0,
                stage: BackendStage::Ready,
                closing: None,
                parents: HashMap::new(),
            }),
            streams: Arc::new(Semaphore::new(2)),
            inventory: Arc::new(Semaphore::new(1)),
        }))
    }
    pub(crate) fn epoch(&self) -> AppResult<u64> {
        Ok(self.0.state.lock().map_err(|_| err())?.epoch)
    }
    pub(crate) fn require_ready(&self) -> AppResult<u64> {
        let state = self.0.state.lock().map_err(|_| err())?;
        if state.stage != BackendStage::Ready {
            return Err(closing());
        }
        Ok(state.epoch)
    }
    fn register(
        &self,
        epoch: u64,
        slot: OwnedSemaphorePermit,
        inventory: bool,
    ) -> AppResult<AudioParent> {
        let mut state = self.0.state.lock().map_err(|_| err())?;
        if state.stage != BackendStage::Ready || state.epoch != epoch {
            return Err(closing());
        }
        let id = Uuid::new_v4();
        let inner = Arc::new(ParentInner {
            registry: self.clone(),
            id,
            epoch,
            slot: StdMutex::new(Some(slot)),
            finishing: AtomicBool::new(false),
            streams: (!inventory).then(|| GroupRegistry::new(2)),
            helpers: GroupRegistry::new(if inventory { 3 } else { 2 }),
        });
        state.parents.insert(id, inner.clone());
        Ok(AudioParent(inner))
    }
    pub(crate) fn stream(&self) -> AppResult<AudioParent> {
        let epoch = self.require_ready()?;
        let slot =
            self.0.streams.clone().try_acquire_owned().map_err(|_| {
                super::runtime::audio_error("actual stream parents are still owned")
            })?;
        self.register(epoch, slot, false)
    }
    pub(crate) async fn inventory(&self) -> AppResult<AudioParent> {
        let epoch = self.require_ready()?;
        let slot = tokio::time::timeout(
            Duration::from_secs(2),
            self.0.inventory.clone().acquire_owned(),
        )
        .await
        .map_err(|_| super::runtime::audio_error("actual inventory parent is still owned"))?
        .map_err(|_| err())?;
        self.register(epoch, slot, true)
    }
    pub(crate) fn begin_close(&self, key: AudioCloseKey) -> AppResult<AudioBackendCloseJoin> {
        let mut state = self.0.state.lock().map_err(|_| err())?;
        if key.backend_epoch != state.epoch {
            return Err(closing());
        }
        if state.closing.as_ref().is_some_and(|old| old != &key) {
            return Err(closing());
        }
        state.stage = BackendStage::Closing;
        state.closing = Some(key.clone());
        Ok(AudioBackendCloseJoin {
            registry: self.clone(),
            key,
        })
    }
    pub(crate) fn reopen_after_close(&self, ack: &AudioBackendCloseAck) -> AppResult<()> {
        let mut state = self.0.state.lock().map_err(|_| err())?;
        if ack.registry != self.0.id
            || state.closing.as_ref() != Some(&ack.key)
            || state.epoch != ack.key.backend_epoch
            || !state.parents.is_empty()
            || self.0.streams.available_permits() != 2
            || self.0.inventory.available_permits() != 1
        {
            return Err(closing());
        }
        state.epoch = state.epoch.checked_add(1).ok_or_else(err)?;
        state.stage = BackendStage::Ready;
        state.closing = None;
        Ok(())
    }
    pub(crate) fn validate_reopen_after_close(&self, ack: &AudioBackendCloseAck) -> AppResult<()> {
        let state = self.0.state.lock().map_err(|_| err())?;
        if ack.registry != self.0.id
            || state.closing.as_ref() != Some(&ack.key)
            || state.epoch != ack.key.backend_epoch
            || !state.parents.is_empty()
            || self.0.streams.available_permits() != 2
            || self.0.inventory.available_permits() != 1
        {
            return Err(closing());
        }
        Ok(())
    }
}
struct ParentInner {
    registry: AudioRegistry,
    id: Uuid,
    epoch: u64,
    // Kept by the registry even if an observer/producer is dropped in error.
    // Only the actual aggregate's explicit finish releases the original slot.
    slot: StdMutex<Option<OwnedSemaphorePermit>>,
    finishing: AtomicBool,
    streams: Option<Arc<GroupRegistry>>,
    helpers: Arc<GroupRegistry>,
}
#[derive(Clone)]
pub(crate) struct AudioParent(Arc<ParentInner>);
impl AudioParent {
    pub(crate) fn require_ready(&self) -> AppResult<()> {
        let state = self.0.registry.0.state.lock().map_err(|_| err())?;
        if state.stage != BackendStage::Ready
            || state.epoch != self.0.epoch
            || self.0.finishing.load(Ordering::Acquire)
            || !state.parents.contains_key(&self.0.id)
        {
            return Err(closing());
        }
        Ok(())
    }
    pub(crate) fn helpers(&self) -> AudioLeafGroup {
        AudioLeafGroup {
            parent: self.clone(),
            group: self.0.helpers.clone(),
        }
    }
    pub(crate) fn streams(&self) -> AppResult<AudioLeafGroup> {
        Ok(AudioLeafGroup {
            parent: self.clone(),
            group: self.0.streams.clone().ok_or_else(err)?,
        })
    }
    pub(crate) async fn finish(self) -> AppResult<()> {
        self.0.finishing.store(true, Ordering::Release);
        if let Some(group) = &self.0.streams {
            group.drain().await?;
        }
        self.0.helpers.drain().await?;
        let mut state = self.0.registry.0.state.lock().map_err(|_| err())?;
        let slot = self.0.slot.lock().map_err(|_| err())?.take();
        if slot.is_none() {
            return Ok(());
        }
        state.parents.remove(&self.0.id);
        drop(slot);
        Ok(())
    }
    pub(crate) fn publish<T>(&self, publish: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        let state = self.0.registry.0.state.lock().map_err(|_| err())?;
        if state.stage != BackendStage::Ready
            || state.epoch != self.0.epoch
            || self.0.finishing.load(Ordering::Acquire)
            || !state.parents.contains_key(&self.0.id)
        {
            return Err(closing());
        }
        publish()
    }
}
#[derive(Clone)]
pub struct AudioBackendCloseJoin {
    registry: AudioRegistry,
    key: AudioCloseKey,
}
#[derive(Clone)]
pub struct AudioBackendCloseAck {
    registry: Uuid,
    key: AudioCloseKey,
}
impl AudioBackendCloseJoin {
    pub async fn wait(&self) -> AppResult<AudioBackendCloseAck> {
        let _streams = self
            .registry
            .0
            .streams
            .clone()
            .acquire_many_owned(2)
            .await
            .map_err(|_| err())?;
        let _inventory = self
            .registry
            .0
            .inventory
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| err())?;
        let state = self.registry.0.state.lock().map_err(|_| err())?;
        if state.closing.as_ref() != Some(&self.key)
            || state.epoch != self.key.backend_epoch
            || !state.parents.is_empty()
        {
            return Err(closing());
        }
        Ok(AudioBackendCloseAck {
            registry: self.registry.0.id,
            key: self.key.clone(),
        })
    }
}

struct GroupRegistry {
    capacity: usize,
    actual: Arc<Semaphore>,
    round: StdMutex<Option<Arc<RoundState>>>,
}
impl GroupRegistry {
    fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            actual: Arc::new(Semaphore::new(1)),
            round: StdMutex::new(None),
        })
    }
    async fn drain(&self) -> AppResult<()> {
        let _all = self
            .actual
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| err())?;
        let round = self.round.lock().map_err(|_| err())?.clone();
        if let Some(round) = round {
            round.wait_joined().await?;
            let mut current = self.round.lock().map_err(|_| err())?;
            if current
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &round))
            {
                current.take();
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct AudioLeafGroup {
    parent: AudioParent,
    group: Arc<GroupRegistry>,
}
impl AudioLeafGroup {
    pub(crate) fn begin(&self) -> AppResult<AudioGroupScope> {
        self.parent.require_ready()?;
        let slot = self.group.actual.clone().try_acquire_owned().map_err(|_| {
            super::runtime::audio_error("original audio helper group remains entered")
        })?;
        self.parent.require_ready()?;
        let (seal, sealed) = watch::channel(false);
        let (done, joined) = watch::channel(false);
        let round = Arc::new(RoundState {
            parent: self.parent.clone(),
            capacity: self.group.capacity,
            leaves: StdMutex::new(Vec::new()),
            seal,
            joined,
            task: Mutex::new(None),
        });
        *self.group.round.lock().map_err(|_| err())? = Some(round.clone());
        let retained = round.clone();
        let task = tokio::spawn(async move {
            let mut sealed = sealed;
            while !*sealed.borrow_and_update() {
                if sealed.changed().await.is_err() {
                    return;
                }
            }
            let leaves = match retained.leaves.lock() {
                Ok(leaves) => leaves.clone(),
                Err(_) => return,
            };
            for leaf in leaves {
                if (AudioProcessJoin { leaf }).wait().await.is_err() {
                    return;
                }
            }
            drop(slot);
            done.send_replace(true);
        });
        *round.task.try_lock().expect("new audio helper group") = Some(task);
        Ok(AudioGroupScope { round })
    }
}
struct RoundState {
    parent: AudioParent,
    capacity: usize,
    leaves: StdMutex<Vec<Arc<LeafState>>>,
    seal: watch::Sender<bool>,
    joined: watch::Receiver<bool>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl RoundState {
    async fn wait_joined(&self) -> AppResult<()> {
        let mut joined = self.joined.clone();
        while !*joined.borrow_and_update() {
            joined.changed().await.map_err(|_| err())?;
        }
        let mut task = self.task.lock().await;
        if let Some(handle) = task.as_mut() {
            handle.await.map_err(|_| err())?;
            task.take();
        }
        Ok(())
    }
}
pub(crate) struct AudioGroupScope {
    round: Arc<RoundState>,
}
impl AudioGroupScope {
    pub(crate) fn leaf(&self) -> AppResult<AudioLeafExecution> {
        self.round.parent.require_ready()?;
        let mut leaves = self.round.leaves.lock().map_err(|_| err())?;
        if *self.round.seal.borrow() || leaves.len() >= self.round.capacity {
            return Err(err());
        }
        let (publish, completion) = watch::channel(None);
        let leaf = Arc::new(LeafState {
            id: Uuid::new_v4(),
            phase: StdMutex::new(LeafPhase::Armed),
            publish,
            completion,
            descriptors: StdMutex::new(0),
            changed: Notify::new(),
            task: Mutex::new(None),
        });
        leaves.push(leaf.clone());
        Ok(AudioLeafExecution {
            round: self.round.clone(),
            leaf: Some(leaf),
        })
    }
    pub(crate) async fn join(self) -> AppResult<()> {
        self.round.seal.send_replace(true);
        self.round.wait_joined().await
    }
}
impl Drop for AudioGroupScope {
    fn drop(&mut self) {
        self.round.seal.send_replace(true);
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum LeafPhase {
    Armed,
    Entered,
    Joined,
}
struct LeafState {
    id: Uuid,
    phase: StdMutex<LeafPhase>,
    publish: watch::Sender<Option<AudioProcessAck>>,
    completion: watch::Receiver<Option<AudioProcessAck>>,
    descriptors: StdMutex<usize>,
    changed: Notify,
    task: Mutex<Option<JoinHandle<()>>>,
}
pub struct AudioLeafExecution {
    round: Arc<RoundState>,
    leaf: Option<Arc<LeafState>>,
}
impl AudioLeafExecution {
    pub(crate) fn enter(mut self) -> AppResult<(AudioProcessSource, AudioProcessJoin)> {
        self.round.parent.require_ready()?;
        let leaf = self.leaf.take().ok_or_else(err)?;
        *leaf.phase.lock().map_err(|_| err())? = LeafPhase::Entered;
        Ok((
            AudioProcessSource { leaf: leaf.clone() },
            AudioProcessJoin { leaf },
        ))
    }
}
impl Drop for AudioLeafExecution {
    fn drop(&mut self) {
        if let Some(leaf) = self.leaf.take() {
            // No producer consumed the pre-issued capability, hence OS spawn
            // never entered. An entered source instead remains in the round.
            if let Ok(mut phase) = leaf.phase.lock() {
                if *phase == LeafPhase::Armed {
                    *phase = LeafPhase::Joined;
                    leaf.publish.send_replace(Some(AudioProcessAck {
                        _leaf: leaf.id,
                        _exit: None,
                    }));
                }
            }
        }
    }
}
#[derive(Clone)]
pub struct AudioProcessAck {
    _leaf: Uuid,
    _exit: Option<i32>,
}
#[derive(Clone)]
pub struct AudioProcessJoin {
    leaf: Arc<LeafState>,
}
impl AudioProcessJoin {
    pub async fn wait(&self) -> AppResult<AudioProcessAck> {
        let mut completion = self.leaf.completion.clone();
        loop {
            let ack = completion.borrow_and_update().clone();
            if let Some(ack) = ack {
                let mut task = self.leaf.task.lock().await;
                if let Some(handle) = task.as_mut() {
                    handle.await.map_err(|_| err())?;
                    task.take();
                }
                return Ok(ack);
            }
            completion.changed().await.map_err(|_| err())?;
        }
    }
}
pub(crate) struct AudioProcessSource {
    leaf: Arc<LeafState>,
}
impl AudioProcessSource {
    pub(crate) fn descriptor(&self) -> AudioDescriptor {
        *self
            .leaf
            .descriptors
            .lock()
            .expect("actual audio descriptors") += 1;
        AudioDescriptor {
            leaf: self.leaf.clone(),
        }
    }
    pub(crate) fn run_owned<F>(self, run: impl FnOnce(Self) -> F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let leaf = self.leaf.clone();
        let task = tokio::spawn(run(self));
        *leaf.task.try_lock().expect("new actual audio child") = Some(task);
    }
    pub(crate) async fn exited(self, exit: Option<i32>) {
        loop {
            let changed = self.leaf.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if *self
                .leaf
                .descriptors
                .lock()
                .expect("actual audio descriptors")
                == 0
            {
                break;
            }
            changed.await;
        }
        *self.leaf.phase.lock().expect("actual audio child") = LeafPhase::Joined;
        self.leaf.publish.send_replace(Some(AudioProcessAck {
            _leaf: self.leaf.id,
            _exit: exit,
        }));
    }
}
pub(crate) struct AudioDescriptor {
    leaf: Arc<LeafState>,
}
impl Drop for AudioDescriptor {
    fn drop(&mut self) {
        let mut count = self
            .leaf
            .descriptors
            .lock()
            .expect("actual audio descriptors");
        *count -= 1;
        drop(count);
        self.leaf.changed.notify_waiters();
    }
}
fn err() -> crate::error::AppError {
    super::runtime::audio_error("actual audio ownership source unavailable")
}
fn closing() -> crate::error::AppError {
    super::runtime::audio_error("audio backend is closing or its epoch changed")
}

#[derive(Clone)]
pub(crate) struct AudioCloseAck {
    key: AudioCloseKey,
    backend: AudioBackendCloseAck,
}
impl AudioCloseAck {
    pub(crate) fn require_key(&self, key: &AudioCloseKey) -> AppResult<()> {
        if &self.key != key {
            return Err(closing());
        }
        Ok(())
    }
    pub(crate) fn key(&self) -> &AudioCloseKey {
        &self.key
    }
    pub(crate) fn backend(&self) -> &AudioBackendCloseAck {
        &self.backend
    }
}
#[derive(Clone)]
pub(crate) struct AudioCloseJoin {
    key: AudioCloseKey,
    completed: watch::Receiver<Option<AudioCloseAck>>,
    result: Arc<StdMutex<Option<String>>>,
    actual: Arc<Mutex<Option<JoinHandle<()>>>>,
}
impl AudioCloseJoin {
    pub(crate) fn start_owned(
        owner: crate::intelligence::jobs::OperationLease,
        key: AudioCloseKey,
        session: crate::session::orchestrator::SessionCloseJoin,
        backend: AudioBackendCloseJoin,
        stop: impl std::future::Future<Output = AppResult<()>> + Send + 'static,
    ) -> AppResult<Self> {
        if session.key() != &key || backend.key != key {
            return Err(closing());
        }
        let (publish, completed) = watch::channel(None);
        let result = Arc::new(StdMutex::new(None));
        let actual = Arc::new(Mutex::new(None));
        let join = Self {
            key: key.clone(),
            completed,
            result: result.clone(),
            actual: actual.clone(),
        };
        let mut stop = Box::pin(stop);
        // Initialized sources can genuinely join without scheduling or native
        // entry. Poll both registered producers; absence of an active session
        // or a facade alone is never the empty-ownership proof.
        let sources = async { tokio::join!(session.wait(), backend.wait()) }.now_or_never();
        if let Some((Ok(()), Ok(backend_ack))) = sources {
            if let Some(stopped) = stop.as_mut().now_or_never() {
                if let Err(error) = stopped.and_then(|()| session.outcome()) {
                    *result.lock().expect("actual audio result") = Some(error.to_string());
                }
                drop(owner);
                publish.send_replace(Some(AudioCloseAck {
                    key,
                    backend: backend_ack,
                }));
                return Ok(join);
            }
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| err())?;
        let task = runtime.spawn(async move {
            let stopped = stop.await;
            if let Err(error) = stopped.and_then(|()| session.outcome()) {
                *result.lock().expect("actual audio result") = Some(error.to_string());
            }
            // Result failures cannot abandon either actual source. Unknown join
            // keeps this same elected owner/process lease, fenced for retry.
            let backend_ack = loop {
                let (session_join, native_join) = tokio::join!(session.wait(), backend.wait());
                match (session_join, native_join) {
                    (Ok(()), Ok(ack)) => break ack,
                    (session_result, backend_result) => {
                        let error = session_result
                            .err()
                            .or_else(|| backend_result.err())
                            .expect("one actual source failed");
                        result
                            .lock()
                            .expect("actual audio result")
                            .get_or_insert_with(|| error.to_string());
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            };
            let ack = AudioCloseAck {
                key,
                backend: backend_ack,
            };
            drop(owner);
            publish.send_replace(Some(ack));
        });
        *actual.try_lock().map_err(|_| err())? = Some(task);
        Ok(join)
    }
    pub(crate) async fn wait(&self) -> AppResult<AudioCloseAck> {
        let mut completed = self.completed.clone();
        loop {
            let ack = completed.borrow_and_update().clone();
            if let Some(ack) = ack {
                ack.require_key(&self.key)?;
                let mut actual = self.actual.lock().await;
                if let Some(task) = actual.as_mut() {
                    task.await.map_err(|_| err())?;
                    actual.take();
                }
                return Ok(ack);
            }
            completed.changed().await.map_err(|_| err())?;
        }
    }
    pub(crate) fn outcome(&self) -> AppResult<()> {
        match self.result.lock().map_err(|_| err())?.clone() {
            Some(error) => Err(crate::error::AppError::Internal(error)),
            None => Ok(()),
        }
    }
}
