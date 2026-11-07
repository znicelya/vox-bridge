use crate::{
    error::{AppError, AppResult},
    intelligence::{
        config::{endpoint, validate_profile},
        types::{IntelligenceErrorCode, ModelProfile, ServiceKind},
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingIdentity {
    pub profile_id: Uuid,
    pub base_url: String,
    pub model: String,
    pub dimensions: u32,
    pub normalization_version: u32,
}
impl EmbeddingIdentity {
    pub(crate) fn semantic_key(&self) -> AppResult<String> {
        self.key()?;
        #[derive(Serialize)]
        struct Semantic<'a> {
            profile_id: Uuid,
            base_url: &'a str,
            model: &'a str,
            normalization_version: u32,
        }
        let value = Semantic {
            profile_id: self.profile_id,
            base_url: &self.base_url,
            model: &self.model,
            normalization_version: self.normalization_version,
        };
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&value).map_err(|_| invalid())?)
        ))
    }
    pub fn from_profile(profile: &ModelProfile, actual_dimension: u32) -> AppResult<Self> {
        validate_profile(profile)?;
        if profile.kind != ServiceKind::Embedding
            || !(128..=4096).contains(&actual_dimension)
            || profile
                .dimensions
                .is_some_and(|dimension| dimension != actual_dimension)
        {
            return Err(invalid());
        }
        Ok(Self {
            profile_id: profile.id,
            base_url: normalized_base(profile)?,
            model: profile.model.clone().ok_or_else(invalid)?,
            dimensions: actual_dimension,
            normalization_version: 1,
        })
    }
    pub fn key(&self) -> AppResult<String> {
        let profile = ModelProfile {
            id: self.profile_id,
            kind: ServiceKind::Embedding,
            name: "Index identity".into(),
            base_url: self.base_url.clone(),
            model: Some(self.model.clone()),
            timeout_ms: 1000,
            max_context_tokens: None,
            max_output_tokens: None,
            dimensions: Some(self.dimensions),
        };
        validate_profile(&profile)?;
        if self.normalization_version != 1 || normalized_base(&profile)? != self.base_url {
            return Err(invalid());
        }
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).map_err(|_| invalid())?)
        ))
    }
}
fn normalized_base(profile: &ModelProfile) -> AppResult<String> {
    let mut url = endpoint(profile)?;
    let path = url
        .path()
        .strip_suffix("/embeddings")
        .ok_or_else(invalid)?
        .to_owned();
    url.set_path(if path.is_empty() { "/" } else { &path });
    Ok(url.to_string())
}
fn invalid() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::Configuration)
}
