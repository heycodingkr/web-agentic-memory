//! Private, bounded, cross-process diagnostic queue. Files contain only validated closed events.

use crate::telemetry::DiagnosticEvent;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const MAX_EVENTS: usize = 100;
const MAX_FILE_BYTES: u64 = 512;
const TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Clone)]
pub(crate) struct TelemetrySpool {
    directory: PathBuf,
}

impl TelemetrySpool {
    pub(crate) fn new(root: &Path, origin_hash: &str) -> Self {
        Self {
            directory: root.join(origin_hash),
        }
    }

    pub(crate) fn enqueue(&self, event: &DiagnosticEvent) -> io::Result<()> {
        if !event.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid diagnostic",
            ));
        }
        ensure_private_directory(&self.directory)?;
        let path = self.path(event.id());
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                let bytes = serde_json::to_vec(event).map_err(io::Error::other)?;
                if bytes.len() as u64 > MAX_FILE_BYTES {
                    let _ = fs::remove_file(&path);
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "diagnostic too large",
                    ));
                }
                if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
            Err(error) => return Err(error),
        }
        self.prune()?;
        Ok(())
    }

    pub(crate) fn pending(&self, max: usize) -> io::Result<Vec<DiagnosticEvent>> {
        if !self.directory.exists() {
            return Ok(Vec::new());
        }
        ensure_private_directory(&self.directory)?;
        let mut events = Vec::new();
        for (_, path) in self.list_files()? {
            if events.len() >= max {
                break;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if !private_file(&metadata) || metadata.len() > MAX_FILE_BYTES {
                continue;
            }
            let bytes = fs::read(&path)?;
            let Ok(event) = serde_json::from_slice::<DiagnosticEvent>(&bytes) else {
                continue;
            };
            if event.valid() && path == self.path(event.id()) {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub(crate) fn delivered(&self, event: &DiagnosticEvent) -> io::Result<()> {
        if !event.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid diagnostic",
            ));
        }
        let path = self.path(event.id());
        let metadata = fs::symlink_metadata(&path)?;
        if !private_file(&metadata) || metadata.len() > MAX_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsafe diagnostic",
            ));
        }
        let stored = serde_json::from_slice::<DiagnosticEvent>(&fs::read(&path)?)
            .map_err(io::Error::other)?;
        if stored != *event {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "diagnostic changed",
            ));
        }
        fs::remove_file(path)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }

    fn prune(&self) -> io::Result<()> {
        let files = self.list_files()?;
        let remove_count = files.len().saturating_sub(MAX_EVENTS);
        for (_, path) in files.into_iter().take(remove_count) {
            let metadata = fs::symlink_metadata(&path)?;
            if private_file(&metadata) {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    fn list_files(&self) -> io::Result<Vec<(SystemTime, PathBuf)>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "symlink in diagnostic queue",
                ));
            }
            if !metadata.is_file() || !private_file(&metadata) {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|id| id.to_str()) else {
                continue;
            };
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") || !valid_id(id) {
                continue;
            }
            let modified = metadata.modified()?;
            if SystemTime::now()
                .duration_since(modified)
                .is_ok_and(|age| age > TTL)
            {
                let _ = fs::remove_file(path);
                continue;
            }
            files.push((modified, path));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Ok(files)
    }
}

fn valid_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsafe diagnostic directory",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    let mut builder = fs::DirBuilder::new();
                    builder.mode(0o700);
                    match builder.create(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error),
                    }
                }
                #[cfg(not(unix))]
                {
                    match fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error),
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for directory in [path, path.parent().unwrap_or(path)] {
            let metadata = fs::symlink_metadata(directory)?;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "diagnostic directory is not private",
                ));
            }
        }
    }
    Ok(())
}

fn private_file(metadata: &fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o077 == 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{DiagnosticError, DiagnosticOperation, DiagnosticSource, random_id};

    fn test_spool() -> (PathBuf, TelemetrySpool) {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("wam-spool-test-{}", random_id().unwrap()))
            .join("private");
        let spool = TelemetrySpool::new(&root, &"a".repeat(64));
        (root, spool)
    }

    fn event() -> DiagnosticEvent {
        DiagnosticEvent::new(
            DiagnosticSource::Cli,
            DiagnosticOperation::Recall,
            DiagnosticError::NetworkError,
            random_id().unwrap(),
            None,
            Duration::ZERO,
        )
        .unwrap()
    }

    #[test]
    fn bounds_queue_and_expires_old_events() {
        let (root, spool) = test_spool();
        let old = event();
        spool.enqueue(&old).unwrap();
        let old_path = spool.path(old.id());
        fs::File::open(&old_path)
            .unwrap()
            .set_modified(SystemTime::now() - TTL - Duration::from_secs(1))
            .unwrap();
        for _ in 0..102 {
            spool.enqueue(&event()).unwrap();
        }
        assert!(spool.pending(200).unwrap().len() <= MAX_EVENTS);
        assert!(!old_path.exists());
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn private_permissions_and_symlink_rejection() {
        let (root, spool) = test_spool();
        let item = event();
        spool.enqueue(&item).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&spool.directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(spool.path(item.id()))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            fs::set_permissions(spool.path(item.id()), fs::Permissions::from_mode(0o644)).unwrap();
            assert!(spool.pending(10).unwrap().is_empty());
            assert!(spool.delivered(&item).is_err());
            let link = root.parent().unwrap().join("link");
            symlink(&root, &link).unwrap();
            let linked = TelemetrySpool::new(&link, &"a".repeat(64));
            assert!(linked.enqueue(&event()).is_err());
        }
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn malformed_and_oversized_entries_are_never_pending_or_acknowledged() {
        let (root, spool) = test_spool();
        let item = event();
        spool.enqueue(&item).unwrap();
        let path = spool.path(item.id());

        fs::write(&path, b"not-json").unwrap();
        assert!(spool.pending(10).unwrap().is_empty());
        assert!(spool.delivered(&item).is_err());
        assert!(
            path.exists(),
            "invalid event must not be deleted as delivered"
        );

        fs::write(&path, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert!(spool.pending(10).unwrap().is_empty());
        assert!(spool.delivered(&item).is_err());
        assert!(path.exists(), "oversized event must not be acknowledged");

        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn delivery_acknowledges_only_the_event_that_was_enqueued() {
        let (root, spool) = test_spool();
        let item = event();
        spool.enqueue(&item).unwrap();
        let path = spool.path(item.id());
        let mut changed = serde_json::to_value(&item).unwrap();
        changed["trace_id"] = serde_json::Value::String("1".repeat(32));
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();

        assert!(spool.delivered(&item).is_err());
        assert!(
            path.exists(),
            "changed event must remain queued for inspection"
        );

        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn concurrent_writers_keep_distinct_event_files() {
        let (root, spool) = test_spool();
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let spool = spool.clone();
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        spool.enqueue(&event()).unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let events = spool.pending(100).unwrap();
        assert_eq!(events.len(), 40);
        let ids: std::collections::HashSet<_> = events.iter().map(DiagnosticEvent::id).collect();
        assert_eq!(ids.len(), 40);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }
}
