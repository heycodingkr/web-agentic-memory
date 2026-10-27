use super::codec::{from_sql_u64, kind_text, parse_job, priority_rank, sql_u64};
use super::{JOB_COLUMNS, JOB_COLUMNS_J, PostgresQueue, begin_transaction};
use crate::queue::{
    Batch, Job, JobKind, Priority, QueueBackend, QueueConfig, QueueError, retry_delay,
};
use postgres::Transaction;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;

fn check_batch_tx(tx: &mut Transaction<'_>, batch: &Batch) -> Result<(), QueueError> {
    if batch.job_ids.len() != batch.generations.len() {
        return Err(QueueError::StaleLease);
    }
    for (id, generation) in batch.job_ids.iter().zip(&batch.generations) {
        let id = sql_u64(*id)?;
        let row = tx
            .query_opt(
                "SELECT scope_key, state, lease_generation, lease_holder FROM wam_gatekeeper_jobs WHERE id = $1 FOR UPDATE",
                &[&id],
            )
            .map_err(|_| QueueError::StorageUnavailable)?
            .ok_or(QueueError::UnknownJob)?;
        let scope: String = row.get(0);
        let state: String = row.get(1);
        let stored_generation = from_sql_u64(row.get(2))?;
        let holder: Option<String> = row.get(3);
        if scope != batch.scope_key
            || state != "leased"
            || stored_generation != *generation
            || holder.as_deref() != Some(&batch.worker)
        {
            return Err(QueueError::StaleLease);
        }
    }
    Ok(())
}

fn fail_expired(
    tx: &mut Transaction<'_>,
    job: &Job,
    cfg: &QueueConfig,
    now: u64,
) -> Result<(), QueueError> {
    let attempts = job.attempts.saturating_add(1);
    let dead = attempts >= cfg.max_attempts;
    let state = if dead { "dead" } else { "queued" };
    let finished = if dead { Some(sql_u64(now)?) } else { None };
    let id = sql_u64(job.id)?;
    let attempts = i32::try_from(attempts).map_err(|_| QueueError::StorageUnavailable)?;
    let now = sql_u64(now)?;
    tx.execute(
        "UPDATE wam_gatekeeper_jobs SET state = $2, attempts = $3, not_before = $4, lease_expires_at = NULL, lease_holder = NULL, finished_at = $5, last_error = 'lease expired' WHERE id = $1",
        &[&id, &state, &attempts, &now, &finished],
    )
    .map_err(|_| QueueError::StorageUnavailable)?;
    Ok(())
}

fn fail_job_tx(
    tx: &mut Transaction<'_>,
    job: &Job,
    cfg: &QueueConfig,
    now: u64,
    error: &str,
) -> Result<(), QueueError> {
    let attempts = job.attempts.saturating_add(1);
    let dead = attempts >= cfg.max_attempts;
    let state = if dead { "dead" } else { "retry" };
    let finished = if dead { Some(sql_u64(now)?) } else { None };
    let not_before = if dead {
        now
    } else {
        now.saturating_add(retry_delay(cfg, job.id, attempts))
    };
    let id = sql_u64(job.id)?;
    let attempts = i32::try_from(attempts).map_err(|_| QueueError::StorageUnavailable)?;
    let not_before = sql_u64(not_before)?;
    tx.execute(
        "UPDATE wam_gatekeeper_jobs SET state = $2, attempts = $3, not_before = $4, lease_expires_at = NULL, lease_holder = NULL, finished_at = $5, last_error = $6 WHERE id = $1",
        &[&id, &state, &attempts, &not_before, &finished, &error],
    )
    .map_err(|_| QueueError::StorageUnavailable)?;
    Ok(())
}

impl QueueBackend for PostgresQueue {
    fn backend_config(&self) -> &QueueConfig {
        &self.config
    }

    fn backend_job(&self, id: u64) -> Result<Option<Job>, QueueError> {
        self.job(id)
    }

