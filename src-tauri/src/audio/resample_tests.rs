use super::{PcmSpec, Resampler};
use crate::audio::SampleFormat;

#[test]
fn converts_24khz_output_to_a_48khz_device_without_changing_duration() {
    let input = vec![0i16; 24_000];
    let output = Resampler::convert(
        &input,
        PcmSpec::pcm16_mono(24_000),
        PcmSpec::pcm16_mono(48_000),
    )
    .unwrap();

    assert_eq!(output.len(), 48_000);
}

#[test]
fn converts_48khz_capture_to_canonical_16khz_without_changing_duration() {
    let input = vec![0i16; 48_000];
    let output = Resampler::convert(
        &input,
        PcmSpec::pcm16_mono(48_000),
        PcmSpec::pcm16_mono(16_000),
    )
    .unwrap();

    assert_eq!(output.len(), 16_000);
}

#[test]
fn resampling_preserves_silence() {
    let input = vec![0i16; 1_600];
    let output = Resampler::convert(
        &input,
        PcmSpec::pcm16_mono(16_000),
        PcmSpec::pcm16_mono(24_000),
    )
    .unwrap();

    assert_eq!(output.len(), 2_400);
    assert!(output.iter().all(|sample| *sample == 0));
}

#[test]
fn rejects_non_mono_input_or_output() {
    let stereo = PcmSpec {
        sample_rate: 48_000,
        channels: 2,
        sample_format: SampleFormat::I16,
    };

    assert!(Resampler::convert(&[0; 32], stereo, PcmSpec::pcm16_mono(24_000)).is_err());
    assert!(Resampler::convert(&[0; 32], PcmSpec::pcm16_mono(24_000), stereo).is_err());
}

#[test]
fn rejects_unsupported_sample_formats_and_invalid_rates() {
    let float = PcmSpec {
        sample_rate: 24_000,
        channels: 1,
        sample_format: SampleFormat::F32,
    };
    let invalid_rate = PcmSpec::pcm16_mono(0);

    assert!(Resampler::convert(&[0; 32], float, PcmSpec::pcm16_mono(48_000)).is_err());
    assert!(Resampler::convert(&[0; 32], PcmSpec::pcm16_mono(24_000), float).is_err());
    assert!(Resampler::convert(&[0; 32], invalid_rate, PcmSpec::pcm16_mono(48_000)).is_err());
}

#[test]
fn audio_frame_deserialization_enforces_pcm_invariants() {
    let invalid_spec = r#"{
        "source":"microphone",
        "sequence":1,
        "captured_at":{"secs_since_epoch":1,"nanos_since_epoch":0},
        "spec":{"sample_rate":16000,"channels":2,"sample_format":"i16"},
        "samples":[0]
    }"#;
    let empty_samples = r#"{
        "source":"microphone",
        "sequence":1,
        "captured_at":{"secs_since_epoch":1,"nanos_since_epoch":0},
        "spec":{"sample_rate":16000,"channels":1,"sample_format":"i16"},
        "samples":[]
    }"#;

    assert!(serde_json::from_str::<super::super::AudioFrame>(invalid_spec).is_err());
    assert!(serde_json::from_str::<super::super::AudioFrame>(empty_samples).is_err());
}
