use super::{types::*, *};
use crate::intelligence::types::IntelligenceErrorCode as Code;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/knowledge/fixtures")
        .join(name)
}
fn block(text: &str, page: Option<u32>, heading: Option<&str>, start: u32) -> TextBlock {
    TextBlock {
        text: text.into(),
        location: TextLocation {
            page,
            paragraph: 1,
            heading: heading.map(str::to_owned),
            start_char: start,
            end_char: start + text.chars().count() as u32,
        },
    }
}

#[test]
fn parser_chunks_cover_every_unicode_scalar_with_exact_overlap_and_page_boundaries() {
    let text = "泵的额定电压为24V。🦀".repeat(510);
    let input = vec![
        block(&text, Some(2), Some("电气参数"), 31),
        block("next page", Some(3), None, 7000),
    ];
    let chunks = chunk_blocks(&input).unwrap();
    let page: Vec<_> = chunks
        .iter()
        .filter(|c| c.location.page == Some(2))
        .collect();
    assert!(page.len() > 1);
    let chars: Vec<_> = text.chars().collect();
    let mut covered = 31;
    for (i, c) in page.iter().enumerate() {
        assert!(c.text.chars().count() <= 2400);
        assert_eq!(c.location.heading.as_deref(), Some("电气参数"));
        let start = (c.location.start_char - 31) as usize;
        let end = (c.location.end_char - 31) as usize;
        assert_eq!(c.text, chars[start..end].iter().collect::<String>());
        if i > 0 {
            assert_eq!(c.location.start_char, covered - 320);
        }
        covered = c.location.end_char;
    }
    assert_eq!(covered, 31 + chars.len() as u32);
    assert_eq!(chunks.last().unwrap().location.page, Some(3));
    assert_eq!(chunk_hash(&text).len(), 64);
    assert_ne!(chunk_hash(&text), chunk_hash("different"));
}

#[test]
fn parser_text_normalizes_without_losing_paragraphs_tabs_or_numeric_punctuation() {
    let blocks = parser_worker::parse_file(&fixture("unicode.txt"), DocumentKind::Text).unwrap();
    assert_eq!(
        blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
        ["泵 24V / 1.25A", "English\tvalue: 3,200.5"]
    );
    assert_eq!(blocks[0].location.start_char, 0);
    assert_eq!(
        blocks[1].location.start_char,
        blocks[0].location.end_char + 1
    );
    for b in &blocks {
        assert_eq!(
            b.location.end_char - b.location.start_char,
            b.text.chars().count() as u32
        );
    }
    let md = parser_worker::parse_file(&fixture("manual.md"), DocumentKind::Markdown).unwrap();
    assert!(
        md.iter()
            .any(|b| b.location.heading.as_deref() == Some("电气参数") && b.text.contains("24V"))
    );
}

#[test]
fn parser_pdf_retains_two_real_page_markers_and_docx_keeps_headings_cells_and_entities() {
    let pdf = parser_worker::parse_file(&fixture("two-pages.pdf"), DocumentKind::Pdf).unwrap();
    assert!(
        pdf.iter()
            .any(|b| b.location.page == Some(1) && b.text.contains("PAGE ONE"))
    );
    assert!(
        pdf.iter()
            .any(|b| b.location.page == Some(2) && b.text.contains("PAGE TWO"))
    );
    let docx = parser_worker::parse_file(&fixture("structured.docx"), DocumentKind::Docx).unwrap();
    assert!(
        docx.iter()
            .any(|b| b.location.heading.as_deref() == Some("Electrical")
                && b.text == "Pump & valve\t24V\nRated < 40A")
    );
    assert!(docx.iter().any(|b| b.text == "Cell A"));
    assert!(docx.iter().any(|b| b.text == "Cell B"));
}

#[test]
fn parser_rejects_unknown_kind_wrong_signature_empty_invalid_utf8_scanned_and_corrupt_documents() {
    for (name, kind, code) in [
        (
            "unicode.txt",
            DocumentKind::Unknown,
            Code::UnsupportedDocument,
        ),
        ("unicode.txt", DocumentKind::Pdf, Code::UnsupportedDocument),
        ("empty.txt", DocumentKind::Text, Code::UnsupportedDocument),
        ("invalid.txt", DocumentKind::Text, Code::UnsupportedDocument),
        ("scanned.pdf", DocumentKind::Pdf, Code::UnsupportedDocument),
        ("corrupt.docx", DocumentKind::Docx, Code::ParseFailed),
    ] {
        assert_eq!(
            parser_worker::parse_file(&fixture(name), kind),
            Err(code),
            "{name}"
        );
    }
}

