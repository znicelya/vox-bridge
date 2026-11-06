use super::{
    MAX_PAGES, MAX_PARAGRAPHS, MAX_SCALARS, PARSER_ID,
    types::{DocumentKind, TextBlock},
};
use crate::intelligence::types::IntelligenceErrorCode as Code;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) const READY: &[u8; 8] = b"VBPW\x01\0\0\0";
pub(crate) const MAX_REQUEST_BYTES: usize = 64 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(tag = "platform", content = "units", rename_all = "snake_case")]
pub(crate) enum NativePath {
    Unix(Vec<u8>),
    Windows(Vec<u16>),
}
impl NativePath {
    pub fn from_path(path: PathBuf) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            Self::Unix(path.into_os_string().into_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self::Windows(path.as_os_str().encode_wide().collect())
        }
    }
    pub fn into_path(self) -> Result<PathBuf, Code> {
        match self {
            #[cfg(unix)]
            Self::Unix(bytes) if !bytes.contains(&0) => {
                use std::os::unix::ffi::OsStringExt;
                Ok(std::ffi::OsString::from_vec(bytes).into())
            }
            #[cfg(windows)]
            Self::Windows(units) if !units.contains(&0) => {
                use std::os::windows::ffi::OsStringExt;
                Ok(std::ffi::OsString::from_wide(&units).into())
            }
            _ => Err(Code::ParseFailed),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub version: u32,
    pub parser_id: String,
    pub path: NativePath,
    pub kind: DocumentKind,
}
impl Request {
    pub fn new(path: PathBuf, kind: DocumentKind) -> Self {
        Self {
            version: 1,
            parser_id: PARSER_ID.into(),
            path: NativePath::from_path(path),
            kind,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Response {
    Parsed { blocks: Vec<TextBlock> },
    Failed { error: Code },
}

pub(crate) fn validate_blocks(blocks: &[TextBlock]) -> Result<(), Code> {
    if blocks.is_empty() {
        return Err(Code::UnsupportedDocument);
    }
    if blocks.len() > MAX_PARAGRAPHS {
        return Err(Code::ResourceLimit);
    }
    let mut total = 0usize;
    let mut prior = None;
    for block in blocks {
        let count = block.text.chars().count();
        total = total.checked_add(count).ok_or(Code::ResourceLimit)?;
        if total > MAX_SCALARS {
            return Err(Code::ResourceLimit);
        }
        if block
            .location
            .page
            .is_some_and(|page| page == 0 || page > MAX_PAGES as u32)
            || block.location.paragraph == 0
            || block.location.paragraph > MAX_PARAGRAPHS as u32
            || block
                .location
                .end_char
                .checked_sub(block.location.start_char)
                != Some(count as u32)
            || prior.is_some_and(|end| block.location.start_char < end)
        {
            return Err(Code::ParseFailed);
        }
        prior = Some(block.location.end_char);
    }
    Ok(())
}
