#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub input: String,
    pub output: String,
    pub seconds: u32,
    pub timeout_seconds: u32,
}
use std::{
    sync::mpsc as sync_mpsc,
    thread,
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use tokio::sync::mpsc;
use vox_bridge_lib::{
    audio::{
        AudioFrame, PcmSpec, Resampler,
        capture::AudioCapture,
        loopback::{SequenceCounts, tone_pcm16},
        platform::{AudioDirection, AudioEndpoint},
        playback::AudioPlayback,
    },
    domain::audio::AudioSource,
};

#[derive(Serialize)]
pub struct DeviceName {
    direction: &'static str,
    cpal_name: String,
}

pub fn list_devices() -> Result<Vec<DeviceName>, String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    for (direction, devices) in [
        ("input", host.input_devices()),
        ("output", host.output_devices()),
    ] {
        for device in devices.map_err(|error| error.to_string())? {
            names.push(DeviceName {
                direction,
                cpal_name: device.name().map_err(|error| error.to_string())?,
            });
        }
    }
    Ok(names)
}

fn selected_endpoint(
    names: &[DeviceName],
    name: &str,
    direction: AudioDirection,
) -> Result<AudioEndpoint, String> {
    let direction_name = if direction == AudioDirection::Input {
        "input"
    } else {
        "output"
    };
    if matches!(name, "default" | "sysdefault")
        || names
            .iter()
            .filter(|device| device.direction == direction_name && device.cpal_name == name)
            .count()
            != 1
    {
        return Err(format!(
            "select one uniquely named {direction_name} device from --list-devices; unavailable, ambiguous or default selection: {name}"
        ));
    }
    Ok(AudioEndpoint {
        id: name.into(),
        display_name: name.into(),
        direction,
        sample_rates: vec![],
        channels: 1,
        ready: true,
    })
}

#[derive(Serialize)]
pub struct PhysicalReport {
    mode: &'static str,
    input_cpal_name: String,
    output_cpal_name: String,
    capture_observed: SequenceCounts,
    playback_enqueued: SequenceCounts,
    capture_dropped_chunks: u64,
    capture_dropped_frames: u64,
    playback_dropped_samples: u64,
    elapsed_ms: u128,
}

// A dedicated executable can enforce the deadline even if a backend blocks in
// stream creation or teardown. Process exit releases the OS stream handles.
struct DeadlineGuard(sync_mpsc::SyncSender<()>);
impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}
fn deadline(seconds: u32) -> DeadlineGuard {
    let (sender, receiver) = sync_mpsc::sync_channel(1);
    thread::spawn(move || {
        if matches!(
            receiver.recv_timeout(Duration::from_secs(u64::from(seconds))),
            Err(sync_mpsc::RecvTimeoutError::Timeout)
        ) {
            eprintln!(
                "physical loopback exceeded {seconds}s deadline; exiting and closing audio handles"
            );
            std::process::exit(124);
        }
    });
    DeadlineGuard(sender)
}

/// Emits a finite tone and records actual CPAL capture observations. There is no
/// device fallback or restart. Counts alone do not establish an acoustic pass.
pub fn run(options: Options) -> Result<PhysicalReport, String> {
    let _deadline = deadline(options.timeout_seconds);
    let started = Instant::now();
    let names = list_devices()?;
    let input = selected_endpoint(&names, &options.input, AudioDirection::Input)?;
    let output = selected_endpoint(&names, &options.output, AudioDirection::Output)?;
    let capture_spec = PcmSpec::pcm16_mono(16_000);
    let playback_spec = PcmSpec::pcm16_mono(24_000);
    let tone = Resampler::convert(
        &tone_pcm16(options.seconds as usize * 16_000),
        capture_spec,
        playback_spec,
    )
    .map_err(|error| error.to_string())?;
    let (capture_tx, mut capture_rx) = mpsc::channel(32);
    let (playback_tx, playback_rx) = mpsc::channel(4);
    let capture = AudioCapture::start(input, capture_tx).map_err(|error| error.to_string())?;
    let playback = AudioPlayback::start(output, playback_rx).map_err(|error| error.to_string())?;
    let mut capture_observed = SequenceCounts::default();
    let mut playback_enqueued = SequenceCounts::default();
    let stream_started = Instant::now();
    let mut chunks = tone.chunks(480);
    while stream_started.elapsed() < Duration::from_secs(u64::from(options.seconds) + 1) {
        if capture.is_disconnected() || playback.is_disconnected() {
            return Err(
                "selected audio device disconnected; stopped without resume or fallback".into(),
            );
        }
        while let Ok(frame) = capture_rx.try_recv() {
            capture_observed
                .observe(&frame, capture_spec)
                .map_err(|error| error.to_string())?;
        }
        if stream_started.elapsed() >= Duration::from_millis(playback_enqueued.frames * 20) {
            if let Some(samples) = chunks.next() {
                let frame = AudioFrame::new(
                    AudioSource::Microphone,
                    playback_enqueued.frames + 1,
                    playback_spec,
                    samples.to_vec(),
                )
                .map_err(|error| error.to_string())?;
                playback_tx
                    .try_send(frame.clone())
                    .map_err(|error| format!("playback enqueue failed: {error}"))?;
                playback_enqueued
                    .observe(&frame, playback_spec)
                    .map_err(|error| error.to_string())?;
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    let capture_dropped_chunks = capture.dropped_chunks();
    let capture_dropped_frames = capture.dropped_frames();
    let playback_dropped_samples = playback.dropped_samples();
    capture.stop();
    playback.stop();
    while let Ok(frame) = capture_rx.try_recv() {
        capture_observed
            .observe(&frame, capture_spec)
            .map_err(|error| error.to_string())?;
    }
    playback_enqueued
        .finish(
            u64::from(options.seconds) * 50,
            options.seconds as usize * 24_000,
        )
        .map_err(|error| error.to_string())?;
    if capture_observed.frames == 0
        || capture_dropped_chunks != 0
        || capture_dropped_frames != 0
        || playback_dropped_samples != 0
    {
        return Err(format!(
            "incomplete physical observation: capture frames {}, dropped chunks {capture_dropped_chunks}, dropped frames {capture_dropped_frames}, dropped playback samples {playback_dropped_samples}",
            capture_observed.frames
        ));
    }
    Ok(PhysicalReport {
        mode: "physical_observation",
        input_cpal_name: options.input,
        output_cpal_name: options.output,
        capture_observed,
        playback_enqueued,
        capture_dropped_chunks,
        capture_dropped_frames,
        playback_dropped_samples,
        elapsed_ms: started.elapsed().as_millis(),
    })
}
