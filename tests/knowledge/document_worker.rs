//! Offline integration uses the real application binary, never current_exe().
use tokio::sync::watch;
use vox_bridge_lib::knowledge::{
    parser::{DocumentParser, ProcessDocumentParser},
    types::DocumentKind,
};
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/knowledge/fixtures")
        .join(name)
}
#[tokio::test]
async fn real_limited_worker_parses_text_pdf_docx_without_desktop_initialization() {
    let parser = ProcessDocumentParser::with_executable(env!("CARGO_BIN_EXE_vox-bridge").into());
    let (_cancel, receiver) = watch::channel(false);
    for (name, kind) in [
        ("unicode.txt", DocumentKind::Text),
        ("two-pages.pdf", DocumentKind::Pdf),
        ("structured.docx", DocumentKind::Docx),
    ] {
        let blocks = parser
            .parse(fixture(name), kind, receiver.clone())
            .await
            .unwrap();
        assert!(!blocks.is_empty());
        assert!(
            blocks
                .iter()
                .any(|b| b.text.contains("24V") || b.text.contains("PAGE"))
        );
    }
}

mod document_service;
#[cfg(unix)]
mod worker_limits;
