//! Privacy-preserving local usage counters and owner-bound recall receipts.

use crate::ports::{
    OutcomeReportResult, ReceiptMemoryAttribution, ReceiptOutcome, UsageStore, UsageStoreError,
};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use wam_contracts::{MemoryOutcome, OutcomeReportRequest, PublicUsageMetrics, SCHEMA_VERSION};

const RECEIPT_TTL_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredRecallReceipt {
    pub(crate) id: String,
    pub(crate) owner_account: String,
    pub(crate) memory_ids: Vec<String>,
    #[serde(default)]
    pub(crate) memory_uses: Vec<ReceiptMemoryAttribution>,
    pub(crate) issued_at: u64,
    pub(crate) outcome: Option<MemoryOutcome>,
    #[serde(default)]
    pub(crate) used_version_ids: Vec<String>,
    pub(crate) reported_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct UsageState {
    pub(crate) schema_version: u8,
    pub(crate) next_receipt_id: u64,
    pub(crate) recall_requests: u64,
    pub(crate) authenticated_recall_requests: u64,
    pub(crate) anonymous_recall_requests: u64,
    pub(crate) matched_memory_hits: u64,
    pub(crate) detail_requests: u64,
    pub(crate) success_outcomes: u64,
    pub(crate) partial_outcomes: u64,
    pub(crate) failed_outcomes: u64,
    pub(crate) receipts: Vec<StoredRecallReceipt>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_receipt_id: 1,
            recall_requests: 0,
            authenticated_recall_requests: 0,
            anonymous_recall_requests: 0,
            matched_memory_hits: 0,
            detail_requests: 0,
            success_outcomes: 0,
            partial_outcomes: 0,
            failed_outcomes: 0,
            receipts: Vec::new(),
        }
    }
}

fn prune_expired(state: &mut UsageState, now: u64) {
    state
        .receipts
        .retain(|receipt| now.saturating_sub(receipt.issued_at) <= RECEIPT_TTL_SECS);
}

