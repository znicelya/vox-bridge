use super::*;
use serde::{Deserialize, Serialize};
use std::io::Read;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
pub(super) const MAX_PROFILES: usize = 128;
pub(super) const SERVICE: &str = "com.voxbridge.intelligence";
pub(super) fn key(id: Uuid) -> String {
    format!("{id}/api-key")
}
pub(super) fn reference(id: Uuid) -> String {
    format!("{SERVICE}/{}", key(id))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredProfile {
    pub profile: ModelProfile,
    #[serde(default)]
    pub credential_reference: Option<String>,
    #[serde(default)]
    pub revision: Uuid,
}
impl StoredProfile {
    pub fn referenced(&self) -> bool {
        self.credential_reference.as_deref() == Some(reference(self.profile.id).as_str())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    pub version: u32,
    pub profiles: Vec<StoredProfile>,
    pub settings: IntelligenceSettings,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![],
            settings: IntelligenceSettings::default(),
        }
    }
}
pub(super) fn read(tx: &ConfigTransaction<'_>) -> AppResult<Document> {
    let file = match std::fs::File::open(tx.data_dir().join("service-configs.json")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Document::default()),
        Err(_) => return Err(configuration()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| configuration())?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(configuration());
    }
    let doc: Document = serde_json::from_slice(&bytes).map_err(|_| configuration())?;
    if doc.version != 1 || doc.profiles.len() > MAX_PROFILES {
        return Err(configuration());
    }
    let mut ids = std::collections::HashSet::new();
    for saved in &doc.profiles {
        validate_profile(&saved.profile)?;
        if !ids.insert(saved.profile.id) {
            return Err(configuration());
        }
    }
    Ok(doc)
}
pub(super) fn write(tx: &mut ConfigTransaction<'_>, doc: &Document) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(doc).map_err(|_| configuration())?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(configuration());
    }
    tx.save_service_configs(&bytes).map_err(safe_config_error)
}
pub(super) fn selected(settings: &IntelligenceSettings, id: Uuid) -> bool {
    [
        settings.summary_profile_id,
        settings.embedding_profile_id,
        settings.tavily_profile_id,
    ]
    .contains(&Some(id))
}
pub(super) fn validate_settings(doc: &Document, settings: &IntelligenceSettings) -> AppResult<()> {
    for (id, kind) in [
        (settings.summary_profile_id, ServiceKind::Llm),
        (settings.embedding_profile_id, ServiceKind::Embedding),
        (settings.tavily_profile_id, ServiceKind::Tavily),
    ] {
        if let Some(id) = id
            && !doc
                .profiles
                .iter()
                .any(|p| p.profile.id == id && p.profile.kind == kind)
        {
            return Err(configuration());
        }
    }
    Ok(())
}
