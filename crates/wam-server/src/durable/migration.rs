use super::{
    ApplicationRepository,
    state::{AppState, Consent, RETENTION_SECS, Work},
};
use crate::{application::ServiceError, memory_repository::LocalRepository};
use serde::de::DeserializeOwned;
use std::{fs, path::Path};

const LEGACY_FILES: [&str; 5] = [
    "profiles.json",
    "contributions.json",
    "usage.json",
    "scores.json",
    "identity.json",
];

fn read_legacy<T: DeserializeOwned + Default>(
    directory: &Path,
    name: &str,
) -> Result<T, ServiceError> {
    let path = directory.join(name);
    if !path.exists() {
        return Ok(T::default());
    }
    if !fs::symlink_metadata(&path)
        .map_err(|_| ServiceError::Unavailable)?
        .file_type()
        .is_file()
    {
        return Err(ServiceError::Unavailable);
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| ServiceError::Unavailable)?)
        .map_err(|_| ServiceError::Unavailable)
}

impl ApplicationRepository {
    /// Migrates the former five-file development store while holding the new
    /// store's exclusive process lock. Legacy files are removed only after the
    /// complete candidate snapshot has been durably written.
    pub fn open_directory(directory: &Path) -> Result<Self, ServiceError> {
        let repository = Self::open(directory.join("application.json"))?;
        let legacy_exists = LEGACY_FILES
            .iter()
            .any(|name| directory.join(name).exists());
        let imported = repository.read(|state| Ok(state.legacy_imported))?;
        if !imported {
            repository.update(|current| {
                if !current.accounts.is_empty()
                    || !current.contributions.contributions.is_empty()
                    || !current.scores.events.is_empty()
                {
                    return Err(ServiceError::InvalidInput(
                        "legacy_migration_requires_empty_store",
                    ));
                }
                let mut state = AppState::demo();
                if legacy_exists {
                    let profiles = LocalRepository::open(directory.join("profiles.json"))
                        .map_err(|_| ServiceError::Unavailable)?;
                    state.accounts = AppState::from_repository(&profiles).accounts;
                    state.contributions = read_legacy(directory, "contributions.json")?;
                    state.usage = read_legacy(directory, "usage.json")?;
                    state.scores = read_legacy(directory, "scores.json")?;
                    state.identity = read_legacy(directory, "identity.json")?;
                    for entry in &mut state.contributions.contributions {
                        if matches!(
                            entry.record.status.as_str(),
                            "received" | "queued" | "in_review" | "approved"
                        ) {
                            entry.record.status = "consent_required".into();
                        }
                        state.consents.insert(
                            entry.record.id.clone(),
                            Consent {
                                contribution_id: entry.record.id.clone(),
                                version: "local-development-v1".into(),
                                nonce: entry.record.id.clone(),
                                received_at: entry.record.submitted_at,
                                purge_at: entry.record.submitted_at.saturating_add(RETENTION_SECS),
                                withdrawn_at: None,
                                purged_at: None,
                            },
                        );
                        if matches!(entry.record.status.as_str(), "queued" | "in_review") {
                            let id = format!("contribution:{}", entry.record.id);
                            state.work.insert(
                                id.clone(),
                                Work {
                                    id,
                                    contribution_id: Some(entry.record.id.clone()),
                                    memory_id: None,
                                    kind: "contribution".into(),
                                    state: if entry.record.status == "in_review" {
                                        "waiting_review"
                                    } else {
                                        "queued"
                                    }
                                    .into(),
                                    generation: entry.review_generation,
                                    lease_until: None,
                                    holder: None,
                                    enqueued_at: entry.record.submitted_at,
                                    input_digest: None,
                                    not_before: 0,
                                    attempts: 0,
                                },
                            );
                        }
                    }
                    // The old demo catalog used the short, non-global version
                    // id "v1"; preserve attribution while assigning a global id.
                    for receipt in &mut state.usage.receipts {
                        for memory in &mut receipt.memory_uses {
                            if memory.memory_id == "web:example-draft" && memory.version_id == "v1"
                            {
                                memory.version_id = super::version_id("web:example-draft", 1);
                            }
                        }
                        for version in &mut receipt.used_version_ids {
                            if version == "v1" {
                                *version = super::version_id("web:example-draft", 1);
                            }
                        }
                    }
                    for event in &mut state.scores.events {
                        if event.memory_id == "web:example-draft" && event.memory_version_id == "v1"
                        {
                            event.memory_version_id = super::version_id("web:example-draft", 1);
                            if let Some(prefix) = event.idempotency_key.strip_suffix(":v1") {
                                event.idempotency_key =
                                    format!("{prefix}:{}", event.memory_version_id);
                            }
                        }
                    }
                }
                state.legacy_imported = true;
                for account in state.accounts.values() {
                    account.snapshot()?;
                }
                *current = state;
                Ok(())
            })?;
        }
        if legacy_exists {
            // A retry after a crash continues cleanup from the committed marker.
            for name in LEGACY_FILES {
                let path = directory.join(name);
                if path.exists() {
                    fs::remove_file(path).map_err(|_| ServiceError::Unavailable)?;
                }
            }
        }
        Ok(repository)
    }
}
