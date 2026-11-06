use super::*;
use crate::error::AppError;
use crate::intelligence::types::IntelligenceErrorCode as Code;
use crate::knowledge::{
    MAX_INPUT_BYTES,
    copying::{CopyRequest, FileCopier, ManagedCopier},
};
use std::{fs, sync::Arc};
use tokio::sync::watch;

#[test]
fn documents_copy_streams_owned_text_and_preserves_canonical_source_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("电气参数.txt");
    fs::write(&source, "泵的额定电压为24V。\n").unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let id = Uuid::new_v4();
    let copy = ManagedCopier
        .copy(CopyRequest {
            source: source.clone(),
            data_dir: dir.path().into(),
            file_id: id,
            store: store.clone(),
            cancel: watch::channel(false).1,
        })
        .expect("regular UTF-8 source must be copied");
    assert_eq!(copy.original, fs::canonicalize(&source).unwrap());
    assert_eq!(fs::read(&copy.managed).unwrap(), fs::read(&source).unwrap());
    assert_eq!(copy.kind, crate::knowledge::types::DocumentKind::Text);
    assert_eq!(copy.title, "电气参数.txt");
    assert_eq!(
        copy.sha256,
        "ecb213b50cf3642d2a6f6aaa8807ca6b0cc6dcddc0fa4f1d88b7b5204867e079"
    );
    let (name, state): (String, String) = store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT relative_name,state FROM knowledge_managed_files WHERE file_id=?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(
        copy.managed.file_name().unwrap().to_str(),
        Some(name.as_str())
    );
    assert_eq!(state, "copying");
    assert!(!copy.managed.with_extension("tmp").exists());
}

#[test]
fn documents_copy_rejects_oversize_before_read_and_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("huge.txt");
    fs::File::create(&source)
        .unwrap()
        .set_len(MAX_INPUT_BYTES + 1)
        .unwrap();
    let result = ManagedCopier.copy(CopyRequest {
        source: source.clone(),
        data_dir: dir.path().into(),
        file_id: Uuid::new_v4(),
        store: Arc::new(LocalStore::in_memory().unwrap()),
        cancel: watch::channel(false).1,
    });
    assert!(matches!(
        result,
        Err(AppError::Intelligence(Code::ResourceLimit))
    ));
    assert_eq!(fs::metadata(source).unwrap().len(), MAX_INPUT_BYTES + 1);
}

#[test]
fn documents_copy_signatures_cancellation_and_symlinks_fail_without_following_source() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    for (name, data, code) in [
        (
            "wrong.pdf",
            b"ordinary text".as_slice(),
            Code::UnsupportedDocument,
        ),
        (
            "wrong.docx",
            b"%PDF-1.7".as_slice(),
            Code::UnsupportedDocument,
        ),
        (
            "wrong.txt",
            b"%PDF-1.7".as_slice(),
            Code::UnsupportedDocument,
        ),
        ("invalid.txt", b"\xff".as_slice(), Code::UnsupportedDocument),
        ("empty.txt", b"".as_slice(), Code::UnsupportedDocument),
    ] {
        let source = dir.path().join(name);
        fs::write(&source, data).unwrap();
        let result = ManagedCopier.copy(CopyRequest {
            source,
            data_dir: dir.path().into(),
            file_id: Uuid::new_v4(),
            store: store.clone(),
            cancel: watch::channel(false).1,
        });
        assert!(matches!(result,Err(AppError::Intelligence(found)) if found==code));
    }
    let source = dir.path().join("valid.txt");
    fs::write(&source, b"retained source").unwrap();
    let result = ManagedCopier.copy(CopyRequest {
        source: source.clone(),
        data_dir: dir.path().into(),
        file_id: Uuid::new_v4(),
        store: store.clone(),
        cancel: watch::channel(true).1,
    });
    assert!(matches!(
        result,
        Err(AppError::Intelligence(Code::Cancelled))
    ));
    #[cfg(unix)]
    {
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&source, &link).unwrap();
        assert!(
            ManagedCopier
                .copy(CopyRequest {
                    source: link,
                    data_dir: dir.path().into(),
                    file_id: Uuid::new_v4(),
                    store,
                    cancel: watch::channel(false).1
                })
                .is_err()
        );
    }
    assert_eq!(fs::read(source).unwrap(), b"retained source");
}

