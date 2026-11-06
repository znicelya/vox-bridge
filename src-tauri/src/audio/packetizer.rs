//! Bounded 16 kHz input packetization with explicit discontinuity handling.
use std::time::{Duration, SystemTime};

use super::{AudioFrame, PcmSpec};
use crate::{
    domain::audio::AudioSource,
    error::{AppError, AppResult},
};

pub const INPUT_PACKET_SAMPLES: usize = 1280;
pub const MAX_CAPTURE_FRAME_SAMPLES: usize = 16_000;
const INPUT_SPEC: PcmSpec = PcmSpec::pcm16_mono(16_000);

pub struct AudioPacketizer {
    source: AudioSource,
    next_sequence: u64,
    carry: Vec<i16>,
    carry_at: Option<SystemTime>,
    last_input_sequence: Option<u64>,
}

impl AudioPacketizer {
    pub fn new(source: AudioSource) -> Self {
        Self {
            source,
            next_sequence: 0,
            carry: Vec::new(),
            carry_at: None,
            last_input_sequence: None,
        }
    }

    pub fn push(&mut self, frame: AudioFrame) -> AppResult<Vec<AudioFrame>> {
        if frame.source() != self.source
            || frame.spec() != INPUT_SPEC
            || frame.samples().len() > MAX_CAPTURE_FRAME_SAMPLES
            || self
                .last_input_sequence
                .is_some_and(|last| frame.sequence() <= last)
        {
            return Err(invalid_packet());
        }
        let gap = self
            .last_input_sequence
            .is_some_and(|last| last.checked_add(1) != Some(frame.sequence()));
        let carry = if gap { &[][..] } else { &self.carry };
        let carry_len = carry.len();
        let sample_count = carry_len + frame.samples().len();
        let packet_count = sample_count / INPUT_PACKET_SAMPLES;
        let next_sequence = self
            .next_sequence
            .checked_add(packet_count as u64)
            .ok_or_else(invalid_packet)?;
        let time_at = |offset: usize| -> AppResult<SystemTime> {
            let (start, samples) = if offset < carry_len {
                (self.carry_at.ok_or_else(invalid_packet)?, offset)
            } else {
                (frame.captured_at(), offset - carry_len)
            };
            // One 16 kHz sample lasts exactly 62,500 nanoseconds.
            start
                .checked_add(Duration::from_nanos(samples as u64 * 62_500))
                .ok_or_else(invalid_packet)
        };
        // Input bounds precede allocation; build locally so any checked
        // timestamp/sequence failure leaves accepted carry untouched.
        let mut samples = Vec::with_capacity(sample_count);
        samples.extend_from_slice(carry);
        samples.extend_from_slice(frame.samples());
        let mut packets = Vec::with_capacity(packet_count);
        for (index, chunk) in samples.chunks_exact(INPUT_PACKET_SAMPLES).enumerate() {
            packets.push(AudioFrame::at(
                self.source,
                self.next_sequence + index as u64,
                time_at(index * INPUT_PACKET_SAMPLES)?,
                INPUT_SPEC,
                chunk.to_vec(),
            )?);
        }
        let tail_start = packet_count * INPUT_PACKET_SAMPLES;
        let carry_at = if tail_start < sample_count {
            Some(time_at(tail_start)?)
        } else {
            None
        };
        self.carry = samples[tail_start..].to_vec();
        self.carry_at = carry_at;
        self.last_input_sequence = Some(frame.sequence());
        self.next_sequence = next_sequence;
        Ok(packets)
    }

    pub fn flush(&mut self) -> AppResult<Option<AudioFrame>> {
        if self.carry.is_empty() {
            return Ok(None);
        }
        let next = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(invalid_packet)?;
        let frame = AudioFrame::at(
            self.source,
            self.next_sequence,
            self.carry_at.ok_or_else(invalid_packet)?,
            INPUT_SPEC,
            self.carry.clone(),
        )?;
        self.carry.clear();
        self.carry_at = None;
        self.next_sequence = next;
        Ok(Some(frame))
    }

    /// Starts a fresh input sequence without reusing an emitted packet number.
    pub fn reset(&mut self) {
        self.carry.clear();
        self.carry_at = None;
        self.last_input_sequence = None;
    }
}

fn invalid_packet() -> AppError {
    AppError::InvalidRequest("invalid or discontinuous input audio packet".into())
}
