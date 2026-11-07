mod chunking;
pub(crate) mod embedding;
pub mod identity;
pub(crate) mod index;
pub mod parser;
mod protocol;
pub mod types;
pub(crate) use chunking::chunk_blocks_bounded;
pub use chunking::{chunk_blocks, chunk_hash};
pub(crate) mod parser_worker;

pub const PARSER_ID: &str = "voxbridge-parser-v1-normalize-v1";
pub const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SCALARS: usize = 2_000_000;
pub const MAX_PARAGRAPHS: usize = 50_000;
pub const MAX_PAGES: usize = 2_000;
pub const MAX_CHUNKS: usize = 10_000;

#[cfg(test)]
mod parser_tests;

pub(crate) mod copying;
#[cfg(test)]
mod document_tests;
pub mod documents;
#[cfg(test)]
mod index_tests;
pub(crate) mod service;
mod store;

mod reconcile;
mod search;

#[cfg(test)]
mod embedding_tests;
