mod codec;
mod lifecycle;
mod owner;
mod preflight;
mod publication;
mod recovery;
mod redb_header;
mod repository;
mod seal;
pub(crate) use codec::{ChunkMetadata, location_digest};
pub(crate) use lifecycle::IndexOwner;
pub(crate) use lifecycle::WorkTicket;
pub(crate) use publication::{DocumentFence, PublicationPlan, ReadyPublication};
pub(crate) use recovery::RecoveryEvidence;
pub(crate) use seal::{Generation, Seal};

use crate::{error::AppError, intelligence::types::IntelligenceErrorCode};
fn repair() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::IndexRepairRequired)
}
#[cfg(test)]
mod tests;

pub(crate) fn recovery_descriptor(json: &str) -> crate::error::AppResult<Generation> {
    Ok(seal::Intent::decode(json)?.descriptor)
}