fn parse_bytes(bytes: &[u8], kind: DocumentKind) -> Result<Vec<TextBlock>, Code> {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture");
    std::fs::write(&path, bytes).unwrap();
    parser_worker::parse_file(&path, kind)
}
fn zip_xml(document: &[u8], styles: Option<&[u8]>) -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(document).unwrap();
    if let Some(styles) = styles {
        zip.start_file("word/styles.xml", options).unwrap();
        zip.write_all(styles).unwrap();
    }
    zip.start_file("../../not-extracted", options).unwrap();
    zip.write_all(b"never opened").unwrap();
    zip.finish().unwrap().into_inner()
}
fn xml(body: &str) -> String {
    format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>"
    )
}
#[test]
fn parser_docx_namespaces_references_empty_nodes_and_optional_styles_are_explicit() {
    let body = xml(
        "<w:p><w:r><w:t>A&amp;B&#x4E2D;&quot;&apos;&lt;&gt;</w:t><w:tab/><w:t>C</w:t><w:br/><w:t>D</w:t></w:r></w:p>",
    );
    let blocks = parse_bytes(&zip_xml(body.as_bytes(), None), DocumentKind::Docx).unwrap();
    assert_eq!(blocks[0].text, "A&B中\"'<>\tC\nD");
    let strict = body
        .replace(
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        )
        .replace("w:", "s:")
        .replace("xmlns:w", "xmlns:s");
    assert_eq!(
        parse_bytes(&zip_xml(strict.as_bytes(), None), DocumentKind::Docx).unwrap()[0].text,
        blocks[0].text
    );
    let foreign = body.replace(
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        "urn:untrusted",
    );
    assert_eq!(
        parse_bytes(&zip_xml(foreign.as_bytes(), None), DocumentKind::Docx),
        Err(Code::UnsupportedDocument)
    );
}
#[test]
fn parser_docx_rejects_dtd_unknown_entities_bad_structure_unsupported_encoding_and_styles() {
    for body in [
        "<!DOCTYPE w:document [<!ENTITY leak SYSTEM 'file:///private'>]><w:document/>",
        "<w:document><w:p></w:document>",
        "<w:document>",
        "<!ENTITY leak 'bad'><w:document/>",
    ] {
        assert_eq!(
            parse_bytes(&zip_xml(body.as_bytes(), None), DocumentKind::Docx),
            Err(Code::ParseFailed)
        );
    }
    let doc = xml("<w:p><w:r><w:t>&madeup;</w:t></w:r></w:p>");
    assert_eq!(
        parse_bytes(&zip_xml(doc.as_bytes(), None), DocumentKind::Docx),
        Err(Code::ParseFailed)
    );
    let doc = xml("<w:p><w:r><w:t>good</w:t></w:r></w:p>");
    let unsupported = format!("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>{doc}");
    assert_eq!(
        parse_bytes(&zip_xml(unsupported.as_bytes(), None), DocumentKind::Docx),
        Err(Code::UnsupportedDocument)
    );
    assert_eq!(
        parse_bytes(
            &zip_xml(doc.as_bytes(), Some(b"<w:styles>")),
            DocumentKind::Docx
        ),
        Err(Code::ParseFailed)
    );
    let deep = xml(&format!(
        "{}text{}",
        "<w:ignored>".repeat(65),
        "</w:ignored>".repeat(65)
    ));
    assert_eq!(
        parse_bytes(&zip_xml(deep.as_bytes(), None), DocumentKind::Docx),
        Err(Code::ResourceLimit)
    );
}
#[test]
fn parser_caps_inflated_xml_input_scalars_and_paragraphs_before_returning_blocks() {
    let oversized = vec![b'x'; MAX_OUTPUT_BYTES + 1];
    assert_eq!(
        parse_bytes(&zip_xml(&oversized, None), DocumentKind::Docx),
        Err(Code::ResourceLimit)
    );
    assert_eq!(
        parse_bytes(&vec![b'x'; MAX_SCALARS + 1], DocumentKind::Text),
        Err(Code::ResourceLimit)
    );
    assert_eq!(
        parse_bytes(
            "x\n\n".repeat(MAX_PARAGRAPHS + 1).as_bytes(),
            DocumentKind::Text
        ),
        Err(Code::ResourceLimit)
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.txt");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_INPUT_BYTES + 1)
        .unwrap();
    assert_eq!(
        parser_worker::parse_file(&path, DocumentKind::Text),
        Err(Code::ResourceLimit)
    );
}
#[test]
fn parser_chunks_combine_only_contiguous_same_page_heading_and_reject_inconsistent_offsets() {
    let a = block("first", Some(1), Some("Title"), 0);
    let mut b = block("second", Some(1), Some("Title"), 6);
    b.location.paragraph = 2;
    let c = block("other", Some(1), Some("Other"), 13);
    let chunks = chunk_blocks(&[a.clone(), b, c]).unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].text, "first\nsecond");
    assert_eq!(chunks[0].location.start_char, 0);
    assert_eq!(chunks[0].location.end_char, 12);
    let mut invalid = a;
    invalid.location.end_char += 1;
    assert!(chunk_blocks(&[invalid]).is_err());
}

