use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmConfig {
    pub model: String,
    pub resource_id: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingConfig {
    pub model: String,
    pub dimensions: Option<u32>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TavilyConfig {
    pub endpoint: String,
    pub search_depth: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    id: Uuid,
    provider: String,
    endpoint: String,
    pub llm: Option<LlmConfig>,
    pub embedding: Option<EmbeddingConfig>,
    pub tavily: Option<TavilyConfig>,
    language_capabilities: Vec<String>,
    secret_reference: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicProviderConfig {
    id: Uuid,
    provider: String,
    endpoint: String,
    pub llm: Option<LlmConfig>,
    pub embedding: Option<EmbeddingConfig>,
    pub tavily: Option<TavilyConfig>,
    language_capabilities: Vec<String>,
    secret_reference: Option<String>,
}

impl ProviderConfig {
    pub fn new(
        id: Uuid,
        provider: impl Into<String>,
        endpoint: impl Into<String>,
        resource_id: Option<String>,
        model: Option<String>,
        language_capabilities: Vec<String>,
        secret_reference: Option<String>,
    ) -> AppResult<Self> {
        let value = Self {
            id,
            provider: provider.into(),
            endpoint: endpoint.into(),
            llm: model.map(|model| LlmConfig { model, resource_id }),
            embedding: None,
            tavily: None,
            language_capabilities,
            secret_reference,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn public_view(&self) -> PublicProviderConfig {
        PublicProviderConfig {
            id: self.id,
            provider: self.provider.clone(),
            endpoint: self.endpoint.clone(),
            llm: self.llm.clone(),
            embedding: self.embedding.clone(),
            tavily: self.tavily.clone(),
            language_capabilities: self.language_capabilities.clone(),
            secret_reference: self.secret_reference.clone(),
        }
    }
    pub fn validate(&self) -> AppResult<()> {
        if self.provider.trim().is_empty() {
            return Err(AppError::Configuration("provider is required".into()));
        }
        if self.endpoint.trim().is_empty() {
            return Err(AppError::Configuration("endpoint is required".into()));
        }
        validate_remote_url(&self.endpoint)?;
        if self
            .llm
            .as_ref()
            .is_some_and(|llm| llm.model.trim().is_empty())
        {
            return Err(AppError::Configuration("LLM model is required".into()));
        }
        if self
            .embedding
            .as_ref()
            .is_some_and(|config| config.model.trim().is_empty())
        {
            return Err(AppError::Configuration(
                "embedding model is required".into(),
            ));
        }
        if self
            .tavily
            .as_ref()
            .is_some_and(|config| config.endpoint.trim().is_empty())
        {
            return Err(AppError::Configuration(
                "Tavily endpoint is required".into(),
            ));
        }
        if let Some(tavily) = &self.tavily {
            validate_remote_url(&tavily.endpoint)?;
        }
        Ok(())
    }
}

fn validate_remote_url(value: &str) -> AppResult<()> {
    let url = Url::parse(value)
        .map_err(|_| AppError::Configuration("remote endpoint must be a valid URL".into()))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(AppError::Configuration(
            "remote endpoint must use HTTPS and include a host".into(),
        ));
    }
    Ok(())
}

impl PublicProviderConfig {
    pub fn secret_reference(&self) -> Option<&str> {
        self.secret_reference.as_deref()
    }
}
