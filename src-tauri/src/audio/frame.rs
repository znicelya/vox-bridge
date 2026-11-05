use std::time::{Duration, SystemTime};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    domain::audio::AudioSource,
    error::{AppError, AppResult},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleFormat {
    I16,
    F32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcmSpec {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: SampleFormat,
}

impl PcmSpec {
    pub const fn pcm16_mono(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            channels: 1,
            sample_format: SampleFormat::I16,
        }
    }

    pub fn validate(self) -> AppResult<()> {
        if self.sample_rate == 0 {
            return Err(AppError::InvalidRequest(
                "audio sample rate must be greater than zero".to_string(),
            ));
        }
        if self.channels != 1 {
            return Err(AppError::InvalidRequest(
                "audio PCM must be mono".to_string(),
            ));
        }
        if self.sample_format != SampleFormat::I16 {
            return Err(AppError::InvalidRequest(
                "audio PCM sample format must be signed 16-bit integer".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AudioFrame {
    pub source: AudioSource,
    pub sequence: u64,
    pub captured_at: SystemTime,
    pub spec: PcmSpec,
    pub samples: Vec<i16>,
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
            spec: PcmSpec,
            samples: Vec<i16>,
        }

        let raw = RawAudioFrame::deserialize(deserializer)?;
        raw.spec.validate().map_err(serde::de::Error::custom)?;
        if raw.samples.is_empty() {
            return Err(serde::de::Error::custom("audio frame must contain samples"));
        }

        Ok(Self {
            source: raw.source,
            sequence: raw.sequence,
            captured_at: raw.captured_at,
            spec: raw.spec,
            samples: raw.samples,
        })
    }
}

impl AudioFrame {
    pub fn new(
        source: AudioSource,
        sequence: u64,
        spec: PcmSpec,
        samples: Vec<i16>,
    ) -> AppResult<Self> {
        spec.validate()?;
        if samples.is_empty() {
            return Err(AppError::InvalidRequest(
                "audio frame must contain samples".to_string(),
            ));
        }

        Ok(Self {
            source,
            sequence,
            captured_at: SystemTime::now(),
            spec,
            samples,
        })
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / self.spec.sample_rate as f64)
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

    pub fn spec(&self) -> PcmSpec {
        self.spec
    }

    pub fn samples(&self) -> &[i16] {
        &self.samples
    }
}
