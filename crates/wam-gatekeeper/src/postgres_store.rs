//! PostgreSQL adapter for immutable curator memory versions and atomic decisions.
//!
//! `connect` is intended for local deployments without TLS. Production callers should build a TLS-enabled
//! `postgres::Client` and pass it to `from_client`.

use crate::queue::QueueLeaseFence;
use crate::store::{
    AuditRecord, CommitResult, DecisionCommit, MemoryMutation, MemoryRow, ScopeKey, Status, Store,
    StoreError, StoredDecision, VersionRow,
};
use postgres::error::SqlState;
use postgres::{Client, NoTls, Transaction};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::sync::{Mutex, MutexGuard};

const MIGRATION_0001: &str = include_str!("../migrations/0001_gatekeeper.sql");

pub struct PostgresStore {
    client: Mutex<Client>,
}

impl PostgresStore {
    /// Connects without TLS for local development and test databases. Use `from_client` with a TLS connector in
    /// deployed environments.
    pub fn connect(database_url: &str) -> Result<Self, StoreError> {
        let client = Client::connect(database_url, NoTls).map_err(map_db_error)?;
        Self::from_client(client)
    }

    /// Applies the versioned schema and adopts an already configured PostgreSQL connection.
    pub fn from_client(mut client: Client) -> Result<Self, StoreError> {
        client.batch_execute(MIGRATION_0001).map_err(map_db_error)?;
        Ok(Self {
            client: Mutex::new(client),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Client>, StoreError> {
        self.client.lock().map_err(|_| StoreError::Unavailable)
    }
}

fn map_db_error(error: postgres::Error) -> StoreError {
    if error.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        StoreError::Conflict
    } else {
        StoreError::Unavailable
    }
}

fn encode<T: Serialize>(value: &T) -> Result<Value, StoreError> {
    serde_json::to_value(value).map_err(|_| StoreError::Unavailable)
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, StoreError> {
    serde_json::from_value(value).map_err(|_| StoreError::Unavailable)
}

fn status_text(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::NeedsReverify => "needs_reverify",
        Status::Superseded => "superseded",
        Status::Retired => "retired",
    }
}

fn parse_status(value: &str) -> Result<Status, StoreError> {
    match value {
        "active" => Ok(Status::Active),
        "needs_reverify" => Ok(Status::NeedsReverify),
        "superseded" => Ok(Status::Superseded),
        "retired" => Ok(Status::Retired),
        _ => Err(StoreError::Unavailable),
    }
}

fn load_memory(client: &mut Client, id: &str) -> Result<Option<MemoryRow>, StoreError> {
    let Some(row) = client
        .query_opt(
            "SELECT site_host, url_pattern, memory_kind, task_category, intent_key, support_count, lifecycle_status, confidence_band FROM wam_gatekeeper_memories WHERE id = $1",
            &[&id],
        )
        .map_err(map_db_error)?
    else {
        return Ok(None);
    };
    let host: String = row.get(0);
    let pattern: String = row.get(1);
    let kind: String = row.get(2);
    let task: String = row.get(3);
    let intent: String = row.get(4);
    let support_count = u32::try_from(row.get::<_, i64>(5)).map_err(|_| StoreError::Unavailable)?;
    let status = parse_status(&row.get::<_, String>(6))?;
    let confidence_band =
        u8::try_from(row.get::<_, i16>(7)).map_err(|_| StoreError::Unavailable)?;
    let versions = client
        .query(
            "SELECT version_no, payload, fingerprint, verified_at, lifecycle_status, evidence, job_id FROM wam_gatekeeper_memory_versions WHERE memory_id = $1 ORDER BY version_no",
            &[&id],
        )
        .map_err(map_db_error)?
        .into_iter()
        .map(|version| {
            let version_no = u32::try_from(version.get::<_, i32>(0))
                .map_err(|_| StoreError::Unavailable)?;
            let payload = decode(version.get::<_, Value>(1))?;
            let fingerprint = version.get(2);
            let verified_at = u64::try_from(version.get::<_, i64>(3))
                .map_err(|_| StoreError::Unavailable)?;
            let version_status = parse_status(&version.get::<_, String>(4))?;
            let evidence = decode(version.get::<_, Value>(5))?;
            let job_id = version.get(6);
            Ok(VersionRow {
                version: version_no,
                payload,
                fingerprint,
                verified_at,
                status: version_status,
                evidence,
                job_id,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    Ok(Some(MemoryRow {
        id: id.to_owned(),
        scope: ScopeKey {
            host,
            pattern,
            kind,
            task,
            intent,
        },
        versions,
        support_count,
        status,
        confidence_band,
    }))
}

fn new_memory_id(tx: &mut Transaction<'_>, scope: &ScopeKey) -> Result<String, StoreError> {
    let host = scope
        .host
        .split(':')
        .next()
        .unwrap_or(&scope.host)
        .replace('.', "-");
    let base = format!("web:{host}-{}", scope.intent);
    let mut candidate = base.clone();
    let mut suffix = 2u64;
    while tx
        .query_opt(
            "SELECT 1 FROM wam_gatekeeper_memories WHERE id = $1",
            &[&candidate],
        )
        .map_err(map_db_error)?
        .is_some()
    {
        candidate = format!("{base}-{suffix}");
        suffix = suffix.checked_add(1).ok_or(StoreError::Unavailable)?;
    }
    Ok(candidate)
}

fn insert_version(
    tx: &mut Transaction<'_>,
    memory_id: &str,
    version: &VersionRow,
) -> Result<(), StoreError> {
    let version_no = i32::try_from(version.version).map_err(|_| StoreError::Unavailable)?;
    let verified_at = i64::try_from(version.verified_at).map_err(|_| StoreError::Unavailable)?;
    let payload = encode(&version.payload)?;
    let evidence = encode(&version.evidence)?;
    tx.execute(
        "INSERT INTO wam_gatekeeper_memory_versions (memory_id, version_no, payload, fingerprint, verified_at, lifecycle_status, evidence, job_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&memory_id, &version_no, &payload, &version.fingerprint, &verified_at, &status_text(version.status), &evidence, &version.job_id],
    )
    .map_err(map_db_error)?;
    Ok(())
}

fn apply_mutation(
    tx: &mut Transaction<'_>,
    mutation: MemoryMutation,
) -> Result<CommitResult, StoreError> {
    match mutation {
        MemoryMutation::None => Ok(CommitResult::default()),
        MemoryMutation::AddSupport { memory_id, count } => {
            let count = i64::from(count);
            let changed = tx
                .execute(
                    "UPDATE wam_gatekeeper_memories SET support_count = support_count + $2 WHERE id = $1 AND lifecycle_status IN ('active', 'needs_reverify')",
                    &[&memory_id, &count],
                )
                .map_err(map_db_error)?;
            if changed == 0 {
                return if tx
                    .query_opt(
                        "SELECT 1 FROM wam_gatekeeper_memories WHERE id = $1",
                        &[&memory_id],
                    )
                    .map_err(map_db_error)?
                    .is_some()
                {
                    Err(StoreError::Conflict)
                } else {
                    Err(StoreError::Missing)
                };
            }
            Ok(CommitResult {
                memory_id: Some(memory_id),
                version: None,
            })
        }
        MemoryMutation::Create {
            scope,
            version,
            confidence_band,
        } => {
            if version.version != 1 || !matches!(version.status, Status::Active) {
                return Err(StoreError::Conflict);
            }
            let id = new_memory_id(tx, &scope)?;
            let confidence_band = i16::from(confidence_band);
            let support_count = 0i64;
            let active_version = 1i32;
            tx.execute(
                "INSERT INTO wam_gatekeeper_memories (id, site_host, url_pattern, memory_kind, task_category, intent_key, support_count, lifecycle_status, active_version, confidence_band) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[&id, &scope.host, &scope.pattern, &scope.kind, &scope.task, &scope.intent, &support_count, &status_text(version.status), &active_version, &confidence_band],
            )
            .map_err(map_db_error)?;
            insert_version(tx, &id, &version)?;
            Ok(CommitResult {
                memory_id: Some(id),
                version: Some(1),
            })
        }
        MemoryMutation::Supersede {
            memory_id,
            expected_version,
            mut next,
            confidence_band,
        } => {
            let Some(row) = tx
                .query_opt(
                    "SELECT active_version, lifecycle_status FROM wam_gatekeeper_memories WHERE id = $1 FOR UPDATE",
                    &[&memory_id],
                )
                .map_err(map_db_error)?
            else {
                return Err(StoreError::Missing);
            };
            let active_version =
                u32::try_from(row.get::<_, i32>(0)).map_err(|_| StoreError::Unavailable)?;
            let status = parse_status(&row.get::<_, String>(1))?;
            if !status.recallable() || active_version != expected_version {
                return Err(StoreError::Conflict);
            }
            let next_version = expected_version
                .checked_add(1)
                .ok_or(StoreError::Conflict)?;
            let expected_version =
                i32::try_from(expected_version).map_err(|_| StoreError::Conflict)?;
            let next_version_i32 = i32::try_from(next_version).map_err(|_| StoreError::Conflict)?;
            tx.execute(
                "UPDATE wam_gatekeeper_memory_versions SET lifecycle_status = 'superseded' WHERE memory_id = $1 AND version_no = $2",
                &[&memory_id, &expected_version],
            )
            .map_err(map_db_error)?;
            next.version = next_version;
            next.status = Status::Active;
            insert_version(tx, &memory_id, &next)?;
            let confidence_band = i16::from(confidence_band);
            tx.execute(
                "UPDATE wam_gatekeeper_memories SET active_version = $2, lifecycle_status = 'active', confidence_band = $3 WHERE id = $1",
                &[&memory_id, &next_version_i32, &confidence_band],
            )
            .map_err(map_db_error)?;
            Ok(CommitResult {
                memory_id: Some(memory_id),
                version: Some(next_version),
            })
        }
        MemoryMutation::Retire {
            memory_id,
            expected_version,
        } => {
            let Some(row) = tx
                .query_opt(
                    "SELECT active_version, lifecycle_status FROM wam_gatekeeper_memories WHERE id = $1 FOR UPDATE",
                    &[&memory_id],
                )
                .map_err(map_db_error)?
            else {
                return Err(StoreError::Missing);
            };
            let active_version =
                u32::try_from(row.get::<_, i32>(0)).map_err(|_| StoreError::Unavailable)?;
            let status = parse_status(&row.get::<_, String>(1))?;
            if !status.recallable() || active_version != expected_version {
                return Err(StoreError::Conflict);
            }
            let version = i32::try_from(expected_version).map_err(|_| StoreError::Conflict)?;
            tx.execute(
                "UPDATE wam_gatekeeper_memory_versions SET lifecycle_status = 'retired' WHERE memory_id = $1 AND version_no = $2",
                &[&memory_id, &version],
            )
            .map_err(map_db_error)?;
            tx.execute(
                "UPDATE wam_gatekeeper_memories SET lifecycle_status = 'retired' WHERE id = $1",
                &[&memory_id],
            )
            .map_err(map_db_error)?;
            Ok(CommitResult {
                memory_id: Some(memory_id),
                version: Some(expected_version),
            })
        }
    }
}

fn validate_lease_fence(
    tx: &mut Transaction<'_>,
    fence: Option<&QueueLeaseFence>,
) -> Result<(), StoreError> {
    let queue_enabled: bool = tx
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM wam_gatekeeper_queue_config WHERE singleton = TRUE)",
            &[],
        )
        .map_err(map_db_error)?
        .get(0);
    let Some(fence) = fence else {
        return if queue_enabled {
            Err(StoreError::Conflict)
        } else {
            Ok(())
        };
    };
    if fence.scope_key.is_empty()
        || fence.job_ids.is_empty()
        || fence.job_ids.len() != fence.generations.len()
    {
        return Err(StoreError::Conflict);
    }
    let required_contributions = if fence.require_attribution {
        if fence.contribution_ids.is_empty() {
            return Err(StoreError::Conflict);
        }
        let mut required = BTreeSet::new();
        for contribution_id in &fence.contribution_ids {
            if contribution_id.trim().is_empty() || !required.insert(contribution_id.clone()) {
                return Err(StoreError::Conflict);
            }
        }
        Some(required)
    } else {
        None
    };
    let mut validated_contributions = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for (job_id, generation) in fence.job_ids.iter().zip(&fence.generations) {
        if !seen.insert(*job_id) {
            return Err(StoreError::Conflict);
        }
        let id = i64::try_from(*job_id).map_err(|_| StoreError::Conflict)?;
        let row = tx
            .query_opt(
                "SELECT scope_key, state, lease_generation, lease_holder, contribution_id, attribution_withdrawn FROM wam_gatekeeper_jobs WHERE id = $1 FOR UPDATE",
                &[&id],
            )
            .map_err(map_db_error)?
            .ok_or(StoreError::Conflict)?;
        let scope: String = row.get(0);
        let state: String = row.get(1);
        let current_generation =
            u64::try_from(row.get::<_, i64>(2)).map_err(|_| StoreError::Unavailable)?;
        let holder: Option<String> = row.get(3);
        let contribution_id: Option<String> = row.get(4);
        let attribution_withdrawn: bool = row.get(5);
        if scope != fence.scope_key
            || state != "leased"
            || current_generation != *generation
            || holder.as_deref() != Some(&fence.worker)
        {
            return Err(StoreError::Conflict);
        }
        if let (Some(required), Some(contribution_id)) =
            (required_contributions.as_ref(), contribution_id)
        {
            if required.contains(&contribution_id) {
                if attribution_withdrawn {
                    return Err(StoreError::Conflict);
                }
                validated_contributions.insert(contribution_id);
            }
        }
    }
    if required_contributions
        .as_ref()
        .is_some_and(|required| required != &validated_contributions)
    {
        return Err(StoreError::Conflict);
    }
    Ok(())
}

impl Store for PostgresStore {
    fn memories_in_host(&self, host: &str) -> Result<Vec<MemoryRow>, StoreError> {
        let mut client = self.lock()?;
        client
            .query(
                "SELECT id FROM wam_gatekeeper_memories WHERE site_host = $1 AND lifecycle_status IN ('active', 'needs_reverify') ORDER BY id",
                &[&host],
            )
            .map_err(map_db_error)?
            .into_iter()
            .map(|row| {
                let id: String = row.get(0);
                load_memory(&mut client, &id)?.ok_or(StoreError::Missing)
            })
            .collect()
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRow>, StoreError> {
        let mut client = self.lock()?;
        load_memory(&mut client, id)
    }

    fn find_by_scope(&self, scope: &ScopeKey) -> Result<Option<MemoryRow>, StoreError> {
        let mut client = self.lock()?;
        let id = client
            .query_opt(
                "SELECT id FROM wam_gatekeeper_memories WHERE site_host = $1 AND url_pattern = $2 AND memory_kind = $3 AND task_category = $4 AND intent_key = $5 AND lifecycle_status IN ('active', 'needs_reverify')",
                &[&scope.host, &scope.pattern, &scope.kind, &scope.task, &scope.intent],
            )
            .map_err(map_db_error)?;
        match id {
            Some(row) => load_memory(&mut client, &row.get::<_, String>(0)),
            None => Ok(None),
        }
    }

    fn apply_decision(&mut self, commit: DecisionCommit) -> Result<CommitResult, StoreError> {
        let mut client = self.lock()?;
        let mut tx = client.transaction().map_err(map_db_error)?;
        if let Some(row) = tx
            .query_opt(
                "SELECT memory_id, version_no FROM wam_gatekeeper_decisions WHERE job_id = $1 AND job_digest = $2",
                &[&commit.job_id, &commit.job_digest],
            )
            .map_err(map_db_error)?
        {
            let result = CommitResult {
                memory_id: row.get(0),
                version: row
                    .get::<_, Option<i32>>(1)
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| StoreError::Unavailable)?,
            };
            tx.commit().map_err(map_db_error)?;
            return Ok(result);
        }

        validate_lease_fence(&mut tx, commit.lease_fence.as_ref())?;

        let result = apply_mutation(&mut tx, commit.mutation)?;
        let confidence_band = commit.confidence_band.map(i16::from);
        let version = result
            .version
            .map(i32::try_from)
            .transpose()
            .map_err(|_| StoreError::Unavailable)?;
        tx.execute(
            "INSERT INTO wam_gatekeeper_decisions (job_id, job_digest, decision_kind, reason_codes, confidence_band, memory_id, version_no, decided_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[&commit.job_id, &commit.job_digest, &commit.kind, &commit.reason_codes, &confidence_band, &result.memory_id, &version, &i64::try_from(commit.at).map_err(|_| StoreError::Unavailable)?],
        )
        .map_err(map_db_error)?;
        tx.commit().map_err(map_db_error)?;
        Ok(result)
    }

    fn log_audit(&mut self, audit: AuditRecord) -> Result<(), StoreError> {
        let encoded = encode(&audit)?;
        let at = i64::try_from(audit.at).map_err(|_| StoreError::Unavailable)?;
        self.lock()?
            .execute(
                "INSERT INTO wam_gatekeeper_audits (job_id, job_digest, outcome, audit, audited_at) VALUES ($1, $2, $3, $4, $5)",
                &[&audit.job_id, &audit.job_digest, &audit.outcome, &encoded, &at],
            )
            .map_err(map_db_error)?;
        Ok(())
    }

    fn decided(&self, job_id: &str, job_digest: &str) -> Result<bool, StoreError> {
        self.lock()?
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM wam_gatekeeper_decisions WHERE job_id = $1 AND job_digest = $2)",
                &[&job_id, &job_digest],
            )
            .map(|row| row.get(0))
            .map_err(map_db_error)
    }
}

impl PostgresStore {
    /// Reads the durable, versioned decision journal for support and operational inspection.
    pub fn stored_decision(
        &self,
        job_id: &str,
        job_digest: &str,
    ) -> Result<Option<StoredDecision>, StoreError> {
        let row = self
            .lock()?
            .query_opt(
                "SELECT decision_kind, reason_codes, confidence_band, memory_id, version_no, decided_at FROM wam_gatekeeper_decisions WHERE job_id = $1 AND job_digest = $2",
                &[&job_id, &job_digest],
            )
            .map_err(map_db_error)?;
        row.map(|row| {
            let at = u64::try_from(row.get::<_, i64>(5)).map_err(|_| StoreError::Unavailable)?;
            let version = row
                .get::<_, Option<i32>>(4)
                .map(u32::try_from)
                .transpose()
                .map_err(|_| StoreError::Unavailable)?;
            let confidence_band = row
                .get::<_, Option<i16>>(2)
                .map(u8::try_from)
                .transpose()
                .map_err(|_| StoreError::Unavailable)?;
            Ok(StoredDecision {
                job_id: job_id.to_owned(),
                job_digest: job_digest.to_owned(),
                kind: row.get(0),
                reason_codes: row.get(1),
                confidence_band,
                result: CommitResult {
                    memory_id: row.get(3),
                    version,
                },
                at,
            })
        })
        .transpose()
    }

    pub fn audit_count(&self, job_id: &str) -> Result<u64, StoreError> {
        self.lock()?
            .query_one(
                "SELECT COUNT(*) FROM wam_gatekeeper_audits WHERE job_id = $1",
                &[&job_id],
            )
            .map(|row| row.get::<_, i64>(0).max(0) as u64)
            .map_err(map_db_error)
    }
}
