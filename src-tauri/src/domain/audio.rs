pub use crate::audio::frame::AudioFrame;

use serde::{Deserialize, Serialize};

pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const PLAYBACK_SAMPLE_RATE_HZ: u32 = 24_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    Microphone,
    System,
}
