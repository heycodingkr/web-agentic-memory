//! Durable PostgreSQL queue used by workers. Enqueue admission uses a transaction-scoped advisory lock so nonce
//! checks and queue limits stay atomic across processes. Scope claims use a per-scope advisory lock plus
//! `FOR UPDATE SKIP LOCKED`; job transitions lock only their rows. Model work runs after lease commit.

use crate::queue::{
    AccountClass, CancelReport, EnqueueError, Job, JobState, NewJob, Priority, QueueConfig,
    QueueError, Stats,
};
use postgres::{Client, NoTls, Transaction};
use serde_json::Value;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::sync::{Mutex, MutexGuard};

mod backend;
mod codec;

use codec::{class_text, from_sql_u64, kind_text, parse_job, priority_text, sql_u64};

const MIGRATION: &str = include_str!("../migrations/0001_gatekeeper.sql");
const QUEUE_LOCK_KEY: i64 = 5_798_162_034;
const JOB_COLUMNS: &str = "id, kind, scope_key, contributor_id, device_id, account_class, contribution_id, priority, enqueued_at, state, attempts, not_before, lease_generation, lease_expires_at, lease_holder, attribution_withdrawn, finished_at, last_error";
const JOB_COLUMNS_J: &str = "j.id, j.kind, j.scope_key, j.contributor_id, j.device_id, j.account_class, j.contribution_id, j.priority, j.enqueued_at, j.state, j.attempts, j.not_before, j.lease_generation, j.lease_expires_at, j.lease_holder, j.attribution_withdrawn, j.finished_at, j.last_error";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostgresEnqueueError {
    Unavailable,
    Rejected(EnqueueError),
}

pub struct PostgresQueue {
    client: Mutex<Client>,
    config: QueueConfig,
}

impl PostgresQueue {
    /// Connects without TLS for local development and tests. Deployed callers should pass a TLS-enabled Client to
    /// `from_client`.
    pub fn connect(database_url: &str, config: QueueConfig) -> Result<Self, QueueError> {
        let client =
            Client::connect(database_url, NoTls).map_err(|_| QueueError::StorageUnavailable)?;
        Self::from_client(client, config)
    }