pub(crate) fn record_recall(
    state: &mut UsageState,
    account_id: Option<&str>,
    memories: &[ReceiptMemoryAttribution],
    pub(crate) issued_at: u64,
) -> Result<Option<String>, UsageStoreError> {
    let receipt = match account_id.filter(|_| !memories.is_empty()) {
        Some(account_id) => {
            let next = state
                .next_receipt_id
                .checked_add(1)
                .ok_or_else(|| UsageStoreError("recall receipt id space exhausted".into()))?;
            Some((
                format!("local-use-{:06}", state.next_receipt_id),
                account_id,
                next,
            ))
        }
        None => None,
    };
    state.recall_requests = state.recall_requests.saturating_add(1);
    if account_id.is_some() {
        state.authenticated_recall_requests = state.authenticated_recall_requests.saturating_add(1);
    } else {
        state.anonymous_recall_requests = state.anonymous_recall_requests.saturating_add(1);
    }
    state.matched_memory_hits = state
        .matched_memory_hits
        .saturating_add(memories.len() as u64);
    prune_expired(state, issued_at);
    if let Some((id, account_id, next)) = receipt {
        state.next_receipt_id = next;
        state.receipts.push(StoredRecallReceipt {
            id: id.clone(),
            owner_account: account_id.to_owned(),
            memory_ids: memories
                .iter()
                .map(|memory| memory.memory_id.clone())
                .collect(),
            memory_uses: memories.to_vec(),
            issued_at,
            outcome: None,
            used_version_ids: Vec::new(),
            reported_at: None,
        });
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

fn receipt_outcome(receipt: &StoredRecallReceipt) -> ReceiptOutcome {
    ReceiptOutcome {
        used_memories: receipt
            .memory_uses
            .iter()
            .filter(|memory| receipt.used_version_ids.contains(&memory.version_id))
            .cloned()
            .collect(),
        reported_at: receipt.reported_at.unwrap_or(receipt.issued_at),
    }
}

pub(crate) fn report_outcome(
    state: &mut UsageState,
    account_id: &str,
    receipt_id: &str,
    request: &OutcomeReportRequest,
    reported_at: u64,
) -> OutcomeReportResult {
    prune_expired(state, reported_at);
    let Some(receipt) = state
        .receipts
        .iter_mut()
        .find(|receipt| receipt.id == receipt_id && receipt.owner_account == account_id)
    else {
        return OutcomeReportResult::NotFound;
    };
    let mut used_version_ids = request.used_version_ids.clone();
    used_version_ids.sort();
    if used_version_ids.len() > 3
        || used_version_ids.windows(2).any(|pair| pair[0] == pair[1])
        || used_version_ids.iter().any(|version| {
            !receipt
                .memory_uses
                .iter()
                .any(|memory| &memory.version_id == version)
        })
    {
        return OutcomeReportResult::InvalidVersions;
    }
    if let Some(previous) = receipt.outcome {
        return if previous == request.outcome && receipt.used_version_ids == used_version_ids {
            OutcomeReportResult::Duplicate(receipt_outcome(receipt))
        } else {
            OutcomeReportResult::Conflict
        };
    }
    receipt.outcome = Some(request.outcome);
    receipt.used_version_ids = used_version_ids;
    receipt.reported_at = Some(reported_at);
    let result = OutcomeReportResult::Recorded(receipt_outcome(receipt));
    match request.outcome {
        MemoryOutcome::Success => state.success_outcomes = state.success_outcomes.saturating_add(1),
        MemoryOutcome::Partial => state.partial_outcomes = state.partial_outcomes.saturating_add(1),
        MemoryOutcome::Failed => state.failed_outcomes = state.failed_outcomes.saturating_add(1),
    }
    result
}

pub(crate) fn public_metrics(state: &UsageState) -> PublicUsageMetrics {
    PublicUsageMetrics {
        schema_version: SCHEMA_VERSION,
        recall_requests: state.recall_requests,
        authenticated_recall_requests: state.authenticated_recall_requests,
        anonymous_recall_requests: state.anonymous_recall_requests,
        matched_memory_hits: state.matched_memory_hits,
        detail_requests: state.detail_requests,
        success_outcomes: state.success_outcomes,
        partial_outcomes: state.partial_outcomes,
        failed_outcomes: state.failed_outcomes,
    }
}

#[derive(Default)]
pub struct InMemoryUsageStore {
    state: Mutex<UsageState>,
}

impl UsageStore for InMemoryUsageStore {
    fn record_recall(
        &self,
        account_id: Option<&str>,
        memories: &[ReceiptMemoryAttribution],
        pub(crate) issued_at: u64,
    ) -> Result<Option<String>, UsageStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
        record_recall(&mut state, account_id, memories, issued_at)
    }

    fn record_detail(&self) -> Result<(), UsageStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
        state.detail_requests = state.detail_requests.saturating_add(1);
        Ok(())
    }

    fn report_outcome(
        &self,
        account_id: &str,
        receipt_id: &str,
        request: &OutcomeReportRequest,
        reported_at: u64,
    ) -> Result<OutcomeReportResult, UsageStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
        Ok(report_outcome(
            &mut state,
            account_id,
            receipt_id,
            request,
            reported_at,
        ))
    }

    fn public_metrics(&self) -> Result<PublicUsageMetrics, UsageStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
        Ok(public_metrics(&state))
    }
}

/// Single-process local adapter. Receipts expire after 30 days; aggregate counters remain.
pub struct FileUsageStore {
    path: PathBuf,
    state: Mutex<UsageState>,
}

