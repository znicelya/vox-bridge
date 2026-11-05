use rubato::{
    Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

use crate::error::{AppError, AppResult};

use super::PcmSpec;

const INPUT_CHUNK: usize = 1024;

/// Stateful band-limited mono PCM16 conversion.
///
/// `process` is intended for an audio worker or queue boundary: it owns the
/// reusable resampler state and returns an owned batch. Device callbacks must
/// enqueue samples and must not call this allocation-bearing method directly.
pub struct Resampler {
    from: PcmSpec,
    to: PcmSpec,
    inner: SincFixedIn<f32>,
    pending: Vec<i16>,
    total_input: usize,
    emitted: usize,
}

impl Resampler {
    pub fn new(from: PcmSpec, to: PcmSpec) -> AppResult<Self> {
        from.validate()?;
        to.validate()?;
        let inner = SincFixedIn::new(
            to.sample_rate() as f64 / from.sample_rate() as f64,
            1.0,
            SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            },
            INPUT_CHUNK,
            1,
        )
        .map_err(|error| AppError::Internal(format!("failed to configure resampler: {error}")))?;
        Ok(Self {
            from,
            to,
            inner,
            pending: Vec::with_capacity(INPUT_CHUNK * 2),
            total_input: 0,
            emitted: 0,
        })
    }

    pub fn process(&mut self, input: &[i16]) -> AppResult<Vec<i16>> {
        self.pending.extend_from_slice(input);
        self.total_input += input.len();
        let mut output = Vec::new();
        while self.pending.len() >= INPUT_CHUNK {
            let block: Vec<f32> = self
                .pending
                .drain(..INPUT_CHUNK)
                .map(|sample| sample as f32 / 32768.0)
                .collect();
            let converted = self.inner.process(&[block], None).map_err(|error| {
                AppError::Internal(format!("failed to resample audio: {error}"))
            })?;
            append_channel(&mut output, converted.first())?;
        }
        self.emitted += output.len();
        Ok(output)
    }

    pub fn flush(&mut self) -> AppResult<Vec<i16>> {
        if self.total_input == 0 {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        if !self.pending.is_empty() {
            let partial: Vec<f32> = self
                .pending
                .iter()
                .map(|sample| *sample as f32 / 32768.0)
                .collect();
            let converted =
                self.inner
                    .process_partial(Some(&[partial]), None)
                    .map_err(|error| {
                        AppError::Internal(format!("failed to flush resampler: {error}"))
                    })?;
            append_channel(&mut output, converted.first())?;
            self.pending.clear();
        }
        let expected = expected_len(self.total_input, self.from, self.to);
        output.truncate(expected.saturating_sub(self.emitted));
        let remaining = expected.saturating_sub(self.emitted + output.len());
        if remaining > 0 {
            let fill = output
                .last()
                .copied()
                .or_else(|| self.pending.last().copied())
                .unwrap_or(0);
            output.resize(output.len() + remaining, fill);
        }
        self.emitted += output.len();
        Ok(output)
    }

    pub fn convert(input: &[i16], from: PcmSpec, to: PcmSpec) -> AppResult<Vec<i16>> {
        let mut resampler = Self::new(from, to)?;
        let mut output = resampler.process(input)?;
        output.extend(resampler.flush()?);
        output.truncate(expected_len(input.len(), from, to));
        Ok(output)
    }
}

fn append_channel(output: &mut Vec<i16>, channel: Option<&Vec<f32>>) -> AppResult<()> {
    let channel =
        channel.ok_or_else(|| AppError::Internal("resampler returned no channel".into()))?;
    output.extend(channel.iter().map(|sample| {
        (sample * 32768.0)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16
    }));
    Ok(())
}

fn expected_len(input_len: usize, from: PcmSpec, to: PcmSpec) -> usize {
    (((input_len as u64 * to.sample_rate() as u64) + (from.sample_rate() as u64 / 2))
        / from.sample_rate() as u64) as usize
}

#[cfg(test)]
#[path = "resample_tests.rs"]
mod tests;