    /// Applies the Gatekeeper migration and checks that every process sharing this queue uses the same policy.
    pub fn from_client(mut client: Client, config: QueueConfig) -> Result<Self, QueueError> {
        client
            .batch_execute(MIGRATION)
            .map_err(|_| QueueError::StorageUnavailable)?;
        let settings = serde_json::to_value(&config).map_err(|_| QueueError::StorageUnavailable)?;
        client
            .execute(
                "INSERT INTO wam_gatekeeper_queue_config (singleton, settings) VALUES (TRUE, $1) ON CONFLICT (singleton) DO NOTHING",
                &[&settings],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        let stored: Value = client
            .query_one(
                "SELECT settings FROM wam_gatekeeper_queue_config WHERE singleton = TRUE",
                &[],
            )
            .map_err(|_| QueueError::StorageUnavailable)?
            .get(0);
        if stored != settings {
            return Err(QueueError::StorageUnavailable);
        }
        Ok(Self {
            client: Mutex::new(client),
            config,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Client>, QueueError> {
        self.client
            .lock()
            .map_err(|_| QueueError::StorageUnavailable)
    }

    /// Idempotent enqueue and backpressure checks use the same transaction lock as leasing, so concurrent servers
    /// cannot exceed limits or accept the same contributor nonce twice.
    pub fn enqueue(&self, new: NewJob, now: u64) -> Result<u64, PostgresEnqueueError> {
        let mut client = self.lock().map_err(|_| PostgresEnqueueError::Unavailable)?;
        let mut tx =
            begin_serialized(&mut client).map_err(|_| PostgresEnqueueError::Unavailable)?;
        let now = sql_u64(now).map_err(|_| PostgresEnqueueError::Unavailable)?;

        if let Some(row) = tx
            .query_opt(
                "SELECT id FROM wam_gatekeeper_jobs WHERE contributor_id = $1 AND nonce = $2",
                &[&new.contributor, &new.nonce],
            )
            .map_err(|_| PostgresEnqueueError::Unavailable)?
        {
            let id = from_sql_u64(row.get(0)).map_err(|_| PostgresEnqueueError::Unavailable)?;
            tx.commit().map_err(|_| PostgresEnqueueError::Unavailable)?;
            return Ok(id);
        }

        let row = tx
            .query_one(
                "SELECT COUNT(*) FILTER (WHERE state NOT IN ('done', 'dead', 'canceled')), COUNT(*) FILTER (WHERE contributor_id = $1 AND state NOT IN ('done', 'dead', 'canceled')), COUNT(*) FILTER (WHERE device_id = $2 AND state NOT IN ('done', 'dead', 'canceled')) FROM wam_gatekeeper_jobs",
                &[&new.contributor, &new.device],
            )
            .map_err(|_| PostgresEnqueueError::Unavailable)?;
        let active = usize::try_from(row.get::<_, i64>(0)).unwrap_or(usize::MAX);
        let mine = usize::try_from(row.get::<_, i64>(1)).unwrap_or(usize::MAX);
        let on_device = usize::try_from(row.get::<_, i64>(2)).unwrap_or(usize::MAX);
        if new.priority != Priority::P0 {
            if mine >= self.config.max_per_contributor {
                return Err(PostgresEnqueueError::Rejected(
                    EnqueueError::ContributorLimit {
                        retry_after_secs: self.config.retry_after_secs,
                    },
                ));
            }
            if on_device >= self.config.max_per_device {
                return Err(PostgresEnqueueError::Rejected(EnqueueError::DeviceLimit {
                    retry_after_secs: self.config.retry_after_secs,
                }));
            }
        }
        let system = new.priority == Priority::P0;
        let ceiling = if system {
            self.config
                .max_total
                .saturating_add(self.config.max_total / 5)
        } else {
            self.config.max_total
        };
        if active >= ceiling {
            return Err(PostgresEnqueueError::Rejected(EnqueueError::QueueFull {
                retry_after_secs: self.config.retry_after_secs,
            }));
        }
        if !system
            && new.class == AccountClass::New
            && active >= self.config.max_total.saturating_mul(8) / 10
        {
            return Err(PostgresEnqueueError::Rejected(EnqueueError::Shedding {
                retry_after_secs: self.config.retry_after_secs,
            }));
        }

        let held: bool = tx
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM wam_gatekeeper_jobs WHERE scope_key = $1 AND state = 'leased')",
                &[&new.scope_key],
            )
            .map_err(|_| PostgresEnqueueError::Unavailable)?
            .get(0);
        let state = if held { "waiting_scope" } else { "queued" };
        let id: i64 = tx
            .query_one(
                "INSERT INTO wam_gatekeeper_jobs (kind, scope_key, contributor_id, device_id, account_class, nonce, contribution_id, priority, enqueued_at, state, attempts, not_before) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0, $9) RETURNING id",
                &[&kind_text(new.kind), &new.scope_key, &new.contributor, &new.device, &class_text(new.class), &new.nonce, &new.contribution_id, &priority_text(new.priority), &now, &state],
            )
            .map_err(|_| PostgresEnqueueError::Unavailable)?
            .get(0);
        tx.commit().map_err(|_| PostgresEnqueueError::Unavailable)?;
        from_sql_u64(id).map_err(|_| PostgresEnqueueError::Unavailable)
    }

    pub fn job(&self, id: u64) -> Result<Option<Job>, QueueError> {
        let id = sql_u64(id)?;
        self.lock()?
            .query_opt(
                &format!("SELECT {JOB_COLUMNS} FROM wam_gatekeeper_jobs WHERE id = $1"),
                &[&id],
            )
            .map_err(|_| QueueError::StorageUnavailable)?
            .map(parse_job)
            .transpose()
    }

    /// Withdraws all matching jobs. Waiting work is canceled and loses its payload reference; leased work retains
    /// the queue lease but loses contributor attribution so the worker can finish safely without awarding credit.
    pub fn cancel_contributor(
        &self,
        contributor: &str,
        now: u64,
    ) -> Result<CancelReport, QueueError> {
        self.cancel_matching(
            "contributor_id = $1 AND state NOT IN ('done', 'dead', 'canceled')",
            contributor,
            now,
        )
    }

    /// Withdraws one contribution without canceling the contributor's other work.
    pub fn cancel_contribution(
        &self,
        contribution_id: &str,
        now: u64,
    ) -> Result<CancelReport, QueueError> {
        self.cancel_matching(
            "contribution_id = $1 AND state NOT IN ('done', 'dead', 'canceled')",
            contribution_id,
            now,
        )
    }

    fn cancel_matching(
        &self,
        predicate: &str,
        value: &str,
        now: u64,
    ) -> Result<CancelReport, QueueError> {
        let now = sql_u64(now)?;
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        let rows = tx
            .query(
                &format!(
                    "SELECT {JOB_COLUMNS} FROM wam_gatekeeper_jobs WHERE {predicate} FOR UPDATE"
                ),
                &[&value],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        let (mut canceled, mut still_leased) = (0usize, 0usize);
        for row in rows {
            let job = parse_job(row)?;
            let id = sql_u64(job.id)?;
            if queue_state_waiting(job.state) {
                tx.execute(
                    "UPDATE wam_gatekeeper_jobs SET state = 'canceled', contribution_id = NULL, finished_at = $2 WHERE id = $1",
                    &[&id, &now],
                )
                .map_err(|_| QueueError::StorageUnavailable)?;
                canceled += 1;
            } else if job.state == JobState::Leased {
                tx.execute(
                    "UPDATE wam_gatekeeper_jobs SET attribution_withdrawn = TRUE WHERE id = $1",
                    &[&id],
                )
                .map_err(|_| QueueError::StorageUnavailable)?;
                still_leased += 1;
            }
        }
        tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
        Ok(CancelReport {
            canceled,
            still_leased,
        })
    }

    /// Applies a persisted human review decision and fences any worker holding an earlier lease for the contribution.
    pub fn resolve_contribution(
        &self,
        contribution_id: &str,
        approved: bool,
        now: u64,
    ) -> Result<usize, QueueError> {
        let now = sql_u64(now)?;
        let state = if approved { "done" } else { "dead" };
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        let changed = tx
            .execute(
                "UPDATE wam_gatekeeper_jobs SET state = $2, lease_generation = lease_generation + 1, lease_expires_at = NULL, lease_holder = NULL, finished_at = $3 WHERE contribution_id = $1 AND state NOT IN ('done', 'dead', 'canceled')",
                &[&contribution_id, &state, &now],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
        usize::try_from(changed).map_err(|_| QueueError::StorageUnavailable)
    }

    /// Removes terminal jobs after the retention interval. Deleting the row also releases its contributor nonce,
    /// matching the in-memory queue's payload/nonce purge behavior.
    pub fn purge_finished(&self, now: u64, retention_secs: u64) -> Result<usize, QueueError> {
        let Some(cutoff) = now.checked_sub(retention_secs) else {
            return Ok(0);
        };
        let cutoff = sql_u64(cutoff)?;
        let mut client = self.lock()?;
        let mut tx = begin_serialized(&mut client)?;
        let changed = tx
            .execute(
                "DELETE FROM wam_gatekeeper_jobs WHERE state IN ('done', 'dead', 'canceled') AND finished_at <= $1",
                &[&cutoff],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
        usize::try_from(changed).map_err(|_| QueueError::StorageUnavailable)
    }

    /// Queue-depth and oldest-waiting-job metrics for operational dashboards.
    pub fn stats(&self, now: u64) -> Result<Stats, QueueError> {
        let mut client = self.lock()?;
        let row = client
            .query_one(
                "SELECT COUNT(*) FILTER (WHERE state NOT IN ('done', 'dead', 'canceled')), COUNT(*) FILTER (WHERE state = 'queued'), COUNT(*) FILTER (WHERE state = 'leased'), COUNT(*) FILTER (WHERE state = 'waiting_scope'), COUNT(*) FILTER (WHERE state = 'waiting_budget'), COUNT(*) FILTER (WHERE state = 'retry'), COUNT(*) FILTER (WHERE state = 'done'), COUNT(*) FILTER (WHERE state = 'dead'), COUNT(*) FILTER (WHERE state = 'canceled'), MIN(enqueued_at) FILTER (WHERE state IN ('queued', 'waiting_scope', 'waiting_budget', 'retry')) FROM wam_gatekeeper_jobs",
                &[],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        let mut by_state = BTreeMap::new();
        let states = [
            (1, JobState::Queued),
            (2, JobState::Leased),
            (3, JobState::WaitingScope),
            (4, JobState::WaitingBudget),
            (5, JobState::Retry),
            (6, JobState::Done),
            (7, JobState::Dead),
            (8, JobState::Canceled),
        ];
        for (column, state) in states {
            let count = usize::try_from(row.get::<_, i64>(column))
                .map_err(|_| QueueError::StorageUnavailable)?;
            if count != 0 {
                by_state.insert(state, count);
            }
        }
        let active =
            usize::try_from(row.get::<_, i64>(0)).map_err(|_| QueueError::StorageUnavailable)?;
        let oldest_enqueued: Option<i64> = row.get(9);
        let oldest_age_secs = oldest_enqueued
            .map(from_sql_u64)
            .transpose()?
            .map_or(0, |at| now.saturating_sub(at));
        Ok(Stats {
            active,
            by_state,
            oldest_age_secs,
            alert: oldest_age_secs > self.config.alert_after_secs,
        })
    }
}

fn queue_state_waiting(state: JobState) -> bool {
    matches!(
        state,
        JobState::Queued | JobState::WaitingScope | JobState::WaitingBudget | JobState::Retry
    )
}

fn begin_transaction(client: &mut Client) -> Result<Transaction<'_>, QueueError> {
    client
        .transaction()
        .map_err(|_| QueueError::StorageUnavailable)
}

fn begin_serialized<'a>(client: &'a mut Client) -> Result<Transaction<'a>, QueueError> {
    let mut tx = begin_transaction(client)?;
    tx.query_one("SELECT pg_advisory_xact_lock($1)", &[&QUEUE_LOCK_KEY])
        .map_err(|_| QueueError::StorageUnavailable)?;
    Ok(tx)
}
