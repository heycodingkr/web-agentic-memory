use super::state::{AppState, version_id};
use crate::application::ServiceError;
use crate::ports::{AuthoredMemoryVersion, MemoryRecord, MemoryUsageAggregate};
use postgres::{Client, Config};
use postgres_native_tls::MakeTlsConnector;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    str::FromStr,
    sync::{Mutex, mpsc},
    time::Duration,
};
use wam_core::{
    keyword::{KeywordVocabulary, TaskVocabulary},
    recall::RecallQuery,
};
use wam_gatekeeper::store::{MemoryRow, VersionRow};

const MIGRATION: &str = include_str!("../../migrations/0001_application.sql");
const MEMORY_USAGE_MIGRATION: &str =
    include_str!("../../migrations/0002_memory_usage_aggregates.sql");
const WORK_BUDGET_MIGRATION: &str = include_str!("../../migrations/0003_work_waiting_budget.sql");
const CATALOG_VOCABULARY_MIGRATION: &str =
    include_str!("../../migrations/0004_catalog_vocabulary_generation.sql");
const IDENTITY_KINDS: [&str; 6] = [
    "oauth_transactions",
    "accounts",
    "sessions",
    "device_requests",
    "devices",
    "approval_attempts",
];

enum Request {
    RecallCandidates {
        query: RecallQuery,
        tasks: TaskVocabulary,
        reply: mpsc::SyncSender<Result<Vec<MemoryRecord>, ServiceError>>,
    },
    KeywordVocabulary {
        reply: mpsc::SyncSender<Result<KeywordVocabulary, ServiceError>>,
    },
    MemoryById {
        id: String,
        tasks: TaskVocabulary,
        reply: mpsc::SyncSender<Result<Option<MemoryRecord>, ServiceError>>,
    },
    MemoryByVersion {
        id: String,
        version_id: String,
        tasks: TaskVocabulary,
        reply: mpsc::SyncSender<Result<Option<MemoryRecord>, ServiceError>>,
    },
    RecordRecall {
        account_id: Option<String>,
        memories: Vec<crate::ports::ReceiptMemoryAttribution>,
        issued_at: u64,
        reply: mpsc::SyncSender<Result<Option<String>, ServiceError>>,
    },
    RecordDetail {
        memory: crate::ports::ReceiptMemoryAttribution,
        account_id: Option<String>,
        viewed_at: u64,
        reply: mpsc::SyncSender<Result<(), ServiceError>>,
    },
    AuthoredMemoryVersions {
        account_id: String,
        reply: mpsc::SyncSender<Result<Vec<AuthoredMemoryVersion>, ServiceError>>,
    },
    MemoryUsage {
        version_ids: Vec<String>,
        from_date: String,
        to_date: String,
        reply: mpsc::SyncSender<Result<Vec<MemoryUsageAggregate>, ServiceError>>,
    },
    PublicMetrics {
        reply: mpsc::SyncSender<Result<wam_contracts::PublicUsageMetrics, ServiceError>>,
    },
    Load {
        write: bool,
        reply: mpsc::SyncSender<Result<AppState, ServiceError>>,
    },
    Save {
        state: Box<AppState>,
        reply: mpsc::SyncSender<Result<(), ServiceError>>,
    },
    End {
        reply: mpsc::SyncSender<Result<(), ServiceError>>,
    },
}
pub(super) struct PostgresBackend {
    sender: Mutex<mpsc::Sender<Request>>,
}