#[test]
fn documents_copy_complete_exact_32mib_and_split_utf8_streaming_boundary() {
    use std::io::{Read, Seek, SeekFrom, Write};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("boundary.txt");
    let mut file = fs::File::create(&source).unwrap();
    let buffer = [b'x'; 64 * 1024];
    for _ in 0..512 {
        file.write_all(&buffer).unwrap();
    }
    file.seek(SeekFrom::Start(65535)).unwrap();
    file.write_all("电".as_bytes()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let copy = ManagedCopier
        .copy(CopyRequest {
            source,
            data_dir: dir.path().into(),
            file_id: Uuid::new_v4(),
            store: Arc::new(LocalStore::in_memory().unwrap()),
            cancel: watch::channel(false).1,
        })
        .unwrap();
    assert_eq!(fs::metadata(&copy.managed).unwrap().len(), MAX_INPUT_BYTES);
    let mut read = fs::File::open(copy.managed).unwrap();
    read.seek(SeekFrom::Start(65534)).unwrap();
    let mut actual = [0; 5];
    read.read_exact(&mut actual).unwrap();
    assert_eq!(&actual, b"x\xe7\x94\xb5x");
}

#[test]
fn documents_copy_complete_detects_path_replacement_after_single_handle_open() {
    use crate::knowledge::copying::copy_with_open;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("race.txt");
    let retained = dir.path().join("retained.txt");
    fs::write(&source, b"initial source").unwrap();
    let result = copy_with_open(
        CopyRequest {
            source: source.clone(),
            data_dir: dir.path().into(),
            file_id: Uuid::new_v4(),
            store: Arc::new(LocalStore::in_memory().unwrap()),
            cancel: watch::channel(false).1,
        },
        |path| {
            let handle = fs::File::open(path)?;
            fs::rename(path, &retained)?;
            fs::write(path, b"replacement source")?;
            Ok(handle)
        },
    );
    assert!(matches!(
        result,
        Err(AppError::Intelligence(Code::UnsupportedDocument))
    ));
    assert_eq!(fs::read(retained).unwrap(), b"initial source");
    assert_eq!(fs::read(source).unwrap(), b"replacement source");
    assert!(!dir.path().join("knowledge").exists());
}

#[test]
fn documents_copy_complete_collision_preserves_preexisting_files_including_temp() {
    for extension in ["txt", "tmp"] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.txt");
        fs::write(&source, b"source remains").unwrap();
        let managed = dir.path().join("knowledge/documents");
        fs::create_dir_all(&managed).unwrap();
        let id = Uuid::new_v4();
        let existing = managed.join(format!("{id}.{extension}"));
        fs::write(&existing, b"foreign file must survive").unwrap();
        let local = Arc::new(LocalStore::in_memory().unwrap());
        let result = ManagedCopier.copy(CopyRequest {
            source: source.clone(),
            data_dir: dir.path().into(),
            file_id: id,
            store: local.clone(),
            cancel: watch::channel(false).1,
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(&existing).unwrap(),
            b"foreign file must survive",
            "failed create/publish cannot confer ownership of an existing filename"
        );
        assert_eq!(fs::read(source).unwrap(), b"source remains");
        assert_eq!(
            local
                .transaction(|tx| Ok(tx.query_row(
                    "SELECT count(*) FROM knowledge_managed_files",
                    [],
                    |r| r.get::<_, u32>(0)
                )?))
                .unwrap(),
            0
        );
    }
}

#[test]
fn documents_copy_intent_collision_preserves_an_unowned_durable_row() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("new.txt");
    fs::write(&source, b"new source").unwrap();
    let local = Arc::new(LocalStore::in_memory().unwrap());
    let id = Uuid::new_v4();
    let name = format!("{id}.pdf");
    local.transaction(|tx|{tx.execute("INSERT INTO knowledge_managed_files (file_id,relative_name,state,created_at_ms) VALUES (?1,?2,'copying',37)",rusqlite::params![id.to_string(),name])?;Ok(())}).unwrap();
    let result = ManagedCopier.copy(CopyRequest {
        source: source.clone(),
        data_dir: dir.path().into(),
        file_id: id,
        store: local.clone(),
        cancel: watch::channel(false).1,
    });
    assert!(matches!(result, Err(AppError::Intelligence(Code::Storage))));
    let row:Option<(String,String,i64)>=local.transaction(|tx|{use rusqlite::OptionalExtension;Ok(tx.query_row("SELECT relative_name,state,created_at_ms FROM knowledge_managed_files WHERE file_id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)}).unwrap();
    assert_eq!(
        row,
        Some((name, "copying".into(), 37)),
        "failed INSERT cannot confer ownership of an existing intent"
    );
    assert_eq!(fs::read(source).unwrap(), b"new source");
    assert!(
        !dir.path()
            .join(format!("knowledge/documents/{id}.txt"))
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn documents_copy_non_utf8_native_path_stays_lossless_but_public_attribution_is_rejected() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join(std::ffi::OsStr::from_bytes(b"native-\xff.txt"));
    fs::write(&path, b"preserved native bytes").unwrap();
    let encoded = crate::knowledge::protocol::NativePath::from_path(path.clone());
    let json = serde_json::to_vec(&encoded).unwrap();
    let decoded: crate::knowledge::protocol::NativePath = serde_json::from_slice(&json).unwrap();
    assert_eq!(decoded.into_path().unwrap(), path);
    let local = Arc::new(LocalStore::in_memory().unwrap());
    let result = ManagedCopier.copy(CopyRequest {
        source: path.clone(),
        data_dir: dir.path().into(),
        file_id: Uuid::new_v4(),
        store: local.clone(),
        cancel: watch::channel(false).1,
    });
    assert!(matches!(
        result,
        Err(AppError::Intelligence(Code::UnsupportedDocument))
    ));
    assert!(!dir.path().join("knowledge").exists());
    assert_eq!(fs::read(path).unwrap(), b"preserved native bytes");
}