#[test]
fn parser_docx_caps_empty_paragraphs_and_combined_styles_document_inflation() {
    let empty = xml(&format!(
        "{}<w:p><w:r><w:t>end</w:t></w:r></w:p>",
        "<w:p/>".repeat(MAX_PARAGRAPHS)
    ));
    assert_eq!(
        parse_bytes(&zip_xml(empty.as_bytes(), None), DocumentKind::Docx),
        Err(Code::ResourceLimit)
    );
    let styles = format!(
        "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><!--{}--></w:styles>",
        "s".repeat(MAX_OUTPUT_BYTES / 2)
    );
    let doc = xml(&format!(
        "<!--{}--><w:p><w:r><w:t>end</w:t></w:r></w:p>",
        "d".repeat(MAX_OUTPUT_BYTES / 2)
    ));
    assert_eq!(
        parse_bytes(
            &zip_xml(doc.as_bytes(), Some(styles.as_bytes())),
            DocumentKind::Docx
        ),
        Err(Code::ResourceLimit)
    );
}
#[test]
fn parser_docx_rejects_malformed_and_duplicate_xml_declarations() {
    let doc = xml("<w:p><w:r><w:t>good</w:t></w:r></w:p>");
    for prefix in [
        "<!-- malformed -- comment -->",
        "<?xml encoding=\"UTF-8\"?>",
        "<?xml version=\"1.0\"?><?xml version=\"1.0\"?>",
    ] {
        assert_eq!(
            parse_bytes(
                &zip_xml(format!("{prefix}{doc}").as_bytes(), None),
                DocumentKind::Docx
            ),
            Err(Code::ParseFailed)
        );
    }
}

#[test]
fn parser_pdf_rejects_encryption_and_page_count_above_limit() {
    let mut document = lopdf::Document::load(fixture("two-pages.pdf")).unwrap();
    document.trailer.set(
        "ID",
        lopdf::Object::Array(vec![
            lopdf::Object::string_literal("synthetic-parser-fixture"),
            lopdf::Object::string_literal("synthetic-parser-fixture"),
        ]),
    );
    let encryption = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V1 {
        document: &document,
        owner_password: "fixture-owner",
        user_password: "fixture-user",
        permissions: lopdf::Permissions::all(),
    })
    .unwrap();
    document.encrypt(&encryption).unwrap();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    assert_eq!(
        parse_bytes(&bytes, DocumentKind::Pdf),
        Err(Code::UnsupportedDocument)
    );
    let mut document = lopdf::Document::load(fixture("two-pages.pdf")).unwrap();
    let first = *document.get_pages().values().next().unwrap();
    let page = document.get_object(first).unwrap().clone();
    let mut kids = Vec::new();
    for _ in 0..=MAX_PAGES {
        kids.push(lopdf::Object::Reference(document.add_object(page.clone())));
    }
    let pages = document
        .get_object_mut((2, 0))
        .unwrap()
        .as_dict_mut()
        .unwrap();
    pages.set("Kids", kids);
    pages.set("Count", (MAX_PAGES + 1) as i64);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    assert_eq!(
        parse_bytes(&bytes, DocumentKind::Pdf),
        Err(Code::ResourceLimit)
    );
}
#[test]
fn parser_chunk_cap_and_multiple_paragraph_reconstruction_are_independent() {
    let mut blocks = Vec::new();
    let mut source = String::new();
    for i in 0..30 {
        let text = format!("段落 {i}: ") + &"value\t24V 3.5A。".repeat(30);
        let start = source.chars().count() as u32;
        source.push_str(&text);
        source.push('\n');
        let mut next = block(&text, Some(1), Some("Heading"), start);
        next.location.paragraph = i + 1;
        blocks.push(next);
    }
    source.pop();
    let chars: Vec<_> = source.chars().collect();
    let chunks = chunk_blocks(&blocks).unwrap();
    let mut covered = 0;
    for chunk in chunks {
        let first_paragraph = blocks
            .iter()
            .rev()
            .find(|b| b.location.start_char <= chunk.location.start_char)
            .unwrap()
            .location
            .paragraph;
        assert_eq!(chunk.location.paragraph, first_paragraph);
        assert_eq!(
            chunk.text,
            chars[chunk.location.start_char as usize..chunk.location.end_char as usize]
                .iter()
                .collect::<String>()
        );
        if covered != 0 {
            assert_eq!(chunk.location.start_char, covered - 320);
        }
        covered = chunk.location.end_char;
    }
    assert_eq!(covered, chars.len() as u32);
    let distinct: Vec<_> = (0..=MAX_CHUNKS)
        .map(|i| block("x", None, Some(&format!("H{i}")), i as u32 * 2))
        .collect();
    assert!(matches!(
        chunk_blocks(&distinct),
        Err(crate::error::AppError::Intelligence(Code::ResourceLimit))
    ));
}