impl PostgresBackend {
    pub fn connect(url: &str) -> Result<Self, ServiceError> {
        let mut config = Config::from_str(url)
            .map_err(|_| ServiceError::InvalidInput("invalid_database_configuration"))?;
        config.connect_timeout(Duration::from_secs(10));
        let local = config.get_hosts().iter().all(|host| match host {
            postgres::config::Host::Tcp(host) => {
                matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1")
            }
            #[cfg(unix)]
            postgres::config::Host::Unix(_) => true,
        });
        if !local && config.get_ssl_mode() == postgres::config::SslMode::Disable {
            return Err(ServiceError::InvalidInput("database_tls_required"));
        }
        if !local {
            config.ssl_mode(postgres::config::SslMode::Require);
        }
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("wam-postgres".into()).spawn(move || {
            let result = (|| {
                let tls = native_tls::TlsConnector::new().map_err(|_| ServiceError::Unavailable)?;
                let mut client = config.connect(MakeTlsConnector::new(tls)).map_err(db_error)?;
                migrate(&mut client)?;
                Ok(client)
            })();
            let mut client = match result {
                Ok(client) => { let _ = ready_tx.send(Ok(())); client }
                Err(error) => { let _ = ready_tx.send(Err(error)); return; }
            };
            let mut prior = None;
            let mut vocabulary_cache: Option<(i64, KeywordVocabulary)> = None;
            for request in receiver {
                match request {
                    Request::RecallCandidates { query, tasks, reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            recall_candidates(&mut client, &query, &tasks)
                        };
                        let _ = reply.send(result);
                    }
                    Request::KeywordVocabulary { reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            keyword_vocabulary_cached(&mut client, &mut vocabulary_cache)
                        };
                        let _ = reply.send(result);
                    }
                    Request::MemoryById { id, tasks, reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            memory_by_id(&mut client, &id, &tasks)
                        };
                        let _ = reply.send(result);
                    }
                    Request::MemoryByVersion { id, version_id, tasks, reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            memory_by_version(&mut client, &id, &version_id, &tasks)
                        };
                        let _ = reply.send(result);
                    }
                    Request::RecordRecall {
                        account_id,
                        memories,
                        issued_at,
                        reply,
                    } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            record_recall(&mut client, account_id.as_deref(), &memories, issued_at)
                        };
                        let _ = reply.send(result);
                    }
                    Request::RecordDetail { memory, account_id, viewed_at, reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            record_memory_detail(&mut client, &memory, account_id.as_deref(), viewed_at)
                        };
                        let _ = reply.send(result);
                    }
                    Request::AuthoredMemoryVersions { account_id, reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            authored_memory_versions(&mut client, &account_id)
                        };
                        let _ = reply.send(result);
                    }
                    Request::MemoryUsage { version_ids, from_date, to_date, reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            memory_usage(&mut client, &version_ids, &from_date, &to_date)
                        };
                        let _ = reply.send(result);
                    }
                    Request::PublicMetrics { reply } => {
                        let result = if prior.is_some() {
                            Err(ServiceError::Unavailable)
                        } else {
                            public_metrics(&mut client)
                        };
                        let _ = reply.send(result);
                    }
                    Request::Load { write, reply } => {
                        let result = (|| {
                            client.batch_execute(if write { "BEGIN" } else { "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY" }).map_err(db_error)?;
                            if write {
                                // All compound transitions use the same short transaction lock.
                                // Model calls and browser replay never hold this lock.
                                client.query_one("SELECT pg_advisory_xact_lock(hashtext(current_schema()), 971803)", &[]).map_err(db_error)?;
                            }
                            let state = load(&mut client)?;
                            prior = Some(state.clone());
                            Ok(state)
                        })();
                        if result.is_err() { let _ = client.batch_execute("ROLLBACK"); prior = None; }
                        let _ = reply.send(result);
                    }
                    Request::Save { state, reply } => {
                        let result = (|| {
                            save(&mut client, prior.as_ref().ok_or(ServiceError::Unavailable)?, &state)?;
                            client.batch_execute("COMMIT").map_err(db_error)
                        })();
                        if result.is_err() { let _ = client.batch_execute("ROLLBACK"); }
                        prior = None;
                        let _ = reply.send(result);
                    }
                    Request::End { reply } => {
                        prior = None;
                        let _ = reply.send(client.batch_execute("ROLLBACK").map_err(db_error));
                    }
                }
            }
        }).map_err(|_| ServiceError::Unavailable)?;
        ready_rx.recv().map_err(|_| ServiceError::Unavailable)??;
        Ok(Self {
            sender: Mutex::new(sender),
        })
    }

    pub fn read<T>(
        &self,
        operation: impl FnOnce(&AppState) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let mut transaction = Guard {
            sender: &sender,
            ended: false,
        };
        let state = transaction.load(false)?;
        let result = operation(&state);
        transaction.end()?;
        result
    }

    pub fn recall_candidates(
        &self,
        query: RecallQuery,
        tasks: TaskVocabulary,
    ) -> Result<Vec<MemoryRecord>, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::RecallCandidates {
                query,
                tasks,
                reply,
            })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn memory_by_id(
        &self,
        id: String,
        tasks: TaskVocabulary,
    ) -> Result<Option<MemoryRecord>, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::MemoryById { id, tasks, reply })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn memory_by_version(
        &self,
        id: String,
        version_id: String,
        tasks: TaskVocabulary,
    ) -> Result<Option<MemoryRecord>, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::MemoryByVersion {
                id,
                version_id,
                tasks,
                reply,
            })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn keyword_vocabulary(&self) -> Result<KeywordVocabulary, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::KeywordVocabulary { reply })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn record_recall(
        &self,
        account_id: Option<String>,
        memories: Vec<crate::ports::ReceiptMemoryAttribution>,
        issued_at: u64,
    ) -> Result<Option<String>, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::RecordRecall {
                account_id,
                memories,
                issued_at,
                reply,
            })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn record_memory_detail(
        &self,
        memory: crate::ports::ReceiptMemoryAttribution,
        account_id: Option<String>,
        viewed_at: u64,
    ) -> Result<(), ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::RecordDetail {
                memory,
                account_id,
                viewed_at,
                reply,
            })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn authored_memory_versions(
        &self,
        account_id: String,
    ) -> Result<Vec<AuthoredMemoryVersion>, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::AuthoredMemoryVersions { account_id, reply })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn memory_usage(
        &self,
        version_ids: Vec<String>,
        from_date: String,
        to_date: String,
    ) -> Result<Vec<MemoryUsageAggregate>, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::MemoryUsage {
                version_ids,
                from_date,
                to_date,
                reply,
            })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn public_metrics(&self) -> Result<wam_contracts::PublicUsageMetrics, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let (reply, response) = mpsc::sync_channel(1);
        sender
            .send(Request::PublicMetrics { reply })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }

    pub fn update<T>(
        &self,
        operation: impl FnOnce(&mut AppState) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let sender = self.sender.lock().map_err(|_| ServiceError::Unavailable)?;
        let mut transaction = Guard {
            sender: &sender,
            ended: false,
        };
        let mut state = transaction.load(true)?;
        let result = operation(&mut state)?;
        transaction.save(state)?;
        Ok(result)
    }
}

struct Guard<'a> {
    sender: &'a mpsc::Sender<Request>,
    ended: bool,
}
impl Guard<'_> {
    fn load(&mut self, write: bool) -> Result<AppState, ServiceError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Load { write, reply })
            .map_err(|_| ServiceError::Unavailable)?;
        response.recv().map_err(|_| ServiceError::Unavailable)?
    }
    fn save(&mut self, state: AppState) -> Result<(), ServiceError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Save {
                state: Box::new(state),
                reply,
            })
            .map_err(|_| ServiceError::Unavailable)?;
        let result = response.recv().map_err(|_| ServiceError::Unavailable)?;
        self.ended = true;
        result
    }
    fn end(&mut self) -> Result<(), ServiceError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.sender
            .send(Request::End { reply })
            .map_err(|_| ServiceError::Unavailable)?;
        let result = response.recv().map_err(|_| ServiceError::Unavailable)?;
        self.ended = true;
        result
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self.end();
        }
    }
}

fn db_error(error: postgres::Error) -> ServiceError {
    match error.code().map(|code| code.code()) {
        Some("23505" | "40001" | "40P01") => ServiceError::Conflict,
        _ => ServiceError::Unavailable,
    }
}

fn recall_candidates(
    client: &mut Client,
    query: &RecallQuery,
    tasks: &TaskVocabulary,
) -> Result<Vec<MemoryRecord>, ServiceError> {
    let task = query.task_category().map(|task| task.as_str());
    let kind = query.kind().map(wam_gatekeeper::record::kind_str);
    let rows = client
        .query(
            include_str!("recall_candidates.sql"),
            &[&query.url().host(), &query.url().path(), &task, &kind],
        )
        .map_err(db_error)?;
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.get(0);
        records.push(memory_record_from_documents(
            &id,
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            tasks,
        )?);
    }
    Ok(records)
}

