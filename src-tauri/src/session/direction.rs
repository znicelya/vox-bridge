use std::time::Duration;

use crate::{
    audio::{AudioFrame, PcmSpec, router::Direction as RouterDirection},
    domain::{
        audio::PLAYBACK_SAMPLE_RATE_HZ,
        session::{SessionDirection, SessionMode},
        transcript::{DiagnosticPayload, GapCause, RuntimeFailureCode},
    },
    error::{AppError, AppResult},
    providers::ProviderError,
};
use tokio::time::Instant;

pub(crate) const MAX_PROVIDER_OUTPUT_SAMPLES: usize = 48_000;
pub(crate) const PLAYBACK_FRAME_SAMPLES: usize = 1_920;

pub(crate) struct DiagnosticAccumulator {
    last_flush: Instant,
    latency: Option<(Option<u64>, Option<u64>)>,
    gaps: Vec<(GapCause, u64)>,
    transcript_overflow: u64,
}

impl DiagnosticAccumulator {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_flush: now,
            latency: None,
            gaps: Vec::new(),
            transcript_overflow: 0,
        }
    }

    pub(crate) fn observe_latency(
        &mut self,
        capture_to_admission_ms: Option<u64>,
        response_to_enqueue_ms: Option<u64>,
    ) {
        let (capture, response) = self.latency.unwrap_or((None, None));
        self.latency = Some((
            capture_to_admission_ms.or(capture),
            response_to_enqueue_ms.or(response),
        ));
    }

    pub(crate) fn observe_gap(&mut self, cause: GapCause, dropped_frames: u64) {
        if let Some((_, count)) = self.gaps.iter_mut().find(|(current, _)| *current == cause) {
            *count = count.saturating_add(dropped_frames);
        } else {
            self.gaps.push((cause, dropped_frames));
        }
    }

    pub(crate) fn observe_transcript_overflow(&mut self, dropped_interims: u64) {
        self.transcript_overflow = self.transcript_overflow.saturating_add(dropped_interims);
    }

    pub(crate) fn take_due(&mut self, now: Instant, force: bool) -> Vec<DiagnosticPayload> {
        if !force && now.duration_since(self.last_flush) < Duration::from_secs(1) {
            return Vec::new();
        }
        self.last_flush = now;
        let mut payloads = Vec::new();
        if let Some((capture_to_send_ms, response_to_enqueue_ms)) = self.latency.take() {
            payloads.push(DiagnosticPayload::Latency {
                // This field ends at provider queue admission, not a wire send.
                capture_to_send_ms,
                response_to_enqueue_ms,
                enqueue_to_callback_ms: None,
                provider_processing_ms: None,
            });
        }
        payloads.extend(self.gaps.drain(..).map(|(cause, dropped_frames)| {
            DiagnosticPayload::Gap {
                cause,
                dropped_frames,
            }
        }));
        if self.transcript_overflow != 0 {
            payloads.push(DiagnosticPayload::TranscriptOverflow {
                dropped_interims: std::mem::take(&mut self.transcript_overflow),
            });
        }
        payloads
    }
}

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
    use crate::domain::{
        audio::AudioSource,
        transcript::{DiagnosticPayload, GapCause},
    };
    use tokio::time::Instant;

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

    #[test]
    fn diagnostics_coalesce_by_category_and_force_flush_pending_values() {
        let start = Instant::now();
        let mut diagnostics = DiagnosticAccumulator::new(start);
        diagnostics.observe_latency(Some(10), Some(20));
        diagnostics.observe_latency(Some(30), Some(40));
        diagnostics.observe_gap(GapCause::CaptureOverflow, 2);
        diagnostics.observe_gap(GapCause::CaptureOverflow, 3);
        diagnostics.observe_transcript_overflow(4);

        assert!(
            diagnostics
                .take_due(start + Duration::from_millis(999), false)
                .is_empty()
        );
        let due = diagnostics.take_due(start + Duration::from_secs(1), false);
        assert_eq!(due.len(), 3);
        assert!(due.iter().any(|payload| matches!(
            payload,
            DiagnosticPayload::Latency {
                capture_to_send_ms: Some(30),
                response_to_enqueue_ms: Some(40),
                ..
            }
        )));
        assert!(due.iter().any(|payload| matches!(
            payload,
            DiagnosticPayload::Gap {
                cause: GapCause::CaptureOverflow,
                dropped_frames: 5
            }
        )));
        assert!(due.iter().any(|payload| matches!(
            payload,
            DiagnosticPayload::TranscriptOverflow {
                dropped_interims: 4
            }
        )));

        diagnostics.observe_gap(GapCause::OutputOverflow, 1);
        assert!(
            diagnostics
                .take_due(start + Duration::from_millis(1_500), false)
                .is_empty()
        );
        assert!(matches!(
            diagnostics
                .take_due(start + Duration::from_millis(1_500), true)
                .as_slice(),
            [DiagnosticPayload::Gap {
                cause: GapCause::OutputOverflow,
                dropped_frames: 1
            }]
        ));
    }
}
