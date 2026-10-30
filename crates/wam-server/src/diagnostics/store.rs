use super::types::{StoredDiagnostic, now};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
const RETENTION: u64 = 7 * 86_400;
const MAX_ROWS: usize = 100_000;
const SCHEMA: &str = include_str!("schema.sql");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticStoreError;
impl std::fmt::Display for DiagnosticStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("diagnostic storage unavailable")
    }
}
impl std::error::Error for DiagnosticStoreError {}
enum Command {
    Record(Box<StoredDiagnostic>),
    Flush(mpsc::SyncSender<Result<(), DiagnosticStoreError>>),
    Recent(
        usize,
        mpsc::SyncSender<Result<Vec<StoredDiagnostic>, DiagnosticStoreError>>,
    ),
    Purge(u64, mpsc::SyncSender<Result<(), DiagnosticStoreError>>),
}
#[derive(Clone)]
pub struct Diagnostics {
    sender: mpsc::SyncSender<Command>,
    dropped: Arc<AtomicU64>,
}
impl Diagnostics {
    pub fn in_memory() -> Self {
        Self::start(|| Ok(Backend::Memory(Vec::new()))).unwrap_or_else(|_| {
            let (sender, _) = mpsc::sync_channel(1);
            Self {
                sender,
                dropped: Arc::new(AtomicU64::new(0)),
            }
        })
    }
    pub fn sqlite(path: impl Into<PathBuf>) -> Result<Self, DiagnosticStoreError> {
        let path = path.into();
        Self::start(move || Backend::sqlite(&path))
    }
    pub fn postgres(url: &str) -> Result<Self, DiagnosticStoreError> {
        let url = url.to_owned();
        Self::start(move || Backend::postgres(&url))
    }
    fn start(
        factory: impl FnOnce() -> Result<Backend, DiagnosticStoreError> + Send + 'static,
    ) -> Result<Self, DiagnosticStoreError> {
        let (sender, receiver) = mpsc::sync_channel(256);
        let (ready, started) = mpsc::sync_channel(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let failures = dropped.clone();
        std::thread::Builder::new()
            .name("wam-diagnostics".into())
            .spawn(move || {
                let mut backend = match factory() {
                    Ok(value) => {
                        let _ = ready.send(Ok(()));
                        value
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let mut pending = VecDeque::new();
                let mut writes_since_purge = 0;
                loop {
                    let command = match receiver.recv_timeout(Duration::from_secs(1)) {
                        Ok(command) => Some(command),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            drain(&mut backend, &mut pending);
                            break;
                        }
                    };
                    match command {
                        Some(Command::Record(event)) => {
                            if pending.len() >= 512 {
                                pending.pop_front();
                                failures.fetch_add(1, Ordering::Relaxed);
                            }
                            pending.push_back(*event);
                            writes_since_purge += drain(&mut backend, &mut pending);
                            if writes_since_purge >= 64 {
                                let _ = backend.purge(now());
                                writes_since_purge = 0;
                            }
                        }
                        Some(Command::Flush(reply)) => {
                            drain(&mut backend, &mut pending);
                            let result = if pending.is_empty() {
                                backend.purge(now())
                            } else {
                                Err(DiagnosticStoreError)
                            };
                            let _ = reply.send(result);
                        }
                        Some(Command::Recent(limit, reply)) => {
                            drain(&mut backend, &mut pending);
                            let _ = reply.send(backend.recent(limit));
                        }
                        Some(Command::Purge(at, reply)) => {
                            let _ = reply.send(backend.purge(at));
                        }
                        None => {
                            drain(&mut backend, &mut pending);
                        }
                    }
                }
            })
            .map_err(|_| DiagnosticStoreError)?;
        started
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| DiagnosticStoreError)??;
        Ok(Self { sender, dropped })
    }
    /// Nonblocking; diagnostic backpressure never changes the business response.
    pub fn record(&self, event: StoredDiagnostic) -> bool {
        if !event.valid()
            || self
                .sender
                .try_send(Command::Record(Box::new(event)))
                .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            false
        } else {
            true
        }
    }
    pub fn dropped_events(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn flush(&self) -> Result<(), DiagnosticStoreError> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .try_send(Command::Flush(tx))
            .map_err(|_| DiagnosticStoreError)?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| DiagnosticStoreError)?
    }
    /// Operator/test API only; no public HTTP log-reading endpoint exists.
    pub fn recent(&self, limit: usize) -> Result<Vec<StoredDiagnostic>, DiagnosticStoreError> {
        if !(1..=1000).contains(&limit) {
            return Err(DiagnosticStoreError);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .try_send(Command::Recent(limit, tx))
            .map_err(|_| DiagnosticStoreError)?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| DiagnosticStoreError)?
    }
    pub fn purge(&self, at: u64) -> Result<(), DiagnosticStoreError> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .try_send(Command::Purge(at, tx))
            .map_err(|_| DiagnosticStoreError)?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| DiagnosticStoreError)?
    }
}
fn drain(backend: &mut Backend, pending: &mut VecDeque<StoredDiagnostic>) -> usize {
    let mut written = 0;
    while let Some(event) = pending.front() {
        if backend.append(event).is_err() {
            break;
        }
        pending.pop_front();
        written += 1;
    }
    written
}
struct PgConnection {
    client: postgres::Client,
    config: postgres::Config,
}
impl PgConnection {
    fn connect(config: &postgres::Config) -> Result<postgres::Client, DiagnosticStoreError> {
        let connector = native_tls::TlsConnector::new().map_err(|_| DiagnosticStoreError)?;
        let mut client = config
            .connect(postgres_native_tls::MakeTlsConnector::new(connector))
            .map_err(|_| DiagnosticStoreError)?;
        client
            .batch_execute("SET statement_timeout='1500ms'; SET lock_timeout='500ms'; BEGIN")
            .map_err(|_| DiagnosticStoreError)?;
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtext(current_schema()),971804)",
                &[],
            )
            .map_err(|_| DiagnosticStoreError)?;
        client
            .batch_execute(SCHEMA)
            .map_err(|_| DiagnosticStoreError)?;
        client
            .batch_execute("COMMIT")
            .map_err(|_| DiagnosticStoreError)?;
        Ok(client)
    }
    fn ensure(&mut self) -> Result<(), DiagnosticStoreError> {
        if self.client.is_closed() {
            self.client = Self::connect(&self.config)?;
        }
        Ok(())
    }
}
impl std::ops::Deref for PgConnection {
    type Target = postgres::Client;
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
impl std::ops::DerefMut for PgConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}
enum Backend {
    Memory(Vec<StoredDiagnostic>),
    Sqlite(Box<rusqlite::Connection>),
    Postgres(Box<PgConnection>),
}
fn label(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
impl Backend {
    fn sqlite(path: &Path) -> Result<Self, DiagnosticStoreError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            if !parent.exists() {
                let mut builder = std::fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(parent).map_err(|_| DiagnosticStoreError)?;
            }
        }
        if std::fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_file()) {
            return Err(DiagnosticStoreError);
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(false).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path).map_err(|_| DiagnosticStoreError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| DiagnosticStoreError)?;
        }
        let connection = rusqlite::Connection::open(path).map_err(|_| DiagnosticStoreError)?;
        connection
            .busy_timeout(Duration::from_millis(100))
            .map_err(|_| DiagnosticStoreError)?;
        connection
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|_| DiagnosticStoreError)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|_| DiagnosticStoreError)?;
        let mut backend = Self::Sqlite(Box::new(connection));
        backend.purge(now())?;
        Ok(backend)
    }
    fn postgres(url: &str) -> Result<Self, DiagnosticStoreError> {
        let mut config: postgres::Config = url.parse().map_err(|_| DiagnosticStoreError)?;
        let local = config.get_hosts().iter().all(|h| match h {
            postgres::config::Host::Tcp(h) => {
                matches!(h.as_str(), "localhost" | "127.0.0.1" | "::1")
            }
            #[cfg(unix)]
            postgres::config::Host::Unix(_) => true,
        });
        if !local {
            if config.get_ssl_mode() == postgres::config::SslMode::Disable {
                return Err(DiagnosticStoreError);
            }
            config.ssl_mode(postgres::config::SslMode::Require);
        }
        config.connect_timeout(Duration::from_secs(5));
        let client = PgConnection::connect(&config)?;
        let mut backend = Self::Postgres(Box::new(PgConnection { client, config }));
        backend.purge(now())?;
        Ok(backend)
    }
    fn append(&mut self, event: &StoredDiagnostic) -> Result<(), DiagnosticStoreError> {
        if let Self::Postgres(connection) = self {
            connection.ensure()?;
        }
        let document = serde_json::to_string(event).map_err(|_| DiagnosticStoreError)?;
        let source = label(&event.source);
        let operation = label(&event.operation);
        let error = event.error_code.as_ref().map(label);
        let status = event.status_code.map(i32::from);
        let at = i64::try_from(event.received_at).map_err(|_| DiagnosticStoreError)?;
        match self {
            Self::Memory(records) => {
                if !records.iter().any(|r| r.event_id == event.event_id) {
                    records.push(event.clone())
                }
                if records.len() > MAX_ROWS {
                    records.remove(0);
                }
                Ok(())
            }
            Self::Sqlite(conn) => {
                conn.execute("INSERT OR IGNORE INTO wam_diagnostic_events(event_id,trace_id,source,operation,error_code,status_code,received_at,document) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![event.event_id,event.trace_id,source,operation,error,status,at,document]).map_err(|_|DiagnosticStoreError)?;
                Ok(())
            }
            Self::Postgres(client) => {
                client.execute("INSERT INTO wam_diagnostic_events(event_id,trace_id,source,operation,error_code,status_code,received_at,document) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(event_id) DO NOTHING",
                &[&event.event_id,&event.trace_id,&source,&operation,&error,&status,&at,&document]).map_err(|_|DiagnosticStoreError)?;
                Ok(())
            }
        }
    }
    fn recent(&mut self, limit: usize) -> Result<Vec<StoredDiagnostic>, DiagnosticStoreError> {
        if let Self::Postgres(connection) = self {
            connection.ensure()?;
        }
        let documents:Vec<String>=match self {
            Self::Memory(records)=>return Ok(records.iter().rev().take(limit).cloned().collect()),
            Self::Sqlite(conn)=>conn.prepare("SELECT document FROM wam_diagnostic_events ORDER BY received_at DESC,event_id DESC LIMIT ?1").map_err(|_|DiagnosticStoreError)?
                .query_map([limit as i64],|row|row.get(0)).map_err(|_|DiagnosticStoreError)?.collect::<Result<_,_>>().map_err(|_|DiagnosticStoreError)?,
            Self::Postgres(client)=>client.query("SELECT document FROM wam_diagnostic_events ORDER BY received_at DESC,event_id DESC LIMIT $1",&[&(limit as i64)]).map_err(|_|DiagnosticStoreError)?.into_iter().map(|row|row.get(0)).collect(),
        };
        documents
            .into_iter()
            .map(|d| serde_json::from_str(&d).map_err(|_| DiagnosticStoreError))
            .collect()
    }
    fn purge(&mut self, at: u64) -> Result<(), DiagnosticStoreError> {
        if let Self::Postgres(connection) = self {
            connection.ensure()?;
        }
        let cutoff =
            i64::try_from(at.saturating_sub(RETENTION)).map_err(|_| DiagnosticStoreError)?;
        match self {
            Self::Memory(records) => {
                records.retain(|r| r.received_at >= cutoff as u64);
                Ok(())
            }
            Self::Sqlite(conn) => {
                conn.execute(
                    "DELETE FROM wam_diagnostic_events WHERE received_at < ?1",
                    [cutoff],
                )
                .map_err(|_| DiagnosticStoreError)?;
                conn.execute("DELETE FROM wam_diagnostic_events WHERE event_id IN (SELECT event_id FROM wam_diagnostic_events ORDER BY received_at DESC,event_id DESC LIMIT -1 OFFSET ?1)",[MAX_ROWS as i64]).map_err(|_|DiagnosticStoreError)?;
                Ok(())
            }
            Self::Postgres(client) => {
                client
                    .execute(
                        "DELETE FROM wam_diagnostic_events WHERE received_at < $1",
                        &[&cutoff],
                    )
                    .map_err(|_| DiagnosticStoreError)?;
                client.execute("DELETE FROM wam_diagnostic_events WHERE event_id IN (SELECT event_id FROM wam_diagnostic_events ORDER BY received_at DESC,event_id DESC OFFSET $1)",&[&(MAX_ROWS as i64)]).map_err(|_|DiagnosticStoreError)?;
                Ok(())
            }
        }
    }
}