fn memory_by_id(
    client: &mut Client,
    id: &str,
    tasks: &TaskVocabulary,
) -> Result<Option<MemoryRecord>, ServiceError> {
    let row = client
        .query_opt(
            "SELECT r.id,r.document,v.document,authors.items,meta.document->'disabled_accounts' \
             FROM wam_memory_records r \
             JOIN wam_memory_versions v ON v.memory_id=r.id AND v.version_no=r.active_version \
             CROSS JOIN wam_app_meta meta \
             LEFT JOIN LATERAL ( \
                 SELECT jsonb_agg(jsonb_build_object('id',a.id,'account_id',a.user_id) ORDER BY a.id) AS items \
                 FROM wam_memory_authors a WHERE a.version_id=v.id AND a.role='author' AND a.user_id IS NOT NULL \
             ) authors ON TRUE \
             WHERE r.id=$1 AND r.status IN ('active','needs_reverify')",
            &[&id],
        )
        .map_err(db_error)?;
    row.map(|row| {
        memory_record_from_documents(
            &row.get::<_, String>(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            tasks,
        )
    })
    .transpose()
}

fn memory_by_version(
    client: &mut Client,
    id: &str,
    requested_version_id: &str,
    tasks: &TaskVocabulary,
) -> Result<Option<MemoryRecord>, ServiceError> {
    let row = client
        .query_opt(
            "SELECT r.id,r.document,v.document,authors.items,meta.document->'disabled_accounts' \
             FROM wam_memory_records r \
             JOIN wam_memory_versions v ON v.memory_id=r.id \
             CROSS JOIN wam_app_meta meta \
             LEFT JOIN LATERAL ( \
                 SELECT jsonb_agg(jsonb_build_object('id',a.id,'account_id',a.user_id) ORDER BY a.id) AS items \
                 FROM wam_memory_authors a WHERE a.version_id=v.id AND a.role='author' AND a.user_id IS NOT NULL \
             ) authors ON TRUE \
             WHERE r.id=$1 AND v.id=$2 AND r.status IN ('active','needs_reverify') \
               AND v.document->>'status' IN ('active','needs_reverify','superseded')",
            &[&id, &requested_version_id],
        )
        .map_err(db_error)?;
    row.map(|row| {
        memory_record_from_documents(
            &row.get::<_, String>(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            tasks,
        )
    })
    .transpose()
}

fn memory_record_from_documents(
    id: &str,
    mut memory_document: Value,
    version_document: Value,
    authors_document: Option<Value>,
    disabled_document: Value,
    tasks: &TaskVocabulary,
) -> Result<MemoryRecord, ServiceError> {
    let disabled: Vec<String> =
        serde_json::from_value(disabled_document).map_err(|_| ServiceError::Unavailable)?;
    let disabled: std::collections::BTreeSet<_> = disabled.into_iter().collect();
    let authors: Vec<StoredAuthor> = authors_document
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| ServiceError::Unavailable)?
        .unwrap_or_default();
    let author_account_id = authors
        .into_iter()
        .filter_map(|author| author.account_id)
        .find(|account| {
            !disabled.contains(&wam_gatekeeper::record::sha256_hex(account.as_bytes()))
        });
    memory_document["versions"] = json!([version_document]);
    let memory: MemoryRow =
        serde_json::from_value(memory_document).map_err(|_| ServiceError::Unavailable)?;
    if memory.id != id {
        return Err(ServiceError::Unavailable);
    }
    let version: VersionRow = memory
        .versions
        .last()
        .cloned()
        .ok_or(ServiceError::Unavailable)?;
    super::state::memory_record_from_parts(&memory, &version, tasks, author_account_id)
}

#[derive(serde::Deserialize)]
struct StoredAuthor {
    account_id: Option<String>,
}

fn keyword_vocabulary(client: &mut Client) -> Result<KeywordVocabulary, ServiceError> {
    let mut values: std::collections::BTreeSet<String> =
        ["blog", "draft", "publish", "search", "write"]
            .map(str::to_owned)
            .into();
    for row in client
        .query(
            "SELECT DISTINCT i.keyword \
             FROM wam_memory_index i \
             JOIN wam_memory_versions v ON v.id=i.version_id \
             JOIN wam_memory_records r ON r.id=v.memory_id AND v.version_no=r.active_version \
             WHERE r.status IN ('active','needs_reverify') AND i.keyword<>''",
            &[],
        )
        .map_err(db_error)?
    {
        values.insert(row.get(0));
    }
    let digest = wam_gatekeeper::record::sha256_hex(
        values
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("|")
            .as_bytes(),
    );
    KeywordVocabulary::new(format!("catalog-{}", &digest[..12]), values)
        .map_err(|_| ServiceError::Unavailable)
}

fn keyword_vocabulary_cached(
    client: &mut Client,
    cache: &mut Option<(i64, KeywordVocabulary)>,
) -> Result<KeywordVocabulary, ServiceError> {
    let generation: i64 = client
        .query_one(
            "SELECT generation FROM wam_catalog_vocabulary_generation WHERE singleton",
            &[],
        )
        .map_err(db_error)?
        .get(0);
    if let Some((cached_generation, vocabulary)) = cache
        && *cached_generation == generation
    {
        return Ok(vocabulary.clone());
    }
    let vocabulary = keyword_vocabulary(client)?;
    *cache = Some((generation, vocabulary.clone()));
    Ok(vocabulary)
}

fn record_recall(
    client: &mut Client,
    account_id: Option<&str>,
    memories: &[crate::ports::ReceiptMemoryAttribution],
    issued_at: u64,
) -> Result<Option<String>, ServiceError> {
    client.batch_execute("BEGIN").map_err(db_error)?;
    let result = (|| {
        // Keep this tiny write in the same order as state transitions so receipts
        // cannot be lost when a concurrent application snapshot commits.
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtext(current_schema()), 971803)",
                &[],
            )
            .map_err(db_error)?;
        let mut document: Value = client
            .query_one(
                "SELECT document FROM wam_app_meta WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(db_error)?
            .get(0);
        let disabled_accounts: Vec<String> =
            serde_json::from_value(document["disabled_accounts"].clone())
                .map_err(|_| ServiceError::Unavailable)?;
        let disabled_accounts: std::collections::BTreeSet<_> =
            disabled_accounts.into_iter().collect();
        if let Some(account) = account_id {
            let exists = client
                .query_opt("SELECT 1 FROM wam_users WHERE id=$1", &[&account])
                .map_err(db_error)?
                .is_some();
            if !exists
                || disabled_accounts
                    .contains(&wam_gatekeeper::record::sha256_hex(account.as_bytes()))
            {
                return Err(ServiceError::Unauthorized);
            }
        }
        let mut authoritative = Vec::with_capacity(memories.len());
        for memory in memories {
            let exists = client
                .query_opt(
                    "SELECT 1 FROM wam_memory_records r JOIN wam_memory_versions v ON v.memory_id=r.id WHERE r.id=$1 AND v.id=$2",
                    &[&memory.memory_id, &memory.version_id],
                )
                .map_err(db_error)?
                .is_some();
            if !exists {
                return Err(ServiceError::Conflict);
            }
            let author_account_id = memory
                .author_account_id
                .as_deref()
                .filter(|author| {
                    !disabled_accounts
                        .contains(&wam_gatekeeper::record::sha256_hex(author.as_bytes()))
                })
                .map(str::to_owned);
            authoritative.push(crate::ports::ReceiptMemoryAttribution {
                memory_id: memory.memory_id.clone(),
                version_id: memory.version_id.clone(),
                author_account_id,
            });
        }
        for memory in &authoritative {
            let Some(author) = memory.author_account_id.as_deref() else {
                continue;
            };
            if account_id == Some(author) {
                continue;
            }
            let counter = if account_id.is_some() {
                "other_account_recall_hits"
            } else {
                "anonymous_recall_hits"
            };
            increment_memory_usage(client, memory, issued_at, counter, 1)?;
        }
        prune_memory_usage(client, issued_at)?;
        let mut usage: crate::usage_store::UsageState =
            serde_json::from_value(document["usage"].clone())
                .map_err(|_| ServiceError::Unavailable)?;
        usage.receipts.clear();
        let local_receipt =
            crate::usage_store::record_recall(&mut usage, account_id, &authoritative, issued_at)
                .map_err(|_| ServiceError::Unavailable)?;
        let receipt = if local_receipt.is_some() {
            let mut receipt = usage.receipts.pop().ok_or(ServiceError::Unavailable)?;
            let opaque_id = format!(
                "use-{}",
                crate::identity::random_secret().map_err(|_| ServiceError::Unavailable)?
            );
            receipt.id = opaque_id;
            Some(receipt)
        } else {
            None
        };
        let retention_cutoff = issued_at.saturating_sub(crate::usage_store::RECEIPT_TTL_SECS);
        client
            .execute(
                "DELETE FROM wam_usage_receipts WHERE issued_at < $1",
                &[&i64::try_from(retention_cutoff).map_err(|_| ServiceError::Unavailable)?],
            )
            .map_err(db_error)?;
        if let Some(receipt) = receipt.as_ref() {
            let document = serde_json::to_value(receipt).map_err(|_| ServiceError::Unavailable)?;
            client
                .execute(
                    "INSERT INTO wam_usage_receipts(id,owner_id,issued_at,expires_at,document) VALUES($1,$2,$3,$4,$5)",
                    &[
                        &receipt.id,
                        &receipt.owner_account,
                        &i64::try_from(receipt.issued_at).map_err(|_| ServiceError::Unavailable)?,
                        &i64::try_from(receipt.issued_at.saturating_add(crate::usage_store::RECEIPT_TTL_SECS)).map_err(|_| ServiceError::Unavailable)?,
                        &document,
                    ],
                )
                .map_err(db_error)?;
            for memory in &receipt.memory_uses {
                client
                    .execute(
                        "INSERT INTO wam_receipt_memories(receipt_id,version_id,author_id,used) VALUES($1,$2,$3,FALSE)",
                        &[&receipt.id, &memory.version_id, &memory.author_account_id],
                    )
                    .map_err(db_error)?;
            }
        }
        usage.memory_analytics.clear();
        document["usage"] = serde_json::to_value(usage).map_err(|_| ServiceError::Unavailable)?;
        client
            .execute(
                "UPDATE wam_app_meta SET document=$1 WHERE singleton",
                &[&document],
            )
            .map_err(db_error)?;
        client.batch_execute("COMMIT").map_err(db_error)?;
        Ok(receipt.map(|receipt| receipt.id))
    })();
    if result.is_err() {
        let _ = client.batch_execute("ROLLBACK");
    }
    result
}

