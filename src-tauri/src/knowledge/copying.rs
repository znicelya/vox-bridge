//! One-handle managed copying; all filesystem effects belong to the file-work owner.
use super::{
    MAX_INPUT_BYTES,
    store::{encode_path, error, safe},
    types::DocumentKind,
};
use crate::{
    error::AppResult, intelligence::types::IntelligenceErrorCode as Code, storage::LocalStore,
};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::watch;
use uuid::Uuid;

pub(crate) struct CopyRequest {
    pub source: PathBuf,
    pub data_dir: PathBuf,
    pub file_id: Uuid,
    pub store: Arc<LocalStore>,
    pub cancel: watch::Receiver<bool>,
}
pub(crate) struct CopiedFile {
    pub original: PathBuf,
    pub managed: PathBuf,
    pub kind: DocumentKind,
    pub title: String,
    pub sha256: String,
}
pub(crate) trait FileCopier: Send + Sync {
    fn copy(&self, request: CopyRequest) -> AppResult<CopiedFile>;
}
pub(crate) struct ManagedCopier;
impl FileCopier for ManagedCopier {
    fn copy(&self, request: CopyRequest) -> AppResult<CopiedFile> {
        copy_with_open(request, open_source)
    }
}

fn io(_: std::io::Error) -> crate::error::AppError {
    error(Code::Storage)
}
fn check_cancel(cancel: &watch::Receiver<bool>) -> AppResult<()> {
    if *cancel.borrow() {
        Err(error(Code::Cancelled))
    } else {
        Ok(())
    }
}
fn kind(path: &Path) -> AppResult<(DocumentKind, &'static str)> {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => Ok((DocumentKind::Pdf, "pdf")),
        Some("docx") => Ok((DocumentKind::Docx, "docx")),
        Some("txt") => Ok((DocumentKind::Text, "txt")),
        Some("md") => Ok((DocumentKind::Markdown, "md")),
        _ => Err(error(Code::UnsupportedDocument)),
    }
}
fn regular(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.is_file() && meta.file_attributes() & 0x400 == 0
    }
    #[cfg(not(windows))]
    {
        meta.is_file() && !meta.file_type().is_symlink()
    }
}
fn open_source(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    options.open(path)
}

#[cfg(unix)]
fn same(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
#[cfg(windows)]
fn same(a: &Metadata, b: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    a.file_size() == b.file_size()
        && a.last_write_time() == b.last_write_time()
        && a.creation_time() == b.creation_time()
        && a.file_attributes() == b.file_attributes()
}
#[cfg(windows)]
fn handle_identity(file: &File) -> AppResult<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: live retained file handle and valid output storage, initialized on success.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(error(Code::UnsupportedDocument));
    }
    // SAFETY: the successful API call initializes every field of the structure.
    let info = unsafe { info.assume_init() };
    Ok((
        info.dwVolumeSerialNumber,
        ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
    ))
}
fn verify_path(path: &Path, file: &File, initial: &Metadata) -> AppResult<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| error(Code::UnsupportedDocument))?;
    if !regular(&meta) || !same(initial, &meta) || !same(initial, &file.metadata().map_err(io)?) {
        return Err(error(Code::UnsupportedDocument));
    }
    #[cfg(windows)]
    {
        let other = open_source(path).map_err(|_| error(Code::UnsupportedDocument))?;
        if handle_identity(file)? != handle_identity(&other)? {
            return Err(error(Code::UnsupportedDocument));
        }
    }
    Ok(())
}

pub(super) fn managed_dir(data_dir: &Path, create: bool) -> AppResult<PathBuf> {
    managed_location(data_dir, create, false)
}
pub(super) fn expected_managed_dir(data_dir: &Path) -> AppResult<PathBuf> {
    managed_location(data_dir, false, true)
}
fn managed_location(data_dir: &Path, create: bool, allow_missing: bool) -> AppResult<PathBuf> {
    let base = fs::canonicalize(data_dir).map_err(io)?;
    let mut path = base;
    let mut missing = false;
    for name in ["knowledge", "documents"] {
        path.push(name);
        if missing {
            continue;
        }
        if create {
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io(e)),
            }
        }
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => {
                missing = true;
                continue;
            }
            Err(error) => return Err(io(error)),
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(error(Code::Storage));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(error(Code::Storage));
            }
        }
    }
    Ok(path)
}

