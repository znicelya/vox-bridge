use super::{PcmSpec, Resampler};

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
fn streaming_resampler_keeps_a_non_silent_signal_continuous_across_chunks() {
    let from = PcmSpec::pcm16_mono(16_000);
    let to = PcmSpec::pcm16_mono(24_000);
    let input: Vec<i16> = (0..1_600)
        .map(|index| ((index as f64 * 0.04).sin() * 12_000.0) as i16)
        .collect();

    let mut streaming = Resampler::new(from, to).unwrap();
    let mut output = Vec::new();
    output.extend(streaming.process(&input[..400]).unwrap());
    output.extend(streaming.process(&input[400..1_000]).unwrap());
    output.extend(streaming.process(&input[1_000..]).unwrap());
    output.extend(streaming.flush().unwrap());

    assert_eq!(output.len(), 2_400);
    assert!(output.iter().any(|sample| sample.abs() > 1_000));
    assert!(
        output
            .windows(2)
            .all(|pair| (pair[1] as i32 - pair[0] as i32).abs() < 2_000)
    );
    assert_eq!(output, Resampler::convert(&input, from, to).unwrap());
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
    assert!(PcmSpec::new(48_000, 2, super::super::SampleFormat::I16).is_err());
}

#[test]
fn rejects_unsupported_sample_formats_and_invalid_rates() {
    assert!(PcmSpec::new(24_000, 1, super::super::SampleFormat::F32).is_err());
    assert!(PcmSpec::new(0, 1, super::super::SampleFormat::I16).is_err());
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
