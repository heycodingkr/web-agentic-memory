//! Small persistent cache shared by the CLI and stdio MCP processes.

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

pub const MAX_ENTRIES: u64 = 2_000;
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheFreshness {
    Fresh,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedValue {
    pub payload: Vec<u8>,
    pub freshness: CacheFreshness,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct LocalCacheStatus {
    pub entries: u64,
    pub bytes: u64,
    pub max_entries: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalCacheError;

impl fmt::Display for LocalCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("local cache is unavailable")
    }
}

impl std::error::Error for LocalCacheError {}

pub struct LocalCache {
    connection: Mutex<Connection>,
    key: [u8; 32],
}

impl LocalCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LocalCacheError> {
        let path = path.as_ref();
        prepare_parent(path.parent().unwrap_or_else(|| Path::new(".")))?;
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(LocalCacheError);
        }
        let cache_existed = path.exists();
        let (key, key_created) = read_or_create_key(&path.with_extension("key"))?;
        create_private_file(path)?;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )
        .map_err(|_| LocalCacheError)?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| LocalCacheError)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 CREATE TABLE IF NOT EXISTS wam_cache_entries (
                    cache_key TEXT PRIMARY KEY,
                    origin_hash TEXT NOT NULL,
                    schema_version INTEGER NOT NULL,
                    stored_at INTEGER NOT NULL,
                    fresh_until INTEGER NOT NULL,
                    stale_until INTEGER NOT NULL,
                    last_access INTEGER NOT NULL,
                    payload BLOB NOT NULL,
                    CHECK(length(payload) <= 262144)
                 );
                 CREATE INDEX IF NOT EXISTS wam_cache_lru
                    ON wam_cache_entries(last_access ASC);",
            )
            .map_err(|_| LocalCacheError)?;
        if key_created && cache_existed {
            connection
                .execute("DELETE FROM wam_cache_entries", [])
                .map_err(|_| LocalCacheError)?;
        }
        set_private_file(path)?;
        Ok(Self {
            connection: Mutex::new(connection),
            key,
        })
    }

    pub fn keyed_digest(&self, material: &[u8]) -> String {
        hmac_sha256(&self.key, material)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn get(
        &self,
        key: &str,
        origin_hash: &str,
        schema_version: u8,
        now: u64,
    ) -> Result<Option<CachedValue>, LocalCacheError> {
        let connection = self.connection.lock().map_err(|_| LocalCacheError)?;
        let entry: Option<(u64, u64, Vec<u8>)> = connection
            .query_row(
                "SELECT fresh_until, stale_until, payload
                 FROM wam_cache_entries
                 WHERE cache_key=?1 AND origin_hash=?2 AND schema_version=?3",
                params![key, origin_hash, schema_version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|_| LocalCacheError)?;
        let Some((fresh_until, stale_until, payload)) = entry else {
            return Ok(None);
        };
        if now > stale_until {
            connection
                .execute("DELETE FROM wam_cache_entries WHERE cache_key=?1", [key])
                .map_err(|_| LocalCacheError)?;
            return Ok(None);
        }
        connection
            .execute(
                "UPDATE wam_cache_entries SET last_access=?2 WHERE cache_key=?1",
                params![key, now],
            )
            .map_err(|_| LocalCacheError)?;
        Ok(Some(CachedValue {
            payload,
            freshness: if now < fresh_until {
                CacheFreshness::Fresh
            } else {
                CacheFreshness::Stale
            },
        }))
    }

    // These fields describe one persisted cache row at the storage boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn put(
        &self,
        key: &str,
        origin_hash: &str,
        schema_version: u8,
        now: u64,
        fresh_for_secs: u64,
        stale_for_secs: u64,
        payload: &[u8],
    ) -> Result<(), LocalCacheError> {
        if payload.is_empty() || payload.len() > MAX_ENTRY_BYTES || fresh_for_secs == 0 {
            return Err(LocalCacheError);
        }
        let mut connection = self.connection.lock().map_err(|_| LocalCacheError)?;
        let transaction = connection.transaction().map_err(|_| LocalCacheError)?;
        transaction
            .execute(
                "INSERT INTO wam_cache_entries
                    (cache_key,origin_hash,schema_version,stored_at,fresh_until,stale_until,last_access,payload)
                 VALUES(?1,?2,?3,?4,?5,?6,?4,?7)
                 ON CONFLICT(cache_key) DO UPDATE SET
                    origin_hash=excluded.origin_hash,
                    schema_version=excluded.schema_version,
                    stored_at=excluded.stored_at,
                    fresh_until=excluded.fresh_until,
                    stale_until=excluded.stale_until,
                    last_access=excluded.last_access,
                    payload=excluded.payload",
                params![
                    key,
                    origin_hash,
                    schema_version,
                    now,
                    now.saturating_add(fresh_for_secs),
                    now.saturating_add(fresh_for_secs).saturating_add(stale_for_secs),
                    payload,
                ],
            )
            .map_err(|_| LocalCacheError)?;
        transaction
            .execute(
                "DELETE FROM wam_cache_entries WHERE cache_key IN (
                    SELECT cache_key FROM wam_cache_entries ORDER BY last_access ASC
                    LIMIT max(0, (SELECT COUNT(*) FROM wam_cache_entries) - ?1)
                 )",
                [MAX_ENTRIES],
            )
            .map_err(|_| LocalCacheError)?;
        transaction
            .execute(
                "DELETE FROM wam_cache_entries WHERE cache_key IN (
                    SELECT cache_key FROM wam_cache_entries ORDER BY last_access ASC
                    LIMIT max(0, (SELECT COALESCE(SUM(length(payload)),0) FROM wam_cache_entries) - ?1)
                 )",
                [MAX_BYTES],
            )
            .map_err(|_| LocalCacheError)?;
        transaction.commit().map_err(|_| LocalCacheError)
    }

    pub fn delete(&self, key: &str) -> Result<(), LocalCacheError> {
        self.connection
            .lock()
            .map_err(|_| LocalCacheError)?
            .execute("DELETE FROM wam_cache_entries WHERE cache_key=?1", [key])
            .map_err(|_| LocalCacheError)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<(), LocalCacheError> {
        self.connection
            .lock()
            .map_err(|_| LocalCacheError)?
            .execute("DELETE FROM wam_cache_entries", [])
            .map_err(|_| LocalCacheError)?;
        Ok(())
    }

    pub fn status(&self) -> Result<LocalCacheStatus, LocalCacheError> {
        self.connection
            .lock()
            .map_err(|_| LocalCacheError)?
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(length(payload)),0) FROM wam_cache_entries",
                [],
                |row| {
                    Ok(LocalCacheStatus {
                        entries: row.get(0)?,
                        bytes: row.get(1)?,
                        max_entries: MAX_ENTRIES,
                        max_bytes: MAX_BYTES,
                    })
                },
            )
            .map_err(|_| LocalCacheError)
    }
}

