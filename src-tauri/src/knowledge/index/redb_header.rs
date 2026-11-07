//! Fixed-size evidence for this pinned writer, never an arbitrary redb decoder.
use super::{repair, seal::METADATA_CAP};
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StorageState {
    Live,
    Closed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeaderSnapshot {
    state: StorageState,
    bytes: u64,
    header: String,
}
impl HeaderSnapshot {
    pub(super) fn read(path: &Path, state: StorageState) -> AppResult<Self> {
        let mut file = File::open(path).map_err(|_| repair())?;
        let bytes = file.metadata().map_err(|_| repair())?.len();
        let mut raw = [0; 320];
        file.read_exact(&mut raw).map_err(|_| repair())?;
        let snapshot = Self {
            state,
            bytes,
            header: raw.iter().map(|b| format!("{b:02x}")).collect(),
        };
        snapshot.validate(bytes)?;
        Ok(snapshot)
    }
    pub(super) fn state(&self) -> StorageState {
        self.state
    }
    pub(super) fn validate(&self, bytes: u64) -> AppResult<()> {
        self.info(bytes).map(|_| ())
    }
    pub(super) fn validates_close(&self, after: &Self) -> AppResult<()> {
        let before = self.info(self.bytes)?;
        let closed = after.info(after.bytes)?;
        if self.state != StorageState::Live
            || after.state != StorageState::Closed
            || closed.transaction <= before.transaction
            || closed.user_root != before.user_root
        {
            return Err(repair());
        }
        Ok(())
    }
    fn info(&self, bytes: u64) -> AppResult<Info> {
        if bytes != self.bytes
            || !(320..=METADATA_CAP).contains(&bytes)
            || self.header.len() != 640
            || !self
                .header
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(repair());
        }
        let mut raw = [0u8; 320];
        for (i, byte) in raw.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&self.header[i * 2..i * 2 + 2], 16).map_err(|_| repair())?;
        }
        let u32_at =
            |at| u32::from_le_bytes(raw[at..at + 4].try_into().expect("fixed header offset"));
        if &raw[..9] != b"redb\x1a\x0a\xa9\x0d\x0a"
            || raw[9] & !7 != 0
            || (raw[9] & 2 != 0) != (self.state == StorageState::Live)
            || raw[10..12] != [0; 2]
            || u32_at(12) != 4096
            || u32_at(16) != 0
            || raw[32..64] != [0; 32]
        {
            return Err(repair());
        }
        let region_pages = u64::from(u32_at(20));
        let full = u64::from(u32_at(24));
        let trailing = u64::from(u32_at(28));
        if region_pages == 0
            || region_pages > 1 << 20
            || trailing > region_pages
            || full + u64::from(trailing > 0) == 0
            || full + u64::from(trailing > 0) > 1 << 20
        {
            return Err(repair());
        }
        let layout = full
            .checked_mul(region_pages)
            .and_then(|n| n.checked_add(trailing))
            .and_then(|n| n.checked_mul(4096))
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(repair)?;
        if layout != bytes {
            return Err(repair());
        }
        for offset in [64, 192] {
            let slot = &raw[offset..offset + 128];
            if slot[0] != 3
                || slot[1] > 1
                || slot[2] > 1
                || slot[3..8] != [0; 5]
                || slot[72..104] != [0; 32]
                || u128::from_le_bytes(slot[112..128].try_into().expect("fixed checksum"))
                    != xxhash_rust::xxh3::xxh3_128_with_seed(&slot[..112], 0)
            {
                return Err(repair());
            }
        }
        let primary = 64 + usize::from(raw[9] & 1) * 128;
        let slot = &raw[primary..primary + 128];
        let transaction = u64::from_le_bytes(slot[104..112].try_into().expect("fixed transaction"));
        if transaction == 0 || slot[1] != 1 || slot[2] != 1 {
            return Err(repair());
        }
        let mut user_root = [0; 33];
        user_root[0] = slot[1];
        user_root[1..].copy_from_slice(&slot[8..40]);
        Ok(Info {
            transaction,
            user_root,
        })
    }
}
struct Info {
    transaction: u64,
    user_root: [u8; 33],
}
