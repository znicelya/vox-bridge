use std::time::Duration;

use crate::{
    audio::{AudioFrame, PcmSpec, router::Direction as RouterDirection},
    domain::{
        audio::PLAYBACK_SAMPLE_RATE_HZ,
        session::{SessionDirection, SessionMode},
        transcript::RuntimeFailureCode,
    },
    error::{AppError, AppResult},
    providers::ProviderError,
};

pub(crate) const MAX_PROVIDER_OUTPUT_SAMPLES: usize = 48_000;
pub(crate) const PLAYBACK_FRAME_SAMPLES: usize = 1_920;

pub(crate) fn router_direction(
    mode: SessionMode,
    direction: SessionDirection,
) -> AppResult<RouterDirection> {
    match (mode, direction) {
        (SessionMode::Live, SessionDirection::SourceToTarget)
        | (SessionMode::Live, SessionDirection::TargetToSource) => Ok(RouterDirection::Live),
        (SessionMode::Call, SessionDirection::SourceToTarget) => Ok(RouterDirection::AtoB),
        (SessionMode::Call, SessionDirection::TargetToSource) => Ok(RouterDirection::BtoA),
        (_, SessionDirection::Bidirectional) => Err(AppError::InvalidRequest(
            "runtime directions must be unidirectional".into(),
        )),
    }
}

pub(crate) fn provider_failure(error: &ProviderError) -> RuntimeFailureCode {
    match error {
        ProviderError::Timeout => RuntimeFailureCode::StartupTimeout,
        ProviderError::UnsupportedAudioFormat
        | ProviderError::InvalidAudioFrame
        | ProviderError::InvalidDirection
        | ProviderError::InvalidConfiguration
        | ProviderError::Protocol => RuntimeFailureCode::InvalidEvent,
        ProviderError::Cancelled => RuntimeFailureCode::Interrupted,
        _ => RuntimeFailureCode::Provider,
    }
}

pub(crate) fn split_provider_audio(
    frame: &AudioFrame,
    next_sequence: &mut u64,
) -> AppResult<Vec<AudioFrame>> {
    if frame.spec() != PcmSpec::pcm16_mono(PLAYBACK_SAMPLE_RATE_HZ)
        || frame.samples().len() > MAX_PROVIDER_OUTPUT_SAMPLES
    {
        return Err(AppError::InvalidRequest(
            "invalid provider playback event".into(),
        ));
    }
    let mut result = Vec::with_capacity(frame.samples().len().div_ceil(PLAYBACK_FRAME_SAMPLES));
    for (index, samples) in frame.samples().chunks(PLAYBACK_FRAME_SAMPLES).enumerate() {
        let sequence = *next_sequence;
        *next_sequence = next_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::InvalidRequest("playback sequence overflow".into()))?;
        let sample_offset = index
            .checked_mul(PLAYBACK_FRAME_SAMPLES)
            .ok_or_else(|| AppError::InvalidRequest("playback offset overflow".into()))?;
        let captured_at = frame
            .captured_at()
            .checked_add(Duration::from_nanos(
                u64::try_from(sample_offset)
                    .map_err(|_| AppError::InvalidRequest("playback offset overflow".into()))?
                    .saturating_mul(1_000_000_000)
                    / u64::from(PLAYBACK_SAMPLE_RATE_HZ),
            ))
            .ok_or_else(|| AppError::InvalidRequest("playback timestamp overflow".into()))?;
        result.push(AudioFrame::at(
            frame.source(),
            sequence,
            captured_at,
            frame.spec(),
            samples.to_vec(),
        )?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audio::AudioSource;

    #[test]
    fn splits_maximum_provider_audio_without_truncation() {
        let samples = (0..MAX_PROVIDER_OUTPUT_SAMPLES)
            .map(|sample| sample as i16)
            .collect::<Vec<_>>();
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            99,
            PcmSpec::pcm16_mono(24_000),
            samples.clone(),
        )
        .unwrap();
        let mut sequence = 7;
        let split = split_provider_audio(&frame, &mut sequence).unwrap();
        assert!(split.iter().all(|frame| frame.samples().len() <= 1_920));
        assert_eq!(
            split
                .iter()
                .flat_map(|frame| frame.samples().iter().copied())
                .collect::<Vec<_>>(),
            samples
        );
        assert_eq!(split.first().unwrap().sequence(), 7);
        assert_eq!(sequence, 32);
    }

    #[test]
    fn rejects_audio_larger_than_the_provider_envelope() {
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![0; MAX_PROVIDER_OUTPUT_SAMPLES + 1],
        )
        .unwrap();
        assert!(split_provider_audio(&frame, &mut 0).is_err());
    }
}
