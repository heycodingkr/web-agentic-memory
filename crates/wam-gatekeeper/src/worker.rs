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

use crate::budget::ModelBudget;
use crate::curator::{CuratorConfig, JobReport, Outcome, run_job};
use crate::provider::LlmProvider;
use crate::queue::{Batch, JobKind, QueueBackend, QueueError};
use crate::record::sha256_hex;
use crate::store::Store;
use crate::tools::{Candidate, Job, JobScope, Policy, Tools, Trigger};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wam_core::TaskVocabulary;

pub const BATCH_JOB_FORMAT_VERSION: u16 = 1;
pub const MAX_CURATOR_BATCH_BYTES: usize = 1024 * 1024;

/// Versioned, immutable input envelope for all candidates leased under one queue scope.
/// The scope-level decision has a stable digest across lease retries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuratorBatch {
    format_version: u16,
    scope_key: String,
    jobs: Vec<Job>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchFormatError {
    EmptyScope,
    EmptyJobs,
    InvalidLimit,
    TooManyCandidates,
    MixedJobScopes,
    MixedTriggers,
    MixedCandidateRoutes,
    MissingContributionMapping,
    UnexpectedContribution,
    DuplicateContribution,
    CandidateCombinesContributors,
    EvidenceOverflow,
    EmptyJobId,
    DuplicateJobId,
    TooLarge,
    UnsupportedVersion,
}

impl CuratorBatch {
    pub fn new(
        scope_key: impl Into<String>,
        jobs: Vec<Job>,
        max_candidates: usize,
    ) -> Result<Self, BatchFormatError> {
        let batch = Self {
            format_version: BATCH_JOB_FORMAT_VERSION,
            scope_key: scope_key.into(),
            jobs,
        };
        batch.validate(max_candidates)?;
        Ok(batch)
    }

    pub fn scope_key(&self) -> &str {
        &self.scope_key
    }

    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        sha256_hex(&bytes)
    }

    pub fn validate(&self, max_candidates: usize) -> Result<(), BatchFormatError> {
        if self.format_version != BATCH_JOB_FORMAT_VERSION {
            return Err(BatchFormatError::UnsupportedVersion);
        }
        if self.scope_key.trim().is_empty() {
            return Err(BatchFormatError::EmptyScope);
        }
        if max_candidates == 0 {
            return Err(BatchFormatError::InvalidLimit);
        }
        if self.jobs.is_empty() {
            return Err(BatchFormatError::EmptyJobs);
        }
        if self.jobs.len() > max_candidates {
            return Err(BatchFormatError::TooManyCandidates);
        }
        if serde_json::to_vec(self)
            .map(|bytes| bytes.len() > MAX_CURATOR_BATCH_BYTES)
            .unwrap_or(true)
        {
            return Err(BatchFormatError::TooLarge);
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut expected_scope: Option<&JobScope> = None;
        let mut expected_trigger = None;
        let mut expected_route: Option<&str> = None;
        for job in &self.jobs {
            if job.id.trim().is_empty() {
                return Err(BatchFormatError::EmptyJobId);
            }
            if !ids.insert(job.id.as_str()) {
                return Err(BatchFormatError::DuplicateJobId);
            }
            if expected_scope.is_some_and(|scope| scope != &job.scope) {
                return Err(BatchFormatError::MixedJobScopes);
            }
            expected_scope.get_or_insert(&job.scope);
            if expected_trigger.is_some_and(|trigger| trigger != job.trigger) {
                return Err(BatchFormatError::MixedTriggers);
            }
            expected_trigger.get_or_insert(job.trigger);
            if expected_route.is_some_and(|route| route != job.candidate.url_pattern) {
                return Err(BatchFormatError::MixedCandidateRoutes);
            }
            expected_route.get_or_insert(&job.candidate.url_pattern);
        }
        Ok(())
    }
}

/// Where the contributions behind a leased batch live (the database in deployment).
pub trait JobSource {
    /// Curator candidates for the contributions of one leased batch (one scope). The same contributions must give
    /// the same job ids and content, so that retries have a stable envelope digest and scope-decision idempotency.
    fn curator_jobs(&self, scope_key: &str, contribution_ids: &[String]) -> Vec<Job>;
}

fn candidate_quality(left: &Candidate, right: &Candidate) -> std::cmp::Ordering {
    left.evidence()
        .success_rate()
        .total_cmp(&right.evidence().success_rate())
        .then_with(|| left.outcomes.success.cmp(&right.outcomes.success))
        .then_with(|| right.outcomes.failed.cmp(&left.outcomes.failed))
        .then_with(|| right.action_sequence.len().cmp(&left.action_sequence.len()))
        .then_with(|| left.observed_at.cmp(&right.observed_at))
}

