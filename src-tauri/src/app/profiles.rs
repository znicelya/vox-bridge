//! Whole-transaction credential policy. No plaintext appears in public values/errors.
use super::AppState;
use crate::{
    config::{ConfigTransaction, ProviderConfig, PublicProviderConfig},
    error::{AppError, AppResult},
    providers::{
        factory::volcengine_secret_reference,
        volc::client::{ENDPOINT, SECRET_SERVICE, secret_key},
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CredentialChange {
    Keep,
    Clear,
    Replace { api_key: String },
}
#[derive(Deserialize)]
pub struct SaveProviderRequest {
    pub config: ProviderConfig,
    pub credential: CredentialChange,
}
#[derive(Serialize)]
pub struct ProviderConfigView {
    pub config: PublicProviderConfig,
    pub credential_configured: bool,
}

fn view(tx: &mut ConfigTransaction<'_>, config: &ProviderConfig) -> AppResult<ProviderConfigView> {
    let credential_configured = config.provider() == "volcengine"
        && config.secret_reference() == Some(volcengine_secret_reference(config.id()).as_str())
        && tx.secret_contains(SECRET_SERVICE, &secret_key(config.id()))?;
    Ok(ProviderConfigView {
        config: config.public_view(),
        credential_configured,
    })
}

impl AppState {
    /// Fresh launches may use only explicitly referenced credentials. Presence
    /// or decryption is deliberately left to the existing connect-time lookup;
    /// an unreferenced cleanup orphan cannot authorize a new session.
    pub(super) async fn load_launch_provider_config(&self, id: Uuid) -> AppResult<ProviderConfig> {
        let owner = self.clone();
        self.config_io
            .run(move |tx| {
                let _owner = owner;
                let saved = tx.load_provider_config(id)?;
                if saved.provider() == "volcengine"
                    && saved.secret_reference()
                        != Some(volcengine_secret_reference(saved.id()).as_str())
                {
                    return Err(AppError::Configuration(
                        "provider credential is not configured".into(),
                    ));
                }
                Ok(saved)
            })
            .await
    }

    pub async fn list_provider_configs(&self) -> AppResult<Vec<ProviderConfigView>> {
        let owner = self.clone();
        self.config_io
            .run(move |tx| {
                let _owner = owner;
                tx.list_provider_configs()?
                    .iter()
                    .map(|config| view(tx, config))
                    .collect()
            })
            .await
    }
    pub async fn save_provider_config(
        &self,
        request: SaveProviderRequest,
    ) -> AppResult<ProviderConfigView> {
        let SaveProviderRequest { config, credential } = request;
        config.validate()?;
        if config.provider() == "volcengine" && config.endpoint() != ENDPOINT {
            return Err(AppError::Configuration(
                "unsupported Volcengine endpoint".into(),
            ));
        }
        if let CredentialChange::Replace { api_key } = &credential {
            if config.provider() != "volcengine"
                || api_key.is_empty()
                || api_key.len() > 4096
                || !api_key.bytes().all(|b| b.is_ascii_graphic())
            {
                return Err(AppError::Configuration(
                    "invalid provider credential".into(),
                ));
            }
        }
        let owner = self.clone();
        self.config_io.run(move |tx| {
            let _owner = owner; // Retain the process lock through native work/compensation.
            let id = config.id();
            let key = secret_key(id);
            let configs = tx.list_provider_configs()?;
            let previous = configs.iter().find(|entry| entry.id() == id);
            // Keep uses persisted same-profile state; untrusted references cannot
            // bind another profile or resurrect an unreferenced cleanup orphan.
            let keep_reference = previous.filter(|entry| entry.provider() == "volcengine"
                && entry.secret_reference() == Some(volcengine_secret_reference(id).as_str()))
                .map(|_| volcengine_secret_reference(id));
            let reference = match &credential {
                CredentialChange::Keep if config.provider() == "volcengine" => keep_reference,
                CredentialChange::Replace { .. } => Some(volcengine_secret_reference(id)),
                _ => None,
            };
            let config = config.with_secret_reference(reference);
            match credential {
                CredentialChange::Keep => tx.save_provider_config(config.clone())?,
                CredentialChange::Replace { api_key } => {
                    let prior = tx.snapshot_secret(SECRET_SERVICE, &key)?;
                    tx.secret_set(SECRET_SERVICE, &key, api_key.as_bytes())?;
                    if tx.save_provider_config(config.clone()).is_err() {
                        if tx.restore_secret(SECRET_SERVICE, &key, prior).is_err() {
                            return Err(AppError::Configuration("profile save failed; credential compensation failed (partial failure)".into()));
                        }
                        return Err(AppError::Configuration("profile save failed; prior credential restored".into()));
                    }
                }
                CredentialChange::Clear => {
                    tx.save_provider_config(config.clone())?;
                    if tx.secret_delete(SECRET_SERVICE, &key).is_err() {
                        return Err(AppError::Configuration("profile saved without credential; credential cleanup failed (partial failure)".into()));
                    }
                }
            }
            view(tx, &config)
        }).await
    }
    pub async fn delete_provider_config(&self, config_id: Uuid) -> AppResult<()> {
        let guard = self.admission.clone().lock_owned().await;
        if self.orchestrator.uses_provider_config(config_id).await {
            return Err(AppError::InvalidRequest(
                "cannot delete an active session's provider profile".into(),
            ));
        }
        let owner = self.clone();
        self.config_io
            .run(move |tx| {
                // ConfigIo may outlive its async caller: keep admission until the
                // entire entered transaction and cleanup have returned.
                let (_guard, _owner) = (guard, owner);
                let config = tx.load_provider_config(config_id)?;
                tx.delete_provider_config(config_id)?;
                if config.provider() == "volcengine"
                    && tx
                        .secret_delete(SECRET_SERVICE, &secret_key(config_id))
                        .is_err()
                {
                    return Err(AppError::Configuration(
                        "profile deleted; credential cleanup failed (partial failure)".into(),
                    ));
                }
                Ok(())
            })
            .await
    }
}
