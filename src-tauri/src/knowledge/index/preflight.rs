//! Bounded byte checks before any eager database/graph decoder is entered.
use super::{
    repair,
    seal::{FileSeal, GRAPH_CAP, Generation, METADATA_CAP, RECORD_CAP, Seal, vector_cap},
};
use crate::error::AppResult;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub(super) fn generation_path(root: &Path, generation: &Generation) -> AppResult<PathBuf> {
    Ok(root
        .join(generation.key()?)
        .join(generation.generation.to_string()))
}
pub(super) fn create(root: &Path, generation: &Generation) -> AppResult<PathBuf> {
    let path = generation_path(root, generation)?;
    directories(path.parent().ok_or_else(repair)?, true)?;
    // Even an existing empty directory is not a newly unique owned creation.
    fs::create_dir(&path).map_err(|_| repair())?;
    Ok(path)
}
pub(super) fn verify(root: &Path, seal: &Seal) -> AppResult<PathBuf> {
    let path = generation_path(root, &seal.descriptor)?;
    let files = inspect(
        &path,
        seal.descriptor.identity.dimensions,
        seal.stored_count,
    )?;
    for actual in &files {
        let expected = seal
            .files
            .iter()
            .find(|f| f.name == actual.name)
            .ok_or_else(repair)?;
        if actual != expected {
            return Err(repair());
        }
    }
    // Only the derived graph may disappear. Base bytes must match exactly.
    if files.len() < 2 {
        return Err(repair());
    }
    if super::redb_header::HeaderSnapshot::read(
        &path.join("metadata.db"),
        seal.metadata_header.state(),
    )? != seal.metadata_header
    {
        return Err(repair());
    }
    Ok(path)
}
pub(super) fn inspect(path: &Path, dimension: u32, count: u64) -> AppResult<Vec<FileSeal>> {
    directories(path, false)?;
    let mut lengths = BTreeMap::new();
    for (n, entry) in fs::read_dir(path).map_err(|_| repair())?.enumerate() {
        if n >= 16 {
            return Err(repair());
        }
        let entry = entry.map_err(|_| repair())?;
        let name = entry.file_name().into_string().map_err(|_| repair())?;
        let cap = match name.as_str() {
            "vectors.bin" => vector_cap(dimension)?,
            "metadata.db" => METADATA_CAP,
            "hnsw.graph" => GRAPH_CAP,
            "srvdb.lock" => 4096,
            _ => return Err(repair()),
        };
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| repair())?;
        if !meta.is_file() || meta.len() > cap {
            return Err(repair());
        }
        lengths.insert(name, meta.len());
    }
    let vector_len = *lengths.get("vectors.bin").ok_or_else(repair)?;
    let metadata_len = *lengths.get("metadata.db").ok_or_else(repair)?;
    if metadata_len == 0 {
        return Err(repair());
    }
    check_vector_header(&path.join("vectors.bin"), dimension, count, vector_len)?;
    let mut result = Vec::with_capacity(3);
    for name in ["vectors.bin", "metadata.db", "hnsw.graph"] {
        if let Some(&bytes) = lengths.get(name) {
            let sha256 = hash_file(&path.join(name), bytes, name == "hnsw.graph")?;
            result.push(FileSeal {
                name: name.into(),
                bytes,
                sha256,
            });
        }
    }
    Ok(result)
}
pub(super) fn directories(path: &Path, create: bool) -> AppResult<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(repair());
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|_| repair())?
            }
            _ => return Err(repair()),
        }
    }
    Ok(())
}
fn check_vector_header(path: &Path, dimension: u32, count: u64, len: u64) -> AppResult<()> {
    let mut file = File::open(path).map_err(|_| repair())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).map_err(|_| repair())?;
    let u16_at = |i| u16::from_le_bytes([header[i], header[i + 1]]);
    let stored = u64::from_le_bytes(header[8..16].try_into().map_err(|_| repair())?);
    let logical = count
        .checked_mul(u64::from(dimension) * 4)
        .and_then(|n| n.checked_add(24))
        .ok_or_else(repair)?;
    if header[..4] != 0x53764442u32.to_le_bytes()
        || u16_at(4) != 4
        || u32::from(u16_at(6)) != dimension
        || count > RECORD_CAP
        || stored != count
        || header[16..] != [0; 8]
        || len < logical
        || len > vector_cap(dimension)?
    {
        return Err(repair());
    }
    Ok(())
}
pub(super) fn hash_file(path: &Path, expected: u64, graph: bool) -> AppResult<String> {
    let mut file = File::open(path).map_err(|_| repair())?;
    if file.metadata().map_err(|_| repair())?.len() != expected {
        return Err(repair());
    }
    let mut hash = Sha256::new();
    let mut crc = crc32fast::Hasher::new();
    let mut remaining = expected;
    let mut graph_crc = None;
    if graph {
        let mut header = [0; 24];
        file.read_exact(&mut header).map_err(|_| repair())?;
        if &header[..8] != b"SRVDHNSW"
            || header[8..12] != [1, 0, 0, 0]
            || u64::from_le_bytes(header[12..20].try_into().map_err(|_| repair())?).checked_add(24)
                != Some(expected)
        {
            return Err(repair());
        }
        graph_crc = Some(u32::from_le_bytes(
            header[20..24].try_into().map_err(|_| repair())?,
        ));
        remaining = remaining.checked_sub(24).ok_or_else(repair)?;
        hash.update(header);
    }
    let mut buffer = [0u8; 65536];
    while remaining > 0 {
        let size = remaining.min(buffer.len() as u64) as usize;
        let read = file.read(&mut buffer[..size]).map_err(|_| repair())?;
        if read == 0 {
            return Err(repair());
        }
        hash.update(&buffer[..read]);
        if graph {
            crc.update(&buffer[..read]);
        }
        remaining -= read as u64;
    }
    let mut extra = [0; 1];
    if file.read(&mut extra).map_err(|_| repair())? != 0
        || file.metadata().map_err(|_| repair())?.len() != expected
        || graph_crc.is_some_and(|expected| expected != crc.finalize())
    {
        return Err(repair());
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Called after controlled engine Drop. Existing files only; no silent create.
pub(super) fn sync_closed(path: &Path, dimension: u32) -> AppResult<()> {
    for name in ["vectors.bin", "metadata.db", "hnsw.graph"] {
        let file_path = path.join(name);
        match fs::symlink_metadata(&file_path) {
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
                let cap = match name {
                    "vectors.bin" => vector_cap(dimension)?,
                    "metadata.db" => METADATA_CAP,
                    _ => GRAPH_CAP,
                };
                if meta.len() > cap {
                    return Err(repair());
                }
                let file = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(file_path)
                    .map_err(|_| repair())?;
                file.sync_all().map_err(|_| repair())?;
            }
            Err(e) if name == "hnsw.graph" && e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(repair()),
        }
    }
    #[cfg(unix)]
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| repair())?;
    Ok(())
}
