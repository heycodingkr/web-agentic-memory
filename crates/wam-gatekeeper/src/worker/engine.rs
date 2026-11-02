use super::batch::{combine_new_contribution_jobs, validate_contribution_mapping};
use super::{CuratorBatch, JobSource};
use crate::budget::ModelBudget;
use crate::curator::{CuratorConfig, JobReport, Outcome, run_job};
use crate::provider::LlmProvider;
use crate::queue::{Batch, JobKind, QueueBackend, QueueError};
use crate::record::sha256_hex;
use crate::store::Store;
use crate::tools::{Policy, Tools, Trigger};
use std::collections::{BTreeMap, BTreeSet};
use wam_core::TaskVocabulary;

/// Everything a worker touches. Borrowed, so that the caller decides what is shared and how.
pub struct Env<'a> {
    pub queue: &'a mut dyn QueueBackend,
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
        batch_digest: String,
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
    /// Queue storage failed; a leased batch is left to expire and recover through the normal retry path.
    Unavailable { operation: &'static str },
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

impl Worker<'_> {
    /// Takes the next batch, if any, and processes it.
    pub fn run_once(&self, env: &mut Env<'_>, now: u64) -> Step {
        if env.budget.has_room(now) && env.queue.backend_release_budget(now).is_err() {
            return Step::Unavailable {
                operation: "release budget jobs",
            };
        }
        match env.queue.backend_lease(&self.name, now) {
            Ok(Some(batch)) => self.process(env, batch, now),
            Ok(None) => Step::Idle,
            Err(_) => Step::Unavailable {
                operation: "lease queue jobs",
            },
        }
    }

    /// Processes a leased batch: budget check, validated v1 envelope, curator jobs, then complete, retry, or park.
    pub fn process(&self, env: &mut Env<'_>, mut batch: Batch, now: u64) -> Step {
        let scope_key = batch.scope_key.clone();
        if let Err(wait) = env.budget.try_acquire(now, self.estimated_tokens) {
            return match env.queue.backend_wait_budget(&batch) {
                Ok(()) => Step::Parked {
                    retry_after_secs: wait.retry_after_secs,
                    daily: wait.daily,
                    provider_said: None,
                },
                Err(error) => queue_error_step(scope_key, error, "park jobs for model budget"),
            };
        }
        // A contributor who withdrew while the batch was leased is left out: no evidence, no attribution.
        let mut contributions = BTreeSet::new();
        let mut contributor_by_contribution = BTreeMap::new();
        let mut only_withdrawn_contribution_batch = true;
        for id in &batch.job_ids {
            match env.queue.backend_job(*id) {
                Ok(Some(job)) => {
                    if job.kind != JobKind::Contribution || !job.attribution_withdrawn {
                        only_withdrawn_contribution_batch = false;
                    }
                    if !job.attribution_withdrawn {
                        if let Some(contribution_id) = job.contribution_id {
                            contributions.insert(contribution_id.clone());
                            contributor_by_contribution.insert(contribution_id, job.contributor);
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    return Step::Unavailable {
                        operation: "load leased queue jobs",
                    };
                }
            }
        }
        if contributions.is_empty() && only_withdrawn_contribution_batch {
            let batch_digest = sha256_hex(format!("withdrawn:{:?}", batch.job_ids).as_bytes());
            return match env.queue.backend_complete(&batch, now) {
                Ok(()) => Step::Done {
                    scope_key,
                    reports: Vec::new(),
                    replayed: 0,
                    batch_digest,
                },
                Err(error) => queue_error_step(scope_key, error, "complete withdrawn jobs"),
            };
        }
        let contribution_ids: Vec<String> = contributions.into_iter().collect();
        let jobs = env.source.curator_jobs(&scope_key, &contribution_ids);
        let curation_batch =
            match CuratorBatch::new(&scope_key, jobs, env.queue.backend_config().coalesce_max) {
                Ok(batch) => batch,
                Err(error) => {
                    let error = format!("invalid curator batch: {error:?}");
                    return match env.queue.backend_fail(&batch, now, &error) {
                        Ok(()) => Step::Retry { scope_key, error },
                        Err(queue_error) => {
                            queue_error_step(scope_key, queue_error, "fail invalid curator batch")
                        }
                    };
                }
            };
        let batch_digest = curation_batch.digest();
        if curation_batch
            .jobs()
            .iter()
            .all(|job| job.trigger == Trigger::NewContribution)
        {
            if let Err(error) =
                validate_contribution_mapping(curation_batch.jobs(), &contribution_ids)
            {
                let error = format!("invalid contribution mapping: {error:?}");
                return match env.queue.backend_fail(&batch, now, &error) {
                    Ok(()) => Step::Retry { scope_key, error },
                    Err(queue_error) => queue_error_step(
                        scope_key,
                        queue_error,
                        "fail invalid contribution mapping",
                    ),
                };
            }
        }
        let mut jobs_to_judge = if curation_batch.jobs().len() > 1
            && curation_batch
                .jobs()
                .iter()
                .all(|job| job.trigger == Trigger::NewContribution)
        {
            match combine_new_contribution_jobs(curation_batch.jobs(), &contributor_by_contribution)
            {
                Ok(job) => vec![job],
                Err(error) => {
                    let error = format!("cannot combine curator candidates: {error:?}");
                    return match env.queue.backend_fail(&batch, now, &error) {
                        Ok(()) => Step::Retry { scope_key, error },
                        Err(queue_error) => queue_error_step(
                            scope_key,
                            queue_error,
                            "fail to combine curator candidates",
                        ),
                    };
                }
            }
        } else {
            curation_batch.jobs().to_vec()
        };
        let batch_fence = batch.fence();
        for job in &mut jobs_to_judge {
            let mut lease_fence = batch_fence.clone();
            lease_fence.contribution_ids = job.contribution_ids.clone();
            lease_fence.require_attribution = job.trigger == Trigger::NewContribution;
            job.lease_fence = Some(lease_fence);
        }
        let mut reports = Vec::new();
        let mut replayed = 0;
        for job in &jobs_to_judge {
            match env.queue.backend_holds(&batch) {
                Ok(true) => {}
                Ok(false) => return Step::Fenced { scope_key },
                Err(_) => {
                    return Step::Unavailable {
                        operation: "check queue lease",
                    };
                }
            }
            match env.store.decided(&job.id, &job.digest()) {
                Ok(true) => {
                    replayed += 1;
                    continue;
                }
                Ok(false) => {}
                Err(_) => {
                    let error = "memory store is temporarily unavailable".to_owned();
                    return match env.queue.backend_fail(&batch, now, &error) {
                        Ok(()) => Step::Retry { scope_key, error },
                        Err(queue_error) => queue_error_step(
                            scope_key,
                            queue_error,
                            "fail unavailable memory store",
                        ),
                    };
                }
            }
            let report = run_job(
                job,
                &mut *env.store,
                &mut *env.provider,
                self.tools,
                &self.policy,
                self.tasks,
                &self.curator,
                now,
            );
            env.budget.record_cost(now, report.cost_usd());
            match &report.outcome {
                Outcome::Decided => reports.push(report),
                // The standalone engine keeps treating an outage or a rejected key like any other failed
                // attempt; only the application worker waits them out (docs/spec/provider-outage.md, out of
                // scope).
                Outcome::NoDecision(why)
                | Outcome::ProviderError(why)
                | Outcome::Unavailable { why, .. }
                | Outcome::ProviderRejected { why } => {
                    let error = why.clone();
                    return match env.queue.backend_fail(&batch, now, &error) {
                        Ok(()) => Step::Retry { scope_key, error },
                        Err(queue_error) => {
                            queue_error_step(scope_key, queue_error, "retry failed curator job")
                        }
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
                    return match env.queue.backend_wait_budget(&batch) {
                        Ok(()) => Step::Parked {
                            retry_after_secs: pause,
                            daily: false,
                            provider_said: Some(said),
                        },
                        Err(queue_error) => {
                            queue_error_step(scope_key, queue_error, "park quota exhausted jobs")
                        }
                    };
                }
            }
            match env.queue.backend_heartbeat(&mut batch, now) {
                Ok(_) => {}
                Err(QueueError::StorageUnavailable) => {
                    return Step::Unavailable {
                        operation: "extend queue lease",
                    };
                }
                Err(_) => return Step::Fenced { scope_key },
            }
        }
        match env.queue.backend_complete(&batch, now) {
            Ok(()) => Step::Done {
                scope_key,
                reports,
                replayed,
                batch_digest,
            },
            Err(error) => queue_error_step(scope_key, error, "complete queue jobs"),
        }
    }
}

fn queue_error_step(scope_key: String, error: QueueError, operation: &'static str) -> Step {
    match error {
        QueueError::StorageUnavailable => Step::Unavailable { operation },
        QueueError::UnknownJob | QueueError::StaleLease => Step::Fenced { scope_key },
    }
}
