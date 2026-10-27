use super::state::{
    ContributionState, all_audit_records, all_queue_records, all_review_records,
    claim_review_record, create_record, decide_review_record, get_record, list_records,
    withdraw_record,
};
use crate::ports::{
    ContributionStore, ContributionStoreError, OwnedContribution, ReviewAuditRecord,
    ReviewContribution,
};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use wam_contracts::{ContributionAction, ContributionRecordResponse, ContributionReviewDecision};

/// A single-process local store. Writes use fsync + rename so a successful receipt
/// survives server restarts and interrupted writes do not truncate the previous file.
pub struct FileContributionStore {
    path: PathBuf,
    state: Mutex<ContributionState>,
}

impl FileContributionStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ContributionStoreError> {
        let path = path.into();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            create_parent_directories(parent).map_err(storage_error)?;
        }
        let (mut state, mut migrated) = if path.exists() {
            let metadata = fs::symlink_metadata(&path).map_err(storage_error)?;
            if !metadata.file_type().is_file() {
                return Err(ContributionStoreError(
                    "contribution store path must be a regular file".into(),
                ));
            }
            #[cfg(unix)]
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(storage_error)?;
            let bytes = fs::read(&path).map_err(storage_error)?;
            let state: ContributionState = serde_json::from_slice(&bytes).map_err(storage_error)?;
            if !matches!(state.schema_version, 1 | 2) || state.next_id == 0 {
                return Err(ContributionStoreError(
                    "unsupported contribution store format".into(),
                ));
            }
            let migrated = state.schema_version == 1;
            (state, migrated)
        } else {
            (ContributionState::default(), false)
        };
        if state.schema_version == 1 {
            state.schema_version = 2;
            migrated = true;
        }
        for stored in &mut state.contributions {
            if stored.record.status == "received" {
                stored.record.status = "queued".to_owned();
                migrated = true;
            }
        }
        let store = Self {
            path,
            state: Mutex::new(state),
        };
        if migrated {
            let state = store
                .state
                .lock()
                .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
            store.persist(&state)?;
        }
        Ok(store)
    }

    fn update<T>(
        &self,
        operation: impl FnOnce(&mut ContributionState) -> Result<T, ContributionStoreError>,
    ) -> Result<T, ContributionStoreError> {
        let mut current = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        let mut next = current.clone();
        let result = operation(&mut next)?;
        self.persist(&next)?;
        *current = next;
        Ok(result)
    }

    fn persist(&self, state: &ContributionState) -> Result<(), ContributionStoreError> {
        let bytes = serde_json::to_vec(state).map_err(storage_error)?;
        let temp_path = temporary_path(&self.path);
        let mut temp = create_private_file(&temp_path).map_err(storage_error)?;
        let result = (|| {
            temp.write_all(&bytes).map_err(storage_error)?;
            temp.sync_all().map_err(storage_error)?;
            fs::rename(&temp_path, &self.path).map_err(storage_error)?;
            if let Some(parent) = self
                .path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(storage_error)?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

impl ContributionStore for FileContributionStore {
    fn create(
        &self,
        account_id: &str,
        url_pattern: String,
        task_category: String,
        actions: Vec<ContributionAction>,
        submitted_at: u64,
    ) -> Result<ContributionRecordResponse, ContributionStoreError> {
        self.update(|state| {
            create_record(
                state,
                account_id,
                url_pattern,
                task_category,
                actions,
                submitted_at,
            )
        })
    }

    fn list_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<ContributionRecordResponse>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(list_records(&state, account_id))
    }

    fn all_for_queue(&self) -> Result<Vec<OwnedContribution>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(all_queue_records(&state))
    }

    fn all_for_review(&self) -> Result<Vec<ReviewContribution>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(all_review_records(&state))
    }

    fn claim_for_review(
        &self,
        id: &str,
    ) -> Result<Option<ReviewContribution>, ContributionStoreError> {
        self.update(|state| claim_review_record(state, id))
    }

    fn decide_for_review(
        &self,
        id: &str,
        review_generation: u64,
        decision: ContributionReviewDecision,
        reviewer: &str,
        decided_at: u64,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError> {
        self.update(|state| {
            Ok(decide_review_record(
                state,
                id,
                review_generation,
                decision,
                reviewer,
                decided_at,
            ))
        })
    }

    fn review_audit_history(&self) -> Result<Vec<ReviewAuditRecord>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(all_audit_records(&state))
    }

    fn get_for_account(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(get_record(&state, account_id, id))
    }

    fn withdraw_for_account(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<bool, ContributionStoreError> {
        self.update(|state| Ok(withdraw_record(state, account_id, id)))
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("contributions.json");
    path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), nanos))
}

#[cfg(unix)]
fn create_parent_directories(path: &Path) -> std::io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.mode(0o700).recursive(true);
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn create_parent_directories(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn storage_error(error: impl std::fmt::Display) -> ContributionStoreError {
    ContributionStoreError(error.to_string())
}
