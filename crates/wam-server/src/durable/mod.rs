//! Transactional application storage. Local development and PostgreSQL share the
//! same state transitions; publication, consent, usage, and score changes commit together.

mod adapters;
mod cleanup;
mod lifecycle;
mod migration;
mod postgres;
mod rollback;
pub mod runtime;
mod state;
pub mod worker;

pub use lifecycle::PurgeReport;
pub use state::{OutboxEvent, version_id};
pub use worker::{
    CommandReplayVerifier, ReplayEvidence, ReplayRequest, ReplayVerifier, WorkResult,
};

use crate::ports::{
    AuthoredMemoryVersion, MemoryRecord, MemoryUsageAggregate, ReceiptMemoryAttribution,
    UsageStoreError,
};
use crate::{
    application::ServiceError,
    identity::{IdentityError, store::IdentityState},
};
use state::AppState;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use wam_core::{
    keyword::{KeywordVocabulary, TaskVocabulary},
    recall::RecallQuery,
};

#[derive(Clone)]
pub struct ApplicationRepository {
    backend: Arc<Backend>,
}

enum Backend {
    Local {
        path: Option<PathBuf>,
        state: Box<Mutex<AppState>>,
        _lock: Option<fs::File>,
    },
    Postgres(postgres::PostgresBackend),
}

