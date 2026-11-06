use super::{
    MAX_INPUT_BYTES, MAX_PAGES, MAX_PARAGRAPHS, MAX_SCALARS,
    types::{DocumentKind, TextBlock, TextLocation},
};
use crate::intelligence::types::IntelligenceErrorCode as Code;
use std::{
    io::{Cursor, Read},
    path::Path,
};
mod docx;

#[derive(Default)]
pub(super) struct Blocks {
    pub rows: Vec<TextBlock>,
    offset: u32,
    scalars: usize,
}
impl Blocks {
    pub fn push(
        &mut self,
        text: &str,
        page: Option<u32>,
        heading: Option<&str>,
    ) -> Result<(), Code> {
        let text = normalize(text)?;
        if text.is_empty() {
            return Ok(());
        }
        let count = text.chars().count();
        self.scalars = self.scalars.checked_add(count).ok_or(Code::ResourceLimit)?;
        if self.scalars > MAX_SCALARS || self.rows.len() >= MAX_PARAGRAPHS {
            return Err(Code::ResourceLimit);
        }
        let end = self
            .offset
            .checked_add(count as u32)
            .ok_or(Code::ResourceLimit)?;
        self.rows.push(TextBlock {
            text,
            location: TextLocation {
                page,
                paragraph: self.rows.len() as u32 + 1,
                heading: heading.map(str::to_owned),
                start_char: self.offset,
                end_char: end,
            },
        });
        self.offset = end.checked_add(1).ok_or(Code::ResourceLimit)?;
        Ok(())
    }
}
fn normalize(text: &str) -> Result<String, Code> {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut space = false;
    while let Some(mut c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            c = '\n';
        }
        if c == '\n' || c == '\t' {
            if space {
                output.pop();
            }
            output.push(c);
            space = false;
        } else if c.is_control() {
            return Err(Code::UnsupportedDocument);
        } else if c.is_whitespace() {
            if !space {
                output.push(' ');
                space = true;
            }
        } else {
            output.push(c);
            space = false;
        }
    }
    Ok(output.trim_matches([' ', '\n']).to_owned())
}

/// Called only inside the restricted worker (or directly by bounded fixture tests).
pub(crate) fn parse_file(path: &Path, kind: DocumentKind) -> Result<Vec<TextBlock>, Code> {
    if kind == DocumentKind::Unknown {
        return Err(Code::UnsupportedDocument);
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| Code::ParseFailed)?;
    if !metadata.is_file() {
        return Err(Code::UnsupportedDocument);
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(Code::ResourceLimit);
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| Code::ParseFailed)?;
    let opened = file.metadata().map_err(|_| Code::ParseFailed)?;
    if !opened.is_file() || opened.file_type().is_symlink() {
        return Err(Code::UnsupportedDocument);
    }
    if opened.len() > MAX_INPUT_BYTES {
        return Err(Code::ResourceLimit);
    }
    // Parse an owned bounded snapshot. A growing/replaced path cannot make ZIP
    // seeks or PDF reads escape the input byte cap after the initial metadata.
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Code::ParseFailed)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(Code::ResourceLimit);
    }
    let mut blocks = Blocks::default();
    match kind {
        DocumentKind::Pdf => {
            if !bytes.starts_with(b"%PDF-") {
                return Err(Code::UnsupportedDocument);
            }
            let document =
                lopdf::Document::load_from(Cursor::new(&bytes)).map_err(|_| Code::ParseFailed)?;
            if document.is_encrypted() {
                return Err(Code::UnsupportedDocument);
            }
            let pages = document.get_pages();
            if pages.len() > MAX_PAGES {
                return Err(Code::ResourceLimit);
            }
            for page in pages.keys() {
                let text = document
                    .extract_text(&[*page])
                    .map_err(|_| Code::ParseFailed)?;
                if text.trim().is_empty() {
                    return Err(Code::UnsupportedDocument);
                }
                for paragraph in text.lines() {
                    blocks.push(paragraph, Some(*page), None)?;
                }
            }
        }
        DocumentKind::Docx => {
            if !bytes.starts_with(b"PK\x03\x04") {
                return Err(Code::UnsupportedDocument);
            }
            docx::parse(Cursor::new(&bytes), &mut blocks)?;
        }
        DocumentKind::Text | DocumentKind::Markdown => {
            let text = std::str::from_utf8(&bytes).map_err(|_| Code::UnsupportedDocument)?;
            let text = text
                .strip_prefix('\u{feff}')
                .unwrap_or(text)
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let mut paragraph = String::new();
            let mut heading = None;
            for line in text.lines() {
                let md_heading = kind == DocumentKind::Markdown
                    && line.starts_with('#')
                    && line.trim_start_matches('#').starts_with(' ');
                if line.trim().is_empty() || md_heading {
                    blocks.push(&paragraph, None, heading.as_deref())?;
                    paragraph.clear();
                    if md_heading {
                        heading = Some(line.trim_start_matches('#').trim().to_owned());
                        blocks.push(line, None, heading.as_deref())?;
                    }
                } else {
                    if !paragraph.is_empty() {
                        paragraph.push('\n');
                    }
                    paragraph.push_str(line);
                }
            }
            blocks.push(&paragraph, None, heading.as_deref())?;
        }
        DocumentKind::Unknown => unreachable!(),
    }
    if blocks.rows.is_empty() {
        Err(Code::UnsupportedDocument)
    } else {
        Ok(blocks.rows)
    }
}

