//! Append-only score event stores for the local server adapter.

use crate::ports::{ScoreAwardResult, ScoreStore, ScoreStoreError, SuccessfulUseAward};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use wam_contracts::{SCHEMA_VERSION, ScoreEventListResponse, ScoreEventSummary};

pub const SUCCESSFUL_USE_POINTS: i64 = 2;
const DAY_SECS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ScoreEvent {
    pub(crate) id: String,
    pub(crate) beneficiary_account_id: String,
    pub(crate) consumer_account_id: String,
    pub(crate) memory_id: String,
    pub(crate) memory_version_id: String,
    pub(crate) source_receipt_id: String,
    pub(crate) event_type: String,
    pub(crate) points_delta: i64,
    pub(crate) idempotency_key: String,
    pub(crate) created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ScoreState {
    pub(crate) schema_version: u8,
    pub(crate) next_event_id: u64,
    pub(crate) events: Vec<ScoreEvent>,
}

impl Default for ScoreState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_event_id: 1,
            events: Vec::new(),
        }
    }
}

fn idempotency_key(award: &SuccessfulUseAward) -> String {
    format!(
        "successful-use:{}:{}",
        award.receipt_id, award.memory_version_id
    )
}

pub(crate) fn record_successful_use(
    state: &mut ScoreState,
    award: SuccessfulUseAward,
) -> Result<ScoreAwardResult, ScoreStoreError> {
    if award.beneficiary_account_id == award.consumer_account_id {
        return Ok(ScoreAwardResult::SelfUse);
    }

    let key = idempotency_key(&award);
    if state
        .events
        .iter()
        .any(|event| event.idempotency_key == key)
    {
        return Ok(ScoreAwardResult::Duplicate);
    }

    let report_day = award.created_at / DAY_SECS;
    if state.events.iter().any(|event| {
        event.event_type == "successful_use"
            && event.consumer_account_id == award.consumer_account_id
            && event.created_at / DAY_SECS == report_day
            && (event.memory_version_id == award.memory_version_id
                || event.beneficiary_account_id == award.beneficiary_account_id)
    }) {
        return Ok(ScoreAwardResult::DailyLimit);
    }

    let next = state
        .next_event_id
        .checked_add(1)
        .ok_or_else(|| ScoreStoreError("score event id space exhausted".into()))?;
    state.events.push(ScoreEvent {
        id: format!("score-{:08}", state.next_event_id),
        beneficiary_account_id: award.beneficiary_account_id,
        consumer_account_id: award.consumer_account_id,
        memory_id: award.memory_id,
        memory_version_id: award.memory_version_id,
        source_receipt_id: award.receipt_id,
        event_type: "successful_use".to_owned(),
        points_delta: SUCCESSFUL_USE_POINTS,
        idempotency_key: key,
        created_at: award.created_at,
    });
    state.next_event_id = next;
    Ok(ScoreAwardResult::Recorded)
}

pub(crate) fn total_points_for(state: &ScoreState, account_id: &str) -> i64 {
    state
        .events
        .iter()
        .filter(|event| event.beneficiary_account_id == account_id)
        .fold(0_i64, |total, event| {
            total.saturating_add(event.points_delta)
        })
}

pub(crate) fn events_for_account(
    state: &ScoreState,
    account_id: &str,
    before: Option<&str>,
    limit: usize,
) -> Result<Option<ScoreEventListResponse>, ScoreStoreError> {
    if !(1..=100).contains(&limit) {
        return Err(ScoreStoreError("invalid score history page size".into()));
    }
    let mut owned = state
        .events
        .iter()
        .rev()
        .filter(|event| event.beneficiary_account_id == account_id);
    if let Some(cursor) = before
        && !owned.by_ref().any(|event| event.id == cursor)
    {
        return Ok(None);
    }
    let events: Vec<_> = owned
        .by_ref()
        .take(limit)
        .map(|event| ScoreEventSummary {
            id: event.id.clone(),
            event_type: event.event_type.clone(),
            points_delta: event.points_delta,
            memory_id: event.memory_id.clone(),
            memory_version_id: event.memory_version_id.clone(),
            created_at: event.created_at,
        })
        .collect();
    let next_cursor = if owned.next().is_some() {
        events.last().map(|event| event.id.clone())
    } else {
        None
    };
    Ok(Some(ScoreEventListResponse {
        schema_version: SCHEMA_VERSION,
        events,
        next_cursor,
    }))
}

#[derive(Default)]
pub struct InMemoryScoreStore {
    state: Mutex<ScoreState>,
}

impl ScoreStore for InMemoryScoreStore {
    fn award_successful_use(
        &self,
        award: SuccessfulUseAward,
    ) -> Result<ScoreAwardResult, ScoreStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ScoreStoreError("score store lock poisoned".into()))?;
        record_successful_use(&mut state, award)
    }

    fn total_points_for(&self, account_id: &str) -> Result<i64, ScoreStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ScoreStoreError("score store lock poisoned".into()))?;
        Ok(total_points_for(&state, account_id))
    }

    fn events_for_account(
        &self,
        account_id: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Option<ScoreEventListResponse>, ScoreStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ScoreStoreError("score store lock poisoned".into()))?;
        events_for_account(&state, account_id, before, limit)
    }
}

/// Single-process durable demo adapter. Production deployments should use a
/// PostgreSQL transaction with a unique idempotency key and daily quota constraint.
pub struct FileScoreStore {
    path: PathBuf,
    state: Mutex<ScoreState>,
}

