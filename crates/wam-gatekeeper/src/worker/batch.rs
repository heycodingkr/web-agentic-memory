use crate::record::sha256_hex;
use crate::tools::{Candidate, Job, JobScope};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;

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

fn candidate_quality(left: &Candidate, right: &Candidate) -> std::cmp::Ordering {
    left.evidence()
        .success_rate()
        .total_cmp(&right.evidence().success_rate())
        .then_with(|| left.outcomes.success.cmp(&right.outcomes.success))
        .then_with(|| right.outcomes.failed.cmp(&left.outcomes.failed))
        .then_with(|| right.action_sequence.len().cmp(&left.action_sequence.len()))
        .then_with(|| left.observed_at.cmp(&right.observed_at))
}

pub(super) fn validate_contribution_mapping(
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
pub(super) fn combine_new_contribution_jobs(
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
