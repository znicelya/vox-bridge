use super::{MAX_CHUNKS, MAX_SCALARS, PARSER_ID, types::TextBlock};
use crate::{
    error::{AppError, AppResult},
    intelligence::types::IntelligenceErrorCode as Code,
};
use sha2::{Digest, Sha256};

/// Offsets count normalized Unicode scalars in the document, including the single
/// newline separating adjacent blocks. A chunk keeps its first paragraph's ID.
pub fn chunk_blocks(blocks: &[TextBlock]) -> AppResult<Vec<TextBlock>> {
    chunk_blocks_with_budget(blocks, None)
}

/// Bounds logical retained chunk data; this is not a process RSS limit.
pub(crate) fn chunk_blocks_bounded(blocks: &[TextBlock]) -> AppResult<Vec<TextBlock>> {
    chunk_blocks_with_budget(blocks, Some(32 * 1024 * 1024))
}

fn chunk_blocks_with_budget(
    blocks: &[TextBlock],
    budget: Option<usize>,
) -> AppResult<Vec<TextBlock>> {
    let mut groups: Vec<(TextBlock, Vec<(u32, u32)>)> = Vec::new();
    let mut total = 0usize;
    for block in blocks {
        let count = block.text.chars().count();
        total = total
            .checked_add(count)
            .ok_or(AppError::Intelligence(Code::ResourceLimit))?;
        if total > MAX_SCALARS
            || block
                .location
                .end_char
                .checked_sub(block.location.start_char)
                != Some(count as u32)
        {
            return Err(AppError::Intelligence(Code::ResourceLimit));
        }
        if block.text.is_empty() {
            continue;
        }
        if let Some((prior, paragraphs)) = groups.last_mut() {
            if prior.location.page == block.location.page
                && prior.location.heading == block.location.heading
                && prior.location.end_char.checked_add(1) == Some(block.location.start_char)
            {
                paragraphs.push((block.location.start_char, block.location.paragraph));
                prior.text.push('\n');
                prior.text.push_str(&block.text);
                prior.location.end_char = block.location.end_char;
                continue;
            }
        }
        groups.push((
            block.clone(),
            vec![(block.location.start_char, block.location.paragraph)],
        ));
    }
    let mut chunks = Vec::new();
    let mut derived_bytes = 0usize;
    for (group, paragraphs) in groups {
        let chars: Vec<char> = group.text.chars().collect();
        let mut start = 0usize;
        while start < chars.len() {
            if chunks.len() == MAX_CHUNKS {
                return Err(AppError::Intelligence(Code::ResourceLimit));
            }
            let ceiling = (start + 2400).min(chars.len());
            let floor = (start + 1600).min(chars.len());
            // Prefer an actual paragraph edge, then sentence punctuation; a short
            // final window is expected and retains the full 320-scalar overlap.
            let boundary = |test: fn(char) -> bool| (floor..ceiling).find(|i| test(chars[*i - 1]));
            let end = if floor == chars.len() {
                chars.len()
            } else {
                boundary(|c| c == '\n')
                    .or_else(|| boundary(|c| matches!(c, '。' | '！' | '？' | '.' | '!' | '?')))
                    .unwrap_or(ceiling)
            };
            if let Some(limit) = budget {
                let text_bytes = chars[start..end].iter().try_fold(0usize, |total, c| {
                    total
                        .checked_add(c.len_utf8())
                        .ok_or(AppError::Intelligence(Code::ResourceLimit))
                })?;
                let chunk_bytes = text_bytes
                    .checked_add(
                        group
                            .location
                            .heading
                            .as_ref()
                            .map_or(0, |heading| heading.len()),
                    )
                    .and_then(|bytes| bytes.checked_add(std::mem::size_of::<TextBlock>()))
                    .ok_or(AppError::Intelligence(Code::ResourceLimit))?;
                derived_bytes = derived_bytes
                    .checked_add(chunk_bytes)
                    .filter(|bytes| *bytes <= limit)
                    .ok_or(AppError::Intelligence(Code::ResourceLimit))?;
            }
            let mut location = group.location.clone();
            location.start_char += start as u32;
            let paragraph_index =
                paragraphs.partition_point(|(offset, _)| *offset <= location.start_char) - 1;
            location.paragraph = paragraphs[paragraph_index].1;
            location.end_char = group.location.start_char + end as u32;
            chunks.push(TextBlock {
                text: chars[start..end].iter().collect(),
                location,
            });
            if end == chars.len() {
                break;
            }
            start = end - 320;
        }
    }
    Ok(chunks)
}

pub fn chunk_hash(text: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"VoxBridge chunk\0");
    hash.update((PARSER_ID.len() as u64).to_le_bytes());
    hash.update(PARSER_ID.as_bytes());
    hash.update((text.len() as u64).to_le_bytes());
    hash.update(text.as_bytes());
    format!("{:x}", hash.finalize())
}
