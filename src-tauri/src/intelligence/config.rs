//! Service profiles share runtime config/secret locking but have their own versioned file.
use super::{
    http::{SecretString, validate_secret},
    store::{probe, profile_in_use},
    types::*,
};
use crate::{
    config::{ConfigIo, ConfigTransaction},
    error::{AppError, AppResult},
    storage::LocalStore,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;
mod persistence;
mod validation;
use persistence::*;
#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "Task 3B adapters consume the validated endpoint constructor"
    )
)]
pub(crate) use validation::endpoint;
pub(crate) use validation::validate_profile;

pub struct ServiceConfigStore {
    io: ConfigIo,
    store: Arc<LocalStore>,
    path: PathBuf,
}
impl ServiceConfigStore {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for Task 3B service and scheduler composition"
        )
    )]
    pub(crate) fn new(io: ConfigIo, store: Arc<LocalStore>) -> Self {
        let path = io.data_dir().join("service-configs.json");
        Self { io, store, path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub async fn save(&self, request: SaveModelProfile) -> AppResult<ModelProfileView> {
        let SaveModelProfile {
            mut profile,
            credential,
        } = request;
        if profile.kind == ServiceKind::Tavily && profile.base_url.is_empty() {
            profile.base_url = "https://api.tavily.com/search".into();
        }
        validate_profile(&profile)?;
        if let crate::app::CredentialChange::Replace { api_key } = &credential {
            validate_secret(api_key.as_bytes())?;
        }
        let store = self.store.clone();
        self.io
            .run(move |tx| {
                let mut doc = read(tx)?;
                let id = profile.id;
                let prior = doc.profiles.iter().find(|p| p.profile.id == id);
                if prior.is_none() && doc.profiles.len() >= MAX_PROFILES {
                    return Err(AppError::Intelligence(IntelligenceErrorCode::ResourceLimit));
                }
                let kind_changed = prior.is_some_and(|p| p.profile.kind != profile.kind);
                let changed = prior.is_none_or(|p| p.profile != profile);
                let credential_reference = match &credential {
                    crate::app::CredentialChange::Keep => {
                        prior.filter(|p| p.referenced()).map(|_| reference(id))
                    }
                    crate::app::CredentialChange::Replace { .. } => Some(reference(id)),
                    crate::app::CredentialChange::Clear => None,
                };
                // Config lock spans the use check, file replacement and secret compensation.
                store
                    .transaction(|db| {
                        if kind_changed && (selected(&doc.settings, id) || profile_in_use(db, id)?)
                        {
                            return Err(busy());
                        }
                        if changed {
                            db.execute(
                                "DELETE FROM service_probes WHERE profile_id=?1",
                                [id.to_string()],
                            )?;
                        }
                        Ok(())
                    })
                    .map_err(super::store::safe_store_error)?;
                let saved = StoredProfile {
                    profile,
                    credential_reference,
                };
                doc.profiles.retain(|p| p.profile.id != id);
                doc.profiles.push(saved);
                match credential {
                    crate::app::CredentialChange::Keep => write(tx, &doc)?,
                    crate::app::CredentialChange::Replace { api_key } => {
                        let before = tx.snapshot_secret(SERVICE, &key(id))?;
                        tx.secret_set(SERVICE, &key(id), api_key.as_bytes())?;
                        if write(tx, &doc).is_err() {
                            // Runtime policy: restore the opaque encrypted before-image.
                            // A compensation failure stays local; never include native details.
                            let _ = tx.restore_secret(SERVICE, &key(id), before);
                            return Err(configuration());
                        }
                    }
                    crate::app::CredentialChange::Clear => {
                        write(tx, &doc)?; // Persist unconfigured before attempting orphan cleanup.
                        tx.secret_delete(SERVICE, &key(id))?;
                    }
                }
                view(tx, &store, doc.profiles.last().ok_or_else(configuration)?)
            })
            .await
            .map_err(safe_config_error)
    }
    pub async fn load(&self, id: Uuid) -> AppResult<ModelProfileView> {
        let store = self.store.clone();
        self.io
            .run(move |tx| {
                let doc = read(tx)?;
                let p = doc
                    .profiles
                    .iter()
                    .find(|p| p.profile.id == id)
                    .ok_or_else(configuration)?;
                view(tx, &store, p)
            })
            .await
            .map_err(safe_config_error)
    }
    pub async fn list(&self) -> AppResult<Vec<ModelProfileView>> {
        let store = self.store.clone();
        self.io
            .run(move |tx| {
                let doc = read(tx)?;
                doc.profiles.iter().map(|p| view(tx, &store, p)).collect()
            })
            .await
            .map_err(safe_config_error)
    }
    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let store = self.store.clone();
        self.io
            .run(move |tx| {
                let mut doc = read(tx)?;
                if !doc.profiles.iter().any(|p| p.profile.id == id) {
                    return Err(configuration());
                }
                if selected(&doc.settings, id) {
                    return Err(busy());
                }
                store
                    .transaction(|db| {
                        if profile_in_use(db, id)? {
                            return Err(busy());
                        }
                        db.execute(
                            "DELETE FROM service_probes WHERE profile_id=?1",
                            [id.to_string()],
                        )?;
                        Ok(())
                    })
                    .map_err(super::store::safe_store_error)?;
                doc.profiles.retain(|p| p.profile.id != id);
                write(tx, &doc)?;
                tx.secret_delete(SERVICE, &key(id))?;
                Ok(())
            })
            .await
            .map_err(safe_config_error)
    }
    pub async fn settings(&self) -> AppResult<IntelligenceSettings> {
        self.io
            .run(move |tx| Ok(read(tx)?.settings))
            .await
            .map_err(safe_config_error)
    }
    pub async fn save_settings(
        &self,
        value: IntelligenceSettings,
    ) -> AppResult<IntelligenceSettings> {
        self.io
            .run(move |tx| {
                let mut doc = read(tx)?;
                validate_settings(&doc, &value)?;
                doc.settings = value.clone();
                write(tx, &doc)?;
                Ok(value)
            })
            .await
            .map_err(safe_config_error)
    }
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for Task 3B service and scheduler composition"
        )
    )]
    pub(crate) async fn resolve_credentials(&self, id: Uuid) -> AppResult<SecretString> {
        self.io
            .run(move |tx| {
                let doc = read(tx)?;
                let saved = doc
                    .profiles
                    .iter()
                    .find(|p| p.profile.id == id)
                    .ok_or_else(configuration)?;
                if !saved.referenced() {
                    return Err(configuration());
                }
                let bytes = tx
                    .secret_get(SERVICE, &key(id))?
                    .ok_or_else(configuration)?;
                SecretString::from_store_bytes(bytes)
            })
            .await
            .map_err(safe_config_error)
    }
    /// Backend composition boundary for snapshot + job/result-version admission.
    /// The callback may capture an owned process/admission lease: ConfigIo keeps
    /// it until entered work really finishes even if the async observer goes away.
    /// Never call ConfigIo from this callback or await network while locks are held.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for Task 3B service and scheduler composition"
        )
    )]
    pub(crate) async fn with_profile_snapshot<T: Send + 'static>(
        &self,
        ids: Vec<Uuid>,
        operation: impl FnOnce(&rusqlite::Transaction<'_>, Vec<ModelProfile>) -> AppResult<T>
        + Send
        + 'static,
    ) -> AppResult<T> {
        if ids.len() > 3 {
            return Err(configuration());
        }
        let store = self.store.clone();
        self.io
            .run(move |tx| {
                let doc = read(tx)?;
                let profiles = ids
                    .into_iter()
                    .map(|id| {
                        doc.profiles
                            .iter()
                            .find(|p| p.profile.id == id)
                            .map(|p| p.profile.clone())
                            .ok_or_else(configuration)
                    })
                    .collect::<AppResult<Vec<_>>>()?;
                store
                    .transaction(|db| operation(db, profiles))
                    .map_err(super::store::safe_store_error)
            })
            .await
            .map_err(safe_config_error)
    }
}
fn view(
    tx: &mut ConfigTransaction<'_>,
    store: &LocalStore,
    saved: &StoredProfile,
) -> AppResult<ModelProfileView> {
    let credential_configured =
        saved.referenced() && tx.secret_contains(SERVICE, &key(saved.profile.id))?;
    let last_probe = store
        .transaction(|db| probe(db, saved.profile.id))
        .map_err(super::store::safe_store_error)?;
    Ok(ModelProfileView {
        profile: saved.profile.clone(),
        credential_configured,
        last_probe,
    })
}
pub(crate) fn configuration() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::Configuration)
}
fn busy() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::Busy)
}
fn safe_config_error(error: AppError) -> AppError {
    match error {
        AppError::Intelligence(_) => error,
        _ => configuration(),
    }
}
#[cfg(test)]
mod tests;
