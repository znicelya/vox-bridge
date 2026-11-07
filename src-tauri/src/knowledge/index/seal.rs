//! The only accepted writer format. Canonical JSON also rejects omitted nulls
//! and permissive UUID spellings accepted by general-purpose deserializers.
use super::{codec::is_hash, redb_header::HeaderSnapshot, repair};
use crate::{error::AppResult, knowledge::identity::EmbeddingIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) const RECORD_CAP: u64 = 200_000;
pub(super) const BATCH_CAP: u32 = 200;
pub(super) const METADATA_CAP: u64 = 17_179_869_184;
pub(super) const METADATA_WATERMARK: u64 = 12_884_901_888;
pub(super) const GRAPH_CAP: u64 = 536_870_912;
const PIN: &str = "1eec46feb2b7e6a6e286fa3361a20cd771dd4e04";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Generation {
    pub identity: EmbeddingIdentity,
    pub requested_dimension: Option<u32>,
    pub generation: Uuid,
}
impl Generation {
    pub(super) fn key(&self) -> AppResult<String> {
        if self
            .requested_dimension
            .is_some_and(|d| d != self.identity.dimensions)
        {
            return Err(repair());
        }
        self.identity.key().map_err(|_| repair())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    policy_version: u32,
    srvdb_revision: String,
    redb_revision: String,
    redb_header_version: u32,
    vector_header_version: u32,
    graph_envelope_version: u32,
    vector_encoding: String,
    index_mode: String,
    metadata_codec: String,
    metadata_schema_version: u32,
    stored_records: u64,
    metadata_value_bytes: u64,
    vectors_bytes: u64,
    metadata_bytes: u64,
    graph_bytes: u64,
}
impl Policy {
    fn current(dimension: u32) -> AppResult<Self> {
        Ok(Self {
            policy_version: 1,
            srvdb_revision: PIN.into(),
            redb_revision: "4.4.0".into(),
            redb_header_version: 3,
            vector_header_version: 4,
            graph_envelope_version: 1,
            vector_encoding: "f32".into(),
            index_mode: "Auto".into(),
            metadata_codec: "chunk_location_ref_v1".into(),
            metadata_schema_version: 2,
            stored_records: RECORD_CAP,
            metadata_value_bytes: 16384,
            vectors_bytes: vector_cap(dimension)?,
            metadata_bytes: METADATA_CAP,
            graph_bytes: GRAPH_CAP,
        })
    }
}
pub(super) fn vector_cap(dimension: u32) -> AppResult<u64> {
    if !(128..=4096).contains(&dimension) {
        return Err(repair());
    }
    let record = u64::from(dimension).checked_mul(4).ok_or_else(repair)?;
    let logical = RECORD_CAP
        .checked_mul(record)
        .and_then(|n| n.checked_add(24))
        .ok_or_else(repair)?;
    Ok(logical
        .checked_add(logical / 2)
        .ok_or_else(repair)?
        .max(logical.checked_add(1024 * record).ok_or_else(repair)?))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileSeal {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Seal {
    version: u32,
    kind: String,
    policy: Policy,
    pub identity: String,
    pub descriptor: Generation,
    pub operation_id: Uuid,
    pub stored_count: u64,
    pub metadata_header: HeaderSnapshot,
    pub files: Vec<FileSeal>,
}
impl Seal {
    pub(super) fn new(
        intent: &Intent,
        count: u64,
        files: Vec<FileSeal>,
        metadata_header: HeaderSnapshot,
    ) -> AppResult<Self> {
        let seal = Self {
            version: 1,
            kind: "srvdb_generation_seal".into(),
            policy: intent.policy.clone(),
            identity: intent.identity.clone(),
            descriptor: intent.descriptor.clone(),
            operation_id: intent.operation_id,
            stored_count: count,
            metadata_header,
            files,
        };
        seal.validate()?;
        Ok(seal)
    }
    pub(super) fn encode(&self) -> AppResult<String> {
        self.validate()?;
        encode(self, 65536)
    }
    pub(super) fn decode(json: &str) -> AppResult<Self> {
        let seal: Self = decode(json, 65536)?;
        seal.validate()?;
        Ok(seal)
    }
    fn validate(&self) -> AppResult<()> {
        if self.version != 1
            || self.kind != "srvdb_generation_seal"
            || self.identity != self.descriptor.key()?
            || self.policy != Policy::current(self.descriptor.identity.dimensions)?
            || self.stored_count > RECORD_CAP
            || !(2..=3).contains(&self.files.len())
        {
            return Err(repair());
        }
        self.metadata_header.validate(self.files[1].bytes)?;
        for (file, name) in self
            .files
            .iter()
            .zip(["vectors.bin", "metadata.db", "hnsw.graph"])
        {
            let cap = match name {
                "vectors.bin" => self.policy.vectors_bytes,
                "metadata.db" => METADATA_CAP,
                _ => GRAPH_CAP,
            };
            if file.name != name || file.bytes == 0 || file.bytes > cap || !is_hash(&file.sha256) {
                return Err(repair());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IntentKind {
    Create,
    Open,
    Close,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    version: u32,
    kind: String,
    policy: Policy,
    pub operation_id: Uuid,
    pub owner_epoch: Uuid,
    pub operation: IntentKind,
    pub identity: String,
    pub descriptor: Generation,
    pub prior_seal_sha256: Option<String>,
    pub expected_stored_count: u64,
}
impl Intent {
    pub(super) fn new(descriptor: Generation, epoch: Uuid, prior: Option<&str>) -> AppResult<Self> {
        let count = prior
            .map(Seal::decode)
            .transpose()?
            .map_or(0, |s| s.stored_count);
        let intent = Self {
            version: 1,
            kind: "srvdb_generation_intent".into(),
            policy: Policy::current(descriptor.identity.dimensions)?,
            operation_id: Uuid::new_v4(),
            owner_epoch: epoch,
            operation: if prior.is_some() {
                IntentKind::Open
            } else {
                IntentKind::Create
            },
            identity: descriptor.key()?,
            descriptor,
            prior_seal_sha256: prior.map(digest),
            expected_stored_count: count,
        };
        intent.validate()?;
        Ok(intent)
    }
    pub(super) fn close(descriptor: Generation, epoch: Uuid, prior: &str) -> AppResult<Self> {
        let mut intent = Self::new(descriptor, epoch, Some(prior))?;
        intent.operation = IntentKind::Close;
        Ok(intent)
    }
    pub(super) fn encode(&self) -> AppResult<String> {
        self.validate()?;
        encode(self, 16384)
    }
    pub(super) fn decode(json: &str) -> AppResult<Self> {
        let intent: Self = decode(json, 16384)?;
        intent.validate()?;
        Ok(intent)
    }
    fn validate(&self) -> AppResult<()> {
        if self.version != 1
            || self.kind != "srvdb_generation_intent"
            || self.identity != self.descriptor.key()?
            || self.policy != Policy::current(self.descriptor.identity.dimensions)?
            || self.expected_stored_count > RECORD_CAP
            || self.prior_seal_sha256.as_ref().is_some_and(|h| !is_hash(h))
            || ((self.operation == IntentKind::Create) != (self.prior_seal_sha256.is_none()))
            || (self.operation == IntentKind::Create && self.expected_stored_count != 0)
        {
            return Err(repair());
        }
        Ok(())
    }
}
pub(super) fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn encode(value: &impl Serialize, cap: usize) -> AppResult<String> {
    let json = serde_json::to_string(value).map_err(|_| repair())?;
    if json.len() > cap {
        return Err(repair());
    }
    Ok(json)
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(json: &str, cap: usize) -> AppResult<T> {
    if json.len() > cap {
        return Err(repair());
    }
    let value: T = serde_json::from_str(json).map_err(|_| repair())?;
    if encode(&value, cap)? != json {
        return Err(repair());
    }
    Ok(value)
}
