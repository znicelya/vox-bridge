// The frozen 5B owner ports are exercised by tests now and consumed by 5C.
#![cfg_attr(not(test), allow(dead_code))]

mod codec;
mod lifecycle;
mod owner;
mod preflight;
mod redb_header;
mod repository;
mod seal;
#[allow(unused_imports)]
pub(crate) use codec::ChunkMetadata;
pub(crate) use lifecycle::IndexOwner;

use crate::{error::AppError, intelligence::types::IntelligenceErrorCode};
fn repair() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::IndexRepairRequired)
}
#[cfg(test)]
mod tests;