pub fn default_cache_path() -> Result<PathBuf, LocalCacheError> {
    if let Some(path) = std::env::var_os("WAM_CACHE_PATH") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches")))
        .ok_or(LocalCacheError)?;
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("AppData/Local")))
        .ok_or(LocalCacheError)?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or(LocalCacheError)?;
    Ok(base.join("wam").join("client-cache.sqlite3"))
}

fn read_or_create_key(path: &Path) -> Result<([u8; 32], bool), LocalCacheError> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(LocalCacheError);
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            let mut key = [0; 32];
            getrandom::fill(&mut key).map_err(|_| LocalCacheError)?;
            file.write_all(&key).map_err(|_| LocalCacheError)?;
            file.sync_all().map_err(|_| LocalCacheError)?;
            set_private_file(path)?;
            Ok((key, true))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(path).map_err(|_| LocalCacheError)?;
            if !metadata.file_type().is_file() || metadata.len() != 32 {
                return Err(LocalCacheError);
            }
            set_private_file(path)?;
            let bytes = fs::read(path).map_err(|_| LocalCacheError)?;
            let key: [u8; 32] = bytes.try_into().map_err(|_| LocalCacheError)?;
            Ok((key, false))
        }
        Err(_) => Err(LocalCacheError),
    }
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(BLOCK + message.len());
    inner.extend(normalized.iter().map(|byte| byte ^ 0x36));
    inner.extend_from_slice(message);
    let inner_hash = Sha256::digest(&inner);
    let mut outer = Vec::with_capacity(BLOCK + inner_hash.len());
    outer.extend(normalized.iter().map(|byte| byte ^ 0x5c));
    outer.extend_from_slice(&inner_hash);
    Sha256::digest(&outer).into()
}