fn record_memory_detail(
    client: &mut Client,
    memory: &crate::ports::ReceiptMemoryAttribution,
    account_id: Option<&str>,
    viewed_at: u64,
) -> Result<(), ServiceError> {
    client.batch_execute("BEGIN").map_err(db_error)?;
    let result = (|| {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtext(current_schema()), 971803)",
                &[],
            )
            .map_err(db_error)?;
        client
            .execute(
                "UPDATE wam_app_meta SET document=jsonb_set( \
                     document, '{usage,detail_requests}', \
                     to_jsonb(LEAST(18446744073709551615::numeric, \
                         COALESCE((document #>> '{usage,detail_requests}')::numeric,0)+1)), TRUE \
                 ) WHERE singleton",
                &[],
            )
            .map_err(db_error)?;
        if let Some(author) = memory.author_account_id.as_deref()
            && account_id != Some(author)
        {
            increment_memory_usage(
                client,
                memory,
                viewed_at,
                if account_id.is_some() {
                    "other_account_detail_views"
                } else {
                    "anonymous_detail_views"
                },
                1,
            )?;
        }
        prune_memory_usage(client, viewed_at)?;
        client.batch_execute("COMMIT").map_err(db_error)
    })();
    if result.is_err() {
        let _ = client.batch_execute("ROLLBACK");
    }
    result
}

fn increment_memory_usage(
    client: &mut Client,
    memory: &crate::ports::ReceiptMemoryAttribution,
    at: u64,
    counter: &str,
    amount: u64,
) -> Result<(), ServiceError> {
    let counter = match counter {
        "anonymous_recall_hits" => "anonymous_recall_hits",
        "other_account_recall_hits" => "other_account_recall_hits",
        "anonymous_detail_views" => "anonymous_detail_views",
        "other_account_detail_views" => "other_account_detail_views",
        "success_outcomes" => "success_outcomes",
        "partial_outcomes" => "partial_outcomes",
        "failed_outcomes" => "failed_outcomes",
        _ => return Err(ServiceError::Internal),
    };
    let date = crate::rate_limit::utc_dates(at, 1)
        .into_iter()
        .next()
        .ok_or(ServiceError::Internal)?;
    increment_memory_usage_on_date(client, memory, &date, counter, amount)
}