pub(super) fn copy_with_open(
    request: CopyRequest,
    open: impl FnOnce(&Path) -> std::io::Result<File>,
) -> AppResult<CopiedFile> {
    let id = request.file_id;
    let mut intent_created = false;
    let mut owned_temp = None;
    let mut published = false;
    let result = (|| {
        check_cancel(&request.cancel)?;
        let (kind, extension) = kind(&request.source)?;
        let initial =
            fs::symlink_metadata(&request.source).map_err(|_| error(Code::UnsupportedDocument))?;
        if !regular(&initial) {
            return Err(error(Code::UnsupportedDocument));
        }
        if initial.len() > MAX_INPUT_BYTES {
            return Err(error(Code::ResourceLimit));
        }
        if initial.len() == 0 {
            return Err(error(Code::UnsupportedDocument));
        }
        let mut source = open(&request.source).map_err(|_| error(Code::UnsupportedDocument))?;
        verify_path(&request.source, &source, &initial)?;
        let original =
            fs::canonicalize(&request.source).map_err(|_| error(Code::UnsupportedDocument))?;
        verify_path(&original, &source, &initial)?;
        encode_path(&original)?;
        let title = original
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| error(Code::UnsupportedDocument))?
            .to_owned();
        let directory = managed_dir(&request.data_dir, true)?;
        let name = format!("{id}.{extension}");
        let managed = directory.join(&name);
        encode_path(&managed)?;
        let temp = directory.join(format!("{id}.tmp"));
        for path in [&managed, &temp] {
            match fs::symlink_metadata(path) {
                Ok(_) => return Err(error(Code::Storage)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        request.store.transaction(|tx| {tx.execute("INSERT INTO knowledge_managed_files (file_id,relative_name,state,created_at_ms) VALUES (?1,?2,'copying',?3)",params![id.to_string(),name,crate::intelligence::jobs::now_ms()])?;Ok(())}).map_err(safe)?;
        intent_created = true;
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(io)?;
        owned_temp = Some(temp.clone());
        let mut buffer = [0_u8; 64 * 1024 + 3];
        let mut carry = 0;
        let mut total = 0_u64;
        let mut hash = Sha256::new();
        loop {
            check_cancel(&request.cancel)?;
            let n = source
                .read(&mut buffer[carry..carry + 64 * 1024])
                .map_err(io)?;
            if n == 0 {
                if carry != 0 {
                    return Err(error(Code::UnsupportedDocument));
                }
                break;
            }
            total = total
                .checked_add(n as u64)
                .ok_or_else(|| error(Code::ResourceLimit))?;
            if total > MAX_INPUT_BYTES {
                return Err(error(Code::ResourceLimit));
            }
            let bytes = &buffer[carry..carry + n];
            if total == n as u64 {
                let pdf = bytes.starts_with(b"%PDF-");
                let zip = bytes.starts_with(b"PK\x03\x04");
                if match kind {
                    DocumentKind::Pdf => !pdf,
                    DocumentKind::Docx => !zip,
                    _ => pdf || zip,
                } {
                    return Err(error(Code::UnsupportedDocument));
                }
            }
            target.write_all(bytes).map_err(io)?;
            hash.update(bytes);
            if matches!(kind, DocumentKind::Text | DocumentKind::Markdown) {
                let used = carry + n;
                if buffer[..used].contains(&0) {
                    return Err(error(Code::UnsupportedDocument));
                }
                carry = match std::str::from_utf8(&buffer[..used]) {
                    Ok(_) => 0,
                    Err(e) if e.error_len().is_none() => used - e.valid_up_to(),
                    Err(_) => return Err(error(Code::UnsupportedDocument)),
                };
                if carry > 3 {
                    return Err(error(Code::UnsupportedDocument));
                }
                if carry > 0 {
                    buffer.copy_within(used - carry..used, 0);
                }
            }
        }
        check_cancel(&request.cancel)?;
        verify_path(&request.source, &source, &initial)?;
        verify_path(&original, &source, &initial)?;
        if total != initial.len() {
            return Err(error(Code::UnsupportedDocument));
        }
        target.sync_all().map_err(io)?;
        drop(target);
        // A unique hard-link publication is atomic and cannot overwrite an existing file.
        fs::hard_link(&temp, &managed).map_err(io)?;
        published = true;
        fs::remove_file(&temp).map_err(io)?;
        owned_temp = None;
        #[cfg(unix)]
        File::open(&directory)
            .and_then(|f| f.sync_all())
            .map_err(io)?;
        Ok(CopiedFile {
            original,
            managed,
            kind,
            title,
            sha256: format!("{:x}", hash.finalize()),
        })
    })();
    if result.is_err() && intent_created {
        request.store.transaction(|tx| {tx.execute("UPDATE knowledge_managed_files SET state='delete_pending' WHERE file_id=?1 AND state='copying'",[id.to_string()])?;Ok(())}).map_err(safe)?;
        if published {
            cleanup_candidate(&request.store, &request.data_dir, id)?;
        } else {
            // Failed create/link does not acknowledge ownership of an existing
            // name. Remove only a temp whose create_new actually succeeded.
            if let Some(temp) = owned_temp {
                let directory = managed_dir(&request.data_dir, false)?;
                match fs::remove_file(&temp) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(io(e)),
                }
                #[cfg(unix)]
                File::open(&directory)
                    .and_then(|f| f.sync_all())
                    .map_err(io)?;
                #[cfg(not(unix))]
                let _ = directory;
            }
            request.store.transaction(|tx|{tx.execute("DELETE FROM knowledge_managed_files WHERE file_id=?1 AND state='delete_pending'",[id.to_string()])?;Ok(())}).map_err(safe)?;
        }
    }
    result
}

pub(super) fn cleanup_candidate(store: &LocalStore, data_dir: &Path, id: Uuid) -> AppResult<()> {
    use rusqlite::OptionalExtension;
    let name:Option<String>=store.transaction(|tx| Ok(tx.query_row("SELECT relative_name FROM knowledge_managed_files WHERE file_id=?1 AND state='delete_pending'",[id.to_string()],|r|r.get(0)).optional()?)).map_err(safe)?;
    let Some(name) = name else {
        return Ok(());
    };
    let expected_prefix = format!("{id}.");
    if !name.starts_with(&expected_prefix)
        || !matches!(
            name.strip_prefix(&expected_prefix),
            Some("pdf" | "docx" | "txt" | "md")
        )
    {
        return Err(error(Code::Storage));
    }
    let directory = managed_dir(data_dir, false)?;
    for name in [name, format!("{id}.tmp")] {
        match fs::remove_file(directory.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
    }
    #[cfg(unix)]
    File::open(directory)
        .and_then(|f| f.sync_all())
        .map_err(io)?;
    store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_managed_files WHERE file_id=?1 AND state='delete_pending'",
                [id.to_string()],
            )?;
            Ok(())
        })
        .map_err(safe)
}