fn validate_contribution_mapping(
    jobs: &[Job],
    contributions: &[String],
) -> Result<(), BatchFormatError> {
    let expected: BTreeSet<&str> = contributions.iter().map(String::as_str).collect();
    let mut mapped = BTreeSet::new();
    for job in jobs {
        if job.contribution_ids.is_empty() {
            return Err(BatchFormatError::MissingContributionMapping);
        }
        for contribution_id in &job.contribution_ids {
            if !expected.contains(contribution_id.as_str()) {
                return Err(BatchFormatError::UnexpectedContribution);
            }
            if !mapped.insert(contribution_id.as_str()) {
                return Err(BatchFormatError::DuplicateContribution);
            }
        }
    }
    if mapped != expected {
        return Err(BatchFormatError::MissingContributionMapping);
    }
    Ok(())
}

/// Reduces one contribution job per account into a single scope-level curator input. Repeated submissions by the
/// same account cannot increase evidence; the strongest deterministic trace from that account represents it.
fn combine_new_contribution_jobs(
    jobs: &[Job],
    contributor_by_contribution: &BTreeMap<String, String>,
) -> Result<Job, BatchFormatError> {
    let Some(first) = jobs.first() else {
        return Err(BatchFormatError::EmptyJobs);
    };
    if jobs.len() == 1 {
        return Ok(first.clone());
    }
    if jobs.iter().any(|job| !job.batch_candidates.is_empty()) {
        return Err(BatchFormatError::CandidateCombinesContributors);
    }
    let mut best_by_contributor: BTreeMap<String, Job> = BTreeMap::new();
    for job in jobs {
        let contributors: BTreeSet<&str> = job
            .contribution_ids
            .iter()
            .map(|id| {
                contributor_by_contribution
                    .get(id)
                    .map(String::as_str)
                    .ok_or(BatchFormatError::MissingContributionMapping)
            })
            .collect::<Result<_, _>>()?;
        if contributors.len() != 1 {
            return Err(BatchFormatError::CandidateCombinesContributors);
        }
        let contributor = contributors
            .first()
            .expect("one contributor was validated")
            .to_string();
        match best_by_contributor.get(&contributor) {
            Some(current)
                if candidate_quality(&current.candidate, &job.candidate)
                    != std::cmp::Ordering::Less => {}
            _ => {
                best_by_contributor.insert(contributor, job.clone());
            }
        }
    }
    let selected_jobs: Vec<Job> = best_by_contributor.into_values().collect();
    let contributor_count =
        u32::try_from(selected_jobs.len()).map_err(|_| BatchFormatError::EvidenceOverflow)?;
    let representative = selected_jobs
        .iter()
        .max_by(|left, right| candidate_quality(&left.candidate, &right.candidate))
        .ok_or(BatchFormatError::EmptyJobs)?;
    let mut candidate = representative.candidate.clone();
    let mut outcomes = crate::tools::Outcomes {
        success: 0,
        partial: 0,
        failed: 0,
    };
    let mut traits = BTreeSet::new();
    for job in &selected_jobs {
        outcomes.success = outcomes
            .success
            .checked_add(job.candidate.outcomes.success)
            .ok_or(BatchFormatError::EvidenceOverflow)?;
        outcomes.partial = outcomes
            .partial
            .checked_add(job.candidate.outcomes.partial)
            .ok_or(BatchFormatError::EvidenceOverflow)?;
        outcomes.failed = outcomes
            .failed
            .checked_add(job.candidate.outcomes.failed)
            .ok_or(BatchFormatError::EvidenceOverflow)?;
        traits.extend(job.candidate.observed_traits.iter().cloned());
    }
    let ui_fingerprint = selected_jobs
        .iter()
        .all(|job| job.candidate.ui_fingerprint == selected_jobs[0].candidate.ui_fingerprint);
    candidate.outcomes = outcomes;
    candidate.observed_traits = traits.into_iter().collect();
    candidate.independent_contributors = contributor_count;
    candidate.ui_fingerprint = if ui_fingerprint {
        selected_jobs[0].candidate.ui_fingerprint.clone()
    } else {
        None
    };
    candidate.observed_at = selected_jobs
        .iter()
        .map(|job| job.candidate.observed_at)
        .max()
        .unwrap_or(candidate.observed_at);

    let mut selected_contributions = BTreeSet::new();
    for job in &selected_jobs {
        selected_contributions.extend(job.contribution_ids.iter().cloned());
    }
    let mut all_job_ids: Vec<&str> = jobs.iter().map(|job| job.id.as_str()).collect();
    all_job_ids.sort_unstable();
    let batch_identity =
        sha256_hex(format!("scope-batch-v1\n{}", all_job_ids.join("\n")).as_bytes());
    let mut batch_candidates = Vec::with_capacity(selected_jobs.len());
    for job in &selected_jobs {
        let mut candidate = job.candidate.clone();
        candidate.independent_contributors = 1;
        batch_candidates.push(candidate);
    }
    Ok(Job {
        id: format!("batch-{batch_identity}"),
        trigger: first.trigger,
        scope: first.scope.clone(),
        candidate,
        batch_candidates,
        contribution_ids: selected_contributions.into_iter().collect(),
        replay_passed: selected_jobs.iter().all(|job| job.replay_passed),
        lease_fence: None,
    })
}

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
            env.budget.record_cost(now, reported_cost(&report));
            match &report.outcome {
                Outcome::Decided => reports.push(report),
                Outcome::NoDecision(why) | Outcome::ProviderError(why) => {
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
