use crate::error::AppResult;
use rubato::{
    Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

use super::PcmSpec;

pub struct Resampler;

impl Resampler {
    pub fn convert(input: &[i16], from: PcmSpec, to: PcmSpec) -> AppResult<Vec<i16>> {
        from.validate()?;
        to.validate()?;
        if input.is_empty() {
            return Ok(Vec::new());
        }
        if from == to {
            return Ok(input.to_vec());
        }

        let output_len = ((input.len() as u64 * to.sample_rate as u64)
            + (from.sample_rate as u64 / 2))
            / from.sample_rate as u64;
        let output_len = output_len.max(1) as usize;
        let ratio = to.sample_rate as f64 / from.sample_rate as f64;
        let mut resampler = SincFixedIn::<f32>::new(
            ratio,
            1.0,
            SincInterpolationParameters {
                sinc_len: 64,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            },
            input.len(),
            1,
        )
        .map_err(|error| {
            crate::error::AppError::Internal(format!("failed to configure resampler: {error}"))
        })?;
        let floating_input: Vec<f32> = input
            .iter()
            .map(|sample| *sample as f32 / 32768.0)
            .collect();
        let converted = resampler
            .process_partial(Some(&[floating_input]), None)
            .map_err(|error| {
                crate::error::AppError::Internal(format!("failed to resample audio: {error}"))
            })?;
        let channel = converted.first().ok_or_else(|| {
            crate::error::AppError::Internal("resampler returned no audio channel".to_string())
        })?;
        let mut output = channel
            .iter()
            .take(output_len)
            .map(|sample| {
                (sample * 32768.0)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect::<Vec<_>>();
        output.resize(output_len, 0);
        Ok(output)
    }
}

#[cfg(test)]
#[path = "resample_tests.rs"]
mod tests;
