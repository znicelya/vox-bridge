use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const SAMPLE_RATE_HZ: u32 = 16_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    Microphone,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFrame {
    pub source: AudioSource,
    pub sequence: u64,
    pub captured_at: SystemTime,
    pub samples: Vec<i16>,
}

impl AudioFrame {
    pub fn new(
        source: AudioSource,
        sequence: u64,
        sample_rate_hz: u32,
        samples: Vec<i16>,
    ) -> AppResult<Self> {
        if sample_rate_hz != SAMPLE_RATE_HZ {
            return Err(AppError::InvalidRequest(format!(
                "audio sample rate must be {SAMPLE_RATE_HZ} Hz"
            )));
        }
        if samples.is_empty() {
            return Err(AppError::InvalidRequest(
                "audio frame must contain samples".to_string(),
            ));
        }

        Ok(Self {
            source,
            sequence,
            captured_at: SystemTime::now(),
            samples,
        })
    }
}