pub(crate) fn serve(
    mut input: impl Read,
    mut output: impl std::io::Write,
    output_cap: usize,
) -> i32 {
    use super::{
        PARSER_ID,
        protocol::{self, Request, Response},
    };
    if output
        .write_all(protocol::READY)
        .and_then(|_| output.flush())
        .is_err()
    {
        return 2;
    }
    let request = (|| -> Result<Request, ()> {
        let mut size = [0; 4];
        input.read_exact(&mut size).map_err(|_| ())?;
        let size = u32::from_le_bytes(size) as usize;
        if size > protocol::MAX_REQUEST_BYTES {
            return Err(());
        }
        let mut body = vec![0; size];
        input.read_exact(&mut body).map_err(|_| ())?;
        let mut trailing = [0; 1];
        if input.read(&mut trailing).map_err(|_| ())? != 0 {
            return Err(());
        }
        let request: Request = serde_json::from_slice(&body).map_err(|_| ())?;
        if request.version != 1 || request.parser_id != PARSER_ID {
            return Err(());
        }
        Ok(request)
    })();
    let Ok(request) = request else {
        return 2;
    };
    let Ok(path) = request.path.into_path() else {
        return 2;
    };
    let response = match parse_file(&path, request.kind) {
        Ok(blocks) => Response::Parsed { blocks },
        Err(error) => Response::Failed { error },
    };
    let bytes = match encode_bounded(&response, output_cap) {
        Ok(bytes) => bytes,
        Err(_) => match encode_bounded(
            &Response::Failed {
                error: Code::ResourceLimit,
            },
            output_cap,
        ) {
            Ok(bytes) => bytes,
            Err(_) => return 2,
        },
    };
    if output
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|_| output.write_all(&bytes))
        .and_then(|_| output.flush())
        .is_ok()
    {
        0
    } else {
        2
    }
}
fn encode_bounded(value: &impl serde::Serialize, limit: usize) -> Result<Vec<u8>, ()> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::from(std::io::ErrorKind::FileTooLarge));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| ())?;
    Ok(writer.bytes)
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use crate::knowledge::{
        MAX_OUTPUT_BYTES,
        protocol::{self, Request, Response},
    };
    fn request() -> Vec<u8> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/knowledge/fixtures/unicode.txt");
        let body = serde_json::to_vec(&Request::new(path, DocumentKind::Text)).unwrap();
        let mut bytes = (body.len() as u32).to_le_bytes().to_vec();
        bytes.extend(body);
        bytes
    }
    #[test]
    fn parser_worker_protocol_emits_readiness_then_one_result_and_preserves_native_path() {
        let mut output = Vec::new();
        assert_eq!(
            serve(request().as_slice(), &mut output, MAX_OUTPUT_BYTES),
            0
        );
        assert_eq!(&output[..8], protocol::READY);
        let length = u32::from_le_bytes(output[8..12].try_into().unwrap()) as usize;
        assert_eq!(length + 12, output.len());
        match serde_json::from_slice::<Response>(&output[12..]).unwrap() {
            Response::Parsed { blocks } => assert!(blocks[0].text.contains("24V")),
            _ => panic!("expected real text"),
        }
    }
    #[test]
    fn parser_worker_protocol_rejects_trailing_truncated_oversize_and_wrong_version_requests() {
        let mut extra = request();
        extra.push(0);
        let mut wrong = request();
        let mut body: serde_json::Value = serde_json::from_slice(&wrong[4..]).unwrap();
        body["version"] = 2.into();
        let body = serde_json::to_vec(&body).unwrap();
        wrong = (body.len() as u32).to_le_bytes().to_vec();
        wrong.extend(body);
        for input in [
            extra,
            request()[..7].to_vec(),
            (protocol::MAX_REQUEST_BYTES as u32 + 1)
                .to_le_bytes()
                .to_vec(),
            wrong,
        ] {
            let mut output = Vec::new();
            assert_ne!(serve(input.as_slice(), &mut output, MAX_OUTPUT_BYTES), 0);
            assert_eq!(
                output,
                protocol::READY,
                "invalid protocol writes no success frame"
            );
        }
    }
    #[test]
    fn parser_worker_serialization_cap_is_enforced_before_writing_a_success_frame() {
        let mut output = Vec::new();
        assert_eq!(serve(request().as_slice(), &mut output, 80), 0);
        assert!(output.len() <= 92);
        match serde_json::from_slice::<Response>(&output[12..]).unwrap() {
            Response::Failed { error } => assert_eq!(error, Code::ResourceLimit),
            _ => panic!("oversize result must not be success"),
        }
    }
}
