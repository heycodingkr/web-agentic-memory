//! Gatekeeper runs and browser replay happen outside transactions. Only its
//! validated final tool call is replayed inside the publication transaction.

use super::{
    ApplicationRepository,
    adapters::append_event,
    budget::{self, Answer, ModelBudgetPolicy},
    state::{AppState, Attribution},
};
use crate::{application::ServiceError, ports::Repository};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use wam_gatekeeper::{
    curator::{CuratorConfig, Outcome, run_job},
    provider::LlmProvider,
    record::{RecordPayload, sha256_hex},
    tools::{Candidate, Job, JobScope, Outcomes, Policy, ToolCtx, Tools, Trigger},
};

const LEASE_SECS: u64 = 300;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayRequest {
    pub schema_version: u8,
    pub scope: JobScope,
    pub candidate: Candidate,
    pub baseline: Option<RecordPayload>,
    pub proposed_record: Option<RecordPayload>,
    pub input_digest: String,
}
impl ReplayRequest {
    pub(super) fn new(
        job: &Job,
        baseline: Option<RecordPayload>,
        proposed_record: Option<RecordPayload>,
    ) -> Self {
        let mut request = Self {
            schema_version: 1,
            scope: job.scope.clone(),
            candidate: job.candidate.clone(),
            baseline,
            proposed_record,
            input_digest: String::new(),
        };
        request.input_digest = sha256_hex(&serde_json::to_vec(&request).unwrap_or_default());
        request
    }
    fn valid_digest(&self) -> bool {
        let mut unsigned = self.clone();
        unsigned.input_digest.clear();
        sha256_hex(&serde_json::to_vec(&unsigned).unwrap_or_default()) == self.input_digest
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayEvidence {
    pub input_digest: String,
    pub samples: u32,
    pub baseline_successes: u32,
    pub candidate_successes: u32,
    pub baseline_tool_calls: u32,
    pub candidate_tool_calls: u32,
}
impl ReplayEvidence {
    pub(super) fn passes(&self, request: &ReplayRequest) -> bool {
        request.valid_digest()
            && self.input_digest == request.input_digest
            && self.samples >= 3
            && self.samples <= 100
            && self.baseline_successes <= self.samples
            && self.candidate_successes == self.samples
            && self.candidate_tool_calls > 0
            && (self.candidate_successes > self.baseline_successes
                || self.candidate_tool_calls < self.baseline_tool_calls)
    }
}
pub trait ReplayVerifier {
    fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError>;
}

/// Operator-selected executable, with a closed JSON stdin/stdout protocol.
/// This never executes a command supplied by a contribution or a model.
pub struct CommandReplayVerifier {
    program: String,
    args: Vec<String>,
    timeout: Duration,
}
impl CommandReplayVerifier {
    pub fn new(
        program: impl Into<String>,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<Self, ServiceError> {
        let program = program.into();
        if program.is_empty() || timeout.is_zero() || timeout > Duration::from_secs(600) {
            return Err(ServiceError::InvalidInput("invalid_replay_configuration"));
        }
        Ok(Self {
            program,
            args,
            timeout,
        })
    }
}
impl ReplayVerifier for CommandReplayVerifier {
    fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ServiceError::Unavailable)?;
        let output = child.stdout.take().ok_or(ServiceError::Unavailable)?;
        let mut stdin = child.stdin.take().ok_or(ServiceError::Unavailable)?;
        let input = serde_json::to_vec(request).map_err(|_| ServiceError::Unavailable)?;
        let (read_tx, read_rx) = std::sync::mpsc::sync_channel(1);
        let (write_tx, write_rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = output.take(65537).read_to_end(&mut bytes).map(|_| bytes);
            let _ = read_tx.send(result);
        });
        std::thread::spawn(move || {
            let _ = write_tx.send(stdin.write_all(&input));
        });
        let started = Instant::now();
        let mut captured = None;
        let mut written = false;
        loop {
            if captured.is_none() {
                if let Ok(result) = read_rx.try_recv() {
                    captured = Some(result.map_err(|_| ServiceError::Unavailable)?);
                }
            }
            if !written {
                if let Ok(result) = write_rx.try_recv() {
                    if result.is_err() {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ServiceError::Unavailable);
                    }
                    written = true;
                }
            }
            if let Some(status) = child.try_wait().map_err(|_| ServiceError::Unavailable)? {
                if !status.success() {
                    return Err(ServiceError::Unavailable);
                }
                if written {
                    if let Some(bytes) = captured.take() {
                        if bytes.len() > 65536 {
                            return Err(ServiceError::Unavailable);
                        }
                        return serde_json::from_slice(&bytes)
                            .map_err(|_| ServiceError::InvalidInput("invalid_replay_evidence"));
                    }
                }
            }
            if started.elapsed() >= self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ServiceError::Unavailable);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Proposal {
    pub contribution_id: String,
    pub generation: u64,
    pub holder: String,
    pub job: Job,
    pub tool: String,
    pub args: Value,
    pub catalog_digest: String,
    pub replay: ReplayEvidence,
    pub replay_request: ReplayRequest,
    pub audit: wam_gatekeeper::store::AuditRecord,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkResult {
    Idle,
    Published {
        memory_id: String,
        version_id: String,
    },
    Supported {
        memory_id: String,
    },
    Retired {
        memory_id: String,
    },
    NeedsReview,
    Rejected,
    Retry,
}

fn allowed_record(job: &Job, record: &RecordPayload) -> bool {
    let text = serde_json::to_string(record)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if [
        "authorization:",
        "bearer ",
        "cookie:",
        "set-cookie:",
        "password=",
        "api_key=",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        return false;
    }
    let Ok(source) = wam_core::CanonicalUrl::parse(
        &format!(
            "https://{}{}",
            job.scope.site_host, job.candidate.url_pattern
        ),
        wam_core::UrlMode::Public,
    ) else {
        return false;
    };
    !record.l0.url_patterns.is_empty()
        && record.l0.url_patterns.iter().all(|route| {
            wam_gatekeeper::record::build_pattern(
                &record.l0.site_host,
                route,
                wam_core::UrlMode::Public,
            )
            .is_ok_and(|pattern| pattern == source.exact_pattern())
        })
}

struct Heartbeat {
    stop: std::sync::mpsc::Sender<()>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Heartbeat {
    fn start(
        repository: ApplicationRepository,
        id: String,
        generation: u64,
        holder: String,
        now: u64,
        started: Instant,
    ) -> Self {
        let (stop, receiver) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            while matches!(
                receiver.recv_timeout(Duration::from_secs(LEASE_SECS / 3)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                if repository
                    .renew(
                        &id,
                        generation,
                        &holder,
                        now.saturating_add(started.elapsed().as_secs()),
                    )
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            stop,
            join: Some(join),
        }
    }
}
impl Drop for Heartbeat {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub(super) fn catalog_digest(state: &AppState, host: &str) -> String {
    let records: Vec<_> = state
        .catalog
        .memories
        .values()
        .filter(|row| row.scope.host == host)
        .collect();
    sha256_hex(&serde_json::to_vec(&records).unwrap_or_default())
}

impl ApplicationRepository {
    /// Processes the next contribution job with no daily ceiling; see [`Self::process_next_budgeted`].
    pub fn process_next(
        &self,
        provider: &mut dyn LlmProvider,
        replay: &dyn ReplayVerifier,
        now: u64,
    ) -> Result<WorkResult, ServiceError> {
        self.process_next_budgeted(provider, replay, &ModelBudgetPolicy::default(), now)
    }

    /// Processes the next contribution job under the model budget `policy`. While the budget gate is closed (the
    /// provider refused for lack of quota or credit, or the daily ceiling is reached) no job is claimed, no replay
    /// runs and the model is not called: the result is `Idle` and every submitted contribution stays `queued`. A
    /// provider refusal parks the job that met it as `waiting_budget`, without using an attempt, and closes the gate
    /// for every worker (`WorkResult::Retry`). A provider that cannot answer for now (rate limited, overloaded,
    /// unreachable, timed out) is handled the same way, but a job waits out only a limited number of outages for
    /// free, and among waiting jobs the one that has waited out the fewest goes first
    /// (docs/spec/provider-outage.md).
    pub fn process_next_budgeted(
        &self,
        provider: &mut dyn LlmProvider,
        replay: &dyn ReplayVerifier,
        policy: &ModelBudgetPolicy,
        now: u64,
    ) -> Result<WorkResult, ServiceError> {
        let started = Instant::now();
        let lease_holder =
            crate::identity::random_secret().map_err(|_| ServiceError::Unavailable)?;
        let claimed = self.update(|state| {
            if !budget::admit(state, now, policy) {
                return Ok(None);
            }
            let candidate_id =
                state
                    .work
                    .values()
                    .filter(|job| {
                        job.kind == "contribution"
                            && job.not_before <= now
                            && (matches!(job.state.as_str(), "queued" | "retry" | "waiting_budget")
                                || job.state == "leased"
                                    && job.lease_until.is_some_and(|until| until <= now))
                    })
                    .filter(|job| {
                        job.contribution_id.as_ref().is_some_and(|id| {
                            state.contributions.contributions.iter().any(|entry| {
                                &entry.record.id == id && entry.record.status == "queued"
                            })
                        })
                    })
                    .min_by_key(|job| {
                        let waited = job
                            .contribution_id
                            .as_deref()
                            .map_or(0, |id| budget::outage_waits(state, id));
                        (waited, job.enqueued_at, job.id.clone())
                    })
                    .map(|job| job.id.clone());
            let Some(id) = candidate_id else {
                return Ok(None);
            };
            let work = state.work.get_mut(&id).ok_or(ServiceError::Conflict)?;
            work.generation += 1;
            work.attempts = work.attempts.saturating_add(1);
            work.holder = Some(lease_holder.clone());
            work.lease_until = Some(now.saturating_add(LEASE_SECS));
            work.state = "leased".into();
            let contribution = work.contribution_id.clone().ok_or(ServiceError::Conflict)?;
            let generation = work.generation;
            state.emit(
                "worker_started",
                &contribution,
                now.saturating_add(started.elapsed().as_secs()),
            );
            let stored = state
                .contributions
                .contributions
                .iter()
                .find(|entry| entry.record.id == contribution)
                .ok_or(ServiceError::Conflict)?;
            let consent = state
                .consents
                .get(&contribution)
                .ok_or(ServiceError::Conflict)?;
            if !state.active(&stored.account_id)
                || consent.withdrawn_at.is_some()
                || consent.purged_at.is_some()
                || consent.purge_at <= now
            {
                return Err(ServiceError::Conflict);
            }
            let url = wam_core::CanonicalUrl::parse(
                &stored.record.url_pattern,
                wam_core::UrlMode::Public,
            )
            .map_err(|_| ServiceError::Conflict)?;
            let job = Job {
                id: format!("application-{}", stored.record.id),
                trigger: Trigger::NewContribution,
                scope: JobScope {
                    site_host: url.host().into(),
                    task_category: stored.record.task_category.clone(),
                    intent_key: stored.record.task_category.clone(),
                },
                candidate: Candidate {
                    url_pattern: url.path().to_owned(),
                    action_sequence: stored
                        .record
                        .actions
                        .iter()
                        .map(|action| {
                            match action {
                                wam_contracts::ContributionAction::Navigate => "navigate",
                                wam_contracts::ContributionAction::Edit => "type",
                                wam_contracts::ContributionAction::Save => "click",
                            }
                            .to_owned()
                        })
                        .collect(),
                    outcomes: Outcomes {
                        success: 0,
                        partial: 0,
                        failed: 0,
                    },
                    observed_traits: vec![],
                    independent_contributors: 1,
                    ui_fingerprint: None,
                    observed_at: stored.record.submitted_at,
                },
                batch_candidates: vec![],
                contribution_ids: vec![stored.record.id.clone()],
                replay_passed: false,
                lease_fence: None,
            };
            let digest = catalog_digest(state, &job.scope.site_host);
            let baseline = state
                .catalog
                .memories
                .values()
                .find(|memory| {
                    memory.scope.host == job.scope.site_host
                        && memory.scope.pattern
                            == format!("{}{}", job.scope.site_host, job.candidate.url_pattern)
                        && memory.scope.task == job.scope.task_category
                        && memory.scope.intent == job.scope.intent_key
                })
                .and_then(|memory| memory.active())
                .map(|version| version.payload.clone());
            let unsettled = budget::unsettled(state, &contribution);
            Ok(Some((
                contribution,
                generation,
                job,
                digest,
                baseline,
                state.catalog.store(),
                unsettled,
            )))
        })?;
        let Some((id, generation, mut job, digest, baseline, mut snapshot, unsettled)) = claimed
        else {
            return Ok(WorkResult::Idle);
        };
        let _heartbeat = Heartbeat::start(
            self.clone(),
            id.clone(),
            generation,
            lease_holder.clone(),
            now,
            started,
        );
        let first_request = ReplayRequest::new(&job, baseline.clone(), None);
        let first_evidence = match replay.verify(&first_request) {
            Ok(evidence) if evidence.passes(&first_request) => evidence,
            _ => {
                self.park(&id, generation, &lease_holder, "waiting_review")?;
                return Ok(WorkResult::NeedsReview);
            }
        };
        job.replay_passed = true;
        job.candidate.outcomes.success = first_evidence.candidate_successes;
        self.renew(
            &id,
            generation,
            &lease_holder,
            now + started.elapsed().as_secs(),
        )?;
        let tools = Tools::load().map_err(|_| ServiceError::Unavailable)?;
        let report = run_job(
            &job,
            &mut snapshot,
            provider,
            &tools,
            &Policy::default(),
            &self.task_vocabulary(),
            &CuratorConfig::default(),
            now,
        );
        let spent = budget::micro_usd(report.cost_usd());
        if let Outcome::QuotaExhausted {
            retry_after_secs, ..
        } = &report.outcome
        {
            let refused = Answer::Refused {
                retry_after_secs: *retry_after_secs,
            };
            self.update(|state| {
                let work = state
                    .work
                    .get(&format!("contribution:{id}"))
                    .ok_or(ServiceError::Conflict)?;
                if work.generation != generation || work.holder.as_deref() != Some(&lease_holder) {
                    return Err(ServiceError::Conflict);
                }
                // The refusal closes the gate for every worker; this job waits for it with its attempt intact.
                budget::record(
                    state,
                    now.saturating_add(started.elapsed().as_secs()),
                    spent,
                    refused,
                    policy,
                );
                let until = state.model_budget.paused_until;
                let work = state
                    .work
                    .get_mut(&format!("contribution:{id}"))
                    .ok_or(ServiceError::Conflict)?;
                work.state = "waiting_budget".into();
                work.holder = None;
                work.lease_until = None;
                work.attempts = work.attempts.saturating_sub(1);
                work.not_before = until;
                state.emit("worker_budget_wait", &id, now);
                Ok(())
            })?;
            return Ok(WorkResult::Retry);
        }
        if let Outcome::Unavailable {
            why,
            retry_after_secs,
        } = &report.outcome
        {
            // The provider could not answer, which says nothing about this job: it waits behind the gate with its
            // attempt intact, within the limit the policy sets per job. Past the limit the outage counts against the
            // job like any failed attempt. The gate closes either way, so nobody else runs into the outage.
            let at = now.saturating_add(started.elapsed().as_secs());
            let waits_for_free = self.update(|state| {
                let work = state
                    .work
                    .get(&format!("contribution:{id}"))
                    .ok_or(ServiceError::Conflict)?;
                if work.generation != generation || work.holder.as_deref() != Some(&lease_holder) {
                    return Err(ServiceError::Conflict);
                }
                let free =
                    budget::record_outage(state, at, spent, &id, why, *retry_after_secs, policy);
                if free {
                    let until = state.model_budget.paused_until;
                    let work = state
                        .work
                        .get_mut(&format!("contribution:{id}"))
                        .ok_or(ServiceError::Conflict)?;
                    work.state = "waiting_budget".into();
                    work.holder = None;
                    work.lease_until = None;
                    work.attempts = work.attempts.saturating_sub(1);
                    work.not_before = until;
                    state.emit("worker_budget_wait", &id, now);
                }
                Ok(free)
            })?;
            if !waits_for_free {
                self.park(&id, generation, &lease_holder, "retry")?;
            }
            return Ok(WorkResult::Retry);
        }
        if spent > 0 || unsettled {
            let at = now.saturating_add(started.elapsed().as_secs());
            let answered = matches!(&report.outcome, Outcome::Decided | Outcome::NoDecision(_));
            self.update(|state| {
                if answered {
                    budget::record_answer(state, at, spent, &id, policy);
                } else {
                    budget::record(state, at, spent, Answer::Other, policy);
                }
                Ok(())
            })?;
        }
        if report.outcome != Outcome::Decided {
            self.park(&id, generation, &lease_holder, "retry")?;
            return Ok(WorkResult::Retry);
        }
        let terminal = report
            .turns
            .iter()
            .rev()
            .find(|turn| {
                turn.tool.starts_with("decide_")
                    && turn.result.get("ok") == Some(&Value::Bool(true))
            })
            .ok_or(ServiceError::Conflict)?;
        let record = terminal
            .args
            .get("record")
            .cloned()
            .map(serde_json::from_value::<RecordPayload>)
            .transpose()
            .map_err(|_| ServiceError::Conflict)?;
        if record
            .as_ref()
            .is_some_and(|record| !allowed_record(&job, record))
        {
            self.park(&id, generation, &lease_holder, "retry")?;
            return Ok(WorkResult::Retry);
        }
        let replay_request = ReplayRequest::new(&job, baseline, record);
        self.renew(
            &id,
            generation,
            &lease_holder,
            now + started.elapsed().as_secs(),
        )?;
        let evidence = match replay.verify(&replay_request) {
            Ok(evidence) if evidence.passes(&replay_request) => evidence,
            _ => {
                self.park(&id, generation, &lease_holder, "waiting_review")?;
                return Ok(WorkResult::NeedsReview);
            }
        };
        let proposal = Proposal {
            contribution_id: id.clone(),
            generation,
            holder: lease_holder.clone(),
            job,
            tool: terminal.tool.clone(),
            args: terminal.args.clone(),
            catalog_digest: digest,
            replay: evidence,
            replay_request,
            audit: report.audit.clone(),
        };
        let finished_at = now.saturating_add(started.elapsed().as_secs());
        self.update(|state| {
            validate_proposal(state, &proposal, true, finished_at)?;
            let low = proposal.args.get("confidence_band").and_then(Value::as_str) == Some("low");
            if low || proposal.tool == "decide_escalate" {
                state.proposals.insert(id.clone(), proposal);
                let work = state
                    .work
                    .get_mut(&format!("contribution:{id}"))
                    .ok_or(ServiceError::Conflict)?;
                work.state = "waiting_review".into();
                work.holder = None;
                work.lease_until = None;
                state.emit("review_required", &id, finished_at);
                return Ok(WorkResult::NeedsReview);
            }
            let result = commit_proposal(
                state,
                &proposal,
                &tools,
                &self.task_vocabulary(),
                finished_at,
            )?;
            Ok(result)
        })
    }

    fn renew(&self, id: &str, generation: u64, holder: &str, now: u64) -> Result<(), ServiceError> {
        self.update(|state| {
            let work = state
                .work
                .get_mut(&format!("contribution:{id}"))
                .ok_or(ServiceError::Conflict)?;
            if work.generation != generation
                || work.holder.as_deref() != Some(holder)
                || work.state != "leased"
                || work.lease_until.is_none_or(|until| until <= now)
            {
                return Err(ServiceError::Conflict);
            }
            work.lease_until = Some(now.saturating_add(LEASE_SECS));
            Ok(())
        })
    }
    fn park(
        &self,
        id: &str,
        generation: u64,
        holder: &str,
        status: &str,
    ) -> Result<(), ServiceError> {
        self.update(|state| {
            let work = state
                .work
                .get_mut(&format!("contribution:{id}"))
                .ok_or(ServiceError::Conflict)?;
            if work.generation != generation || work.holder.as_deref() != Some(holder) {
                return Err(ServiceError::Conflict);
            }
            work.state = if status == "retry" && work.attempts >= 5 {
                "waiting_review"
            } else {
                status
            }
            .into();
            work.not_before = if status == "retry" {
                crate::application::now_unix().saturating_add(5 * (1 << work.generation.min(6)))
            } else {
                0
            };
            work.holder = None;
            work.lease_until = None;
            // The job is in review when it was sent there or when this failure used its last attempt; the event
            // says what happened to the job, not what the caller asked for.
            let review = work.state == "waiting_review";
            state.emit(
                if review {
                    "review_required"
                } else {
                    "worker_retry"
                },
                id,
                crate::application::now_unix(),
            );
            Ok(())
        })
    }
}

pub(super) fn validate_proposal(
    state: &AppState,
    proposal: &Proposal,
    lease: bool,
    now: u64,
) -> Result<(), ServiceError> {
    let stored = state
        .contributions
        .contributions
        .iter()
        .find(|entry| entry.record.id == proposal.contribution_id)
        .ok_or(ServiceError::Conflict)?;
    let consent = state
        .consents
        .get(&proposal.contribution_id)
        .ok_or(ServiceError::Conflict)?;
    let work = state
        .work
        .get(&format!("contribution:{}", proposal.contribution_id))
        .ok_or(ServiceError::Conflict)?;
    if !state.active(&stored.account_id)
        || !state.accounts[&stored.account_id].contribution_enabled
        || consent.version != wam_contracts::CONTRIBUTION_CONSENT_VERSION
        || consent.withdrawn_at.is_some()
        || consent.purged_at.is_some()
        || consent.purge_at <= now
        || !proposal.replay.passes(&proposal.replay_request)
        || proposal.replay_request.scope != proposal.job.scope
        || proposal.replay_request.candidate != proposal.job.candidate
        || proposal
            .args
            .get("record")
            .cloned()
            .map(serde_json::from_value::<RecordPayload>)
            .transpose()
            .map_err(|_| ServiceError::Conflict)?
            != proposal.replay_request.proposed_record
        || proposal
            .replay_request
            .proposed_record
            .as_ref()
            .is_some_and(|record| !allowed_record(&proposal.job, record))
        || catalog_digest(state, &proposal.job.scope.site_host) != proposal.catalog_digest
        || proposal.job.contribution_ids != [proposal.contribution_id.clone()]
        || stored.record.task_category != proposal.job.scope.task_category
        || !matches!(stored.record.status.as_str(), "queued" | "in_review")
    {
        return Err(ServiceError::Conflict);
    }
    let scope =
        wam_core::CanonicalUrl::parse(&stored.record.url_pattern, wam_core::UrlMode::Public)
            .map_err(|_| ServiceError::Conflict)?;
    if scope.host() != proposal.job.scope.site_host
        || scope.path() != proposal.job.candidate.url_pattern
    {
        return Err(ServiceError::Conflict);
    }
    if let Some(existing) = proposal.args.get("existing_id").and_then(Value::as_str) {
        let row = state
            .catalog
            .memories
            .get(existing)
            .ok_or(ServiceError::Conflict)?;
        if row.scope.host != proposal.job.scope.site_host
            || row.scope.task != proposal.job.scope.task_category
            || row.scope.intent != proposal.job.scope.intent_key
            || row.scope.pattern
                != format!(
                    "{}{}",
                    proposal.job.scope.site_host, proposal.job.candidate.url_pattern
                )
        {
            return Err(ServiceError::Conflict);
        }
    }
    if lease
        && (work.state != "leased"
            || work.generation != proposal.generation
            || work.holder.as_deref() != Some(proposal.holder.as_str())
            || work.lease_until.is_none_or(|until| until <= now))
    {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

pub(super) fn commit_proposal(
    state: &mut AppState,
    proposal: &Proposal,
    tools: &Tools,
    tasks: &wam_core::TaskVocabulary,
    now: u64,
) -> Result<WorkResult, ServiceError> {
    let mut catalog = state.catalog.store();
    let policy = Policy::default();
    let mut ctx = ToolCtx::new(&proposal.job, &mut catalog, tools, &policy, tasks, now);
    ctx.call(&proposal.tool, &proposal.args)
        .map_err(|_| ServiceError::Conflict)?;
    let mut decision = ctx.decision().cloned().ok_or(ServiceError::Conflict)?;
    drop(ctx);
    if let Some(old_id) = decision
        .memory_id
        .as_ref()
        .filter(|id| id.len() > 64)
        .cloned()
    {
        let id = format!("web:{}", &sha256_hex(old_id.as_bytes())[..48]);
        if catalog.memories.contains_key(&id) {
            return Err(ServiceError::Conflict);
        }
        let mut row = catalog
            .memories
            .remove(&old_id)
            .ok_or(ServiceError::Conflict)?;
        row.id = id.clone();
        catalog.memories.insert(id.clone(), row);
        for stored in catalog.decisions.values_mut() {
            if stored.result.memory_id.as_ref() == Some(&old_id) {
                stored.result.memory_id = Some(id.clone());
            }
        }
        decision.memory_id = Some(id);
    }
    state.catalog.replace(catalog);
    state.records(tasks)?;
    let owner = state
        .contributions
        .contributions
        .iter()
        .find(|entry| entry.record.id == proposal.contribution_id)
        .ok_or(ServiceError::Conflict)?
        .account_id
        .clone();
    let mut result = WorkResult::Rejected;
    let decided_version = decision.version.or_else(|| {
        decision
            .memory_id
            .as_ref()
            .and_then(|id| state.catalog.memories.get(id))
            .and_then(|memory| memory.versions.last())
            .map(|version| version.version)
    });
    if let (Some(memory), Some(version)) = (&decision.memory_id, decided_version) {
        let author = matches!(decision.kind.as_str(), "create_new" | "supersede");
        if (author
            || matches!(
                decision.kind.as_str(),
                "duplicate_support" | "merge_evidence"
            ))
            && !state.attributions.iter().any(|entry| {
                entry.memory_id == *memory
                    && entry.version == version
                    && entry.account_id.as_deref() == Some(&owner)
            })
        {
            state.attributions.push(Attribution {
                memory_id: memory.clone(),
                version,
                account_id: Some(owner.clone()),
                contribution_id: Some(proposal.contribution_id.clone()),
                role: if author { "author" } else { "support" }.into(),
                evidence_digest: sha256_hex(
                    &serde_json::to_vec(&proposal.replay).map_err(|_| ServiceError::Unavailable)?,
                ),
            });
        }
        let independent = state
            .attributions
            .iter()
            .filter(|entry| {
                entry.memory_id == *memory && entry.version == version && entry.account_id.is_some()
            })
            .filter_map(|entry| entry.account_id.as_deref())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if let Some(row) = state.catalog.memories.get_mut(memory) {
            row.support_count = independent as u32;
        }
        if author {
            append_event(
                state,
                &owner,
                (memory, version),
                "approval",
                20,
                format!("approval:{}", super::version_id(memory, version)),
                now,
            )?;
            result = WorkResult::Published {
                memory_id: memory.clone(),
                version_id: super::version_id(memory, version),
            };
        } else if decision.kind == "retire" {
            super::lifecycle::reverse_version(state, memory, version, now)?;
            result = WorkResult::Retired {
                memory_id: memory.clone(),
            };
        } else {
            result = WorkResult::Supported {
                memory_id: memory.clone(),
            };
        }
    }
    let record = state
        .contributions
        .contributions
        .iter_mut()
        .find(|entry| entry.record.id == proposal.contribution_id)
        .ok_or(ServiceError::Conflict)?;
    record.record.status = if decision.kind == "reject_invalid" {
        "rejected"
    } else {
        "approved"
    }
    .into();
    let work = state
        .work
        .get_mut(&format!("contribution:{}", proposal.contribution_id))
        .ok_or(ServiceError::Conflict)?;
    work.state = "done".into();
    work.holder = None;
    work.lease_until = None;
    state.proposals.remove(&proposal.contribution_id);
    let mut audit = proposal.audit.clone();
    audit.at = now;
    audit.decision = Some(serde_json::to_value(&decision).map_err(|_| ServiceError::Unavailable)?);
    audit.calls.push(serde_json::json!({
        "policy":"publication-v1",
        "replay_digest":sha256_hex(&serde_json::to_vec(&proposal.replay).map_err(|_| ServiceError::Unavailable)?),
        "samples":proposal.replay.samples,
        "baseline_successes":proposal.replay.baseline_successes,
        "candidate_successes":proposal.replay.candidate_successes,
        "baseline_tool_calls":proposal.replay.baseline_tool_calls,
        "candidate_tool_calls":proposal.replay.candidate_tool_calls
    }));
    state.catalog.audits.push(audit);
    state.emit("curation_committed", &proposal.contribution_id, now);
    Ok(result)
}
