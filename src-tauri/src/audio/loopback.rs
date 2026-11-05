//! Development-only, provider-free audio diagnostics. Synthetic results are not hardware evidence.

use serde::Serialize;
use tokio::sync::mpsc;

use super::{
    AudioFrame, PcmSpec, Resampler,
    graph::AudioGraph,
    platform::{AudioDirection, AudioEndpoint},
    queue::BoundedAudioQueue,
    router::{AudioRouter, EndpointHealthCheck, RouteTarget},
};
use crate::domain::audio::AudioSource;
use crate::error::{AppError, AppResult};

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct SequenceCounts {
    pub frames: u64,
    pub samples: usize,
}

impl SequenceCounts {
    pub fn observe(&mut self, frame: &AudioFrame, spec: PcmSpec) -> AppResult<()> {
        if frame.sequence() != self.frames + 1 {
            return Err(AppError::InvalidRequest(format!(
                "expected sequence {}, observed {}",
                self.frames + 1,
                frame.sequence()
            )));
        }
        if frame.spec() != spec {
            return Err(AppError::InvalidRequest(format!(
                "sequence {} has unexpected PCM format",
                frame.sequence()
            )));
        }
        self.frames += 1;
        self.samples += frame.samples().len();
        Ok(())
    }

    pub fn finish(&self, frames: u64, samples: usize) -> AppResult<()> {
        if self.frames != frames || self.samples != samples {
            return Err(AppError::InvalidRequest(format!(
                "expected {frames} frames / {samples} samples, observed {} / {}",
                self.frames, self.samples
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct SyntheticReport {
    pub mode: &'static str,
    pub capture: SequenceCounts,
    pub playback: SequenceCounts,
    pub dropped_frames: u64,
}

/// Exactly 16 samples per 1 kHz period at 16 kHz, approximately -20 dBFS peak.
/// An integer lookup avoids platform-dependent trigonometric rounding.
pub fn tone_pcm16(samples: usize) -> Vec<i16> {
    const PERIOD: [i16; 16] = [
        0, 1254, 2317, 3028, 3277, 3028, 2317, 1254, 0, -1254, -2317, -3028, -3277, -3028, -2317,
        -1254,
    ];
    PERIOD.into_iter().cycle().take(samples).collect()
}

struct SyntheticHealth;
impl EndpointHealthCheck for SyntheticHealth {
    fn is_ready(&self, endpoint: &AudioEndpoint) -> bool {
        endpoint.ready
    }
}

fn synthetic_endpoint(id: &str, direction: AudioDirection) -> AudioEndpoint {
    AudioEndpoint {
        id: id.into(),
        display_name: id.into(),
        direction,
        sample_rates: vec![16_000, 24_000],
        channels: 1,
        ready: true,
    }
}

/// Exercises real frame validation, bounded queues, resampling and routing, with
/// synthetic capture/playback boundaries. Does not open CPAL or contact a provider.
pub async fn run_synthetic(frames: u32) -> AppResult<SyntheticReport> {
    if !(1..=250).contains(&frames) {
        return Err(AppError::InvalidRequest(
            "frames must be between 1 and 250 (20 ms each)".into(),
        ));
    }
    let capture_spec = PcmSpec::pcm16_mono(16_000);
    let playback_spec = PcmSpec::pcm16_mono(24_000);
    let capture_queue = BoundedAudioQueue::new(2);
    let tone = tone_pcm16(frames as usize * 320);
    let mut captured = Vec::with_capacity(tone.len());
    let mut capture = SequenceCounts::default();
    for (index, samples) in tone.chunks_exact(320).enumerate() {
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            index as u64 + 1,
            capture_spec,
            samples.to_vec(),
        )?;
        capture_queue
            .push(frame)
            .await
            .map_err(|_| AppError::Internal("synthetic capture queue rejected frame".into()))?;
        let frame = capture_queue
            .pop()
            .await
            .ok_or_else(|| AppError::Internal("synthetic capture queue lost frame".into()))?;
        capture.observe(&frame, capture_spec)?;
        captured.extend_from_slice(frame.samples());
    }
    capture.finish(u64::from(frames), frames as usize * 320)?;
    if captured != tone || capture_queue.dropped_frames() != 0 {
        return Err(AppError::Internal(
            "synthetic capture dropped or changed PCM".into(),
        ));
    }
    // One continuous resampler stream avoids resetting filter state at every frame.
    let converted = Resampler::convert(&captured, capture_spec, playback_spec)?;
    let graph = AudioGraph::for_live(
        synthetic_endpoint("synthetic-input", AudioDirection::Input),
        synthetic_endpoint("synthetic-M1", AudioDirection::Output),
    )?;
    let (sender, mut receiver) = mpsc::channel(2);
    let router = AudioRouter::for_live(graph, sender, SyntheticHealth);
    let mut playback = SequenceCounts::default();
    for (index, samples) in converted.chunks(480).enumerate() {
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            index as u64 + 1,
            playback_spec,
            samples.to_vec(),
        )?;
        router.route(frame.clone(), RouteTarget::M1)?;
        let received = receiver.try_recv().map_err(|error| {
            AppError::Internal(format!("synthetic playback lost frame: {error}"))
        })?;
        if received != frame {
            return Err(AppError::Internal(
                "synthetic playback changed PCM or metadata".into(),
            ));
        }
        playback.observe(&received, playback_spec)?;
    }
    playback.finish(u64::from(frames), frames as usize * 480)?;
    Ok(SyntheticReport {
        mode: "synthetic",
        capture,
        playback,
        dropped_frames: capture_queue.dropped_frames(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audio::AudioSource;

    fn frame(sequence: u64, spec: PcmSpec, samples: usize) -> AudioFrame {
        AudioFrame::new(AudioSource::Microphone, sequence, spec, vec![1; samples]).unwrap()
    }

    #[test]
    fn tone_is_repeatable_one_khz_mono_pcm_with_bounded_amplitude() {
        let tone = tone_pcm16(16_000);
        assert_eq!(tone.len(), 16_000);
        assert_eq!(&tone[..5], &[0, 1254, 2317, 3028, 3277]);
        assert!(tone.chunks_exact(16).all(|period| period == &tone[..16]));
        assert_eq!(tone.iter().map(|sample| i64::from(*sample)).sum::<i64>(), 0);
        assert!(tone.iter().all(|sample| sample.abs() <= 3277));
    }

    #[test]
    fn sequence_validation_detects_drops_duplicates_and_wrong_formats() {
        let spec = PcmSpec::pcm16_mono(16_000);
        let mut counts = SequenceCounts::default();
        counts.observe(&frame(1, spec, 320), spec).unwrap();
        for wrong in [
            frame(1, spec, 320),
            frame(3, spec, 320),
            frame(2, PcmSpec::pcm16_mono(24_000), 480),
        ] {
            assert!(counts.observe(&wrong, spec).is_err());
            assert_eq!(
                counts,
                SequenceCounts {
                    frames: 1,
                    samples: 320
                }
            );
        }
        counts.observe(&frame(2, spec, 640), spec).unwrap();
        counts.finish(2, 960).unwrap();
        assert!(counts.finish(3, 960).is_err());
        assert!(counts.finish(2, 961).is_err());
    }

    #[tokio::test]
    async fn synthetic_loopback_preserves_frame_counts_and_duration_across_canonical_rates() {
        let report = run_synthetic(50).await.unwrap();
        assert_eq!(report.mode, "synthetic");
        assert_eq!(
            report.capture,
            SequenceCounts {
                frames: 50,
                samples: 16_000
            }
        );
        assert_eq!(
            report.playback,
            SequenceCounts {
                frames: 50,
                samples: 24_000
            }
        );
        assert_eq!(report.dropped_frames, 0);
    }

    #[tokio::test]
    async fn synthetic_loopback_rejects_empty_and_unbounded_requests() {
        for frames in [0, 251, u32::MAX] {
            assert!(matches!(
                run_synthetic(frames).await,
                Err(AppError::InvalidRequest(_))
            ));
        }
    }
}
