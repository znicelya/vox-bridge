//! Witnessed vector-only recovery. Never enters dirty metadata or SrvDB.
use super::{
    preflight, repair,
    seal::{Generation, Intent, IntentKind, Seal, digest},
};
use crate::{error::AppResult, intelligence::types::IntelligenceErrorCode as Code};
use rusqlite::{Transaction, params};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use uuid::Uuid;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryFence {
    pub(crate) source: Generation,
    pub(super) seal_sha256: String,
    pub(super) intent_sha256: String,
}
#[derive(Clone)]
pub(crate) struct RecoveryEvidence {
    pub(crate) fence: RecoveryFence,
    pub(super) seal: Seal,
}
impl RecoveryEvidence {
    pub(crate) fn source(&self) -> &Generation {
        &self.fence.source
    }
    pub(crate) fn count(&self) -> u64 {
        self.seal.stored_count
    }
}
pub(super) fn evidence(
    tx: &Transaction<'_>,
    identity: &str,
    generation: Uuid,
) -> AppResult<RecoveryEvidence> {
    let (seal_json, intent_json): (String, String) = tx
        .query_row(
            "SELECT seal_json,intent_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2 AND dirty=1",
            params![identity, generation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| repair())?;
    let seal = Seal::decode(&seal_json)?;
    let intent = Intent::decode(&intent_json)?;
    if intent.operation != IntentKind::Open
        || intent.prior_seal_sha256.as_deref() != Some(&digest(&seal_json))
        || intent.descriptor != seal.descriptor
        || intent.expected_stored_count != seal.stored_count
        || seal.descriptor.key()? != identity
        || seal.descriptor.generation != generation
    {
        return Err(repair());
    }
    let fence = RecoveryFence {
        source: seal.descriptor.clone(),
        seal_sha256: digest(&seal_json),
        intent_sha256: digest(&intent_json),
    };
    validate(tx, &fence, true)?;
    Ok(RecoveryEvidence { fence, seal })
}
pub(super) fn validate(tx: &Transaction<'_>, fence: &RecoveryFence, active: bool) -> AppResult<()> {
    let identity = fence.source.key()?;
    let source = fence.source.generation.to_string();
    let (seal, intent): (String, String) = tx
        .query_row(
            "SELECT seal_json,intent_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2 AND dirty=1",
            params![identity, source],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| repair())?;
    if digest(&seal) != fence.seal_sha256 || digest(&intent) != fence.intent_sha256 {
        return Err(repair());
    }
    let pointer: Option<String> = tx.query_row(
        "SELECT active_generation FROM \
                    knowledge_index_generations WHERE identity=?1",
        [identity],
        |r| r.get(0),
    )?;
    if (pointer.as_deref() == Some(&source)) != active {
        return Err(repair());
    }
    Ok(())
}
pub(super) struct VectorReader {
    path: PathBuf,
    file: File,
    _lock: File,
    evidence: RecoveryEvidence,
}
impl VectorReader {
    pub(super) fn open(root: &Path, evidence: RecoveryEvidence) -> AppResult<Self> {
        let path = preflight::generation_path(root, evidence.source())?;
        preflight::directories(&path, false)?;
        safe_entries(&path, evidence.source().identity.dimensions)?;
        let lock = exclusive_lock(&path)?;
        regular(&path.join("vectors.bin"), evidence.seal.files[0].bytes)?;
        let file = File::open(path.join("vectors.bin")).map_err(|_| repair())?;
        let mut reader = Self {
            path,
            file,
            _lock: lock,
            evidence,
        };
        reader.verify()?;
        Ok(reader)
    }
    pub(super) fn verify(&mut self) -> AppResult<()> {
        safe_entries(&self.path, self.evidence.source().identity.dimensions)?;
        let expected = &self.evidence.seal.files[0];
        regular(&self.path.join("vectors.bin"), expected.bytes)?;
        if self.file.metadata().map_err(|_| repair())?.len() != expected.bytes {
            return Err(repair());
        }
        self.file.seek(SeekFrom::Start(0)).map_err(|_| repair())?;
        let mut header = [0u8; 24];
        self.file.read_exact(&mut header).map_err(|_| repair())?;
        let dimension = self.evidence.source().identity.dimensions;
        let count = self.evidence.count();
        let logical = count
            .checked_mul(u64::from(dimension) * 4)
            .and_then(|n| n.checked_add(24))
            .ok_or_else(repair)?;
        if header[..4] != 0x53764442u32.to_le_bytes()
            || header[4..6] != 4u16.to_le_bytes()
            || u32::from(u16::from_le_bytes(
                header[6..8].try_into().map_err(|_| repair())?,
            )) != dimension
            || u64::from_le_bytes(header[8..16].try_into().map_err(|_| repair())?) != count
            || header[16..] != [0; 8]
            || expected.bytes < logical
        {
            return Err(repair());
        }
        self.file.seek(SeekFrom::Start(0)).map_err(|_| repair())?;
        let mut hash = Sha256::new();
        let mut remaining = expected.bytes;
        let mut buffer = [0u8; 65536];
        while remaining > 0 {
            let n = remaining.min(buffer.len() as u64) as usize;
            self.file
                .read_exact(&mut buffer[..n])
                .map_err(|_| repair())?;
            hash.update(&buffer[..n]);
            remaining -= n as u64;
        }
        let mut extra = [0];
        if self.file.read(&mut extra).map_err(|_| repair())? != 0
            || format!("{:x}", hash.finalize()) != expected.sha256
            || self.file.metadata().map_err(|_| repair())?.len() != expected.bytes
        {
            return Err(repair());
        }
        // Recheck the named file as well as the retained descriptor. A replacement
        // path cannot be certified by hashing the now-unlinked original handle.
        if preflight::hash_file(&self.path.join("vectors.bin"), expected.bytes, false)?
            != expected.sha256
        {
            return Err(repair());
        }
        Ok(())
    }
    pub(super) fn copy(&mut self, start: u64, limit: u32) -> AppResult<Vec<(u64, Vec<f32>)>> {
        if limit == 0 || limit > 200 || start > self.evidence.count() {
            return Err(repair());
        }
        let end = start
            .checked_add(u64::from(limit))
            .ok_or_else(repair)?
            .min(self.evidence.count());
        let dimension = self.evidence.source().identity.dimensions;
        let mut bytes = vec![0u8; dimension as usize * 4];
        let mut out = Vec::with_capacity((end - start) as usize);
        for id in start..end {
            let offset = id
                .checked_mul(u64::from(dimension) * 4)
                .and_then(|n| n.checked_add(24))
                .ok_or_else(repair)?;
            self.file
                .seek(SeekFrom::Start(offset))
                .map_err(|_| repair())?;
            self.file.read_exact(&mut bytes).map_err(|_| repair())?;
            let vector = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().expect("four bytes")))
                .collect();
            out.push((id, vector));
        }
        Ok(out)
    }
}
pub(super) fn exclusive_lock(path: &Path) -> AppResult<File> {
    let m = std::fs::symlink_metadata(path.join("srvdb.lock")).map_err(|_| repair())?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > 4096 {
        return Err(repair());
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.join("srvdb.lock"))
        .map_err(|_| repair())?;
    lock.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => crate::error::AppError::Intelligence(Code::Busy),
        _ => repair(),
    })?;
    Ok(lock)
}
fn regular(path: &Path, length: u64) -> AppResult<()> {
    let m = std::fs::symlink_metadata(path).map_err(|_| repair())?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() != length {
        return Err(repair());
    }
    Ok(())
}

fn safe_entries(path: &Path, dimension: u32) -> AppResult<()> {
    let mut vectors = false;
    let mut metadata = false;
    for (ordinal, entry) in std::fs::read_dir(path).map_err(|_| repair())?.enumerate() {
        if ordinal >= 16 {
            return Err(repair());
        }
        let entry = entry.map_err(|_| repair())?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(repair)?;
        let cap = match name {
            "vectors.bin" => {
                vectors = true;
                super::seal::vector_cap(dimension)?
            }
            "metadata.db" => {
                metadata = true;
                super::seal::METADATA_CAP
            }
            "hnsw.graph" => super::seal::GRAPH_CAP,
            "srvdb.lock" => 4096,
            _ => return Err(repair()),
        };
        let m = std::fs::symlink_metadata(entry.path()).map_err(|_| repair())?;
        if !m.is_file()
            || m.file_type().is_symlink()
            || m.len() > cap
            || (name == "metadata.db" && m.len() == 0)
        {
            return Err(repair());
        }
    }
    if !vectors || !metadata {
        return Err(repair());
    }
    Ok(())
}
