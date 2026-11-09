//! Original generated paths remain checkable after document/file-row deletion.
use super::*;
use crate::{intelligence::jobs::OperationLease, storage::LocalStore};
use journal::Header;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) const FILE_PAGE: u32 = 100;
// Issued only by joined filesystem work, never reconstructed from SQLite path
// text. The journal retains only its digest, not a potentially 256 KiB codec.
#[derive(Clone)]
pub(crate) struct ManagedDirectory {
    path: PathBuf,
    sha256: String,
}
impl ManagedDirectory {
    fn resolve(data_dir: &Path) -> AppResult<Self> {
        let path =
            crate::knowledge::copying::expected_managed_dir(data_dir).map_err(|_| repair())?;
        let sha256 = path_hash(&path)?;
        Ok(Self { path, sha256 })
    }
    fn require_header(&self, header: &Header) -> AppResult<()> {
        if self.sha256 != header.file_directory_sha256 {
            return Err(repair());
        }
        Ok(())
    }
    fn require_descriptor(&self, descriptor: &Descriptor) -> AppResult<()> {
        if path_hash(&self.path.join(&descriptor.relative_name))? != descriptor.path_sha256 {
            return Err(repair());
        }
        Ok(())
    }
}
fn path_hash(path: &Path) -> AppResult<String> {
    Ok(super::super::seal::digest(
        crate::knowledge::store::encode_path(path).map_err(|_| repair())?,
    ))
}
pub(crate) async fn resolve_directory(
    data_dir: PathBuf,
    owner: OperationLease,
) -> AppResult<ManagedDirectory> {
    tokio::task::spawn_blocking(move || {
        let _owner = owner;
        ManagedDirectory::resolve(&data_dir)
    })
    .await
    .map_err(|_| repair())?
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    id: Uuid,
    relative_name: String,
    document: Uuid,
    version: u32,
    job: Option<Uuid>,
    path_sha256: String,
}
impl Descriptor {
    fn validate(&self, input: &DeleteInput) -> AppResult<()> {
        let prefix = format!("{}.", self.id);
        if self.document != input.document_id
            || self.version == 0
            || self.version > input.document_version
            || self.relative_name.len() > 96
            || !matches!(
                self.relative_name.strip_prefix(&prefix),
                Some("pdf" | "docx" | "txt" | "md")
            )
            || (self.version == input.document_version && self.job != input.document.import_job)
            || !super::super::codec::is_hash(&self.path_sha256)
        {
            return Err(repair());
        }
        Ok(())
    }
    fn require_import(&self, tx: &Transaction<'_>, input: &DeleteInput) -> AppResult<()> {
        if let Some(job) = self.job {
            let owned: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_document_imports \
                WHERE document_id=?1 AND document_version=?2 AND job_id=?3 AND file_id=?4 \
                AND (?2!=?5 OR (parser_identity=?6 AND sha256=?7)))",
                params![
                    self.document.to_string(),
                    self.version,
                    job.to_string(),
                    self.id.to_string(),
                    input.document_version,
                    input.document.parser,
                    input.document.sha256
                ],
                |row| row.get(0),
            )?;
            if !owned {
                return Err(repair());
            }
        }
        require_no_sibling(tx, self)
    }
}
fn require_no_sibling(tx: &Transaction<'_>, descriptor: &Descriptor) -> AppResult<()> {
    if let Some(job) = descriptor.job {
        let additional: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_managed_files \
            WHERE job_id=?1 AND file_id!=?2)",
            params![job.to_string(), descriptor.id.to_string()],
            |row| row.get(0),
        )?;
        if additional {
            return Err(repair());
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePage {
    version: u32,
    operation: Uuid,
    index: u32,
    descriptors: Vec<Descriptor>,
}
pub(super) struct FileProof {
    pub(super) count: u32,
    pub(super) pages: u32,
    pub(super) root: String,
    pub(super) directory_sha256: String,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Cleaned {
    version: u32,
    operation: Uuid,
    publication: Uuid,
    header_sha256: String,
    file_root: String,
}
// Only the joined filesystem check below can issue this receipt. Terminal
// SQLite hooks validate its separate immutable record, never a mutable flag.
pub(crate) struct FileReceipt(Cleaned);
// Not serializable or reconstructible from a prior cleaned flag/receipt.
// The exact complete publication was checked under this still-held owner.
pub(crate) struct CollectionReceipt {
    publication: Uuid,
    root: String,
    _owner: OperationLease,
}
impl CollectionReceipt {
    pub(super) fn require(&self, publication: &journal::Publication) -> AppResult<()> {
        if self.publication != publication.authority.id || self.root != publication.root {
            return Err(repair());
        }
        Ok(())
    }
}
fn cleaned(header: &Header) -> AppResult<Cleaned> {
    Ok(Cleaned {
        version: 1,
        operation: header.input.operation_id,
        publication: header.publication,
        header_sha256: hash(header)?,
        file_root: header.file_root.clone(),
    })
}
pub(super) fn validate_receipt(
    tx: &Transaction<'_>,
    header: &Header,
    required: bool,
) -> AppResult<()> {
    let receipt: Option<Cleaned> = read(tx, &key("cleaned", header.input.operation_id))?;
    match receipt {
        Some(receipt) if receipt != cleaned(header)? => Err(repair()),
        None if required => Err(repair()),
        _ => Ok(()),
    }
}
fn root(
    input: &DeleteInput,
    count: u32,
    directory_sha256: &str,
    hashes: &[String],
) -> AppResult<String> {
    let mut root = Sha256::new();
    field(&mut root, b"voxbridge-delete-files-v1");
    field(&mut root, encode(&input.document)?.as_bytes());
    field(&mut root, &count.to_le_bytes());
    field(&mut root, directory_sha256.as_bytes());
    for hash in hashes {
        field(&mut root, hash.as_bytes())
    }
    Ok(finish(root))
}
fn page_hash(page: &FilePage) -> AppResult<String> {
    hash(&(page.index, &page.descriptors))
}
pub(super) fn capture(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    directory: &ManagedDirectory,
    write: bool,
) -> AppResult<FileProof> {
    let managed = {
        let mut q = tx.prepare("SELECT managed_path FROM documents WHERE id=?1")?;
        let mut rows = q.query([input.document_id.to_string()])?;
        state::optional_text(
            rows.next()?.ok_or_else(repair)?,
            0,
            crate::knowledge::store::MAX_PATH_JSON_BYTES,
        )
        .map_err(|_| repair())?
        .map(crate::knowledge::store::decode_path)
        .transpose()
        .map_err(|_| repair())?
    };
    let mut matched_current = managed.is_none();
    let mut after = String::new();
    let mut count = 0u32;
    let mut hashes = Vec::new();
    loop {
        let descriptors = {
            // Match the complete inverse original-owner set checked after
            // publication, while the document/import associations still exist.
            let mut q = tx.prepare("SELECT file_id,relative_name,document_version,job_id,state,document_id \
                FROM knowledge_managed_files f WHERE file_id>?1 \
                AND (document_id=?2 OR (?3 IS NOT NULL AND job_id=?3) OR EXISTS(SELECT 1 \
                    FROM intelligence_jobs j WHERE j.id=f.job_id AND j.kind='import' AND j.object_id=?2)) \
                ORDER BY file_id LIMIT 100")?;
            let mut rows = q.query(params![
                after,
                input.document_id.to_string(),
                input.document.import_job.map(|job| job.to_string())
            ])?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                let text = |column, max| state::text(r, column, max).map_err(|_| repair());
                let raw_id = text(0, 36)?;
                let id = Uuid::parse_str(raw_id).map_err(|_| repair())?;
                let raw_job = state::optional_text(r, 3, 36).map_err(|_| repair())?;
                let job = raw_job
                    .map(|raw| {
                        let id = Uuid::parse_str(raw).map_err(|_| repair())?;
                        if id.to_string() != raw {
                            return Err(repair());
                        }
                        Ok(id)
                    })
                    .transpose()?;
                let relative_name = text(1, 96)?.to_owned();
                let expected_path = directory.path.join(&relative_name);
                // A missing/changed nullable owner is corruption, never a
                // reason to synthesize a corrected descriptor from the input.
                let raw_document = text(5, 36)?;
                let document = Uuid::parse_str(raw_document).map_err(|_| repair())?;
                if document.to_string() != raw_document {
                    return Err(repair());
                }
                let descriptor = Descriptor {
                    id,
                    relative_name,
                    document,
                    version: r.get(2).map_err(|_| repair())?,
                    job,
                    path_sha256: path_hash(&expected_path)?,
                };
                descriptor.validate(input)?;
                descriptor.require_import(tx, input)?;
                if id.to_string() != raw_id
                    || !matches!(text(4, 32)?, "retained" | "delete_pending")
                {
                    return Err(repair());
                }
                if descriptor.version == input.document_version {
                    if let Some(managed) = &managed {
                        if *managed != expected_path {
                            return Err(repair());
                        }
                        matched_current = true;
                    }
                }
                out.push(descriptor);
            }
            out
        };
        if descriptors.is_empty() {
            break;
        }
        after = descriptors.last().unwrap().id.to_string();
        count = count
            .checked_add(descriptors.len() as u32)
            .filter(|count| *count <= 200_000)
            .ok_or_else(repair)?;
        let page = FilePage {
            version: 1,
            operation: input.operation_id,
            index: hashes.len() as u32,
            descriptors,
        };
        encode(&page)?;
        hashes.push(page_hash(&page)?);
        if write {
            insert(
                tx,
                &page_key("files", input.operation_id, u64::from(page.index)),
                &page,
            )?
        }
    }
    if !matched_current {
        return Err(repair());
    }
    Ok(FileProof {
        count,
        pages: hashes.len() as u32,
        root: root(input, count, &directory.sha256, &hashes)?,
        directory_sha256: directory.sha256.clone(),
    })
}
fn page(tx: &Transaction<'_>, header: &Header, index: u32) -> AppResult<FilePage> {
    let page: FilePage = read(
        tx,
        &page_key("files", header.input.operation_id, u64::from(index)),
    )?
    .ok_or_else(repair)?;
    let count = header
        .file_count
        .checked_sub(index.checked_mul(FILE_PAGE).ok_or_else(repair)?)
        .ok_or_else(repair)?
        .min(FILE_PAGE);
    if page.version != 1
        || page.operation != header.input.operation_id
        || page.index != index
        || page.descriptors.len() as u32 != count
        || count == 0
    {
        return Err(repair());
    }
    for descriptor in &page.descriptors {
        descriptor.validate(&header.input)?
    }
    Ok(page)
}
pub(super) fn validate_pages(tx: &Transaction<'_>, header: &Header) -> AppResult<()> {
    let count: u64 = tx.query_row(
        "SELECT count(*) FROM intelligence_meta WHERE key LIKE ?1",
        [format!("{}.%", key("files", header.input.operation_id))],
        |r| r.get(0),
    )?;
    if count != u64::from(header.file_pages) {
        return Err(repair());
    }
    let mut hashes = Vec::new();
    let mut previous = None;
    for index in 0..header.file_pages {
        let page = page(tx, header, index)?;
        for descriptor in &page.descriptors {
            if previous.is_some_and(|prior| prior >= descriptor.id) {
                return Err(repair());
            }
            previous = Some(descriptor.id);
        }
        hashes.push(page_hash(&page)?);
    }
    if root(
        &header.input,
        header.file_count,
        &header.file_directory_sha256,
        &hashes,
    )? != header.file_root
    {
        return Err(repair());
    }
    Ok(())
}
fn metadata_absent(tx: &Transaction<'_>, descriptor: &Descriptor) -> AppResult<bool> {
    require_no_sibling(tx, descriptor)?;
    let mut q = tx.prepare(
        "SELECT relative_name,document_id,document_version,job_id,state \
        FROM knowledge_managed_files WHERE file_id=?1",
    )?;
    let mut rows = q.query([descriptor.id.to_string()])?;
    let Some(r) = rows.next()? else {
        let reused: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_managed_files WHERE relative_name=?1)",
            [&descriptor.relative_name],
            |r| r.get(0),
        )?;
        if reused {
            return Err(repair());
        }
        return Ok(true);
    };
    if state::text(r, 0, 96).map_err(|_| repair())? != descriptor.relative_name
        || state::optional_text(r, 1, 36)
            .map_err(|_| repair())?
            .is_some()
        || r.get::<_, u32>(2).map_err(|_| repair())? != descriptor.version
        || state::optional_text(r, 3, 36).map_err(|_| repair())?
            != descriptor.job.as_ref().map(|id| id.to_string()).as_deref()
        || state::text(r, 4, 32).map_err(|_| repair())? != "delete_pending"
    {
        return Err(repair());
    }
    Ok(false)
}
// Merge the current owner set with the immutable sorted descriptors. A native
// import owns one generated file; its job still names the original document
// after the document/import rows have been deleted. No whole set is allocated.
fn require_metadata_set(tx: &Transaction<'_>, header: &Header) -> AppResult<()> {
    let mut page_index = 0;
    let mut descriptors = Vec::new().into_iter();
    let mut expected: Option<Descriptor> = None;
    let mut after = String::new();
    let mut seen = 0u32;
    loop {
        let mut query = tx.prepare("SELECT file_id FROM knowledge_managed_files f WHERE file_id>?1 \
            AND (document_id=?2 OR (?3 IS NOT NULL AND job_id=?3) OR EXISTS(SELECT 1 \
                FROM intelligence_jobs j WHERE j.id=f.job_id AND j.kind='import' AND j.object_id=?2)) \
            ORDER BY file_id LIMIT 100")?;
        let mut rows = query.query(params![
            after,
            header.input.document_id.to_string(),
            header.input.document.import_job.map(|job| job.to_string())
        ])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            found = true;
            let raw = state::text(row, 0, 36).map_err(|_| repair())?;
            let id = Uuid::parse_str(raw).map_err(|_| repair())?;
            if id.to_string() != raw {
                return Err(repair());
            }
            seen = seen
                .checked_add(1)
                .filter(|seen| *seen <= 200_000)
                .ok_or_else(repair)?;
            after = raw.to_owned();
            loop {
                if expected
                    .as_ref()
                    .is_some_and(|descriptor| descriptor.id >= id)
                {
                    break;
                }
                expected = match descriptors.next() {
                    Some(descriptor) => Some(descriptor),
                    None if page_index < header.file_pages => {
                        descriptors = page(tx, header, page_index)?.descriptors.into_iter();
                        page_index += 1;
                        descriptors.next()
                    }
                    None => return Err(repair()),
                };
            }
            let descriptor = expected.as_ref().ok_or_else(repair)?;
            if descriptor.id != id || metadata_absent(tx, descriptor)? {
                return Err(repair());
            }
        }
        if !found {
            return Ok(());
        }
    }
}
pub(super) fn require_metadata_absent(tx: &Transaction<'_>, header: &Header) -> AppResult<()> {
    for index in 0..header.file_pages {
        for descriptor in page(tx, header, index)?.descriptors {
            if !metadata_absent(tx, &descriptor)? {
                return Err(repair());
            }
        }
    }
    require_metadata_set(tx, header)?;
    Ok(())
}
fn check_paths(
    directory: &ManagedDirectory,
    descriptors: Vec<(Descriptor, bool)>,
    removed: bool,
) -> AppResult<()> {
    for (descriptor, absent) in descriptors {
        directory.require_descriptor(&descriptor)?;
        for name in [descriptor.relative_name, format!("{}.tmp", descriptor.id)] {
            match std::fs::symlink_metadata(directory.path.join(name)) {
                Ok(metadata) => {
                    if absent || removed || !metadata.is_file() || metadata.file_type().is_symlink()
                    {
                        return Err(repair());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(repair()),
            }
        }
    }
    Ok(())
}
pub(crate) fn guard_managed_cleanup(store: &LocalStore, data_dir: &Path) -> AppResult<()> {
    let pending: bool = store.transaction(|tx| {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 \
        FROM knowledge_managed_files WHERE state='delete_pending')",
            [],
            |row| row.get(0),
        )?)
    })?;
    if !pending {
        return Ok(());
    }
    let mut after = String::new();
    loop {
        let operations = store.transaction(|tx| journal::scan_operations(tx, &after))?;
        if operations.is_empty() {
            return Ok(());
        }
        // Ordinary import cleanup has its own Storage/repair-state path. Only
        // certificate-bearing cleanup enters this stricter authority guard.
        let directory = ManagedDirectory::resolve(data_dir)?;
        for operation in operations {
            after = operation.to_string();
            let header = store.transaction(|tx| {
                let header = journal::load(tx, operation)?.ok_or_else(repair)?.header;
                require_metadata_set(tx, &header)?;
                Ok(header)
            })?;
            directory.require_header(&header)?;
            for index in 0..header.file_pages {
                let descriptors = store.transaction(|tx| {
                    page(tx, &header, index)?
                        .descriptors
                        .into_iter()
                        .map(|descriptor| {
                            let absent = metadata_absent(tx, &descriptor)?;
                            Ok((descriptor, absent))
                        })
                        .collect::<AppResult<Vec<_>>>()
                })?;
                // cleanup_rows is already entered blocking work under its real
                // owner. The transaction above ends before inspecting paths.
                check_paths(&directory, descriptors, false)?;
            }
        }
    }
}
pub(crate) async fn check_files(
    store: Arc<LocalStore>,
    data_dir: PathBuf,
    operation: Uuid,
    owner: OperationLease,
    removed: bool,
) -> AppResult<Option<FileReceipt>> {
    let header = store.transaction(|tx| {
        let header = journal::load(tx, operation)?.ok_or_else(repair)?.header;
        require_metadata_set(tx, &header)?;
        Ok(header)
    })?;
    resolve_directory(data_dir.clone(), owner.clone())
        .await?
        .require_header(&header)?;
    for index in 0..header.file_pages {
        let descriptors = store.transaction(|tx| {
            let page = page(tx, &header, index)?;
            page.descriptors
                .into_iter()
                .map(|descriptor| {
                    let absent = metadata_absent(tx, &descriptor)?;
                    if removed && !absent {
                        return Err(repair());
                    }
                    Ok((descriptor, absent))
                })
                .collect::<AppResult<Vec<_>>>()
        })?;
        let data_dir = data_dir.clone();
        let owner = owner.clone();
        let directory_sha256 = header.file_directory_sha256.clone();
        tokio::task::spawn_blocking(move || {
            let _owner = owner;
            let directory = ManagedDirectory::resolve(&data_dir)?;
            if directory.sha256 != directory_sha256 {
                return Err(repair());
            }
            check_paths(&directory, descriptors, removed)
        })
        .await
        .map_err(|_| repair())??;
    }
    store.transaction(|tx| {
        if removed {
            require_metadata_absent(tx, &header)?
        } else {
            require_metadata_set(tx, &header)?
        }
        Ok(())
    })?;
    Ok(if removed {
        Some(FileReceipt(cleaned(&header)?))
    } else {
        None
    })
}
pub(crate) fn mark_files_cleaned(
    tx: &Transaction<'_>,
    operation: Uuid,
    receipt: &FileReceipt,
) -> AppResult<()> {
    let mut value = journal::load(tx, operation)?.ok_or_else(repair)?;
    if receipt.0 != cleaned(&value.header)? {
        return Err(repair());
    }
    require_metadata_absent(tx, &value.header)?;
    match read::<Cleaned>(tx, &key("cleaned", operation))? {
        Some(existing) if existing != receipt.0 => return Err(repair()),
        Some(_) => {}
        None => insert(tx, &key("cleaned", operation), &receipt.0)?,
    }
    value.files_cleaned = true;
    update(tx, &key("operation", operation), &value)
}
pub(crate) async fn check_collection(
    store: Arc<LocalStore>,
    data_dir: PathBuf,
    publication_id: Uuid,
    owner: OperationLease,
) -> AppResult<CollectionReceipt> {
    let publication = store.transaction(|tx| journal::publication(tx, publication_id))?;
    for batch in 0..publication.batches {
        let operations = store.transaction(|tx| journal::operations(tx, publication_id, batch))?;
        for operation in operations {
            check_files(
                store.clone(),
                data_dir.clone(),
                operation,
                owner.clone(),
                true,
            )
            .await?
            .ok_or_else(repair)?;
        }
    }
    Ok(CollectionReceipt {
        publication: publication_id,
        root: publication.root,
        _owner: owner,
    })
}