#[cfg(unix)]
fn prepare_parent(path: &Path) -> Result<(), LocalCacheError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let created = !path.exists();
    if created {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path).map_err(|_| LocalCacheError)?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalCacheError)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(LocalCacheError);
    }
    if created {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| LocalCacheError)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn prepare_parent(path: &Path) -> Result<(), LocalCacheError> {
    fs::create_dir_all(path).map_err(|_| LocalCacheError)
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<(), LocalCacheError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|_| LocalCacheError)
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<(), LocalCacheError> {
    Ok(())
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<(), LocalCacheError> {
    use std::os::unix::fs::OpenOptionsExt;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => drop(file),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(LocalCacheError),
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<(), LocalCacheError> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => drop(file),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(LocalCacheError),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CacheFreshness, LocalCache, MAX_ENTRIES};
    use std::{
        collections::HashSet,
        fs,
        sync::{Arc, Barrier},
        thread,
    };

    fn temp_db() -> std::path::PathBuf {
        let nonce = crate::telemetry::random_id().expect("OS randomness is available");
        std::env::temp_dir().join(format!(
            "wam-cache-{}-{nonce}/cache.sqlite3",
            std::process::id()
        ))
    }

    #[test]
    fn temp_db_paths_are_unique_under_parallel_tests() {
        const WORKERS: usize = 32;
        let barrier = Arc::new(Barrier::new(WORKERS));
        let workers = (0..WORKERS)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    temp_db()
                })
            })
            .collect::<Vec<_>>();
        let paths = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(paths.iter().collect::<HashSet<_>>().len(), WORKERS);
    }

    #[test]
    fn cache_is_bound_to_origin_schema_and_expiry_window() {
        let path = temp_db();
        let cache = LocalCache::open(&path).unwrap();
        cache
            .put("key", "origin-a", 1, 100, 300, 900, b"public recall")
            .unwrap();
        assert_eq!(
            cache
                .get("key", "origin-a", 1, 200)
                .unwrap()
                .unwrap()
                .freshness,
            CacheFreshness::Fresh
        );
        assert!(cache.get("key", "origin-b", 1, 200).unwrap().is_none());
        assert!(cache.get("key", "origin-a", 2, 200).unwrap().is_none());
        assert_eq!(
            cache
                .get("key", "origin-a", 1, 401)
                .unwrap()
                .unwrap()
                .freshness,
            CacheFreshness::Stale
        );
        let digest = cache.keyed_digest(b"https://example.com/private/path");
        assert!(!digest.contains("private"));
        drop(cache);
        let reopened = LocalCache::open(&path).unwrap();
        assert_eq!(
            reopened.keyed_digest(b"https://example.com/private/path"),
            digest
        );
        assert!(reopened.get("key", "origin-a", 1, 1_301).unwrap().is_none());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn hmac_sha256_matches_the_rfc_4231_test_vector() {
        let key = [0x0b; 20];
        let digest = super::hmac_sha256(&key, b"Hi There");
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn cache_clear_and_size_limits_are_reported() {
        let path = temp_db();
        let cache = LocalCache::open(&path).unwrap();
        for index in 0..(MAX_ENTRIES + 1) {
            cache
                .put(&format!("key-{index}"), "origin", 1, index, 300, 900, b"x")
                .unwrap();
        }
        let status = cache.status().unwrap();
        assert_eq!(status.entries, MAX_ENTRIES);
        assert_eq!(status.max_entries, MAX_ENTRIES);
        assert!(status.bytes <= status.max_bytes);
        cache.clear().unwrap();
        assert_eq!(cache.status().unwrap().entries, 0);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
