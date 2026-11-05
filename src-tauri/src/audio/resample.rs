use crate::error::AppResult;

use super::PcmSpec;

/// Streaming mono PCM16 conversion that carries interpolation state across chunks.
pub struct Resampler {
    from: PcmSpec,
    to: PcmSpec,
    source_position: f64,
    consumed: usize,
    total_input: usize,
    emitted: usize,
    pending: Vec<i16>,
}

impl Resampler {
    pub fn new(from: PcmSpec, to: PcmSpec) -> AppResult<Self> {
        from.validate()?;
        to.validate()?;
        Ok(Self {
            from,
            to,
            source_position: 0.0,
            consumed: 0,
            total_input: 0,
            emitted: 0,
            pending: Vec::new(),
        })
    }

    pub fn process(&mut self, input: &[i16]) -> AppResult<Vec<i16>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        self.pending.extend_from_slice(input);
        self.total_input += input.len();
        let available_end = self.consumed + self.pending.len();
        let ratio = self.from.sample_rate() as f64 / self.to.sample_rate() as f64;
        let mut output = Vec::new();

        while self.source_position.floor() as usize + 1 < available_end {
            let absolute_index = self.source_position.floor() as usize;
            let local_index = absolute_index.saturating_sub(self.consumed);
            let fraction = self.source_position - absolute_index as f64;
            let left = self.pending[local_index] as f64;
            let right = self.pending[local_index + 1] as f64;
            output.push(to_i16(left + (right - left) * fraction));
            self.source_position += ratio;
        }

        let keep_from = self.source_position.floor() as usize;
        let discard = keep_from.saturating_sub(self.consumed);
        if discard > 0 {
            self.pending.drain(..discard.min(self.pending.len()));
            self.consumed += discard;
        }
        self.emitted += output.len();
        Ok(output)
    }

    pub fn flush(&mut self) -> AppResult<Vec<i16>> {
        if self.total_input == 0 {
            return Ok(Vec::new());
        }
        let ratio = self.from.sample_rate() as f64 / self.to.sample_rate() as f64;
        let mut output = Vec::new();
        let expected = ((self.total_input as u64 * self.to.sample_rate() as u64)
            + (self.from.sample_rate() as u64 / 2))
            / self.from.sample_rate() as u64;
        while self.emitted + output.len() < expected as usize {
            let absolute_index = self.source_position.floor() as usize;
            let local_index = absolute_index.saturating_sub(self.consumed);
            let left = self.pending[local_index] as f64;
            let right = self
                .pending
                .get(local_index + 1)
                .copied()
                .unwrap_or(left as i16) as f64;
            let fraction = self.source_position - absolute_index as f64;
            output.push(to_i16(left + (right - left) * fraction));
            self.source_position += ratio;
        }
        self.pending.clear();
        self.consumed = self.total_input;
        self.emitted += output.len();
        Ok(output)
    }

    pub fn convert(input: &[i16], from: PcmSpec, to: PcmSpec) -> AppResult<Vec<i16>> {
        let mut resampler = Self::new(from, to)?;
        let mut output = resampler.process(input)?;
        output.extend(resampler.flush()?);
        let expected = ((input.len() as u64 * to.sample_rate() as u64)
            + (from.sample_rate() as u64 / 2))
            / from.sample_rate() as u64;
        output.truncate(expected as usize);
        Ok(output)
    }
}

fn to_i16(sample: f64) -> i16 {
    sample.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
}

#[cfg(test)]
#[path = "resample_tests.rs"]
mod tests;
