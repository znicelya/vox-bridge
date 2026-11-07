use crate::{
    error::{AppError, AppResult},
    intelligence::types::IntelligenceErrorCode as Code,
    knowledge::types::TextLocation,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChunkMetadata {
    schema_version: u32,
    #[serde(deserialize_with = "canonical_uuid")]
    document_id: Uuid,
    document_version: u32,
    #[serde(deserialize_with = "canonical_uuid")]
    chunk_id: Uuid,
    content_hash: String,
    location: LocationReference,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocationReference {
    kind: String,
    codec: String,
    #[serde(deserialize_with = "required_nullable")]
    page: Option<u32>,
    paragraph: u32,
    start_char: u32,
    end_char: u32,
    sha256: String,
}
impl ChunkMetadata {
    pub(crate) fn new(
        document_id: Uuid,
        document_version: u32,
        chunk_id: Uuid,
        content_hash: String,
        location: &TextLocation,
    ) -> AppResult<Self> {
        let metadata = Self {
            schema_version: 2,
            document_id,
            document_version,
            chunk_id,
            content_hash,
            location: LocationReference {
                kind: "sqlite_chunk_location".into(),
                codec: "text_location_v1".into(),
                page: location.page,
                paragraph: location.paragraph,
                start_char: location.start_char,
                end_char: location.end_char,
                sha256: location_digest(location)?,
            },
        };
        metadata.validate()?;
        Ok(metadata)
    }
    pub(super) fn encode(&self) -> AppResult<String> {
        self.validate()?;
        let json = serde_json::to_string(self).map_err(|_| invalid())?;
        if json.len() > 16384 {
            return Err(AppError::Intelligence(Code::ResourceLimit));
        }
        Ok(json)
    }
    pub(super) fn decode(json: &str) -> AppResult<Self> {
        if json.len() > 16384 {
            return Err(invalid());
        }
        let metadata: Self = serde_json::from_str(json).map_err(|_| invalid())?;
        metadata.validate()?;
        Ok(metadata)
    }
    pub(crate) fn matches_location(&self, location: &TextLocation) -> AppResult<bool> {
        Ok(self.location.page == location.page
            && self.location.paragraph == location.paragraph
            && self.location.start_char == location.start_char
            && self.location.end_char == location.end_char
            && self.location.sha256 == location_digest(location)?)
    }
    fn validate(&self) -> AppResult<()> {
        if self.schema_version != 2
            || self.document_version == 0
            || !is_hash(&self.content_hash)
            || self.location.kind != "sqlite_chunk_location"
            || self.location.codec != "text_location_v1"
            || !is_hash(&self.location.sha256)
            || self.location.end_char < self.location.start_char
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn invalid() -> AppError {
    AppError::Intelligence(Code::IndexRepairRequired)
}
fn required_nullable<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    Option::deserialize(deserializer)
}
fn canonical_uuid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let id = Uuid::parse_str(&raw).map_err(|_| serde::de::Error::custom("invalid UUID"))?;
    if id.to_string() != raw {
        return Err(serde::de::Error::custom("noncanonical UUID"));
    }
    Ok(id)
}
fn location_digest(location: &TextLocation) -> AppResult<String> {
    struct HashWriter(Sha256);
    impl Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut hash = HashWriter(Sha256::new());
    serde_json::to_writer(&mut hash, location).map_err(|_| invalid())?;
    Ok(format!("{:x}", hash.0.finalize()))
}
