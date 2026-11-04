use std::time::SystemTime;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{AppError, AppResult};

pub const SAMPLE_RATE_HZ: u32 = 16_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    Microphone,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AudioFrame {
    source: AudioSource,
    sequence: u64,
    captured_at: SystemTime,
    samples: Vec<i16>,
}

impl<'de> Deserialize<'de> for AudioFrame {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawAudioFrame {
            source: AudioSource,
            sequence: u64,
            captured_at: SystemTime,
            samples: Vec<i16>,
        }
        let raw = RawAudioFrame::deserialize(deserializer)?;
        if raw.samples.is_empty() {
            return Err(serde::de::Error::custom("audio frame must contain samples"));
        }
        Ok(Self {
            source: raw.source,
            sequence: raw.sequence,
            captured_at: raw.captured_at,
            samples: raw.samples,
        })
    }
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

    pub fn source(&self) -> AudioSource {
        self.source
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn captured_at(&self) -> SystemTime {
        self.captured_at
    }
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }
}
