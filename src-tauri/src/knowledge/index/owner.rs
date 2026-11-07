//! Synchronous engine work. The facade supplies the dedicated owning thread;
//! every library call, including destruction, enters this private Rayon pool.
use super::{
    codec::ChunkMetadata,
    preflight,
    redb_header::{HeaderSnapshot, StorageState},
    repair,
    repository::{DurableIntent, IndexRepository, PublicationAck},
    seal::{self, BATCH_CAP, IntentKind, METADATA_WATERMARK, RECORD_CAP, Seal},
};
use crate::{
    error::{AppError, AppResult},
    intelligence::types::IntelligenceErrorCode as Code,
};
use rayon::{ThreadPool, ThreadPoolBuilder};
use srvdb::{DatabaseConfig, IndexMode, SrvDB, Vector, VectorEngine, VectorStorage};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub(crate) struct PersistedReceipt {
    intent: DurableIntent,
    seal_json: String,
}
impl PersistedReceipt {
    pub(super) fn intent(&self) -> &DurableIntent {
        &self.intent
    }
    pub(super) fn seal_json(&self) -> &str {
        &self.seal_json
    }
}
enum Phase {
    Dirty,
    Awaiting(String),
    Clean(PersistedReceipt),
}
struct Closing {
    intent: DurableIntent,
    before: HeaderSnapshot,
    count: u64,
    dropped: bool,
    receipt: Option<PersistedReceipt>,
}
struct Entry {
    db: SrvDB,
    intent: DurableIntent,
    phase: Phase,
    used: u64,
}
struct Maintenance {
    token: Uuid,
    key: Key,
    snapshot_end: u64,
    release_requested: Arc<AtomicBool>,
}
type Key = (String, Uuid);
pub(super) struct Runtime {
    root: PathBuf,
    repository: Arc<IndexRepository>,
    epoch: Uuid,
    entries: BTreeMap<Key, Entry>,
    closing: BTreeMap<Key, Closing>,
    pool: PoolOwner,
    sequence: u64,
    maintenance: Option<Maintenance>,
    #[cfg(test)]
    pub(super) close_fault: Option<CloseFault>,
    #[cfg(test)]
    pub(super) fail_once_after_persist: bool,
}
#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum CloseFault {
    Marker,
    Sync,
}
impl Runtime {
    pub(super) fn new(
        root: PathBuf,
        repository: Arc<IndexRepository>,
        epoch: Uuid,
    ) -> AppResult<Self> {
        Ok(Self {
            root,
            repository,
            epoch,
            entries: BTreeMap::new(),
            closing: BTreeMap::new(),
            pool: PoolOwner::new()?,
            sequence: 0,
            maintenance: None,
            #[cfg(test)]
            close_fault: None,
            #[cfg(test)]
            fail_once_after_persist: false,
        })
    }
    pub(super) fn open(&mut self, intent: &DurableIntent) -> AppResult<u64> {
        self.validate(intent)?;
        let key = key(intent);
        self.ensure_mutable(&key)?;
        self.sequence = self.sequence.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(&key) {
            if entry.intent.body() == intent.body() {
                return self.pool.install(|| Ok(entry.db.count()));
            }
            match &entry.phase {
                Phase::Clean(receipt)
                    if intent.body().prior_seal_sha256.as_ref()
                        == Some(&seal::digest(receipt.seal_json())) =>
                {
                    entry.intent = intent.clone();
                    entry.phase = Phase::Dirty;
                    entry.used = self.sequence;
                    return self.pool.install(|| Ok(entry.db.count()));
                }
                _ => return Err(busy()),
            }
        }
        if self.entries.len() >= 2 {
            let victim = self
                .entries
                .iter()
                .filter(|(_, e)| matches!(e.phase, Phase::Clean(_)))
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
                .ok_or_else(busy)?;
            self.close_and_publish(&victim)?;
        }
        let path = match intent.body().operation {
            IntentKind::Create => preflight::create(&self.root, &intent.body().descriptor)?,
            IntentKind::Open => {
                preflight::verify(&self.root, &intent.prior()?.ok_or_else(repair)?)?
            }
            IntentKind::Close => return Err(repair()),
        };
        let path = path.to_str().ok_or_else(repair)?;
        let (db, count) = self.pool.install(|| {
            let config = DatabaseConfig::new(intent.body().descriptor.identity.dimensions as usize)
                .map_err(|_| repair())?;
            let mut db = SrvDB::new_with_config(path, config).map_err(|_| repair())?;
            db.set_mode(IndexMode::Auto);
            let count = db.count();
            if count != intent.body().expected_stored_count {
                return Err(repair());
            }
            Ok((db, count))
        })?;
        self.entries.insert(
            key,
            Entry {
                db,
                intent: intent.clone(),
                phase: Phase::Dirty,
                used: self.sequence,
            },
        );
        Ok(count)
    }
    pub(super) fn add(
        &mut self,
        intent: &DurableIntent,
        rows: Vec<(Vec<f32>, ChunkMetadata)>,
    ) -> AppResult<Vec<u64>> {
        self.validate(intent)?;
        self.ensure_mutable(&key(intent))?;
        validate_batch(rows.len())?;
        let mut vectors = Vec::with_capacity(rows.len());
        let mut metadata = Vec::with_capacity(rows.len());
        for (vector, meta) in rows {
            validate_vector(&vector, intent.body().descriptor.identity.dimensions)?;
            vectors.push(Vector::new(vector));
            metadata.push(meta.encode()?);
        }
        let path = preflight::generation_path(&self.root, &intent.body().descriptor)?;
        if std::fs::metadata(path.join("metadata.db"))
            .map_err(|_| repair())?
            .len()
            >= METADATA_WATERMARK
        {
            return Err(AppError::Intelligence(Code::ResourceLimit));
        }
        let entry = entry_mut(&mut self.entries, intent)?;
        if !matches!(entry.phase, Phase::Dirty) {
            return Err(busy());
        }
        self.pool.install(|| {
            reserve(entry.db.count(), vectors.len())?;
            let ids = entry
                .db
                .add_batch(&vectors, &metadata)
                .map_err(|_| repair())?;
            if ids.len() != vectors.len() {
                return Err(repair());
            }
            Ok(ids)
        })
    }
    pub(super) fn persist(&mut self, intent: &DurableIntent) -> AppResult<PersistedReceipt> {
        self.validate(intent)?;
        self.ensure_mutable(&key(intent))?;
        let entry = entry_mut(&mut self.entries, intent)?;
        match &entry.phase {
            Phase::Awaiting(encoded) => {
                return Ok(PersistedReceipt {
                    intent: intent.clone(),
                    seal_json: encoded.clone(),
                });
            }
            Phase::Clean(_) => return Err(busy()),
            Phase::Dirty => {}
        }
        // Once issued, a receipt pins these bytes until fenced publication.
        // Failed persistence keeps the trusted live engine available for retry.
        let count = self.pool.install(|| -> AppResult<u64> {
            entry.db.persist().map_err(|_| repair())?;
            Ok(entry.db.count())
        })?;
        #[cfg(test)]
        if self.fail_once_after_persist {
            self.fail_once_after_persist = false;
            return Err(repair());
        }
        let path = preflight::generation_path(&self.root, &intent.body().descriptor)?;
        let files = preflight::inspect(&path, intent.body().descriptor.identity.dimensions, count)?;
        let header = HeaderSnapshot::read(&path.join("metadata.db"), StorageState::Live)?;
        let encoded = Seal::new(intent.body(), count, files, header)?.encode()?;
        entry.phase = Phase::Awaiting(encoded.clone());
        Ok(PersistedReceipt {
            intent: intent.clone(),
            seal_json: encoded,
        })
    }
    pub(super) fn publish(&mut self, receipt: &PersistedReceipt) -> AppResult<PublicationAck> {
        self.validate(receipt.intent())?;
        let entry = entry_mut(&mut self.entries, receipt.intent())?;
        match &entry.phase {
            Phase::Awaiting(encoded) if encoded == receipt.seal_json() => {}
            _ => return Err(repair()),
        }
        match self.repository.publish(receipt) {
            Ok(ack) => Ok(ack),
            Err(failure) => self.repository.recover_ack(receipt).map_err(|_| failure),
        }
    }
    pub(super) fn acknowledge(&mut self, ack: &PublicationAck) -> AppResult<()> {
        if ack.intent().body().owner_epoch != self.epoch {
            return Err(repair());
        }
        self.repository.validate_ack(ack)?;
        if let Some(closing) = self.closing.get(&key(ack.intent())) {
            if closing.intent.body() != ack.intent().body()
                || closing
                    .receipt
                    .as_ref()
                    .is_none_or(|r| r.seal_json() != ack.seal_json())
            {
                return Err(repair());
            }
            self.closing.remove(&key(ack.intent()));
            return Ok(());
        }
        let entry = entry_mut(&mut self.entries, ack.intent())?;
        match &entry.phase {
            Phase::Awaiting(encoded) if encoded == ack.seal_json() => {
                entry.phase = Phase::Clean(PersistedReceipt {
                    intent: ack.intent().clone(),
                    seal_json: encoded.clone(),
                })
            }
            Phase::Clean(receipt) if receipt.seal_json() == ack.seal_json() => {}
            _ => return Err(repair()),
        }
        Ok(())
    }
    pub(super) fn scan(
        &mut self,
        intent: &DurableIntent,
        start: u64,
        limit: u32,
    ) -> AppResult<Vec<(u64, Option<ChunkMetadata>)>> {
        self.validate(intent)?;
        validate_batch(limit as usize)?;
        let entry = entry_mut(&mut self.entries, intent)?;
        self.pool.install(|| {
            let count = entry.db.count();
            if start > count {
                return Err(repair());
            }
            let end = start
                .checked_add(u64::from(limit))
                .ok_or_else(repair)?
                .min(count);
            (start..end)
                .map(|id| {
                    if entry.db.is_deleted(id) {
                        return Ok((id, None));
                    }
                    let encoded = entry
                        .db
                        .get_metadata(id)
                        .map_err(|_| repair())?
                        .ok_or_else(repair)?;
                    Ok((id, Some(ChunkMetadata::decode(&encoded)?)))
                })
                .collect()
        })
    }
    pub(super) fn delete(&mut self, intent: &DurableIntent, ids: Vec<u64>) -> AppResult<Vec<bool>> {
        self.validate(intent)?;
        self.ensure_mutable(&key(intent))?;
        validate_batch(ids.len())?;
        let entry = entry_mut(&mut self.entries, intent)?;
        if !matches!(entry.phase, Phase::Dirty) {
            return Err(busy());
        }
        self.pool.install(|| {
            if ids.iter().any(|id| *id >= entry.db.count()) {
                return Err(repair());
            }
            ids.into_iter()
                .map(|id| entry.db.delete(id).map_err(|_| repair()))
                .collect()
        })
    }
    pub(super) fn replace(
        &mut self,
        intent: &DurableIntent,
        id: u64,
        vector: Vec<f32>,
        metadata: ChunkMetadata,
    ) -> AppResult<u64> {
        self.validate(intent)?;
        self.ensure_mutable(&key(intent))?;
        validate_vector(&vector, intent.body().descriptor.identity.dimensions)?;
        let metadata = metadata.encode()?;
        let path = preflight::generation_path(&self.root, &intent.body().descriptor)?;
        if std::fs::metadata(path.join("metadata.db"))
            .map_err(|_| repair())?
            .len()
            >= METADATA_WATERMARK
        {
            return Err(AppError::Intelligence(Code::ResourceLimit));
        }
        let entry = entry_mut(&mut self.entries, intent)?;
        if !matches!(entry.phase, Phase::Dirty) {
            return Err(busy());
        }
        self.pool.install(|| {
            reserve(entry.db.count(), 1)?;
            entry
                .db
                .replace(id, &Vector::new(vector), &metadata)
                .map_err(|_| repair())
        })
    }
    pub(super) fn search(
        &mut self,
        intent: &DurableIntent,
        vector: Vec<f32>,
        limit: u32,
    ) -> AppResult<Vec<(u64, f32, ChunkMetadata)>> {
        self.validate(intent)?;
        validate_batch(limit as usize)?;
        validate_vector(&vector, intent.body().descriptor.identity.dimensions)?;
        let entry = entry_mut(&mut self.entries, intent)?;
        self.pool.install(|| {
            let hits = entry
                .db
                .search(&Vector::new(vector), limit as usize)
                .map_err(|_| repair())?;
            if hits.len() > limit as usize {
                return Err(repair());
            }
            hits.into_iter()
                .map(|hit| {
                    if hit.id >= entry.db.count()
                        || entry.db.is_deleted(hit.id)
                        || !hit.score.is_finite()
                    {
                        return Err(repair());
                    }
                    let metadata =
                        ChunkMetadata::decode(hit.metadata.as_deref().ok_or_else(repair)?)?;
                    Ok((hit.id, hit.score, metadata))
                })
                .collect()
        })
    }
    pub(super) fn begin_maintenance(
        &mut self,
        intent: &DurableIntent,
        release_requested: Arc<AtomicBool>,
    ) -> AppResult<Uuid> {
        self.validate(intent)?;
        if self.maintenance.is_some() {
            return Err(busy());
        }
        let source_key = key(intent);
        let entry = entry_mut(&mut self.entries, intent)?;
        if !matches!(entry.phase, Phase::Awaiting(_)) {
            return Err(busy());
        }
        let count = entry.db.count();
        let dimension = intent.body().descriptor.identity.dimensions;
        let path = preflight::generation_path(&self.root, &intent.body().descriptor)?;
        let reader_path = path.to_str().ok_or_else(repair)?.to_owned();
        self.pool.install(|| {
            let reader =
                VectorStorage::new(&reader_path, dimension as usize).map_err(|_| repair())?;
            if reader.count() != count || reader.dimension() != dimension as usize {
                return Err(repair());
            }
            Ok(())
        })?;
        let token = Uuid::new_v4();
        self.maintenance = Some(Maintenance {
            token,
            key: source_key,
            snapshot_end: count,
            release_requested,
        });
        Ok(token)
    }
    pub(super) fn copy_vectors(
        &mut self,
        intent: &DurableIntent,
        token: Uuid,
        start: u64,
        limit: u32,
    ) -> AppResult<Vec<(u64, Vec<f32>)>> {
        self.validate(intent)?;
        validate_batch(limit as usize)?;
        let source_key = key(intent);
        let snapshot_end = self
            .maintenance
            .as_ref()
            .filter(|maintenance| maintenance.token == token && maintenance.key == source_key)
            .map(|maintenance| maintenance.snapshot_end)
            .ok_or_else(busy)?;
        let entry = entry_mut(&mut self.entries, intent)?;
        if !matches!(entry.phase, Phase::Awaiting(_)) || entry.db.count() != snapshot_end {
            return Err(busy());
        }
        if start > snapshot_end {
            return Err(repair());
        }
        let end = start
            .checked_add(u64::from(limit))
            .ok_or_else(repair)?
            .min(snapshot_end);
        let dimension = intent.body().descriptor.identity.dimensions;
        let path = preflight::generation_path(&self.root, &intent.body().descriptor)?;
        let reader_path = path.to_str().ok_or_else(repair)?.to_owned();
        self.pool.install(|| {
            let reader =
                VectorStorage::new(&reader_path, dimension as usize).map_err(|_| repair())?;
            if reader.count() != snapshot_end || reader.dimension() != dimension as usize {
                return Err(repair());
            }
            let mut rows = Vec::new();
            for id in start..end {
                if entry.db.is_deleted(id) {
                    continue;
                }
                let vector = reader.get(id).ok_or_else(repair)?.to_vec();
                validate_vector(&vector, dimension)?;
                rows.push((id, vector));
            }
            Ok(rows)
        })
    }
    pub(super) fn end_maintenance(&mut self, intent: &DurableIntent, token: Uuid) -> AppResult<()> {
        self.validate(intent)?;
        let source_key = key(intent);
        if self
            .maintenance
            .as_ref()
            .is_none_or(|maintenance| maintenance.token != token || maintenance.key != source_key)
        {
            return Err(busy());
        }
        self.maintenance = None;
        Ok(())
    }
    pub(super) fn release_requested_maintenance(&mut self) {
        if self
            .maintenance
            .as_ref()
            .is_some_and(|maintenance| maintenance.release_requested.load(Ordering::Acquire))
        {
            self.maintenance = None;
        }
    }
    pub(super) fn close_generation(
        &mut self,
        generation: &seal::Generation,
    ) -> AppResult<PersistedReceipt> {
        self.close_key(&(generation.key()?, generation.generation))
    }
    fn close_key(&mut self, key: &Key) -> AppResult<PersistedReceipt> {
        self.ensure_mutable(key)?;
        if !self.closing.contains_key(key) {
            let entry = self.entries.get(key).ok_or_else(repair)?;
            let Phase::Clean(receipt) = &entry.phase else {
                return Err(busy());
            };
            let prior = Seal::decode(receipt.seal_json())?;
            preflight::verify(&self.root, &prior)?;
            if prior.metadata_header.state() != StorageState::Live {
                return Err(repair());
            }
            // If this transaction fails, the live clean engine remains owned.
            let intent = self.repository.close(receipt, self.epoch)?;
            self.closing.insert(
                key.clone(),
                Closing {
                    intent,
                    before: prior.metadata_header,
                    count: prior.stored_count,
                    dropped: false,
                    receipt: None,
                },
            );
            let entry = self.entries.remove(key).ok_or_else(repair)?;
            self.pool.install(|| drop(entry));
            self.closing.get_mut(key).ok_or_else(repair)?.dropped = true;
        }
        let closing = self.closing.get_mut(key).ok_or_else(repair)?;
        self.repository.validate(&closing.intent)?;
        if let Some(receipt) = &closing.receipt {
            return Ok(receipt.clone());
        }
        if !closing.dropped {
            return Err(repair());
        }
        let path = preflight::generation_path(&self.root, &closing.intent.body().descriptor)?;
        #[cfg(test)]
        let fault = self.close_fault.take();
        #[cfg(test)]
        if matches!(fault, Some(CloseFault::Marker)) {
            use std::io::{Read, Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path.join("metadata.db"))
                .map_err(|_| repair())?;
            file.seek(SeekFrom::Start(9)).map_err(|_| repair())?;
            let mut byte = [0];
            file.read_exact(&mut byte).map_err(|_| repair())?;
            byte[0] |= 2;
            file.seek(SeekFrom::Start(9)).map_err(|_| repair())?;
            file.write_all(&byte).map_err(|_| repair())?;
            file.sync_all().map_err(|_| repair())?;
        }
        let after = HeaderSnapshot::read(&path.join("metadata.db"), StorageState::Closed)?;
        closing.before.validates_close(&after)?;
        #[cfg(test)]
        if matches!(fault, Some(CloseFault::Sync)) {
            return Err(repair());
        }
        preflight::sync_closed(&path, closing.intent.body().descriptor.identity.dimensions)?;
        let files = preflight::inspect(
            &path,
            closing.intent.body().descriptor.identity.dimensions,
            closing.count,
        )?;
        if HeaderSnapshot::read(&path.join("metadata.db"), StorageState::Closed)? != after {
            return Err(repair());
        }
        let encoded = Seal::new(closing.intent.body(), closing.count, files, after)?.encode()?;
        let receipt = PersistedReceipt {
            intent: closing.intent.clone(),
            seal_json: encoded,
        };
        closing.receipt = Some(receipt.clone());
        Ok(receipt)
    }
    fn close_and_publish(&mut self, key: &Key) -> AppResult<()> {
        let receipt = match self.closing.get(key).and_then(|c| c.receipt.clone()) {
            Some(receipt) => receipt,
            None => self.close_key(key)?,
        };
        let ack = match self.repository.publish(&receipt) {
            Ok(ack) => ack,
            Err(error) => match self.repository.recover_ack(&receipt) {
                Ok(ack) => ack,
                Err(_) => return Err(error),
            },
        };
        self.acknowledge(&ack)
    }
    pub(super) fn shutdown(&mut self) -> AppResult<()> {
        self.maintenance = None;
        for key in self.entries.keys().cloned().collect::<Vec<_>>() {
            let awaiting = self.entries.get(&key).and_then(|e| match &e.phase {
                Phase::Awaiting(encoded) => Some(PersistedReceipt {
                    intent: e.intent.clone(),
                    seal_json: encoded.clone(),
                }),
                _ => None,
            });
            if let Some(receipt) = awaiting {
                if let Ok(ack) = self.repository.recover_ack(&receipt) {
                    self.acknowledge(&ack)?;
                }
            }
            if self
                .entries
                .get(&key)
                .is_some_and(|e| matches!(e.phase, Phase::Clean(_)))
            {
                self.close_and_publish(&key)?;
            } else {
                // Unpublished/dirty disposal cannot create trusted provenance.
                let entry = self.entries.remove(&key);
                self.pool.install(|| drop(entry));
            }
        }
        for key in self.closing.keys().cloned().collect::<Vec<_>>() {
            self.close_and_publish(&key)?;
        }
        Ok(())
    }
    fn validate(&self, intent: &DurableIntent) -> AppResult<()> {
        if intent.body().owner_epoch != self.epoch {
            return Err(repair());
        }
        self.repository.validate(intent)
    }
    fn ensure_mutable(&self, key: &Key) -> AppResult<()> {
        if self
            .maintenance
            .as_ref()
            .is_some_and(|maintenance| &maintenance.key == key)
        {
            return Err(busy());
        }
        Ok(())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = self.shutdown();
        }
        // Exceptional final-owner fallback cannot leave live mappings behind.
        // A failed close publishes no new seal; old exact hashes still fence reopen.
        let entries = std::mem::take(&mut self.entries);
        self.pool.install(|| drop(entries));
    }
}
fn key(intent: &DurableIntent) -> Key {
    (
        intent.body().identity.clone(),
        intent.body().descriptor.generation,
    )
}
fn entry_mut<'a>(
    entries: &'a mut BTreeMap<Key, Entry>,
    intent: &DurableIntent,
) -> AppResult<&'a mut Entry> {
    let entry = entries.get_mut(&key(intent)).ok_or_else(repair)?;
    if entry.intent.body() != intent.body() {
        return Err(repair());
    }
    Ok(entry)
}
fn busy() -> AppError {
    AppError::Intelligence(Code::Busy)
}
fn validate_batch(count: usize) -> AppResult<()> {
    if count == 0 || count > BATCH_CAP as usize {
        return Err(AppError::Intelligence(Code::ResourceLimit));
    }
    Ok(())
}
fn validate_vector(vector: &[f32], dimension: u32) -> AppResult<()> {
    if vector.len() != dimension as usize
        || vector.iter().any(|n| !n.is_finite())
        || !vector.iter().any(|n| *n != 0.0)
    {
        return Err(AppError::InvalidRequest("invalid embedding vector".into()));
    }
    Ok(())
}
fn reserve(count: u64, added: usize) -> AppResult<()> {
    if count
        .checked_add(added as u64)
        .is_none_or(|n| n > RECORD_CAP)
    {
        return Err(AppError::Intelligence(Code::ResourceLimit));
    }
    Ok(())
}
/// ThreadPool::drop requests termination; these retained handles prove it.
struct PoolOwner {
    pool: Option<ThreadPool>,
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}
impl PoolOwner {
    fn new() -> AppResult<Self> {
        let handles = Arc::new(Mutex::new(Vec::with_capacity(2)));
        let spawned = handles.clone();
        let mut owner = Self {
            pool: None,
            handles,
        };
        let pool = ThreadPoolBuilder::new()
            .num_threads(2)
            .spawn_handler(move |thread| {
                let handle = std::thread::Builder::new()
                    .name(format!("knowledge-rayon-{}", thread.index()))
                    .spawn(move || thread.run())?;
                spawned.lock().unwrap().push(handle);
                Ok(())
            })
            .build()
            .map_err(|_| AppError::Intelligence(Code::Interrupted))?;
        owner.pool = Some(pool);
        Ok(owner)
    }
    fn install<T: Send>(&self, work: impl FnOnce() -> T + Send) -> T {
        self.pool.as_ref().expect("live private pool").install(work)
    }
}
impl Drop for PoolOwner {
    fn drop(&mut self) {
        drop(self.pool.take());
        let handles = std::mem::take(&mut *self.handles.lock().unwrap());
        for handle in handles {
            let _ = handle.join();
        }
    }
}
