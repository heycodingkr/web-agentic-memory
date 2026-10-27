//! Curation job queue (docs/spec/queue.md): accept fast, process safely, lose nothing.
//!
//! An in-memory queue with the policies of the spec. A Postgres adapter (`FOR UPDATE SKIP LOCKED`, advisory lock
//! per scope; docs/spec/database.md) implements the same behavior later. Time is passed in (`now`, seconds) so
//! tests control it. Nothing here calls a model: the worker leases a batch, runs the curator on it, then completes,
//! fails, or parks it while the model budget is exhausted.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Priority {
    /// Reverify, rollback: safety work that goes first.
    P0,
    P1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobKind {
    Contribution,
    Reverify,
    Withdrawal,
    Rollback,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum JobState {
    Queued,
    Leased,
    /// Another worker holds this scope.
    WaitingScope,
    /// The model budget is exhausted; the job is parked, not lost.
    WaitingBudget,
    /// Backing off after a failed attempt.
    Retry,
    Done,
    Dead,
    Canceled,
}

impl JobState {
    pub fn active(self) -> bool {
        !self.terminal()
    }

    pub fn terminal(self) -> bool {
        matches!(self, Self::Done | Self::Dead | Self::Canceled)
    }

    /// Waiting for a worker to pick it up.
    fn pending(self) -> bool {
        matches!(self, Self::Queued | Self::WaitingScope | Self::Retry)
    }

    /// Not finished and not being worked on.
    fn waiting(self) -> bool {
        self.pending() || self == Self::WaitingBudget
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountClass {
    Established,
    /// Anonymous or new accounts are limited first when the queue fills up.
    New,
}

#[derive(Clone, Debug)]
pub struct NewJob {
    pub kind: JobKind,
    pub scope_key: String,
    pub contributor: String,
    pub device: String,
    pub class: AccountClass,
    /// The submitter's idempotency key: resubmitting it returns the same job, even after withdrawal.
    pub nonce: String,
    pub contribution_id: String,
    pub priority: Priority,
}

#[derive(Clone, Debug)]
pub struct Job {
    pub id: u64,
    pub kind: JobKind,
    pub scope_key: String,
    pub contributor: String,
    pub device: String,
    pub class: AccountClass,
    /// `None` once the payload was purged (withdrawal).
    pub contribution_id: Option<String>,
    pub priority: Priority,
    pub enqueued_at: u64,
    pub state: JobState,
    pub attempts: u32,
    pub not_before: u64,
    /// Fencing token: grows with every lease, so a worker that lost its lease cannot commit.
    pub lease_generation: u64,
    pub lease_expires_at: Option<u64>,
    pub lease_holder: Option<String>,
    pub attribution_withdrawn: bool,
    pub finished_at: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct QueueConfig {
    pub max_total: usize,
    pub max_per_contributor: usize,
    pub max_per_device: usize,
    pub lease_secs: u64,
    /// Fresh contributions wait this long so that same-scope jobs can be judged together.
    pub coalesce_window_secs: u64,
    pub coalesce_max: usize,
    pub max_attempts: u32,
    pub backoff_base_secs: u64,
    pub backoff_cap_secs: u64,
    pub alert_after_secs: u64,
    pub retry_after_secs: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_total: 1000,
            max_per_contributor: 50,
            max_per_device: 50,
            lease_secs: 300,
            coalesce_window_secs: 60,
            coalesce_max: 20,
            max_attempts: 5,
            backoff_base_secs: 30,
            backoff_cap_secs: 1800,
            alert_after_secs: 900,
            retry_after_secs: 30,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnqueueError {
    /// The whole queue is at its depth limit.
    QueueFull {
        retry_after_secs: u64,
    },
    /// The queue is filling up: new and anonymous accounts are limited first.
    Shedding {
        retry_after_secs: u64,
    },
    ContributorLimit {
        retry_after_secs: u64,
    },
    DeviceLimit {
        retry_after_secs: u64,
    },
}

impl EnqueueError {
    pub fn http_status(&self) -> u16 {
        match self {
            Self::QueueFull { .. } => 503,
            _ => 429,
        }
    }

    pub fn retry_after_secs(&self) -> u64 {
        match self {
            Self::QueueFull { retry_after_secs }
            | Self::Shedding { retry_after_secs }
            | Self::ContributorLimit { retry_after_secs }
            | Self::DeviceLimit { retry_after_secs } => *retry_after_secs,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueueError {
    UnknownJob,
    /// The lease was taken over (or the job finished): this worker must drop its result.
    StaleLease,
}

#[derive(Clone, Debug)]
pub struct Batch {
    pub scope_key: String,
    pub job_ids: Vec<u64>,
    /// The fencing token of each job at lease time, in the order of `job_ids`.
    pub generations: Vec<u64>,
    pub worker: String,
    pub lease_expires_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelReport {
    pub canceled: usize,
    pub still_leased: usize,
}

#[derive(Clone, Debug)]
pub struct Stats {
    pub active: usize,
    pub by_state: BTreeMap<JobState, usize>,
    pub oldest_age_secs: u64,
    pub alert: bool,
}

pub struct JobQueue {
    cfg: QueueConfig,
    jobs: BTreeMap<u64, Job>,
    nonces: BTreeMap<(String, String), u64>,
    /// When each contributor was last served (a logical tick): the one served longest ago goes next.
    last_served: BTreeMap<String, u64>,
    tick: u64,
    next_id: u64,
}

fn backoff(cfg: &QueueConfig, job: &Job) -> u64 {
    let exponent = job.attempts.saturating_sub(1).min(20);
    let capped = cfg
        .backoff_base_secs
        .saturating_mul(1u64 << exponent)
        .min(cfg.backoff_cap_secs);
    // Jitter keeps retries of different jobs from lining up; it is derived from the job so tests are repeatable.
    let jitter = job.id.wrapping_mul(7919) ^ u64::from(job.attempts).wrapping_mul(104_729);
    capped + jitter % (capped / 2 + 1)
}

impl JobQueue {
    pub fn new(cfg: QueueConfig) -> Self {
        Self {
            cfg,
            jobs: BTreeMap::new(),
            nonces: BTreeMap::new(),
            last_served: BTreeMap::new(),
            tick: 0,
            next_id: 1,
        }
    }

    pub fn config(&self) -> &QueueConfig {
        &self.cfg
    }

    pub fn job(&self, id: u64) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    fn active_count(&self) -> usize {
        self.jobs.values().filter(|job| job.state.active()).count()
    }

    fn scope_held(&self, scope: &str) -> bool {
        self.jobs
            .values()
            .any(|job| job.state == JobState::Leased && job.scope_key == scope)
    }

    /// Accepts a job or refuses it up front. A job that was accepted is never dropped to make room.
    pub fn enqueue(&mut self, new: NewJob, now: u64) -> Result<u64, EnqueueError> {
        if let Some(existing) = self
            .nonces
            .get(&(new.contributor.clone(), new.nonce.clone()))
        {
            return Ok(*existing);
        }
        let retry_after_secs = self.cfg.retry_after_secs;
        let system = new.priority == Priority::P0;
        if !system {
            let active = self.jobs.values().filter(|job| job.state.active());
            let (mine, on_device) = active.fold((0, 0), |(mine, device), job| {
                (
                    mine + usize::from(job.contributor == new.contributor),
                    device + usize::from(job.device == new.device),
                )
            });
            if mine >= self.cfg.max_per_contributor {
                return Err(EnqueueError::ContributorLimit { retry_after_secs });
            }
            if on_device >= self.cfg.max_per_device {
                return Err(EnqueueError::DeviceLimit { retry_after_secs });
            }
        }
        let active = self.active_count();
        // Safety work (reverify, rollback) gets headroom above the limit so that it is never locked out.
        let ceiling = if system {
            self.cfg.max_total + self.cfg.max_total / 5
        } else {
            self.cfg.max_total
        };
        if active >= ceiling {
            return Err(EnqueueError::QueueFull { retry_after_secs });
        }
        if !system && new.class == AccountClass::New && active >= self.cfg.max_total * 8 / 10 {
            return Err(EnqueueError::Shedding { retry_after_secs });
        }
        let id = self.next_id;
        self.next_id += 1;
        let state = if self.scope_held(&new.scope_key) {
            JobState::WaitingScope
        } else {
            JobState::Queued
        };
        self.nonces.insert((new.contributor.clone(), new.nonce), id);
        self.jobs.insert(
            id,
            Job {
                id,
                kind: new.kind,
                scope_key: new.scope_key,
                contributor: new.contributor,
                device: new.device,
                class: new.class,
                contribution_id: Some(new.contribution_id),
                priority: new.priority,
                enqueued_at: now,
                state,
                attempts: 0,
                not_before: now,
                lease_generation: 0,
                lease_expires_at: None,
                lease_holder: None,
                attribution_withdrawn: false,
                finished_at: None,
                last_error: None,
            },
        );
        Ok(id)
    }

    /// Next batch for a worker: the most urgent scope, chosen fairly across contributors, with every waiting job of
    /// that scope (up to the coalescing limit) so that one curator pass judges them together. A scope opens its
    /// coalescing window when its first job arrives; it is released when the window is over, the batch is full, or
    /// the work is urgent or a retry. A scope that another worker holds is never handed out.
    pub fn lease(&mut self, worker: &str, now: u64) -> Option<Batch> {
        self.reap(now);
        let held: BTreeSet<String> = self
            .jobs
            .values()
            .filter(|job| job.state == JobState::Leased)
            .map(|job| job.scope_key.clone())
            .collect();
        let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        for job in self
            .jobs
            .values()
            .filter(|job| job.state.pending() && job.not_before <= now)
        {
            groups
                .entry(job.scope_key.clone())
                .or_default()
                .push(job.id);
        }
        let mut best: Option<((Priority, u64, u64), String)> = None;
        for (scope, ids) in &groups {
            if held.contains(scope) {
                for id in ids {
                    self.jobs.get_mut(id).expect("job exists").state = JobState::WaitingScope;
                }
                continue;
            }
            let released = ids.len() >= self.cfg.coalesce_max
                || ids.iter().any(|id| {
                    let job = &self.jobs[id];
                    job.priority == Priority::P0
                        || job.attempts > 0
                        || now >= job.enqueued_at + self.cfg.coalesce_window_secs
                });
            if !released {
                continue;
            }
            let key = ids
                .iter()
                .map(|id| {
                    let job = &self.jobs[id];
                    (
                        job.priority,
                        self.last_served.get(&job.contributor).copied().unwrap_or(0),
                        job.id,
                    )
                })
                .min()
                .expect("a group is never empty");
            if best.as_ref().is_none_or(|(top, _)| key < *top) {
                best = Some((key, scope.clone()));
            }
        }
        let (_, scope) = best?;
        let mut ids = groups.remove(&scope).expect("the chosen group exists");
        ids.sort_by_key(|id| (self.jobs[id].priority, *id));
        ids.truncate(self.cfg.coalesce_max);
        let expires = now + self.cfg.lease_secs;
        self.tick += 1;
        let mut generations = Vec::with_capacity(ids.len());
        for id in &ids {
            let job = self.jobs.get_mut(id).expect("job exists");
            job.state = JobState::Leased;
            job.lease_generation += 1;
            job.lease_expires_at = Some(expires);
            job.lease_holder = Some(worker.to_owned());
            generations.push(job.lease_generation);
            self.last_served.insert(job.contributor.clone(), self.tick);
        }
        Some(Batch {
            scope_key: scope,
            job_ids: ids,
            generations,
            worker: worker.to_owned(),
            lease_expires_at: expires,
        })
    }

    /// Fencing: only the holder of the newest generation may report a result. A worker whose lease ran out but
    /// whom nobody replaced yet may still finish (the scope was not touched meanwhile).
    fn check_lease(&self, batch: &Batch) -> Result<(), QueueError> {
        for (id, generation) in batch.job_ids.iter().zip(&batch.generations) {
            let job = self.jobs.get(id).ok_or(QueueError::UnknownJob)?;
            if job.state != JobState::Leased || job.lease_generation != *generation {
                return Err(QueueError::StaleLease);
            }
        }
        Ok(())
    }

    /// Does this worker still hold the newest lease of every job in the batch?
    pub fn holds(&self, batch: &Batch) -> bool {
        self.check_lease(batch).is_ok()
    }

    /// Extends the lease of a batch that is still being worked on.
    pub fn heartbeat(&mut self, batch: &mut Batch, now: u64) -> Result<u64, QueueError> {
        self.check_lease(batch)?;
        let expires = now + self.cfg.lease_secs;
        for id in &batch.job_ids {
            self.jobs.get_mut(id).expect("checked").lease_expires_at = Some(expires);
        }
        batch.lease_expires_at = expires;
        Ok(expires)
    }

    pub fn complete(&mut self, batch: &Batch, now: u64) -> Result<(), QueueError> {
        self.check_lease(batch)?;
        for id in &batch.job_ids {
            let job = self.jobs.get_mut(id).expect("checked");
            job.state = JobState::Done;
            job.finished_at = Some(now);
            job.lease_expires_at = None;
            job.lease_holder = None;
        }
        Ok(())
    }

    /// A failed attempt (model error, timeout): retry with exponential backoff and jitter, then dead-letter.
    pub fn fail(&mut self, batch: &Batch, now: u64, error: &str) -> Result<(), QueueError> {
        self.check_lease(batch)?;
        for id in &batch.job_ids {
            self.fail_job(*id, now, error);
        }
        Ok(())
    }

    fn fail_job(&mut self, id: u64, now: u64, error: &str) {
        let job = self.jobs.get_mut(&id).expect("job exists");
        job.attempts += 1;
        job.lease_expires_at = None;
        job.lease_holder = None;
        job.last_error = Some(error.to_owned());
        if job.attempts >= self.cfg.max_attempts {
            job.state = JobState::Dead;
            job.finished_at = Some(now);
        } else {
            job.state = JobState::Retry;
            job.not_before = now + backoff(&self.cfg, job);
        }
    }

    /// The model budget is used up: park the batch without spending an attempt. Nothing is lost.
    pub fn wait_budget(&mut self, batch: &Batch) -> Result<(), QueueError> {
        self.check_lease(batch)?;
        for id in &batch.job_ids {
            let job = self.jobs.get_mut(id).expect("checked");
            job.state = JobState::WaitingBudget;
            job.lease_expires_at = None;
            job.lease_holder = None;
        }
        Ok(())
    }

    /// The budget is back: parked jobs become ready again.
    pub fn release_budget(&mut self, now: u64) -> usize {
        let mut released = 0;
        for job in self
            .jobs
            .values_mut()
            .filter(|job| job.state == JobState::WaitingBudget)
        {
            job.state = JobState::Queued;
            job.not_before = now;
            released += 1;
        }
        released
    }

    /// Leases that expired (a worker died): the job goes back at once, costing one attempt, so that a job that
    /// keeps killing its worker ends up dead instead of looping.
    pub fn reap(&mut self, now: u64) -> usize {
        let expired: Vec<u64> = self
            .jobs
            .values()
            .filter(|job| {
                job.state == JobState::Leased && job.lease_expires_at.is_some_and(|at| at <= now)
            })
            .map(|job| job.id)
            .collect();
        for id in &expired {
            self.fail_job(*id, now, "lease expired");
            let job = self.jobs.get_mut(id).expect("job exists");
            if job.state == JobState::Retry {
                job.state = JobState::Queued;
                job.not_before = now;
            }
        }
        expired.len()
    }

    /// Consent withdrawn: waiting jobs are canceled and their payload reference purged. A job that is already
    /// being worked on may finish, but its result carries no attribution. The nonces stay, so a delayed replay of
    /// an old request cannot bring a canceled job back.
    pub fn cancel_contributor(&mut self, contributor: &str, now: u64) -> CancelReport {
        let (mut canceled, mut still_leased) = (0, 0);
        for job in self
            .jobs
            .values_mut()
            .filter(|job| job.contributor == contributor)
        {
            if job.state.waiting() {
                job.state = JobState::Canceled;
                job.contribution_id = None;
                job.finished_at = Some(now);
                canceled += 1;
            } else if job.state == JobState::Leased {
                job.attribution_withdrawn = true;
                still_leased += 1;
            }
        }
        CancelReport {
            canceled,
            still_leased,
        }
    }

    /// Drops finished jobs after the retention period (the payload purge of docs/spec/privacy.md).
    pub fn purge_finished(&mut self, now: u64, retention_secs: u64) -> usize {
        let before = self.jobs.len();
        self.jobs.retain(|_, job| {
            !(job.state.terminal() && job.finished_at.is_some_and(|at| at + retention_secs <= now))
        });
        self.nonces.retain(|_, id| self.jobs.contains_key(id));
        before - self.jobs.len()
    }

    pub fn stats(&self, now: u64) -> Stats {
        let mut by_state = BTreeMap::new();
        let mut oldest = 0;
        for job in self.jobs.values() {
            *by_state.entry(job.state).or_insert(0) += 1;
            if job.state.waiting() {
                oldest = oldest.max(now.saturating_sub(job.enqueued_at));
            }
        }
        Stats {
            active: self.active_count(),
            by_state,
            oldest_age_secs: oldest,
            alert: oldest > self.cfg.alert_after_secs,
        }
    }
}