impl FileUsageStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, UsageStoreError> {
        let path = path.into();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            create_parent_directories(parent).map_err(storage_error)?;
        }
        let mut state = if path.exists() {
            let metadata = fs::symlink_metadata(&path).map_err(storage_error)?;
            if !metadata.file_type().is_file() {
                return Err(UsageStoreError(
                    "usage store path must be a regular file".into(),
                ));
            }
            #[cfg(unix)]
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(storage_error)?;
            let bytes = fs::read(&path).map_err(storage_error)?;
            let state: UsageState = serde_json::from_slice(&bytes).map_err(storage_error)?;
            if state.schema_version != 1 || state.next_receipt_id == 0 {
                return Err(UsageStoreError("unsupported usage store format".into()));
            }
            state
        } else {
            UsageState::default()
        };
        let old_receipt_count = state.receipts.len();
        prune_expired(&mut state, now_unix());
        let pruned_on_open = state.receipts.len() != old_receipt_count;
        let store = Self {
            path,
            state: Mutex::new(state),
        };
        if pruned_on_open {
            let state = store
                .state
                .lock()
                .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
            store.persist(&state)?;
        }
        Ok(store)
    }

    fn update<T>(
        &self,
        operation: impl FnOnce(&mut UsageState) -> Result<T, UsageStoreError>,
    ) -> Result<T, UsageStoreError> {
        let mut current = self
            .state
            .lock()
            .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
        let mut next = current.clone();
        let result = operation(&mut next)?;
        self.persist(&next)?;
        *current = next;
        Ok(result)
    }

    fn persist(&self, state: &UsageState) -> Result<(), UsageStoreError> {
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

impl UsageStore for FileUsageStore {
    fn record_recall(
        &self,
        account_id: Option<&str>,
        memories: &[ReceiptMemoryAttribution],
        pub(crate) issued_at: u64,
    ) -> Result<Option<String>, UsageStoreError> {
        self.update(|state| record_recall(state, account_id, memories, issued_at))
    }

    fn record_detail(&self) -> Result<(), UsageStoreError> {
        self.update(|state| {
            state.detail_requests = state.detail_requests.saturating_add(1);
            Ok(())
        })
    }

    fn report_outcome(
        &self,
        account_id: &str,
        receipt_id: &str,
        request: &OutcomeReportRequest,
        reported_at: u64,
    ) -> Result<OutcomeReportResult, UsageStoreError> {
        self.update(|state| {
            Ok(report_outcome(
                state,
                account_id,
                receipt_id,
                request,
                reported_at,
            ))
        })
    }

    fn public_metrics(&self) -> Result<PublicUsageMetrics, UsageStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| UsageStoreError("usage store lock poisoned".into()))?;
        Ok(public_metrics(&state))
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("usage.json");
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

fn storage_error(error: impl std::fmt::Display) -> UsageStoreError {
    UsageStoreError(error.to_string())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    fn outcome(value: MemoryOutcome) -> OutcomeReportRequest {
        OutcomeReportRequest {
            outcome: value,
            used_version_ids: Vec::new(),
        }
    }

    fn attribution(memory_id: &str, version_id: &str) -> ReceiptMemoryAttribution {
        ReceiptMemoryAttribution {
            memory_id: memory_id.to_owned(),
            version_id: version_id.to_owned(),
            author_account_id: Some("author-a".to_owned()),
        }
    }

    fn path() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "wam-usage-store-{}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ))
            .join("usage.json")
    }

    #[test]
    fn outcomes_are_owner_bound_idempotent_and_public_metrics_are_aggregate_only() {
        let store = InMemoryUsageStore::default();
        let memories = vec![attribution("web:example-draft", "web:example-draft.v1")];
        let receipt = store
            .record_recall(Some("account-a"), &memories, 100)
            .expect("record authenticated recall")
            .expect("receipt for a hit");
        assert_eq!(
            store
                .report_outcome("account-b", &receipt, &outcome(MemoryOutcome::Success), 101)
                .expect("other account report"),
            OutcomeReportResult::NotFound
        );
        assert_eq!(
            store
                .report_outcome("account-a", &receipt, &outcome(MemoryOutcome::Success), 102)
                .expect("first report"),
            OutcomeReportResult::Recorded(ReceiptOutcome {
                used_memories: Vec::new(),
                reported_at: 102,
            })
        );
        assert_eq!(
            store
                .report_outcome("account-a", &receipt, &outcome(MemoryOutcome::Success), 103)
                .expect("idempotent report"),
            OutcomeReportResult::Duplicate(ReceiptOutcome {
                used_memories: Vec::new(),
                reported_at: 102,
            })
        );
        assert_eq!(
            store
                .report_outcome("account-a", &receipt, &outcome(MemoryOutcome::Failed), 104)
                .expect("conflicting report"),
            OutcomeReportResult::Conflict
        );

        store
            .record_recall(None, &[], 105)
            .expect("anonymous recall");
        let metrics = serde_json::to_string(&store.public_metrics().expect("public metrics"))
            .expect("serialize metrics");
        assert!(metrics.contains("\"success_outcomes\":1"));
        assert!(metrics.contains("\"anonymous_recall_requests\":1"));
        assert!(!metrics.contains("account-a"));
        assert!(!metrics.contains("web:example-draft"));
        assert!(!metrics.contains(&receipt));
    }

    #[test]
    fn legacy_reported_receipts_do_not_assume_any_candidates_were_used() {
        let mut state = UsageState::default();
        let receipt = record_recall(
            &mut state,
            Some("account-a"),
            &[attribution("mem-1", "mem-1.v1")],
            100,
        )
        .unwrap()
        .unwrap();
        let selected = OutcomeReportRequest {
            outcome: MemoryOutcome::Success,
            used_version_ids: vec!["mem-1.v1".to_owned()],
        };
        report_outcome(&mut state, "account-a", &receipt, &selected, 101);
        let mut legacy = serde_json::to_value(state).unwrap();
        legacy["receipts"][0]
            .as_object_mut()
            .unwrap()
            .remove("used_version_ids");
        let mut restored: UsageState = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            report_outcome(
                &mut restored,
                "account-a",
                &receipt,
                &outcome(MemoryOutcome::Success),
                102,
            ),
            OutcomeReportResult::Duplicate(ReceiptOutcome {
                used_memories: Vec::new(),
                reported_at: 101,
            })
        );
        assert_eq!(
            report_outcome(&mut restored, "account-a", &receipt, &selected, 103),
            OutcomeReportResult::Conflict
        );
    }

    #[test]
    fn file_store_preserves_the_used_version_selection_across_restart() {
        let path = path();
        let now = now_unix();
        let selected = OutcomeReportRequest {
            outcome: MemoryOutcome::Success,
            used_version_ids: vec!["mem-2.v1".to_owned()],
        };
        let receipt = {
            let store = FileUsageStore::open(&path).unwrap();
            let receipt = store
                .record_recall(
                    Some("account-a"),
                    &[
                        attribution("mem-1", "mem-1.v1"),
                        attribution("mem-2", "mem-2.v1"),
                    ],
                    now,
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                store
                    .report_outcome("account-a", &receipt, &selected, now)
                    .unwrap(),
                OutcomeReportResult::Recorded(ReceiptOutcome {
                    used_memories: vec![attribution("mem-2", "mem-2.v1")],
                    reported_at: now,
                })
            );
            receipt
        };
        let reopened = FileUsageStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .report_outcome("account-a", &receipt, &selected, now + 1)
                .unwrap(),
            OutcomeReportResult::Duplicate(ReceiptOutcome {
                used_memories: vec![attribution("mem-2", "mem-2.v1")],
                reported_at: now,
            })
        );
        assert_eq!(
            reopened
                .report_outcome(
                    "account-a",
                    &receipt,
                    &outcome(MemoryOutcome::Success),
                    now + 2,
                )
                .unwrap(),
            OutcomeReportResult::Conflict
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_store_persists_receipt_outcomes_but_expires_receipt_payload() {
        let path = path();
        let now = now_unix();
        let receipt = {
            let store = FileUsageStore::open(&path).expect("open usage store");
            store
                .record_recall(Some("account-a"), &[attribution("mem-1", "mem-1.v1")], now)
                .expect("record recall")
                .expect("receipt")
        };
        let reopened = FileUsageStore::open(&path).expect("reopen usage store");
        assert_eq!(
            reopened
                .report_outcome(
                    "account-a",
                    &receipt,
                    &outcome(MemoryOutcome::Partial),
                    now + 1
                )
                .expect("report outcome"),
            OutcomeReportResult::Recorded(ReceiptOutcome {
                used_memories: Vec::new(),
                reported_at: now + 1,
            })
        );
        assert_eq!(
            reopened
                .public_metrics()
                .expect("read metrics")
                .partial_outcomes,
            1
        );
        assert_eq!(
            reopened
                .report_outcome(
                    "account-a",
                    &receipt,
                    &outcome(MemoryOutcome::Partial),
                    now + RECEIPT_TTL_SECS + 1
                )
                .expect("expired report"),
            OutcomeReportResult::NotFound
        );
        let _ = fs::remove_dir_all(path.parent().expect("temp directory"));
    }
}