impl ApplicationRepository {
    pub(crate) fn indexed_memory_by_id(
        &self,
        id: &str,
        tasks: &TaskVocabulary,
    ) -> Result<Option<MemoryRecord>, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { .. } => Ok(None),
            Backend::Postgres(backend) => backend.memory_by_id(id.to_owned(), tasks.clone()),
        }
    }

    pub(crate) fn indexed_memory_by_version(
        &self,
        id: &str,
        version_id: &str,
        tasks: &TaskVocabulary,
    ) -> Result<Option<MemoryRecord>, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { .. } => Ok(None),
            Backend::Postgres(backend) => {
                backend.memory_by_version(id.to_owned(), version_id.to_owned(), tasks.clone())
            }
        }
    }

    pub(crate) fn direct_record_memory_detail(
        &self,
        memory: &ReceiptMemoryAttribution,
        account_id: Option<&str>,
        viewed_at: u64,
    ) -> Option<Result<(), UsageStoreError>> {
        match self.backend.as_ref() {
            Backend::Local { .. } => None,
            Backend::Postgres(backend) => Some(
                backend
                    .record_memory_detail(memory.clone(), account_id.map(str::to_owned), viewed_at)
                    .map_err(|_| UsageStoreError("usage storage unavailable".into())),
            ),
        }
    }

    pub(crate) fn indexed_authored_memory_versions(
        &self,
        account_id: &str,
    ) -> Option<Result<Vec<AuthoredMemoryVersion>, ServiceError>> {
        match self.backend.as_ref() {
            Backend::Local { .. } => None,
            Backend::Postgres(backend) => Some(backend.authored_memory_versions(account_id.into())),
        }
    }

    pub(crate) fn direct_memory_usage(
        &self,
        version_ids: &[String],
        from_date: &str,
        to_date: &str,
    ) -> Option<Result<Vec<MemoryUsageAggregate>, UsageStoreError>> {
        match self.backend.as_ref() {
            Backend::Local { .. } => None,
            Backend::Postgres(backend) => Some(
                backend
                    .memory_usage(version_ids.to_vec(), from_date.into(), to_date.into())
                    .map_err(|_| UsageStoreError("usage storage unavailable".into())),
            ),
        }
    }

    pub(crate) fn direct_public_metrics(
        &self,
    ) -> Option<Result<wam_contracts::PublicUsageMetrics, UsageStoreError>> {
        match self.backend.as_ref() {
            Backend::Local { .. } => None,
            Backend::Postgres(backend) => Some(
                backend
                    .public_metrics()
                    .map_err(|_| UsageStoreError("usage storage unavailable".into())),
            ),
        }
    }

    pub(crate) fn direct_record_recall(
        &self,
        account_id: Option<&str>,
        memories: &[ReceiptMemoryAttribution],
        issued_at: u64,
    ) -> Option<Result<Option<String>, UsageStoreError>> {
        match self.backend.as_ref() {
            Backend::Local { .. } => None,
            Backend::Postgres(backend) => Some(
                backend
                    .record_recall(account_id.map(str::to_owned), memories.to_vec(), issued_at)
                    .map_err(|_| UsageStoreError("usage storage unavailable".into())),
            ),
        }
    }

    pub(crate) fn indexed_recall_candidates(
        &self,
        query: &RecallQuery,
        tasks: &TaskVocabulary,
    ) -> Result<Option<Vec<MemoryRecord>>, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { .. } => Ok(None),
            Backend::Postgres(backend) => backend
                .recall_candidates(query.clone(), tasks.clone())
                .map(Some),
        }
    }

    pub(crate) fn indexed_keyword_vocabulary(
        &self,
    ) -> Result<Option<KeywordVocabulary>, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { .. } => Ok(None),
            Backend::Postgres(backend) => backend.keyword_vocabulary().map(Some),
        }
    }

    pub fn in_memory() -> Self {
        Self {
            backend: Arc::new(Backend::Local {
                path: None,
                state: Box::new(Mutex::new(AppState::default())),
                _lock: None,
            }),
        }
    }

    pub fn demo() -> Self {
        Self {
            backend: Arc::new(Backend::Local {
                path: None,
                state: Box::new(Mutex::new(AppState::demo())),
                _lock: None,
            }),
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ServiceError> {
        let path = path.into();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            if !parent.exists() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    fs::DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(parent)
                        .map_err(|_| ServiceError::Unavailable)?;
                }
                #[cfg(not(unix))]
                fs::create_dir_all(parent).map_err(|_| ServiceError::Unavailable)?;
            }
        }
        let lock_path = path.with_extension("lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|meta| !meta.file_type().is_file()) {
            return Err(ServiceError::Unavailable);
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options
            .open(lock_path)
            .map_err(|_| ServiceError::Unavailable)?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| ServiceError::Unavailable)?;
        let state = if path.exists() {
            let metadata = fs::symlink_metadata(&path).map_err(|_| ServiceError::Unavailable)?;
            if !metadata.file_type().is_file() {
                return Err(ServiceError::Unavailable);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                    .map_err(|_| ServiceError::Unavailable)?;
            }
            serde_json::from_slice::<AppState>(
                &fs::read(&path).map_err(|_| ServiceError::Unavailable)?,
            )
            .map_err(|_| ServiceError::Unavailable)?
        } else {
            AppState::default()
        };
        if state.schema_version != 1 {
            return Err(ServiceError::Unavailable);
        }
        for account in state.accounts.values() {
            account.snapshot()?;
        }
        if !path.exists() {
            persist(&path, &state)?;
        }
        Ok(Self {
            backend: Arc::new(Backend::Local {
                path: Some(path),
                state: Box::new(Mutex::new(state)),
                _lock: Some(lock),
            }),
        })
    }

    /// The blocking PostgreSQL driver lives on a dedicated thread, including when
    /// called by an async HTTP/MCP handler. URLs are never included in errors/logs.
    pub fn connect(database_url: &str) -> Result<Self, ServiceError> {
        Ok(Self {
            backend: Arc::new(Backend::Postgres(postgres::PostgresBackend::connect(
                database_url,
            )?)),
        })
    }

    pub(crate) fn read<T>(
        &self,
        operation: impl FnOnce(&AppState) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { state, .. } => {
                let state = state.lock().map_err(|_| ServiceError::Unavailable)?;
                operation(&state)
            }
            Backend::Postgres(backend) => backend.read(operation),
        }
    }

    pub(crate) fn update<T>(
        &self,
        operation: impl FnOnce(&mut AppState) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { path, state, .. } => {
                let mut current = state.lock().map_err(|_| ServiceError::Unavailable)?;
                let mut candidate = current.clone();
                let result = operation(&mut candidate)?;
                if let Some(path) = path {
                    if serde_json::to_value(&candidate).map_err(|_| ServiceError::Unavailable)?
                        != serde_json::to_value(&*current).map_err(|_| ServiceError::Unavailable)?
                    {
                        persist(path, &candidate)?;
                    }
                }
                *current = candidate;
                Ok(result)
            }
            Backend::Postgres(backend) => backend.update(operation),
        }
    }

    pub(crate) fn identity_read<T>(
        &self,
        operation: impl FnOnce(&IdentityState) -> T,
    ) -> Result<T, ServiceError> {
        self.read(|state| Ok(operation(&state.identity)))
    }

    pub(crate) fn identity_update<T>(
        &self,
        operation: impl FnOnce(&mut IdentityState) -> Result<T, IdentityError>,
    ) -> Result<T, IdentityError> {
        // One-time OAuth failures and approval rate limits intentionally commit
        // their consumed nonce/attempt before returning the domain error.
        self.update(|state| {
            let result = operation(&mut state.identity);
            let document =
                serde_json::to_value(&state.identity).map_err(|_| ServiceError::Unavailable)?;
            for binding in document["accounts"].as_array().into_iter().flatten() {
                if let Some(account) = binding["account_id"].as_str() {
                    adapters::ensure_account(state, account)?;
                }
            }
            Ok(result)
        })
        .map_err(|_| IdentityError::Storage)?
    }

    pub fn pending_outbox(&self, limit: usize) -> Result<Vec<OutboxEvent>, ServiceError> {
        if !(1..=1000).contains(&limit) {
            return Err(ServiceError::InvalidInput("invalid_limit"));
        }
        self.read(|state| {
            Ok(state
                .outbox
                .iter()
                .filter(|event| event.delivered_at.is_none())
                .take(limit)
                .cloned()
                .collect())
        })
    }

    /// Deletion delivery has its own filter so unrelated pending projections
    /// cannot starve privacy cleanup.
    pub fn pending_account_deletions(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboxEvent>, ServiceError> {
        if !(1..=1000).contains(&limit) {
            return Err(ServiceError::InvalidInput("invalid_limit"));
        }
        self.read(|state| {
            Ok(state
                .outbox
                .iter()
                .filter(|event| event.kind == "account_deleted" && event.delivered_at.is_none())
                .take(limit)
                .cloned()
                .collect())
        })
    }

    pub fn acknowledge_outbox(&self, id: &str, now: u64) -> Result<bool, ServiceError> {
        self.update(|state| {
            let Some(event) = state.outbox.iter_mut().find(|event| event.id == id) else {
                return Ok(false);
            };
            event.delivered_at.get_or_insert(now);
            Ok(true)
        })
    }
}

fn persist(path: &Path, state: &AppState) -> Result<(), ServiceError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        if !parent.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(parent)
                    .map_err(|_| ServiceError::Unavailable)?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(parent).map_err(|_| ServiceError::Unavailable)?;
        }
    }
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        crate::identity::random_secret().map_err(|_| ServiceError::Unavailable)?
    ));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&temporary)
            .map_err(|_| ServiceError::Unavailable)?;
        let bytes = serde_json::to_vec(state).map_err(|_| ServiceError::Unavailable)?;
        file.write_all(&bytes)
            .map_err(|_| ServiceError::Unavailable)?;
        file.sync_all().map_err(|_| ServiceError::Unavailable)?;
        fs::rename(&temporary, path).map_err(|_| ServiceError::Unavailable)?;
        if let Some(parent) = parent {
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| ServiceError::Unavailable)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
