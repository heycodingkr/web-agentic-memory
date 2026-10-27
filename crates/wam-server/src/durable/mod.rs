//! Transactional application storage. Local development and PostgreSQL share the
//! same state transitions; publication, consent, usage, and score changes commit together.

mod adapters;
mod lifecycle;
mod postgres;
mod state;
pub mod worker;

pub use state::{OutboxEvent, version_id};
pub use worker::{CommandReplayVerifier, ReplayEvidence, ReplayRequest, ReplayVerifier, WorkResult};

use crate::{application::ServiceError, identity::{IdentityError, store::IdentityState}};
use state::AppState;
use std::{fs, io::Write, path::{Path, PathBuf}, sync::{Arc, Mutex}};

#[derive(Clone)]
pub struct ApplicationRepository {
    backend: Arc<Backend>,
}

enum Backend {
    Local { path: Option<PathBuf>, state: Mutex<AppState> },
    Postgres(postgres::PostgresBackend),
}

impl ApplicationRepository {
    pub fn in_memory() -> Self {
        Self { backend: Arc::new(Backend::Local { path: None, state: Mutex::new(AppState::default()) }) }
    }

    pub fn demo() -> Self {
        Self { backend: Arc::new(Backend::Local { path: None, state: Mutex::new(AppState::demo()) }) }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ServiceError> {
        let path = path.into();
        let state = if path.exists() {
            let metadata = fs::symlink_metadata(&path).map_err(|_| ServiceError::Unavailable)?;
            if !metadata.file_type().is_file() { return Err(ServiceError::Unavailable); }
            serde_json::from_slice::<AppState>(&fs::read(&path).map_err(|_| ServiceError::Unavailable)?)
                .map_err(|_| ServiceError::Unavailable)?
        } else { AppState::default() };
        if state.schema_version != 1 { return Err(ServiceError::Unavailable); }
        for account in state.accounts.values() { account.snapshot()?; }
        let repository = Self { backend: Arc::new(Backend::Local { path: Some(path), state: Mutex::new(state) }) };
        repository.update(|_| Ok(()))?;
        Ok(repository)
    }

    /// The blocking PostgreSQL driver lives on a dedicated thread, including when
    /// called by an async HTTP/MCP handler. URLs are never included in errors/logs.
    pub fn connect(database_url: &str) -> Result<Self, ServiceError> {
        Ok(Self { backend: Arc::new(Backend::Postgres(postgres::PostgresBackend::connect(database_url)?)) })
    }

    pub(crate) fn read<T>(&self, operation: impl FnOnce(&AppState) -> Result<T, ServiceError>) -> Result<T, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { state, .. } => operation(&state.lock().map_err(|_| ServiceError::Unavailable)?),
            Backend::Postgres(backend) => backend.read(operation),
        }
    }

    pub(crate) fn update<T>(&self, operation: impl FnOnce(&mut AppState) -> Result<T, ServiceError>) -> Result<T, ServiceError> {
        match self.backend.as_ref() {
            Backend::Local { path, state } => {
                let mut current = state.lock().map_err(|_| ServiceError::Unavailable)?;
                let mut candidate = current.clone();
                let result = operation(&mut candidate)?;
                if let Some(path) = path { persist(path, &candidate)?; }
                *current = candidate;
                Ok(result)
            }
            Backend::Postgres(backend) => backend.update(operation),
        }
    }

    pub(crate) fn identity_read<T>(&self, operation: impl FnOnce(&IdentityState) -> T) -> Result<T, ServiceError> {
        self.read(|state| Ok(operation(&state.identity)))
    }

    pub(crate) fn identity_update<T>(&self, operation: impl FnOnce(&mut IdentityState) -> Result<T, IdentityError>) -> Result<T, IdentityError> {
        // One-time OAuth failures and approval rate limits intentionally commit
        // their consumed nonce/attempt before returning the domain error.
        self.update(|state| Ok(operation(&mut state.identity))).map_err(|_| IdentityError::Storage)?
    }

    pub fn pending_outbox(&self, limit: usize) -> Result<Vec<OutboxEvent>, ServiceError> {
        if !(1..=1000).contains(&limit) { return Err(ServiceError::InvalidInput("invalid_limit")); }
        self.read(|state| Ok(state.outbox.iter().filter(|event| event.delivered_at.is_none()).take(limit).cloned().collect()))
    }

    pub fn acknowledge_outbox(&self, id: &str, now: u64) -> Result<bool, ServiceError> {
        self.update(|state| {
            let Some(event) = state.outbox.iter_mut().find(|event| event.id == id) else { return Ok(false) };
            event.delivered_at.get_or_insert(now);
            Ok(true)
        })
    }
}

fn persist(path: &Path, state: &AppState) -> Result<(), ServiceError> {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        if !parent.exists() {
            #[cfg(unix)] {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new().recursive(true).mode(0o700).create(parent).map_err(|_| ServiceError::Unavailable)?;
            }
            #[cfg(not(unix))] fs::create_dir_all(parent).map_err(|_| ServiceError::Unavailable)?;
        }
    }
    let temporary = path.with_extension(format!("{}.{}.tmp", std::process::id(), crate::identity::random_secret().map_err(|_| ServiceError::Unavailable)?));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|_| ServiceError::Unavailable)?;
        let bytes = serde_json::to_vec(state).map_err(|_| ServiceError::Unavailable)?;
        file.write_all(&bytes).map_err(|_| ServiceError::Unavailable)?;
        file.sync_all().map_err(|_| ServiceError::Unavailable)?;
        fs::rename(&temporary, path).map_err(|_| ServiceError::Unavailable)?;
        if let Some(parent) = parent {
            fs::File::open(parent).and_then(|directory| directory.sync_all()).map_err(|_| ServiceError::Unavailable)?;
        }
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(temporary); }
    result
}
