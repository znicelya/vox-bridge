use super::super::preflight;
use std::path::Path;

fn header(count: u64) -> [u8; 24] {
    let mut header = [0; 24];
    header[..4].copy_from_slice(&0x53764442u32.to_le_bytes());
    header[4..6].copy_from_slice(&4u16.to_le_bytes());
    header[6..8].copy_from_slice(&128u16.to_le_bytes());
    header[8..16].copy_from_slice(&count.to_le_bytes());
    header
}
fn specimen(path: &Path) {
    std::fs::write(path.join("vectors.bin"), header(0)).unwrap();
    std::fs::write(
        path.join("metadata.db"),
        b"bounded synthetic preflight bytes; never passed to SrvDB",
    )
    .unwrap();
}
#[test]
fn index_preflight_accepts_non_record_aligned_slack_and_rejects_dangerous_headers() {
    let dir = tempfile::tempdir().unwrap();
    specimen(dir.path());
    let mut slack = header(0).to_vec();
    slack.extend_from_slice(&[0; 13]);
    std::fs::write(dir.path().join("vectors.bin"), &slack).unwrap();
    assert!(
        preflight::inspect(dir.path(), 128, 0).is_ok(),
        "allocation slack need not be record aligned"
    );
    for (offset, byte) in [(0, 0), (4, 3), (6, 127), (16, 1), (17, 1), (18, 1), (23, 1)] {
        let mut invalid = header(0);
        invalid[offset] = byte;
        std::fs::write(dir.path().join("vectors.bin"), invalid).unwrap();
        assert!(preflight::inspect(dir.path(), 128, 0).is_err());
        assert_eq!(
            std::fs::read(dir.path().join("vectors.bin")).unwrap(),
            invalid
        );
    }
    for (stored, expected) in [(1, 0), (1, 1), (200001, 200001), (u64::MAX, u64::MAX)] {
        std::fs::write(dir.path().join("vectors.bin"), header(stored)).unwrap();
        assert!(preflight::inspect(dir.path(), 128, expected).is_err());
    }
}
#[test]
fn index_preflight_preserves_unknown_files_temps_and_invalid_present_graph() {
    let dir = tempfile::tempdir().unwrap();
    specimen(dir.path());
    for name in [
        "ivf.index",
        "foreign",
        ".metadata.db.tmp",
        ".vectors.bin.tmp",
        ".hnsw.graph.tmp",
    ] {
        std::fs::write(dir.path().join(name), b"preserve").unwrap();
        assert!(preflight::inspect(dir.path(), 128, 0).is_err());
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"preserve");
        std::fs::remove_file(dir.path().join(name)).unwrap();
    }
    std::fs::write(dir.path().join("hnsw.graph"), b"arbitrary postcard data").unwrap();
    assert!(preflight::inspect(dir.path(), 128, 0).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("hnsw.graph")).unwrap(),
        b"arbitrary postcard data"
    );
}
#[cfg(unix)]
#[test]
fn index_preflight_rejects_symlink_before_reading_target() {
    let dir = tempfile::tempdir().unwrap();
    specimen(dir.path());
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), b"preserve").unwrap();
    std::fs::remove_file(dir.path().join("metadata.db")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("metadata.db")).unwrap();
    assert!(preflight::inspect(dir.path(), 128, 0).is_err());
    assert_eq!(std::fs::read(outside.path()).unwrap(), b"preserve");
}