fn increment_memory_usage_on_date(
    client: &mut Client,
    memory: &crate::ports::ReceiptMemoryAttribution,
    date: &str,
    counter: &str,
    amount: u64,
) -> Result<(), ServiceError> {
    let sql = format!(
        "INSERT INTO wam_memory_usage_aggregates(memory_id,version_id,utc_date,{counter}) \
         VALUES($1,$2,$3::text::date,$4) \
         ON CONFLICT(version_id,utc_date) DO UPDATE SET {counter}=LEAST(9223372036854775807, \
            wam_memory_usage_aggregates.{counter}+EXCLUDED.{counter})"
    );
    client
        .execute(
            &sql,
            &[
                &memory.memory_id,
                &memory.version_id,
                &date,
                &i64::try_from(amount).map_err(|_| ServiceError::Internal)?,
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

fn prune_memory_usage(client: &mut Client, at: u64) -> Result<(), ServiceError> {
    let cutoff = crate::rate_limit::utc_dates(at, 90)
        .into_iter()
        .next()
        .ok_or(ServiceError::Internal)?;
    client
        .execute(
            "DELETE FROM wam_memory_usage_aggregates WHERE utc_date < $1::text::date",
            &[&cutoff],
        )
        .map_err(db_error)?;
    Ok(())
}

fn memory_usage(
    client: &mut Client,
    version_ids: &[String],
    from_date: &str,
    to_date: &str,
) -> Result<Vec<MemoryUsageAggregate>, ServiceError> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    client
        .query(
            "SELECT memory_id,version_id,to_char(utc_date,'YYYY-MM-DD'), \
                    anonymous_recall_hits,other_account_recall_hits,anonymous_detail_views, \
                    other_account_detail_views,success_outcomes,partial_outcomes,failed_outcomes \
             FROM wam_memory_usage_aggregates \
             WHERE version_id=ANY($1) AND utc_date BETWEEN $2::text::date AND $3::text::date \
             ORDER BY version_id,utc_date",
            &[&version_ids, &from_date, &to_date],
        )
        .map_err(db_error)?
        .into_iter()
        .map(|row| {
            let count = |index| row.get::<_, i64>(index).max(0) as u64;
            Ok(MemoryUsageAggregate {
                memory_id: row.get(0),
                version_id: row.get(1),
                utc_date: row.get(2),
                anonymous_recall_hits: count(3),
                other_account_recall_hits: count(4),
                anonymous_detail_views: count(5),
                other_account_detail_views: count(6),
                success_outcomes: count(7),
                partial_outcomes: count(8),
                failed_outcomes: count(9),
            })
        })
        .collect()
}

fn authored_memory_versions(
    client: &mut Client,
    account_id: &str,
) -> Result<Vec<AuthoredMemoryVersion>, ServiceError> {
    client
        .query(
            "SELECT DISTINCT r.id,v.id,COALESCE(v.document #>> '{payload,l1,title}',r.id), \
                    COALESCE(r.document #>> '{scope,host}','') \
             FROM wam_memory_authors a \
             JOIN wam_memory_versions v ON v.id=a.version_id \
             JOIN wam_memory_records r ON r.id=v.memory_id \
             WHERE a.user_id=$1 AND a.role='author' \
             ORDER BY r.id,v.id",
            &[&account_id],
        )
        .map_err(db_error)?
        .into_iter()
        .map(|row| {
            Ok(AuthoredMemoryVersion {
                memory_id: row.get(0),
                version_id: row.get(1),
                title: row.get(2),
                site: row.get(3),
            })
        })
        .collect()
}

fn public_metrics(client: &mut Client) -> Result<wam_contracts::PublicUsageMetrics, ServiceError> {
    let usage_document: Value = client
        .query_one(
            "SELECT document->'usage' FROM wam_app_meta WHERE singleton",
            &[],
        )
        .map_err(db_error)?
        .get(0);
    let usage: crate::usage_store::UsageState =
        serde_json::from_value(usage_document).map_err(|_| ServiceError::Unavailable)?;
    Ok(crate::usage_store::public_metrics(&usage))
}

fn migrate(client: &mut Client) -> Result<(), ServiceError> {
    client
        .batch_execute("BEGIN; SELECT pg_advisory_xact_lock(hashtext(current_schema()), 971803)")
        .map_err(db_error)?;
    let result = (|| {
        client.batch_execute("CREATE TABLE IF NOT EXISTS wam_schema_migrations(version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at BIGINT NOT NULL)").map_err(db_error)?;
        for (version, migration) in [
            (1_i32, MIGRATION),
            (2, MEMORY_USAGE_MIGRATION),
            (3, WORK_BUDGET_MIGRATION),
            (4, CATALOG_VOCABULARY_MIGRATION),
        ] {
            let checksum = wam_gatekeeper::record::sha256_hex(migration.as_bytes());
            if let Some(row) = client
                .query_opt(
                    "SELECT checksum FROM wam_schema_migrations WHERE version=$1",
                    &[&version],
                )
                .map_err(db_error)?
            {
                if row.get::<_, String>(0) != checksum {
                    return Err(ServiceError::InvalidInput("migration_checksum_mismatch"));
                }
            } else {
                client.batch_execute(migration).map_err(db_error)?;
                client.execute("INSERT INTO wam_schema_migrations(version,checksum,applied_at) VALUES($1,$2,extract(epoch from now())::bigint)", &[&version,&checksum]).map_err(db_error)?;
            }
        }
        if client
            .query_opt("SELECT singleton FROM wam_app_meta WHERE singleton", &[])
            .map_err(db_error)?
            .is_none()
        {
            let empty = AppState::default();
            let (metadata, _) = documents(&empty)?;
            client
                .execute(
                    "INSERT INTO wam_app_meta(singleton,document) VALUES(TRUE,$1)",
                    &[&metadata],
                )
                .map_err(db_error)?;
        }
        client.batch_execute("COMMIT").map_err(db_error)
    })();
    if result.is_err() {
        let _ = client.batch_execute("ROLLBACK");
    }
    result
}

type Rows = BTreeMap<String, Value>;
type Sections = BTreeMap<&'static str, Rows>;

fn take(root: &mut Value, path: &[&str], empty: Value) -> Value {
    let mut target = root;
    for key in path {
        target = &mut target[*key];
    }
    std::mem::replace(target, empty)
}
fn keyed(value: Value, key: impl Fn(usize, &Value) -> String) -> Rows {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .enumerate()
                .map(|(index, value)| (key(index, value), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}
fn object_rows(value: Value) -> Rows {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn documents(state: &AppState) -> Result<(Value, Sections), ServiceError> {
    let mut root = serde_json::to_value(state).map_err(|_| ServiceError::Unavailable)?;
    if let Some(usage) = root.get_mut("usage").and_then(Value::as_object_mut) {
        usage.remove("memory_analytics");
    }
    let mut rows = Sections::new();
    rows.insert(
        "users",
        object_rows(take(&mut root, &["accounts"], json!({}))),
    );
    rows.insert(
        "contributions",
        keyed(
            take(&mut root, &["contributions", "contributions"], json!([])),
            |_, value| value["record"]["id"].as_str().unwrap_or_default().into(),
        ),
    );
    rows.insert(
        "consents",
        object_rows(take(&mut root, &["consents"], json!({}))),
    );
    rows.insert(
        "receipts",
        keyed(
            take(&mut root, &["usage", "receipts"], json!([])),
            |_, value| value["id"].as_str().unwrap_or_default().into(),
        ),
    );
    rows.insert(
        "scores",
        keyed(
            take(&mut root, &["scores", "events"], json!([])),
            |_, value| value["id"].as_str().unwrap_or_default().into(),
        ),
    );
    rows.insert(
        "authors",
        keyed(take(&mut root, &["attributions"], json!([])), |index, _| {
            format!("attribution-{index:016}")
        }),
    );
    rows.insert("work", object_rows(take(&mut root, &["work"], json!({}))));
    rows.insert(
        "outbox",
        keyed(take(&mut root, &["outbox"], json!([])), |_, value| {
            value["id"].as_str().unwrap_or_default().into()
        }),
    );
    rows.insert(
        "proposals",
        object_rows(take(&mut root, &["proposals"], json!({}))),
    );
    rows.insert(
        "decisions",
        keyed(
            take(&mut root, &["catalog", "decisions"], json!([])),
            |_, value| {
                format!(
                    "{}:{}",
                    value["job_id"].as_str().unwrap_or_default(),
                    value["job_digest"].as_str().unwrap_or_default()
                )
            },
        ),
    );
    rows.insert(
        "review_audit",
        keyed(
            take(&mut root, &["contributions", "review_audit"], json!([])),
            |index, _| format!("{index:016}"),
        ),
    );
    rows.insert(
        "curator_audit",
        keyed(
            take(&mut root, &["catalog", "audits"], json!([])),
            |index, _| format!("{index:016}"),
        ),
    );
    let memories = object_rows(take(&mut root, &["catalog", "memories"], json!({})));
    let mut record_rows = Rows::new();
    let mut version_rows = Rows::new();
    for (id, mut memory) in memories {
        let versions = take(&mut memory, &["versions"], json!([]));
        for version in versions.as_array().into_iter().flatten() {
            let number = u32::try_from(
                version["version"]
                    .as_u64()
                    .ok_or(ServiceError::Unavailable)?,
            )
            .map_err(|_| ServiceError::Unavailable)?;
            let key = version_id(&id, number);
            version_rows.insert(key, json!({"memory_id":id,"version":version}));
        }
        record_rows.insert(id, memory);
    }
    rows.insert("records", record_rows);
    rows.insert("versions", version_rows);
    for kind in IDENTITY_KINDS {
        let identity = take(&mut root, &["identity", kind], json!([]));
        let values = keyed(identity, |_, value| {
            let key = match kind {
                "oauth_transactions" => "state_hash",
                "accounts" => "subject_hash",
                "sessions" => "token_hash",
                "device_requests" => "device_code_hash",
                "devices" => "id",
                _ => "account_id",
            };
            value[key].as_str().unwrap_or_default().into()
        });
        rows.insert(kind, values);
    }
    Ok((root, rows))
}

fn query_documents(
    client: &mut Client,
    table: &str,
    key: &str,
    order: &str,
) -> Result<Rows, ServiceError> {
    client
        .query(
            &format!("SELECT {key},document FROM {table} ORDER BY {order}"),
            &[],
        )
        .map_err(db_error)?
        .into_iter()
        .map(|row| Ok((row.get::<_, String>(0), row.get::<_, Value>(1))))
        .collect()
}
fn load(client: &mut Client) -> Result<AppState, ServiceError> {
    let mut root: Value = client
        .query_one("SELECT document FROM wam_app_meta WHERE singleton", &[])
        .map_err(db_error)?
        .get(0);
    root["accounts"] = serde_json::to_value(query_documents(client, "wam_users", "id", "id")?)
        .map_err(|_| ServiceError::Unavailable)?;
    for (table, key, path, object, order) in [
        (
            "wam_contributions",
            "id",
            vec!["contributions", "contributions"],
            false,
            "id",
        ),
        (
            "wam_consents",
            "contribution_id",
            vec!["consents"],
            true,
            "contribution_id",
        ),
        (
            "wam_usage_receipts",
            "id",
            vec!["usage", "receipts"],
            false,
            "id",
        ),
        (
            "wam_score_events",
            "id",
            vec!["scores", "events"],
            false,
            "substring(id from 7)::bigint",
        ),
        (
            "wam_memory_authors",
            "id",
            vec!["attributions"],
            false,
            "id",
        ),
        ("wam_work_items", "id", vec!["work"], true, "id"),
        ("wam_outbox", "id", vec!["outbox"], false, "id"),
        (
            "wam_verified_proposals",
            "contribution_id",
            vec!["proposals"],
            true,
            "contribution_id",
        ),
        (
            "wam_decisions",
            "id",
            vec!["catalog", "decisions"],
            false,
            "id",
        ),
    ] {
        // BTreeMap sorting is fine for fixed-width ids; score order is rebuilt
        // from its numerical sequence to remain stable beyond the padding width.
        let values = if object {
            serde_json::to_value(query_documents(client, table, key, order)?)
                .map_err(|_| ServiceError::Unavailable)?
        } else {
            Value::Array(
                client
                    .query(
                        &format!("SELECT document FROM {table} ORDER BY {order}"),
                        &[],
                    )
                    .map_err(db_error)?
                    .into_iter()
                    .map(|row| row.get(0))
                    .collect(),
            )
        };
        let mut target = &mut root;
        for key in path {
            target = &mut target[key];
        }
        *target = values;
    }
    let mut records = query_documents(client, "wam_memory_records", "id", "id")?;
    for row in client
        .query(
            "SELECT memory_id, document FROM wam_memory_versions ORDER BY memory_id,version_no",
            &[],
        )
        .map_err(db_error)?
    {
        let id: String = row.get(0);
        let memory = records.get_mut(&id).ok_or(ServiceError::Unavailable)?;
        memory["versions"]
            .as_array_mut()
            .ok_or(ServiceError::Unavailable)?
            .push(row.get(1));
    }
    root["catalog"]["memories"] =
        serde_json::to_value(records).map_err(|_| ServiceError::Unavailable)?;
    for (kind, path) in [
        ("review", vec!["contributions", "review_audit"]),
        ("curator", vec!["catalog", "audits"]),
    ] {
        let values: Vec<Value> = client
            .query(
                "SELECT document FROM wam_audit WHERE kind=$1 ORDER BY id",
                &[&kind],
            )
            .map_err(db_error)?
            .into_iter()
            .map(|row| row.get(0))
            .collect();
        let mut target = &mut root;
        for key in path {
            target = &mut target[key];
        }
        *target = json!(values);
    }
    for kind in IDENTITY_KINDS {
        let values: Vec<Value> = client
            .query(
                "SELECT document FROM wam_identity_records WHERE kind=$1 ORDER BY id",
                &[&kind],
            )
            .map_err(db_error)?
            .into_iter()
            .map(|row| row.get(0))
            .collect();
        root["identity"][kind] = json!(values);
    }
    let state: AppState = serde_json::from_value(root).map_err(|_| ServiceError::Unavailable)?;
    if state.schema_version != 1 {
        return Err(ServiceError::Unavailable);
    }
    for account in state.accounts.values() {
        account.snapshot()?;
    }
    Ok(state)
}

fn write_rows(client: &mut Client, rows: &Rows, old: &Rows, sql: &str) -> Result<(), ServiceError> {
    for (key, document) in rows {
        if old.get(key) != Some(document) {
            client.execute(sql, &[key, document]).map_err(db_error)?;
        }
    }
    Ok(())
}
fn prune(client: &mut Client, table: &str, key: &str, rows: &Rows) -> Result<(), ServiceError> {
    let ids: Vec<_> = rows.keys().cloned().collect();
    client
        .execute(
            &format!("DELETE FROM {table} WHERE {key} <> ALL($1::text[])"),
            &[&ids],
        )
        .map_err(db_error)?;
    Ok(())
}
fn save(client: &mut Client, prior: &AppState, state: &AppState) -> Result<(), ServiceError> {
    persist_outcome_usage(client, prior, state)?;
    let (metadata, rows) = documents(state)?;
    let (prior_metadata, old) = documents(prior)?;
    let get = |name| rows.get(name).expect("known storage section");
    let previous = |name| old.get(name).expect("known storage section");
    write_rows(
        client,
        get("users"),
        previous("users"),
        "INSERT INTO wam_users(id,document) VALUES($1,$2) ON CONFLICT(id) DO UPDATE SET document=EXCLUDED.document",
    )?;
    for (id, document) in get("contributions") {
        let nonce = state
            .consents
            .get(id)
            .map(|consent| consent.nonce.as_str())
            .unwrap_or(id);
        let prior_nonce = prior
            .consents
            .get(id)
            .map(|consent| consent.nonce.as_str())
            .unwrap_or(id);
        if previous("contributions").get(id) == Some(document) && nonce == prior_nonce {
            continue;
        }
        client.execute("INSERT INTO wam_contributions(id,owner_id,nonce,state,received_at,document) SELECT $1,NULLIF(($2::jsonb)->>'account_id',''),$3,($2::jsonb)#>>'{record,status}',(($2::jsonb)#>>'{record,submitted_at}')::bigint,$2 ON CONFLICT(id) DO UPDATE SET owner_id=EXCLUDED.owner_id,nonce=EXCLUDED.nonce,state=EXCLUDED.state,document=EXCLUDED.document", &[id,document,&nonce]).map_err(db_error)?;
    }
    write_rows(
        client,
        get("consents"),
        previous("consents"),
        "INSERT INTO wam_consents(contribution_id,purge_at,withdrawn_at,document) SELECT $1,(($2::jsonb)->>'purge_at')::bigint,(($2::jsonb)->>'withdrawn_at')::bigint,$2 ON CONFLICT(contribution_id) DO UPDATE SET purge_at=EXCLUDED.purge_at,withdrawn_at=EXCLUDED.withdrawn_at,document=EXCLUDED.document",
    )?;
    write_rows(
        client,
        get("records"),
        previous("records"),
        "INSERT INTO wam_memory_records(id,site_host,url_pattern,kind,task_category,intent_key,status,active_version,document) SELECT $1,($2::jsonb)#>>'{scope,host}',($2::jsonb)#>>'{scope,pattern}',($2::jsonb)#>>'{scope,kind}',($2::jsonb)#>>'{scope,task}',($2::jsonb)#>>'{scope,intent}',($2::jsonb)->>'status',1,$2 ON CONFLICT(id) DO UPDATE SET status=EXCLUDED.status,document=EXCLUDED.document",
    )?;
    for (key, wrapper) in get("versions") {
        if previous("versions").get(key) == Some(wrapper) {
            continue;
        }
        let memory = wrapper["memory_id"]
            .as_str()
            .ok_or(ServiceError::Unavailable)?;
        let version = &wrapper["version"];
        let number = i32::try_from(
            version["version"]
                .as_u64()
                .ok_or(ServiceError::Unavailable)?,
        )
        .map_err(|_| ServiceError::Unavailable)?;
        client.execute("INSERT INTO wam_memory_versions(id,memory_id,version_no,document) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET document=EXCLUDED.document",
            &[key, &memory, &number, version]).map_err(db_error)?;
        let l0 = &version["payload"]["l0"];
        let words = l0["keywords"].as_array().ok_or(ServiceError::Unavailable)?;
        let keywords: Vec<&str> = if words.is_empty() {
            vec![""]
        } else {
            words
                .iter()
                .map(|word| word.as_str().ok_or(ServiceError::Unavailable))
                .collect::<Result<_, _>>()?
        };
        for route in l0["url_patterns"]
            .as_array()
            .ok_or(ServiceError::Unavailable)?
        {
            for keyword in &keywords {
                let host = l0["site_host"].as_str().ok_or(ServiceError::Unavailable)?;
                let route = route.as_str().ok_or(ServiceError::Unavailable)?;
                let kind = l0["kind"].as_str().ok_or(ServiceError::Unavailable)?;
                let task = l0["task_category"].as_str().unwrap_or("navigate");
                client.execute("INSERT INTO wam_memory_index(version_id,site_host,url_pattern,kind,task_category,keyword) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
                    &[key,&host,&route,&kind,&task,keyword]).map_err(db_error)?;
            }
        }
    }
    for (id, memory) in &state.catalog.memories {
        let version = i32::try_from(
            memory
                .versions
                .last()
                .ok_or(ServiceError::Unavailable)?
                .version,
        )
        .map_err(|_| ServiceError::Unavailable)?;
        client.execute("UPDATE wam_memory_records SET active_version=$2 WHERE id=$1 AND active_version<>$2", &[id,&version]).map_err(db_error)?;
    }
    // Add derived version IDs to attribution documents only as relational keys.
    for (key, document) in get("authors") {
        if previous("authors").get(key) == Some(document) {
            continue;
        }
        let memory = document["memory_id"]
            .as_str()
            .ok_or(ServiceError::Unavailable)?;
        let number = u32::try_from(
            document["version"]
                .as_u64()
                .ok_or(ServiceError::Unavailable)?,
        )
        .map_err(|_| ServiceError::Unavailable)?;
        let version = version_id(memory, number);
        client.execute("INSERT INTO wam_memory_authors(id,version_id,user_id,contribution_id,role,document) SELECT $1,$3,($2::jsonb)->>'account_id',($2::jsonb)->>'contribution_id',($2::jsonb)->>'role',$2 ON CONFLICT(id) DO UPDATE SET user_id=EXCLUDED.user_id,contribution_id=EXCLUDED.contribution_id,document=EXCLUDED.document", &[key,document,&version]).map_err(db_error)?;
    }
    write_rows(
        client,
        get("receipts"),
        previous("receipts"),
        "INSERT INTO wam_usage_receipts(id,owner_id,issued_at,expires_at,document) SELECT $1,($2::jsonb)->>'owner_account',(($2::jsonb)->>'issued_at')::bigint,(($2::jsonb)->>'issued_at')::bigint+2592000,$2 ON CONFLICT(id) DO UPDATE SET document=EXCLUDED.document",
    )?;
    for receipt in &state.usage.receipts {
        for memory in &receipt.memory_uses {
            let used = receipt.used_version_ids.contains(&memory.version_id);
            client.execute("INSERT INTO wam_receipt_memories(receipt_id,version_id,author_id,used) VALUES($1,$2,$3,$4) ON CONFLICT(receipt_id,version_id) DO UPDATE SET author_id=EXCLUDED.author_id,used=EXCLUDED.used",
                &[&receipt.id,&memory.version_id,&memory.author_account_id,&used]).map_err(db_error)?;
        }
    }
    for event in &state.scores.events {
        let document = serde_json::to_value(event).map_err(|_| ServiceError::Unavailable)?;
        if previous("scores").get(&event.id) == Some(&document) {
            continue;
        }
        if previous("scores").contains_key(&event.id) {
            client.execute("UPDATE wam_score_events SET beneficiary_id=(SELECT id FROM wam_users WHERE id=($2::jsonb)->>'beneficiary_account_id'), consumer_id=(SELECT id FROM wam_users WHERE id=($2::jsonb)->>'consumer_account_id'),document=$2 WHERE id=$1", &[&event.id,&document]).map_err(db_error)?;
        } else {
            client.execute("INSERT INTO wam_score_events(id,beneficiary_id,consumer_id,memory_id,version_id,receipt_id,event_type,points_delta,idempotency_key,created_at,reversal_of,document) SELECT $1,(SELECT id FROM wam_users WHERE id=($2::jsonb)->>'beneficiary_account_id'),(SELECT id FROM wam_users WHERE id=($2::jsonb)->>'consumer_account_id'),($2::jsonb)->>'memory_id',($2::jsonb)->>'memory_version_id',(SELECT id FROM wam_usage_receipts WHERE id=($2::jsonb)->>'source_receipt_id'),($2::jsonb)->>'event_type',(($2::jsonb)->>'points_delta')::bigint,($2::jsonb)->>'idempotency_key',(($2::jsonb)->>'created_at')::bigint,CASE WHEN ($2::jsonb)->>'event_type'='reversal' THEN substring(($2::jsonb)->>'idempotency_key' from 10) ELSE NULL END,$2", &[&event.id,&document]).map_err(db_error)?;
        }
    }
    write_rows(
        client,
        get("work"),
        previous("work"),
        "INSERT INTO wam_work_items(id,contribution_id,memory_id,state,generation,lease_until,holder,document) SELECT $1,($2::jsonb)->>'contribution_id',($2::jsonb)->>'memory_id',($2::jsonb)->>'state',(($2::jsonb)->>'generation')::bigint,(($2::jsonb)->>'lease_until')::bigint,($2::jsonb)->>'holder',$2 ON CONFLICT(id) DO UPDATE SET state=EXCLUDED.state,generation=EXCLUDED.generation,lease_until=EXCLUDED.lease_until,holder=EXCLUDED.holder,document=EXCLUDED.document",
    )?;
    write_rows(
        client,
        get("outbox"),
        previous("outbox"),
        "INSERT INTO wam_outbox(id,kind,target,created_at,delivered_at,document) SELECT $1,($2::jsonb)->>'kind',($2::jsonb)->>'target',(($2::jsonb)->>'created_at')::bigint,(($2::jsonb)->>'delivered_at')::bigint,$2 ON CONFLICT(id) DO UPDATE SET delivered_at=EXCLUDED.delivered_at,document=EXCLUDED.document",
    )?;
    write_rows(
        client,
        get("proposals"),
        previous("proposals"),
        "INSERT INTO wam_verified_proposals(contribution_id,document) VALUES($1,$2) ON CONFLICT(contribution_id) DO UPDATE SET document=EXCLUDED.document",
    )?;
    write_rows(
        client,
        get("decisions"),
        previous("decisions"),
        "INSERT INTO wam_decisions(id,job_id,job_digest,memory_id,document) SELECT $1,($2::jsonb)->>'job_id',($2::jsonb)->>'job_digest',($2::jsonb)#>>'{result,memory_id}',$2 ON CONFLICT(id) DO NOTHING",
    )?;
    for (kind, section) in [("review", "review_audit"), ("curator", "curator_audit")] {
        for (key, document) in get(section) {
            if previous(section).get(key) != Some(document) {
                client.execute("INSERT INTO wam_audit(kind,id,document) VALUES($1,$2,$3) ON CONFLICT(kind,id) DO NOTHING", &[&kind,key,document]).map_err(db_error)?;
            }
        }
    }
    for kind in IDENTITY_KINDS {
        for (key, document) in get(kind) {
            if previous(kind).get(key) == Some(document) {
                continue;
            }
            let secret = if matches!(kind, "sessions" | "devices") {
                document["token_hash"].as_str()
            } else {
                None
            };
            client.execute("INSERT INTO wam_identity_records(kind,id,user_id,secret_hash,expires_at,document) SELECT $1,$2,($3::jsonb)->>'account_id',$4,(($3::jsonb)->>'expires_at')::bigint,$3 ON CONFLICT(kind,id) DO UPDATE SET user_id=EXCLUDED.user_id,secret_hash=EXCLUDED.secret_hash,expires_at=EXCLUDED.expires_at,document=EXCLUDED.document",
                &[&kind,key,document,&secret]).map_err(db_error)?;
        }
        let ids: Vec<_> = get(kind).keys().cloned().collect();
        client
            .execute(
                "DELETE FROM wam_identity_records WHERE kind=$1 AND id<>ALL($2::text[])",
                &[&kind, &ids],
            )
            .map_err(db_error)?;
    }
    for (table, key, section) in [
        ("wam_verified_proposals", "contribution_id", "proposals"),
        ("wam_work_items", "id", "work"),
        ("wam_memory_authors", "id", "authors"),
        ("wam_consents", "contribution_id", "consents"),
        ("wam_contributions", "id", "contributions"),
        ("wam_usage_receipts", "id", "receipts"),
        ("wam_users", "id", "users"),
    ] {
        prune(client, table, key, get(section))?;
    }
    if metadata != prior_metadata {
        client
            .execute(
                "UPDATE wam_app_meta SET document=$1 WHERE singleton",
                &[&metadata],
            )
            .map_err(db_error)?;
    }
    Ok(())
}

fn persist_outcome_usage(
    client: &mut Client,
    prior: &AppState,
    state: &AppState,
) -> Result<(), ServiceError> {
    for aggregate in &state.usage.memory_analytics {
        let previous = prior.usage.memory_analytics.iter().find(|candidate| {
            candidate.version_id == aggregate.version_id && candidate.utc_date == aggregate.utc_date
        });
        let memory = crate::ports::ReceiptMemoryAttribution {
            memory_id: aggregate.memory_id.clone(),
            version_id: aggregate.version_id.clone(),
            author_account_id: None,
        };
        for (field, counter) in [
            ("success_outcomes", aggregate.success_outcomes),
            ("partial_outcomes", aggregate.partial_outcomes),
            ("failed_outcomes", aggregate.failed_outcomes),
        ] {
            let old = previous.map_or(0, |previous| match field {
                "success_outcomes" => previous.success_outcomes,
                "partial_outcomes" => previous.partial_outcomes,
                _ => previous.failed_outcomes,
            });
            let delta = counter.saturating_sub(old);
            if delta > 0 {
                increment_memory_usage_on_date(client, &memory, &aggregate.utc_date, field, delta)?;
            }
        }
    }
    Ok(())
}
