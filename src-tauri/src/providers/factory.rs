//! Provider selection and complete, network-free session preflight.
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;
use uuid::Uuid;

use super::{
    aliyun::AliyunProvider,
    disabled::DisabledProvider,
    palabra::PalabraProvider,
    volc::client::{ENDPOINT, SECRET_SERVICE, VolcengineProvider, secret_key},
    *,
};
use crate::{
    audio::PcmSpec,
    config::{ProviderConfig, SecretStore},
    domain::session::{LanguagePair, ProviderId, SessionConfig, SessionDirection, SessionMode},
};

pub struct ProviderFactory {
    secrets: Arc<SecretStore>,
    #[cfg(test)]
    test_provider: Option<Arc<dyn TranslationProvider>>,
}
/// All direction/configuration validation has completed; no stream is open yet.
#[derive(Clone)]
pub struct PreparedProviderSession {
    provider: Arc<dyn TranslationProvider>,
    configs: Vec<ProviderSessionConfig>,
}
pub struct DirectedProviderStream {
    pub config: ProviderSessionConfig,
    pub stream: ProviderStream,
}
impl ProviderFactory {
    pub fn new(secrets: Arc<SecretStore>) -> Self {
        Self {
            secrets,
            #[cfg(test)]
            test_provider: None,
        }
    }

    /// Returns documented capabilities, including explicit unavailability.
    /// Credentials are not accessed by selection or preflight.
    pub fn capabilities(&self, saved: &ProviderConfig) -> ProviderResult<ProviderCapabilities> {
        Ok(self.resolve(saved)?.capabilities())
    }

    pub fn prepare(
        &self,
        saved: &ProviderConfig,
        session_id: Uuid,
        session: &SessionConfig,
        input_audio: PcmSpec,
        output_audio: PcmSpec,
    ) -> ProviderResult<PreparedProviderSession> {
        let provider = self.resolve(saved)?;
        let caps = provider.capabilities();
        if !caps.ready {
            return Err(ProviderError::Unavailable);
        }
        if !caps.supports_tts {
            return Err(ProviderError::UnsupportedTts);
        }
        if input_audio != caps.input_audio || input_audio.validate().is_err() {
            return Err(ProviderError::UnsupportedAudioFormat);
        }
        let live_direction = session.direction();
        let directions: &[SessionDirection] = if session.mode() == SessionMode::Call {
            &[
                SessionDirection::SourceToTarget,
                SessionDirection::TargetToSource,
            ]
        } else {
            std::slice::from_ref(&live_direction)
        };
        let mut configs = Vec::with_capacity(directions.len());
        for direction in directions {
            let pair = session.languages();
            let languages = match direction {
                SessionDirection::SourceToTarget => pair.clone(),
                SessionDirection::TargetToSource => LanguagePair::new(pair.target(), pair.source())
                    .map_err(|_| ProviderError::InvalidConfiguration)?,
                SessionDirection::Bidirectional => return Err(ProviderError::InvalidDirection),
            };
            let config = ProviderSessionConfig {
                session_id,
                direction: *direction,
                mode: session.mode(),
                languages,
                target_audio: output_audio,
                provider_config_id: saved.id(),
            };
            caps.validate(&config)?;
            configs.push(config);
        }
        Ok(PreparedProviderSession { provider, configs })
    }

    fn resolve(&self, saved: &ProviderConfig) -> ProviderResult<Arc<dyn TranslationProvider>> {
        saved
            .validate()
            .map_err(|_| ProviderError::InvalidConfiguration)?;
        #[cfg(test)]
        if let Some(provider) = &self.test_provider {
            if provider.id().as_str() == saved.provider() {
                return Ok(provider.clone());
            }
        }
        match saved.provider() {
            "volcengine" => {
                // This release uses the reviewed official transport. Reject a
                // saved override instead of silently sending its key elsewhere.
                if saved.endpoint() != ENDPOINT
                    || saved.secret_reference().is_some_and(|reference| {
                        reference != volcengine_secret_reference(saved.id())
                    })
                {
                    return Err(ProviderError::InvalidConfiguration);
                }
                Ok(Arc::new(VolcengineProvider::new(self.secrets.clone())))
            }
            "aliyun" => Ok(Arc::new(AliyunProvider)),
            "palabra" => Ok(Arc::new(PalabraProvider)),
            id => Ok(Arc::new(DisabledProvider::new(
                ProviderId::new(id).map_err(|_| ProviderError::InvalidConfiguration)?,
            ))),
        }
    }
    #[cfg(test)]
    pub(crate) fn with_test_provider(mut self, provider: Arc<dyn TranslationProvider>) -> Self {
        self.test_provider = Some(provider);
        self
    }
}
impl PreparedProviderSession {
    pub fn configs(&self) -> &[ProviderSessionConfig] {
        &self.configs
    }

    pub fn provider_id(&self) -> ProviderId {
        self.provider.id()
    }

    pub async fn connect_direction(
        self,
        direction: SessionDirection,
    ) -> ProviderResult<DirectedProviderStream> {
        if direction == SessionDirection::Bidirectional {
            return Err(ProviderError::InvalidDirection);
        }
        let config = self
            .configs
            .into_iter()
            .find(|config| config.direction == direction)
            .ok_or(ProviderError::InvalidDirection)?;
        let stream = timeout(
            Duration::from_secs(10),
            self.provider.connect(config.clone()),
        )
        .await
        .unwrap_or(Err(ProviderError::Timeout))?;
        Ok(DirectedProviderStream { config, stream })
    }

    /// Opening returns independent streams; callers still must consume each
    /// SessionStarted event before admitting capture or reporting readiness.
    pub async fn connect(self) -> ProviderResult<Vec<DirectedProviderStream>> {
        let mut streams: Vec<DirectedProviderStream> = Vec::with_capacity(self.configs.len());
        for config in self.configs {
            let result = timeout(
                Duration::from_secs(10),
                self.provider.connect(config.clone()),
            )
            .await
            .unwrap_or(Err(ProviderError::Timeout));
            match result {
                Ok(stream) => streams.push(DirectedProviderStream { config, stream }),
                Err(error) => {
                    for opened in &streams {
                        // Request cleanup, then drop each stream even if an
                        // adapter's cancellation acknowledgement fails/stalls.
                        let _ = timeout(Duration::from_secs(1), opened.stream.cancel()).await;
                    }
                    return Err(error);
                }
            }
        }
        Ok(streams)
    }
}
/// Opaque backend reference for the UUID-selected encrypted API-key entry.
pub fn volcengine_secret_reference(config_id: Uuid) -> String {
    format!("{SECRET_SERVICE}/{}", secret_key(config_id))
}
#[cfg(test)]
#[path = "factory_tests.rs"]
mod tests;
