//! Curation job queue (docs/spec/queue.md): accept fast, process safely, lose nothing.
//!
//! An in-memory queue with the policies of the spec. The Postgres adapter (`FOR UPDATE SKIP LOCKED`, advisory lock
//! per scope; docs/spec/database.md) implements the same backend contract. Time is passed in (`now`, seconds) so
//! tests control it. Nothing here calls a model: the worker leases a batch, runs the curator on it, then completes,
//! fails, or parks it while the model budget is exhausted.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod backoff;
mod memory;

pub(crate) use backoff::retry_delay;

type ScopeKindKey = (String, JobKind);
type LeasePriorityKey = (Priority, u64, u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Priority {
    /// Reverify, rollback: safety work that goes first.
    P0,
    P1,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    /// The queue database could not safely complete an operation.
    StorageUnavailable,
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

/// Durable commit fence copied into each curator job. It is excluded from the immutable curator input digest because
/// a retried queue lease gets a new generation while the contribution content remains the same.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueueLeaseFence {
    pub worker: String,
    pub scope_key: String,
    pub job_ids: Vec<u64>,
    pub generations: Vec<u64>,
    /// Contribution rows whose evidence this curator job uses.
    pub contribution_ids: Vec<String>,
    /// New contributions require an active consent/attribution check at decision commit time.
    pub require_attribution: bool,
}

impl Batch {
    pub fn fence(&self) -> QueueLeaseFence {
        QueueLeaseFence {
            worker: self.worker.clone(),
            scope_key: self.scope_key.clone(),
            job_ids: self.job_ids.clone(),
            generations: self.generations.clone(),
            contribution_ids: Vec::new(),
            require_attribution: false,
        }
    }
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

/// Queue operations needed by a worker. Owned jobs let local and durable adapters share the worker without exposing
/// an in-memory map or a database row lifetime.
pub trait QueueBackend {
    fn backend_config(&self) -> &QueueConfig;
    fn backend_job(&self, id: u64) -> Result<Option<Job>, QueueError>;
    fn backend_release_budget(&mut self, now: u64) -> Result<usize, QueueError>;
    fn backend_lease(&mut self, worker: &str, now: u64) -> Result<Option<Batch>, QueueError>;
    fn backend_holds(&self, batch: &Batch) -> Result<bool, QueueError>;
    fn backend_heartbeat(&mut self, batch: &mut Batch, now: u64) -> Result<u64, QueueError>;
    fn backend_complete(&mut self, batch: &Batch, now: u64) -> Result<(), QueueError>;
    fn backend_fail(&mut self, batch: &Batch, now: u64, error: &str) -> Result<(), QueueError>;
    fn backend_wait_budget(&mut self, batch: &Batch) -> Result<(), QueueError>;
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