    fn backend_release_budget(&mut self, now: u64) -> Result<usize, QueueError> {
        let now = sql_u64(now)?;
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        let changed = tx
            .execute(
                "UPDATE wam_gatekeeper_jobs SET state = 'queued', not_before = $1 WHERE state = 'waiting_budget'",
                &[&now],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
        usize::try_from(changed).map_err(|_| QueueError::StorageUnavailable)
    }

    fn backend_lease(&mut self, worker: &str, now: u64) -> Result<Option<Batch>, QueueError> {
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        let now_db = sql_u64(now)?;
        let expired = tx
            .query(
                &format!("SELECT {JOB_COLUMNS} FROM wam_gatekeeper_jobs WHERE state = 'leased' AND lease_expires_at <= $1 ORDER BY lease_expires_at, id LIMIT 100 FOR UPDATE SKIP LOCKED"),
                &[&now_db],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        for row in expired {
            let job = parse_job(row)?;
            fail_expired(&mut tx, &job, &self.config, now)?;
        }

        let rows = tx
            .query(
                &format!("SELECT {JOB_COLUMNS_J}, COALESCE(f.last_served_tick, 0) FROM wam_gatekeeper_jobs j LEFT JOIN wam_gatekeeper_queue_fairness f ON f.contributor_id = j.contributor_id WHERE j.state IN ('queued', 'waiting_scope', 'retry') AND j.not_before <= $1 ORDER BY j.id"),
                &[&now_db],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        let mut groups: BTreeMap<(String, JobKind), Vec<(Job, u64)>> = BTreeMap::new();
        for row in rows {
            let fairness_tick = from_sql_u64(row.get(18))?;
            let job = parse_job(row)?;
            groups
                .entry((job.scope_key.clone(), job.kind))
                .or_default()
                .push((job, fairness_tick));
        }

        let mut ready_groups = Vec::new();
        for (group, jobs) in &groups {
            let released = jobs.len() >= self.config.coalesce_max
                || jobs.iter().any(|(job, _)| {
                    job.priority == Priority::P0
                        || job.attempts > 0
                        || job
                            .enqueued_at
                            .saturating_add(self.config.coalesce_window_secs)
                            <= now
                });
            if !released {
                continue;
            }
            let key = jobs
                .iter()
                .map(|(job, last)| (priority_rank(job.priority), *last, job.id))
                .min()
                .ok_or(QueueError::StorageUnavailable)?;
            ready_groups.push((key, group.clone()));
        }
        ready_groups.sort_by_key(|(key, group)| (*key, group.clone()));
        let max_candidates =
            i64::try_from(self.config.coalesce_max).map_err(|_| QueueError::StorageUnavailable)?;
        let mut chosen: Option<(String, JobKind, Vec<Job>)> = None;
        for (_, (scope, kind)) in ready_groups {
            let got_scope_lock: bool = tx
                .query_one(
                    "SELECT pg_try_advisory_xact_lock(hashtextextended($1, 0))",
                    &[&scope],
                )
                .map_err(|_| QueueError::StorageUnavailable)?
                .get(0);
            if !got_scope_lock {
                continue;
            }
            let already_leased: bool = tx
                .query_one(
                    "SELECT EXISTS (SELECT 1 FROM wam_gatekeeper_jobs WHERE scope_key = $1 AND state = 'leased')",
                    &[&scope],
                )
                .map_err(|_| QueueError::StorageUnavailable)?
                .get(0);
            if already_leased {
                continue;
            }
            let candidates = tx
                .query(
                    &format!("SELECT {JOB_COLUMNS} FROM wam_gatekeeper_jobs WHERE scope_key = $1 AND kind = $2 AND state IN ('queued', 'waiting_scope', 'retry') AND not_before <= $3 ORDER BY CASE priority WHEN 'p0' THEN 0 ELSE 1 END, id LIMIT $4 FOR UPDATE SKIP LOCKED"),
                    &[&scope, &kind_text(kind), &now_db, &max_candidates],
                )
                .map_err(|_| QueueError::StorageUnavailable)?
                .into_iter()
                .map(parse_job)
                .collect::<Result<Vec<_>, _>>()?;
            if candidates.is_empty() {
                continue;
            }
            let released = candidates.len() >= self.config.coalesce_max
                || candidates.iter().any(|job| {
                    job.priority == Priority::P0
                        || job.attempts > 0
                        || job
                            .enqueued_at
                            .saturating_add(self.config.coalesce_window_secs)
                            <= now
                });
            if released {
                chosen = Some((scope, kind, candidates));
                break;
            }
        }
        let Some((scope, _kind, candidates)) = chosen else {
            tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
            return Ok(None);
        };
        let expires = now
            .checked_add(self.config.lease_secs)
            .ok_or(QueueError::StorageUnavailable)?;
        let expires_db = sql_u64(expires)?;
        let mut job_ids = Vec::with_capacity(candidates.len());
        let mut generations = Vec::with_capacity(candidates.len());
        let mut contributors = BTreeSet::new();
        for job in &candidates {
            let id = sql_u64(job.id)?;
            let row = tx
                .query_one(
                    "UPDATE wam_gatekeeper_jobs SET state = 'leased', lease_generation = lease_generation + 1, lease_expires_at = $2, lease_holder = $3 WHERE id = $1 RETURNING lease_generation",
                    &[&id, &expires_db, &worker],
                )
                .map_err(|_| QueueError::StorageUnavailable)?;
            job_ids.push(job.id);
            generations.push(from_sql_u64(row.get(0))?);
            contributors.insert(job.contributor.clone());
        }
        tx.execute(
            "UPDATE wam_gatekeeper_jobs SET state = 'waiting_scope' WHERE scope_key = $1 AND state IN ('queued', 'waiting_scope', 'retry')",
            &[&scope],
        )
        .map_err(|_| QueueError::StorageUnavailable)?;
        let tick: i64 = tx
            .query_one(
                "UPDATE wam_gatekeeper_queue_meta SET fairness_tick = fairness_tick + 1 WHERE singleton = TRUE RETURNING fairness_tick",
                &[],
            )
            .map_err(|_| QueueError::StorageUnavailable)?
            .get(0);
        for contributor in contributors {
            tx.execute(
                "INSERT INTO wam_gatekeeper_queue_fairness (contributor_id, last_served_tick) VALUES ($1, $2) ON CONFLICT (contributor_id) DO UPDATE SET last_served_tick = EXCLUDED.last_served_tick",
                &[&contributor, &tick],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        }
        tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
        Ok(Some(Batch {
            scope_key: scope,
            job_ids,
            generations,
            worker: worker.to_owned(),
            lease_expires_at: expires,
        }))
    }

    fn backend_holds(&self, batch: &Batch) -> Result<bool, QueueError> {
        if batch.job_ids.len() != batch.generations.len() {
            return Ok(false);
        }
        let mut client = self.lock()?;
        for (id, generation) in batch.job_ids.iter().zip(&batch.generations) {
            let id = sql_u64(*id)?;
            let Some(row) = client
                .query_opt(
                    "SELECT scope_key, state, lease_generation, lease_holder FROM wam_gatekeeper_jobs WHERE id = $1",
                    &[&id],
                )
                .map_err(|_| QueueError::StorageUnavailable)?
            else {
                return Ok(false);
            };
            let scope: String = row.get(0);
            let state: String = row.get(1);
            let generation_matches = from_sql_u64(row.get(2))? == *generation;
            let holder: Option<String> = row.get(3);
            if scope != batch.scope_key
                || state != "leased"
                || !generation_matches
                || holder.as_deref() != Some(&batch.worker)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn backend_heartbeat(&mut self, batch: &mut Batch, now: u64) -> Result<u64, QueueError> {
        let expires = now
            .checked_add(self.config.lease_secs)
            .ok_or(QueueError::StorageUnavailable)?;
        let expires_db = sql_u64(expires)?;
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        check_batch_tx(&mut tx, batch)?;
        for id in &batch.job_ids {
            tx.execute(
                "UPDATE wam_gatekeeper_jobs SET lease_expires_at = $2 WHERE id = $1",
                &[&sql_u64(*id)?, &expires_db],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        }
        tx.commit().map_err(|_| QueueError::StorageUnavailable)?;
        batch.lease_expires_at = expires;
        Ok(expires)
    }

    fn backend_complete(&mut self, batch: &Batch, now: u64) -> Result<(), QueueError> {
        let now = sql_u64(now)?;
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        check_batch_tx(&mut tx, batch)?;
        for id in &batch.job_ids {
            tx.execute(
                "UPDATE wam_gatekeeper_jobs SET state = 'done', finished_at = $2, lease_expires_at = NULL, lease_holder = NULL WHERE id = $1",
                &[&sql_u64(*id)?, &now],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        }
        tx.commit().map_err(|_| QueueError::StorageUnavailable)
    }

    fn backend_fail(&mut self, batch: &Batch, now: u64, error: &str) -> Result<(), QueueError> {
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        check_batch_tx(&mut tx, batch)?;
        for (id, _) in batch.job_ids.iter().zip(&batch.generations) {
            let row = tx
                .query_one(
                    &format!(
                        "SELECT {JOB_COLUMNS} FROM wam_gatekeeper_jobs WHERE id = $1 FOR UPDATE"
                    ),
                    &[&sql_u64(*id)?],
                )
                .map_err(|_| QueueError::StorageUnavailable)?;
            fail_job_tx(&mut tx, &parse_job(row)?, &self.config, now, error)?;
        }
        tx.commit().map_err(|_| QueueError::StorageUnavailable)
    }

    fn backend_wait_budget(&mut self, batch: &Batch) -> Result<(), QueueError> {
        let mut client = self.lock()?;
        let mut tx = begin_transaction(&mut client)?;
        check_batch_tx(&mut tx, batch)?;
        for id in &batch.job_ids {
            tx.execute(
                "UPDATE wam_gatekeeper_jobs SET state = 'waiting_budget', lease_expires_at = NULL, lease_holder = NULL WHERE id = $1",
                &[&sql_u64(*id)?],
            )
            .map_err(|_| QueueError::StorageUnavailable)?;
        }
        tx.commit().map_err(|_| QueueError::StorageUnavailable)
    }
}
