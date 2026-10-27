//! The worker: lease a batch from the queue, run the curator on it, and report back (docs/spec/queue.md).
//!
//! One worker handles one batch at a time. The same loop runs against the scripted fake in tests, a local CLI during
//! development, and an HTTP model in deployment: only the provider differs.
//!
//! In memory the decisions are applied as the curator makes them, so fencing is enforced by re-checking the lease
//! before every curator job. The database adapter applies a decision in one transaction that also checks the lease
//! generation (docs/spec/database.md), which closes the window completely.
//!
//! A leased batch is processed one curator job after another under the scope lease. Judging a whole batch in a
//! single model call (the saving the coalescing window is for) needs a multi-candidate job format; until then the
//! batch shares the lease, the budget check, and the retry, and a later contribution simply finds the memory that
//! an earlier one created.

use crate::budget::ModelBudget;
use crate::curator::{CuratorConfig, JobReport, Outcome, run_job};
use crate::provider::LlmProvider;
use crate::queue::{Batch, JobQueue};
use crate::store::Store;
use crate::tools::{Job, Policy, Tools};
use wam_core::TaskVocabulary;

/// Where the contributions behind a batch live (the database in deployment).
pub trait JobSource {
    /// Curator jobs for the contributions of one leased batch (one scope). The same contributions must give the
    /// same job ids and content, so that a retried batch is recognized as already decided.
    fn curator_jobs(&self, scope_key: &str, contribution_ids: &[String]) -> Vec<Job>;
}

/// Everything a worker touches. Borrowed, so that the caller decides what is shared and how.
pub struct Env<'a> {
    pub queue: &'a mut JobQueue,
    pub budget: &'a mut ModelBudget,
    pub store: &'a mut dyn Store,
    pub provider: &'a mut dyn LlmProvider,
    pub source: &'a dyn JobSource,
}

#[derive(Debug)]
pub enum Step {
    /// Nothing is ready to run.
    Idle,
    /// The batch was processed; `replayed` curator jobs had already been decided by an earlier attempt.
    Done {
        scope_key: String,
        reports: Vec<JobReport>,
        replayed: usize,
    },
    /// The model failed or did not decide: the batch is back in the queue with backoff, or dead after the last try.
    Retry { scope_key: String, error: String },
    /// The model budget is spent, or the provider said its quota is. The batch waits (`waiting_budget`); nothing
    /// was lost or charged. `provider_said` is the provider's own wording when it was the one that ran out.
    Parked {
        retry_after_secs: u64,
        daily: bool,
        provider_said: Option<String>,
    },
    /// This worker lost its lease while working; it applied nothing more.
    Fenced { scope_key: String },
}

pub struct Worker<'a> {
    pub name: String,
    pub tools: &'a Tools,
    pub tasks: &'a TaskVocabulary,
    pub policy: Policy,
    pub curator: CuratorConfig,
    /// Rough size of one curator pass, for the token bucket.
    pub estimated_tokens: u64,
    /// How long to stop calling the model when the provider says its quota is spent without saying when it
    /// returns (15 minutes is a sensible default).
    pub quota_pause_secs: u64,
}

fn reported_cost(report: &JobReport) -> f64 {
    report
        .turns
        .iter()
        .filter_map(|turn| turn.meta["cost_usd"].as_f64())
        .sum()
}

impl Worker<'_> {
    /// Takes the next batch, if any, and processes it.
    pub fn run_once(&self, env: &mut Env<'_>, now: u64) -> Step {
        if env.budget.has_room(now) {
            env.queue.release_budget(now);
        }
        match env.queue.lease(&self.name, now) {
            Some(batch) => self.process(env, batch, now),
            None => Step::Idle,
        }
    }

    /// Processes a leased batch: budget check, curator jobs, then complete, retry, or park.
    pub fn process(&self, env: &mut Env<'_>, mut batch: Batch, now: u64) -> Step {
        let scope_key = batch.scope_key.clone();
        if let Err(wait) = env.budget.try_acquire(now, self.estimated_tokens) {
            return match env.queue.wait_budget(&batch) {
                Ok(()) => Step::Parked {
                    retry_after_secs: wait.retry_after_secs,
                    daily: wait.daily,
                    provider_said: None,
                },
                Err(_) => Step::Fenced { scope_key },
            };
        }
        // A contributor who withdrew while the batch was leased is left out: no evidence, no attribution.
        let contributions: Vec<String> = batch
            .job_ids
            .iter()
            .filter_map(|id| env.queue.job(*id))
            .filter(|job| !job.attribution_withdrawn)
            .filter_map(|job| job.contribution_id.clone())
            .collect();
        let mut reports = Vec::new();
        let mut replayed = 0;
        for job in env.source.curator_jobs(&scope_key, &contributions) {
            if !env.queue.holds(&batch) {
                return Step::Fenced { scope_key };
            }
            if env.store.decided(&job.id, &job.digest()) {
                replayed += 1;
                continue;
            }
            let report = run_job(
                &job,
                &mut *env.store,
                &mut *env.provider,
                self.tools,
                &self.policy,
                self.tasks,
                &self.curator,
                now,
            );
            env.budget.record_cost(now, reported_cost(&report));
            match &report.outcome {
                Outcome::Decided => reports.push(report),
                Outcome::NoDecision(why) | Outcome::ProviderError(why) => {
                    let error = why.clone();
                    return match env.queue.fail(&batch, now, &error) {
                        Ok(()) => Step::Retry { scope_key, error },
                        Err(_) => Step::Fenced { scope_key },
                    };
                }
                Outcome::QuotaExhausted {
                    why,
                    retry_after_secs,
                } => {
                    // Nothing can run until the quota returns: park the batch (no attempt is charged) and keep
                    // every worker from calling the model meanwhile. Decisions already made stay applied; the
                    // retry skips them.
                    let pause = retry_after_secs.unwrap_or(self.quota_pause_secs);
                    env.budget.pause_until(now + pause);
                    let said = why.clone();
                    return match env.queue.wait_budget(&batch) {
                        Ok(()) => Step::Parked {
                            retry_after_secs: pause,
                            daily: false,
                            provider_said: Some(said),
                        },
                        Err(_) => Step::Fenced { scope_key },
                    };
                }
            }
            if env.queue.heartbeat(&mut batch, now).is_err() {
                return Step::Fenced { scope_key };
            }
        }
        match env.queue.complete(&batch, now) {
            Ok(()) => Step::Done {
                scope_key,
                reports,
                replayed,
            },
            Err(_) => Step::Fenced { scope_key },
        }
    }
}