impl FileScoreStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ScoreStoreError> {
        let path = path.into();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            create_parent_directories(parent).map_err(storage_error)?;
        }
        let state = if path.exists() {
            let metadata = fs::symlink_metadata(&path).map_err(storage_error)?;
            if !metadata.file_type().is_file() {
                return Err(ScoreStoreError(
                    "score store path must be a regular file".into(),
                ));
            }
            #[cfg(unix)]
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(storage_error)?;
            let bytes = fs::read(&path).map_err(storage_error)?;
            let state: ScoreState = serde_json::from_slice(&bytes).map_err(storage_error)?;
            if state.schema_version != 1 || state.next_event_id == 0 {
                return Err(ScoreStoreError("unsupported score store format".into()));
            }
            state
        } else {
            ScoreState::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn update<T>(
        &self,
        operation: impl FnOnce(&mut ScoreState) -> Result<T, ScoreStoreError>,
    ) -> Result<T, ScoreStoreError> {
        let mut current = self
            .state
            .lock()
            .map_err(|_| ScoreStoreError("score store lock poisoned".into()))?;
        let mut next = current.clone();
        let result = operation(&mut next)?;
        self.persist(&next)?;
        *current = next;
        Ok(result)
    }

    fn persist(&self, state: &ScoreState) -> Result<(), ScoreStoreError> {
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

impl ScoreStore for FileScoreStore {
    fn award_successful_use(
        &self,
        award: SuccessfulUseAward,
    ) -> Result<ScoreAwardResult, ScoreStoreError> {
        self.update(|state| record_successful_use(state, award))
    }

    fn total_points_for(&self, account_id: &str) -> Result<i64, ScoreStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ScoreStoreError("score store lock poisoned".into()))?;
        Ok(total_points_for(&state, account_id))
    }

    fn events_for_account(
        &self,
        account_id: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Option<ScoreEventListResponse>, ScoreStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ScoreStoreError("score store lock poisoned".into()))?;
        events_for_account(&state, account_id, before, limit)
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("scores.json");
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

fn storage_error(error: impl std::fmt::Display) -> ScoreStoreError {
    ScoreStoreError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    fn path() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "wam-score-store-{}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ))
            .join("scores.json")
    }

    fn award(receipt_id: &str, consumer: &str, created_at: u64) -> SuccessfulUseAward {
        SuccessfulUseAward {
            beneficiary_account_id: "author-a".to_owned(),
            consumer_account_id: consumer.to_owned(),
            memory_id: "memory-a".to_owned(),
            memory_version_id: "memory-a.v1".to_owned(),
            receipt_id: receipt_id.to_owned(),
            created_at,
        }
    }

    #[test]
    fn use_awards_are_author_bound_idempotent_and_limited_per_consumer_version_and_author_day() {
        let store = InMemoryScoreStore::default();
        assert_eq!(
            store
                .award_successful_use(award("receipt-1", "consumer-a", 10))
                .unwrap(),
            ScoreAwardResult::Recorded
        );
        assert_eq!(
            store
                .award_successful_use(award("receipt-1", "consumer-a", 11))
                .unwrap(),
            ScoreAwardResult::Duplicate
        );
        assert_eq!(
            store
                .award_successful_use(award("receipt-2", "consumer-a", 12))
                .unwrap(),
            ScoreAwardResult::DailyLimit
        );
        assert_eq!(
            store
                .award_successful_use(award("receipt-3", "consumer-a", DAY_SECS))
                .unwrap(),
            ScoreAwardResult::Recorded
        );
        let mut another_version = award("receipt-4", "consumer-a", DAY_SECS + 1);
        another_version.memory_version_id = "memory-a.v2".to_owned();
        assert_eq!(
            store.award_successful_use(another_version).unwrap(),
            ScoreAwardResult::DailyLimit
        );
        let mut another_author = award("receipt-5", "consumer-a", DAY_SECS + 2);
        another_author.beneficiary_account_id = "author-b".to_owned();
        another_author.memory_id = "memory-b".to_owned();
        another_author.memory_version_id = "memory-b.v1".to_owned();
        assert_eq!(
            store.award_successful_use(another_author).unwrap(),
            ScoreAwardResult::Recorded
        );
        let mut self_use = award("receipt-6", "author-a", DAY_SECS);
        self_use.beneficiary_account_id = "author-a".to_owned();
        assert_eq!(
            store.award_successful_use(self_use).unwrap(),
            ScoreAwardResult::SelfUse
        );
        assert_eq!(store.total_points_for("author-a").unwrap(), 4);
        assert_eq!(store.total_points_for("author-b").unwrap(), 2);
    }

    #[test]
    fn file_store_keeps_idempotent_score_events_across_reopen() {
        let path = path();
        {
            let store = FileScoreStore::open(&path).unwrap();
            assert_eq!(
                store
                    .award_successful_use(award("receipt-1", "consumer-a", 100))
                    .unwrap(),
                ScoreAwardResult::Recorded
            );
        }
        let reopened = FileScoreStore::open(&path).unwrap();
        assert_eq!(reopened.total_points_for("author-a").unwrap(), 2);
        assert_eq!(
            reopened
                .award_successful_use(award("receipt-1", "consumer-a", 101))
                .unwrap(),
            ScoreAwardResult::Duplicate
        );
        assert_eq!(reopened.total_points_for("author-a").unwrap(), 2);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
