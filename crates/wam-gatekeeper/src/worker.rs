//! The worker: lease a batch from the queue, run the curator on it, and report back (docs/spec/queue.md).
//!
//! One worker handles one batch at a time. The same loop runs against the scripted fake in tests, a local CLI during
//! development, and an HTTP model in deployment: only the provider differs.
//!
//! In memory the decisions are applied as the curator makes them, so fencing is enforced by re-checking the lease
//! before every curator job. The database adapter applies a decision in one transaction that also checks the lease
//! generation (docs/spec/database.md), which closes the window completely.
//!
//! A leased batch is validated as a versioned multi-candidate input envelope. New contributions from distinct
//! accounts are reduced to one scope-level candidate set and judged in one curator interaction under the scope lease.
//! A later batch sees the memory committed by the earlier one.

use crate::tools::Job;

/// Where the contributions behind a leased batch live (the database in deployment).
pub trait JobSource {
    /// Curator candidates for the contributions of one leased batch (one scope). The same contributions must give
    /// the same job ids and content, so that retries have a stable envelope digest and scope-decision idempotency.
    fn curator_jobs(&self, scope_key: &str, contribution_ids: &[String]) -> Vec<Job>;
}

mod batch;
mod engine;

pub use batch::{
    BATCH_JOB_FORMAT_VERSION, BatchFormatError, CuratorBatch, MAX_CURATOR_BATCH_BYTES,
};
pub use engine::{Env, Step, Worker};
