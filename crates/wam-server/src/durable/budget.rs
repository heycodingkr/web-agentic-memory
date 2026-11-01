//! The model budget gate of the application worker (docs/spec/queue.md, QUE-08 and QUE-20..23).
//!
//! The model can stop being usable in three ways: the provider says its credit is spent (an HTTP 402 for a spent
//! balance or an account without billing, or a usage limit), the operator's own daily ceiling is reached, or the
//! provider cannot answer for now (rate limited, overloaded, unreachable, timed out: docs/spec/provider-outage.md).
//! Either way nothing may be lost and nothing may keep calling a provider that will not answer. While the gate is
//! closed no worker claims a job, runs a replay or calls the model, so submitted contributions stay `queued` and the
//! one job that met the refusal waits as `waiting_budget` without having used an attempt. An outage is not the job's
//! fault, but a job the provider can never answer must not block the queue for ever, so a job waits out only a
//! limited number of outages for free and the jobs that waited least go first. The gate lives in the durable
//! application state, so every worker and every restart sees the same pause. Recall never depends on it.

use std::collections::BTreeMap;

use super::{ApplicationRepository, state::AppState};
use crate::application::ServiceError;
use serde::{Deserialize, Serialize};

const DAY_SECS: u64 = 86_400;
const MICRO: f64 = 1_000_000.0;
/// A provider pause is never shorter than this, whatever the provider's `Retry-After` said.
const MIN_PAUSE_SECS: u64 = 30;
/// Sanity bounds for the settings: a ceiling above this is a typo, not a budget.
const MAX_DAILY_CAP_USD: f64 = 1_000_000.0;
const MAX_PAUSE_SECS: u64 = DAY_SECS;
/// What an outage costs a job before it is charged: free waits per job, and the pause that starts at the floor and
/// doubles with every outage in a row up to the maximum.
const OUTAGE_PAUSE_SECS: u64 = MIN_PAUSE_SECS;
const OUTAGE_PAUSE_MAX_SECS: u64 = 600;
const OUTAGE_FREE_WAITS: u32 = 8;
const MAX_FREE_WAITS: u32 = 1000;
const MAX_REASON_CHARS: usize = 80;

/// What the operator allows. `Default` is "no daily ceiling"; the provider's own quota still parks jobs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelBudgetPolicy {
    daily_cap_micro_usd: Option<u64>,
    quota_pause_secs: u64,
    quota_pause_max_secs: u64,
    outage_pause_secs: u64,
    outage_pause_max_secs: u64,
    outage_free_waits: u32,
}

impl Default for ModelBudgetPolicy {
    fn default() -> Self {
        Self {
            daily_cap_micro_usd: None,
            // The wait docs/spec/queue.md names when the provider does not say when its quota returns.
            quota_pause_secs: 900,
            quota_pause_max_secs: 3600,
            outage_pause_secs: OUTAGE_PAUSE_SECS,
            outage_pause_max_secs: OUTAGE_PAUSE_MAX_SECS,
            outage_free_waits: OUTAGE_FREE_WAITS,
        }
    }
}

impl ModelBudgetPolicy {
    /// `daily_cap_usd`: estimated model spend per UTC day after which no job starts until the next UTC day. It is
    /// checked before a job, so the last job may overshoot it. `quota_pause_secs`: how long to stop when the provider
    /// refuses without saying when to come back; it doubles with every consecutive refusal up to `quota_pause_max_secs`.
    pub fn new(
        daily_cap_usd: Option<f64>,
        quota_pause_secs: u64,
        quota_pause_max_secs: u64,
    ) -> Result<Self, ServiceError> {
        let invalid = || ServiceError::InvalidInput("invalid_model_budget_configuration");
        let daily_cap_micro_usd = match daily_cap_usd {
            None => None,
            Some(usd) if usd.is_finite() && usd > 0.0 && usd <= MAX_DAILY_CAP_USD => {
                Some((usd * MICRO).round() as u64)
            }
            Some(_) => return Err(invalid()),
        };
        if !(MIN_PAUSE_SECS..=MAX_PAUSE_SECS).contains(&quota_pause_secs)
            || !(quota_pause_secs..=MAX_PAUSE_SECS).contains(&quota_pause_max_secs)
        {
            return Err(invalid());
        }
        Ok(Self {
            daily_cap_micro_usd,
            quota_pause_secs,
            quota_pause_max_secs,
            ..Self::default()
        })
    }

    /// How an outage of the provider is waited out. `pause_secs`: how long to stop after the first outage when the
    /// provider names no time; it doubles with every outage in a row up to `pause_max_secs`. `free_waits`: how many
    /// outages one job may wait out without using an attempt; once they are used, an outage counts against the job like
    /// any failed attempt. `0` counts every outage against the job (the behaviour before docs/spec/provider-outage.md).
    pub fn with_outage(
        mut self,
        pause_secs: u64,
        pause_max_secs: u64,
        free_waits: u32,
    ) -> Result<Self, ServiceError> {
        if !(MIN_PAUSE_SECS..=MAX_PAUSE_SECS).contains(&pause_secs)
            || !(pause_secs..=MAX_PAUSE_SECS).contains(&pause_max_secs)
            || free_waits > MAX_FREE_WAITS
        {
            return Err(ServiceError::InvalidInput(
                "invalid_model_budget_configuration",
            ));
        }
        self.outage_pause_secs = pause_secs;
        self.outage_pause_max_secs = pause_max_secs;
        self.outage_free_waits = free_waits;
        Ok(self)
    }

    /// `WAM_LLM_DAILY_BUDGET_USD` (decimal, optional), `WAM_LLM_QUOTA_PAUSE_SECS` (default 900),
    /// `WAM_LLM_QUOTA_PAUSE_MAX_SECS` (default 3600), `WAM_LLM_OUTAGE_PAUSE_SECS` (default 30),
    /// `WAM_LLM_OUTAGE_PAUSE_MAX_SECS` (default 600) and `WAM_LLM_OUTAGE_FREE_WAITS` (default 8). Blank counts as
    /// unset; anything unreadable is refused so a typo cannot silently remove the ceiling.
    pub fn from_env() -> Result<Self, ServiceError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ServiceError> {
        let invalid = || ServiceError::InvalidInput("invalid_model_budget_configuration");
        let read = |name: &str| get(name).filter(|value| !value.trim().is_empty());
        let defaults = Self::default();
        let cap = read("WAM_LLM_DAILY_BUDGET_USD")
            .map(|value| value.trim().parse::<f64>().map_err(|_| invalid()))
            .transpose()?;
        let secs = |name: &str, fallback: u64| {
            read(name).map_or(Ok(fallback), |value| {
                value.trim().parse::<u64>().map_err(|_| invalid())
            })
        };
        let free_waits = read("WAM_LLM_OUTAGE_FREE_WAITS")
            .map_or(Ok(defaults.outage_free_waits), |value| {
                value.trim().parse::<u32>().map_err(|_| invalid())
            })?;
        Self::new(
            cap,
            secs("WAM_LLM_QUOTA_PAUSE_SECS", defaults.quota_pause_secs)?,
            secs(
                "WAM_LLM_QUOTA_PAUSE_MAX_SECS",
                defaults.quota_pause_max_secs,
            )?,
        )?
        .with_outage(
            secs("WAM_LLM_OUTAGE_PAUSE_SECS", defaults.outage_pause_secs)?,
            secs(
                "WAM_LLM_OUTAGE_PAUSE_MAX_SECS",
                defaults.outage_pause_max_secs,
            )?,
            free_waits,
        )
    }

    pub fn daily_cap_usd(&self) -> Option<f64> {
        self.daily_cap_micro_usd.map(|micro| micro as f64 / MICRO)
    }

    /// A ceiling counts what providers report as `cost_usd`; a provider that reports none would never trip it.
    pub fn has_daily_cap(&self) -> bool {
        self.daily_cap_micro_usd.is_some()
    }
}

/// Why the gate is closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetBlock {
    /// The provider refused: its quota or credit is spent, or its billing is not set up.
    ProviderQuota,
    /// The provider cannot answer for now: rate limited, overloaded, unreachable or timed out.
    ProviderOutage,
    /// The operator's daily ceiling is reached.
    DailyCap,
}

impl BudgetBlock {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderQuota => "provider_quota",
            Self::ProviderOutage => "provider_outage",
            Self::DailyCap => "daily_cap",
        }
    }
}

/// What set the pause that is running. When two causes overlap, the one whose pause ends later is reported.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PauseCause {
    /// A refusal for lack of quota or credit (documents from before outages were told apart mean this).
    #[default]
    Quota,
    /// The provider could not answer.
    Outage,
}

impl PauseCause {
    fn block(self) -> BudgetBlock {
        match self {
            Self::Quota => BudgetBlock::ProviderQuota,
            Self::Outage => BudgetBlock::ProviderOutage,
        }
    }
}

/// The durable part of the gate, kept inside the application state (and so in `wam_app_meta` on PostgreSQL).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ModelBudgetState {
    /// No model call before this time (unix seconds): the provider refused. `0`: not paused.
    #[serde(default)]
    pub paused_until: u64,
    /// Consecutive refusals without an answer in between; it drives the pause when the provider names no time.
    #[serde(default)]
    pub strikes: u32,
    /// The UTC day (unix seconds / 86400) that `spent_micro_usd` counts. It only moves forward.
    #[serde(default)]
    pub day: u64,
    /// Estimated model spend of that day, in millionths of a US dollar.
    #[serde(default)]
    pub spent_micro_usd: u64,
    /// The daily ceiling was reached; remembered so that reaching and leaving it are each announced once.
    #[serde(default)]
    pub capped: bool,
    /// Consecutive provider outages without an answer in between; it drives the pause when the provider names no
    /// time. Counted apart from `strikes`.
    #[serde(default)]
    pub outage_strikes: u32,
    /// What set the pause that is running.
    #[serde(default)]
    pub pause_cause: PauseCause,
    /// The last outage in words: fixed text from our own provider code, cleaned, never what the provider sent.
    #[serde(default)]
    pub reason: String,
    /// How often each job (by contribution id) has waited out an outage without using an attempt. It decides the
    /// order of claims and when an outage starts to count against a job.
    #[serde(default)]
    pub outage_waits: BTreeMap<String, u32>,
}

/// What the provider did with a job's model calls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Answer {
    /// The provider replied (whatever the job then decided).
    Answered,
    /// The provider refused for lack of quota or credit.
    Refused { retry_after_secs: Option<u64> },
    /// Neither: a transient failure that says nothing about the quota.
    Other,
}

pub(crate) fn micro_usd(usd: f64) -> u64 {
    if usd.is_finite() && usd > 0.0 {
        (usd * MICRO).round() as u64
    } else {
        0
    }
}

fn roll_day(budget: &mut ModelBudgetState, now: u64) {
    let today = now / DAY_SECS;
    if today > budget.day {
        budget.day = today;
        budget.spent_micro_usd = 0;
    }
}

fn over_cap(budget: &ModelBudgetState, policy: &ModelBudgetPolicy) -> bool {
    policy
        .daily_cap_micro_usd
        .is_some_and(|cap| budget.spent_micro_usd >= cap)
}

/// May a job start now? Called inside the claim transaction. It also announces, once, that a closed gate reopened
/// (`worker_budget_resumed`) and that a ceiling already exceeded by the stored spend closed it.
pub(crate) fn admit(state: &mut AppState, now: u64, policy: &ModelBudgetPolicy) -> bool {
    let budget = &mut state.model_budget;
    roll_day(budget, now);
    let capped = over_cap(budget, policy);
    let mut events: Vec<(&'static str, BudgetBlock)> = Vec::new();
    if capped && !budget.capped {
        budget.capped = true;
        events.push(("worker_budget_paused", BudgetBlock::DailyCap));
    } else if !capped && budget.capped {
        budget.capped = false;
        events.push(("worker_budget_resumed", BudgetBlock::DailyCap));
    }
    let paused = now < budget.paused_until;
    if !paused && budget.paused_until != 0 {
        budget.paused_until = 0;
        events.push(("worker_budget_resumed", budget.pause_cause.block()));
    }
    for (kind, block) in events {
        state.emit(kind, block.as_str(), now);
    }
    !capped && !paused
}

/// Books what a job's model calls cost and what the provider said, at time `now`. A refusal closes the gate for the
/// time the provider named (at least 30 seconds, at most the configured maximum) or, without a hint, for the
/// configured pause doubled once per consecutive refusal. A pause never shortens one that is already set.
pub(crate) fn record(
    state: &mut AppState,
    now: u64,
    spent_micro_usd: u64,
    answer: Answer,
    policy: &ModelBudgetPolicy,
) {
    let budget = &mut state.model_budget;
    roll_day(budget, now);
    budget.spent_micro_usd = budget.spent_micro_usd.saturating_add(spent_micro_usd);
    let mut events: Vec<(&'static str, BudgetBlock)> = Vec::new();
    match answer {
        Answer::Answered => {
            budget.strikes = 0;
            budget.outage_strikes = 0;
            budget.reason.clear();
        }
        Answer::Refused { retry_after_secs } => {
            budget.strikes = budget.strikes.saturating_add(1);
            let doubled = policy
                .quota_pause_secs
                .saturating_mul(1 << (budget.strikes - 1).min(20));
            let wait = retry_after_secs
                .unwrap_or(doubled)
                .clamp(MIN_PAUSE_SECS, policy.quota_pause_max_secs);
            if now >= budget.paused_until {
                events.push(("worker_budget_paused", BudgetBlock::ProviderQuota));
            }
            let until = now.saturating_add(wait);
            if until > budget.paused_until {
                budget.paused_until = until;
                budget.pause_cause = PauseCause::Quota;
            }
        }
        Answer::Other => {}
    }
    if over_cap(budget, policy) && !budget.capped {
        budget.capped = true;
        events.push(("worker_budget_paused", BudgetBlock::DailyCap));
    }
    for (kind, block) in events {
        state.emit(kind, block.as_str(), now);
    }
}

/// The text an operator reads for an outage. It comes from our own provider code and says what kind of outage it was
/// (`HTTP 503 from model provider`); whatever it carries beyond plain characters is dropped and it stays short.
fn clean_reason(why: &str) -> String {
    why.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " ._:()/-".contains(*c))
        .take(MAX_REASON_CHARS)
        .collect()
}

/// Forgets the waits of jobs that are not waiting any more (finished, canceled or gone), so the table stays as small as
/// the queue.
fn prune_waits(state: &mut AppState) {
    let work = &state.work;
    state.model_budget.outage_waits.retain(|id, _| {
        work.get(&format!("contribution:{id}")).is_some_and(|job| {
            matches!(
                job.state.as_str(),
                "queued" | "retry" | "waiting_budget" | "leased"
            )
        })
    });
}

/// How often `contribution` has waited out an outage without using an attempt. The claim takes the job with the fewest
/// first, so that probes go round the queue and a job the provider cannot answer falls to the back.
pub(crate) fn outage_waits(state: &AppState, contribution: &str) -> u32 {
    state
        .model_budget
        .outage_waits
        .get(contribution)
        .copied()
        .unwrap_or(0)
}

/// Is there anything about this contribution's last calls for the gate to settle when the provider answers: an
/// outage or refusal in a row, or waits this job has collected?
pub(crate) fn unsettled(state: &AppState, contribution: &str) -> bool {
    let budget = &state.model_budget;
    budget.strikes > 0 || budget.outage_strikes > 0 || outage_waits(state, contribution) > 0
}

/// The provider answered a job's model calls (whatever the job then decided): books the cost, ends the refusals and
/// outages in a row, and forgets what this job had waited.
pub(crate) fn record_answer(
    state: &mut AppState,
    now: u64,
    spent_micro_usd: u64,
    job: &str,
    policy: &ModelBudgetPolicy,
) {
    record(state, now, spent_micro_usd, Answer::Answered, policy);
    state.model_budget.outage_waits.remove(job);
    prune_waits(state);
}

/// Books an outage that `job` (a contribution id) met at time `now`, and closes the gate. The gate stays closed for the
/// longer of the `Retry-After` the provider named (at most the configured maximum for provider-named times) and the
/// backoff (the outage pause doubled once per outage in a row, at most the outage maximum), but at least 30 seconds, and
/// a pause never shortens one that is already set. Returns whether the job waits for free, that is without using an
/// attempt: a job may wait out only a limited number of outages for free, after which an outage counts against it like
/// any failed attempt (the gate closes either way, because an outage is everybody's).
pub(crate) fn record_outage(
    state: &mut AppState,
    now: u64,
    spent_micro_usd: u64,
    job: &str,
    why: &str,
    retry_after_secs: Option<u64>,
    policy: &ModelBudgetPolicy,
) -> bool {
    prune_waits(state);
    let free = outage_waits(state, job) < policy.outage_free_waits;
    let budget = &mut state.model_budget;
    roll_day(budget, now);
    budget.spent_micro_usd = budget.spent_micro_usd.saturating_add(spent_micro_usd);
    budget.outage_strikes = budget.outage_strikes.saturating_add(1);
    let backoff = policy
        .outage_pause_secs
        .saturating_mul(1 << (budget.outage_strikes - 1).min(20))
        .min(policy.outage_pause_max_secs);
    let named = retry_after_secs
        .unwrap_or(0)
        .min(policy.quota_pause_max_secs);
    let until = now.saturating_add(backoff.max(named).max(MIN_PAUSE_SECS));
    let mut events: Vec<(&'static str, BudgetBlock)> = Vec::new();
    if now >= budget.paused_until {
        events.push(("worker_budget_paused", BudgetBlock::ProviderOutage));
    }
    if until > budget.paused_until {
        budget.paused_until = until;
        budget.pause_cause = PauseCause::Outage;
    }
    budget.reason = clean_reason(why);
    if free {
        *budget.outage_waits.entry(job.to_owned()).or_insert(0) += 1;
    }
    if over_cap(budget, policy) && !budget.capped {
        budget.capped = true;
        events.push(("worker_budget_paused", BudgetBlock::DailyCap));
    }
    for (kind, block) in events {
        state.emit(kind, block.as_str(), now);
    }
    free
}

/// What an operator needs to know about the gate.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModelBudgetStatus {
    /// May a job start right now?
    pub open: bool,
    /// Why not, when it may not (`provider_quota`, `provider_outage` or `daily_cap`).
    pub blocked_by: Option<&'static str>,
    /// When the gate reopens by itself (unix seconds), when it is closed.
    pub until: Option<u64>,
    /// Consecutive provider refusals without an answer in between.
    pub strikes: u32,
    /// Consecutive provider outages without an answer in between.
    pub outage_strikes: u32,
    /// What the last outage was, in fixed words, while the gate is closed because of an outage.
    pub reason: Option<String>,
    pub spent_today_usd: f64,
    pub daily_cap_usd: Option<f64>,
    /// Contribution jobs waiting to start: queued, retrying, or parked for budget.
    pub waiting_jobs: usize,
}

impl ModelBudgetStatus {
    /// One line for the operator's log while the gate is closed: why, until when, how many jobs wait and what to do
    /// about it. `None` while it is open.
    pub fn closed_notice(&self) -> Option<String> {
        let (blocked_by, until) = (self.blocked_by?, self.until?);
        let detail = match (blocked_by, &self.reason) {
            ("provider_outage", Some(reason)) => format!(": {reason}"),
            _ => String::new(),
        };
        let next = match blocked_by {
            "provider_outage" => {
                "The worker tries again by itself; `wam-worker --resume-budget` tries again now."
            }
            "provider_quota" => "After adding credit, reopen it with `wam-worker --resume-budget`.",
            _ => "It reopens by itself at the next UTC day.",
        };
        Some(format!(
            "model budget closed ({blocked_by}{detail}) until {until}; {} contribution job(s) wait. {next}",
            self.waiting_jobs
        ))
    }
}

pub(crate) fn status(state: &AppState, now: u64, policy: &ModelBudgetPolicy) -> ModelBudgetStatus {
    let budget = &state.model_budget;
    let day = now / DAY_SECS;
    let spent = if day > budget.day {
        0
    } else {
        budget.spent_micro_usd
    };
    let capped = policy.daily_cap_micro_usd.is_some_and(|cap| spent >= cap);
    let (blocked_by, until) = if capped {
        (Some(BudgetBlock::DailyCap), Some((day + 1) * DAY_SECS))
    } else if now < budget.paused_until {
        (Some(budget.pause_cause.block()), Some(budget.paused_until))
    } else {
        (None, None)
    };
    ModelBudgetStatus {
        open: blocked_by.is_none(),
        blocked_by: blocked_by.map(BudgetBlock::as_str),
        until,
        strikes: budget.strikes,
        outage_strikes: budget.outage_strikes,
        reason: (blocked_by == Some(BudgetBlock::ProviderOutage))
            .then(|| budget.reason.clone())
            .filter(|reason| !reason.is_empty()),
        spent_today_usd: spent as f64 / MICRO,
        daily_cap_usd: policy.daily_cap_usd(),
        waiting_jobs: state
            .work
            .values()
            .filter(|job| {
                job.kind == "contribution"
                    && matches!(job.state.as_str(), "queued" | "retry" | "waiting_budget")
            })
            .count(),
    }
}

impl ApplicationRepository {
    /// The gate as an operator sees it (`wam-worker --budget-status`).
    pub fn model_budget_status(
        &self,
        policy: &ModelBudgetPolicy,
        now: u64,
    ) -> Result<ModelBudgetStatus, ServiceError> {
        self.read(|state| Ok(status(state, now, policy)))
    }

    /// Reopens the provider pause now (`wam-worker --resume-budget`), for instance right after credit was added, and
    /// puts the parked jobs back in the queue (`queued`) so that they start at once. It returns how many were
    /// parked. It does not lift the daily ceiling: that reopens at the next UTC day, or when the ceiling is raised.
    pub fn resume_model_budget(&self, now: u64) -> Result<usize, ServiceError> {
        self.update(|state| {
            let budget = &mut state.model_budget;
            let was_closed =
                budget.paused_until != 0 || budget.strikes != 0 || budget.outage_strikes != 0;
            budget.paused_until = 0;
            budget.strikes = 0;
            budget.outage_strikes = 0;
            budget.pause_cause = PauseCause::default();
            budget.reason.clear();
            budget.outage_waits.clear();
            let mut released = 0;
            for job in state.work.values_mut() {
                if job.kind == "contribution" && job.state == "waiting_budget" {
                    job.state = "queued".into();
                    job.not_before = 0;
                    released += 1;
                }
            }
            if was_closed || released > 0 {
                state.emit("worker_budget_resumed", "manual", now);
            }
            Ok(released)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn kinds(state: &AppState) -> Vec<(String, String)> {
        state
            .outbox
            .iter()
            .map(|event| (event.kind.clone(), event.target.clone()))
            .collect()
    }

    fn refused(hint: Option<u64>) -> Answer {
        Answer::Refused {
            retry_after_secs: hint,
        }
    }

    const T0: u64 = 1_800_000_000;

    #[test]
    fn without_a_hint_each_consecutive_refusal_doubles_the_pause_up_to_the_maximum() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        let mut now = T0;
        for expected in [900, 1800, 3600, 3600] {
            record(&mut state, now, 0, refused(None), &policy);
            assert_eq!(state.model_budget.paused_until, now + expected);
            assert!(!admit(&mut state, now + expected - 1, &policy));
            assert!(admit(&mut state, now + expected, &policy));
            now += expected;
        }
        assert_eq!(state.model_budget.strikes, 4);
        // Every pause was announced once when it began and once when it ended.
        let all = kinds(&state);
        assert_eq!(
            all.iter()
                .filter(|(kind, _)| kind == "worker_budget_paused")
                .count(),
            4
        );
        assert_eq!(
            all.iter()
                .filter(|(kind, _)| kind == "worker_budget_resumed")
                .count(),
            4
        );
    }

    #[test]
    fn a_provider_hint_is_used_within_bounds_and_an_answer_ends_the_backoff() {
        let policy = ModelBudgetPolicy::default();
        for (hint, expected) in [(5, 30), (120, 120), (100_000, 3600)] {
            let mut state = AppState::default();
            record(&mut state, T0, 0, refused(Some(hint)), &policy);
            assert_eq!(state.model_budget.paused_until, T0 + expected, "{hint}");
        }
        let mut state = AppState::default();
        record(&mut state, T0, 0, refused(None), &policy);
        record(&mut state, T0 + 900, 0, refused(None), &policy);
        assert_eq!(state.model_budget.strikes, 2);
        record(&mut state, T0 + 3000, 0, Answer::Answered, &policy);
        assert_eq!(state.model_budget.strikes, 0);
        record(&mut state, T0 + 4000, 0, refused(None), &policy);
        assert_eq!(state.model_budget.paused_until, T0 + 4000 + 900);
    }

    #[test]
    fn a_failure_that_is_not_a_refusal_changes_nothing_and_a_pause_never_shortens() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        record(&mut state, T0, 0, Answer::Other, &policy);
        assert_eq!(
            state.model_budget,
            ModelBudgetState {
                day: T0 / DAY_SECS,
                ..ModelBudgetState::default()
            }
        );
        record(&mut state, T0, 0, refused(Some(3000)), &policy);
        record(&mut state, T0 + 1, 0, refused(Some(60)), &policy);
        assert_eq!(state.model_budget.paused_until, T0 + 3000);
    }

    #[test]
    fn the_daily_ceiling_closes_the_gate_until_the_next_utc_day() {
        let policy = ModelBudgetPolicy::new(Some(0.05), 900, 3600).unwrap();
        let day_start = (T0 / DAY_SECS) * DAY_SECS;
        let evening = day_start + 20 * 3600;
        let mut state = AppState::default();
        assert!(admit(&mut state, evening, &policy));
        record(&mut state, evening, 30_000, Answer::Answered, &policy);
        assert!(admit(&mut state, evening + 1, &policy));
        record(&mut state, evening + 2, 30_000, Answer::Answered, &policy);
        assert!(!admit(&mut state, evening + 3, &policy));
        assert!(!admit(&mut state, day_start + DAY_SECS - 1, &policy));
        assert!(admit(&mut state, day_start + DAY_SECS, &policy));
        assert_eq!(state.model_budget.spent_micro_usd, 0);
        assert_eq!(
            kinds(&state),
            vec![
                ("worker_budget_paused".to_owned(), "daily_cap".to_owned()),
                ("worker_budget_resumed".to_owned(), "daily_cap".to_owned()),
            ]
        );
    }

    #[test]
    fn the_spending_day_only_moves_forward_when_clocks_disagree() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        let tomorrow = (T0 / DAY_SECS + 1) * DAY_SECS + 10;
        record(&mut state, tomorrow, 7, Answer::Answered, &policy);
        // A worker whose clock is still on the previous day must not reset or rewind the count.
        record(&mut state, tomorrow - 100, 5, Answer::Answered, &policy);
        assert_eq!(state.model_budget.spent_micro_usd, 12);
        assert_eq!(state.model_budget.day, tomorrow / DAY_SECS);
    }

    #[test]
    fn status_tells_an_operator_why_and_until_when() {
        let policy = ModelBudgetPolicy::new(Some(1.0), 900, 3600).unwrap();
        let mut state = AppState::default();
        assert!(status(&state, T0, &policy).open);
        record(&mut state, T0, 250_000, refused(Some(600)), &policy);
        let closed = status(&state, T0 + 1, &policy);
        assert_eq!(
            (closed.open, closed.blocked_by, closed.until, closed.strikes),
            (false, Some("provider_quota"), Some(T0 + 600), 1)
        );
        assert_eq!(closed.spent_today_usd, 0.25);
        assert_eq!(closed.daily_cap_usd, Some(1.0));
        let reopened = status(&state, T0 + 600, &policy);
        assert!(reopened.open);
        // A new UTC day starts with nothing spent.
        let next_day = (T0 / DAY_SECS + 1) * DAY_SECS;
        assert_eq!(status(&state, next_day, &policy).spent_today_usd, 0.0);
    }

    #[test]
    fn the_settings_are_read_strictly() {
        let defaults = ModelBudgetPolicy::from_lookup(lookup(&[])).unwrap();
        assert_eq!(defaults, ModelBudgetPolicy::default());
        let blank = ModelBudgetPolicy::from_lookup(lookup(&[("WAM_LLM_DAILY_BUDGET_USD", "  ")]));
        assert_eq!(blank.unwrap(), ModelBudgetPolicy::default());
        let set = ModelBudgetPolicy::from_lookup(lookup(&[
            ("WAM_LLM_DAILY_BUDGET_USD", "7.5"),
            ("WAM_LLM_QUOTA_PAUSE_SECS", "60"),
            ("WAM_LLM_QUOTA_PAUSE_MAX_SECS", "600"),
        ]))
        .unwrap();
        assert_eq!(set.daily_cap_usd(), Some(7.5));
        assert!(set.has_daily_cap());
        for bad in [
            vec![("WAM_LLM_DAILY_BUDGET_USD", "0")],
            vec![("WAM_LLM_DAILY_BUDGET_USD", "-3")],
            vec![("WAM_LLM_DAILY_BUDGET_USD", "NaN")],
            vec![("WAM_LLM_DAILY_BUDGET_USD", "inf")],
            vec![("WAM_LLM_DAILY_BUDGET_USD", "about five")],
            vec![("WAM_LLM_DAILY_BUDGET_USD", "1000001")],
            vec![("WAM_LLM_QUOTA_PAUSE_SECS", "5")],
            vec![("WAM_LLM_QUOTA_PAUSE_SECS", "soon")],
            vec![
                ("WAM_LLM_QUOTA_PAUSE_SECS", "1200"),
                ("WAM_LLM_QUOTA_PAUSE_MAX_SECS", "600"),
            ],
        ] {
            assert!(
                matches!(
                    ModelBudgetPolicy::from_lookup(lookup(&bad)),
                    Err(ServiceError::InvalidInput(
                        "invalid_model_budget_configuration"
                    ))
                ),
                "{bad:?}"
            );
        }
    }

    // --- provider outages (docs/spec/provider-outage.md, docs/test/provider-outage.md) ---

    use crate::durable::state::Work;

    const OUTAGE: &str = "HTTP 503 from model provider";

    /// A work row for `contribution` in `work_state`, as the claim would have left it.
    fn job(state: &mut AppState, contribution: &str, work_state: &str) {
        let id = format!("contribution:{contribution}");
        state.work.insert(
            id.clone(),
            Work {
                id,
                contribution_id: Some(contribution.into()),
                memory_id: None,
                state: work_state.into(),
                generation: 1,
                lease_until: None,
                holder: None,
                enqueued_at: 0,
                kind: "contribution".into(),
                input_digest: None,
                not_before: 0,
                attempts: 0,
            },
        );
    }

    fn count_events(state: &AppState, kind: &str, target: &str) -> usize {
        kinds(state)
            .iter()
            .filter(|(k, t)| k == kind && t == target)
            .count()
    }

    #[test]
    fn an_outage_backs_off_from_thirty_seconds_to_ten_minutes() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        let mut now = T0;
        for expected in [30, 60, 120, 240, 480, 600, 600] {
            assert!(record_outage(
                &mut state, now, 0, "a", OUTAGE, None, &policy
            ));
            assert_eq!(state.model_budget.paused_until, now + expected);
            assert!(!admit(&mut state, now + expected - 1, &policy));
            assert!(admit(&mut state, now + expected, &policy));
            now += expected;
        }
        assert_eq!(state.model_budget.outage_strikes, 7);
        // Closing and reopening were each announced once per pause, naming the outage.
        assert_eq!(
            count_events(&state, "worker_budget_paused", "provider_outage"),
            7
        );
        assert_eq!(
            count_events(&state, "worker_budget_resumed", "provider_outage"),
            7
        );
        // Credit refusals are counted apart and untouched.
        assert_eq!(state.model_budget.strikes, 0);
    }

    #[test]
    fn retry_after_beats_a_shorter_backoff_and_is_bounded() {
        let policy = ModelBudgetPolicy::default();
        // (hint, outages already in a row, how long the gate closes)
        for (hint, before, expected) in [
            (Some(5), 0, 30),
            (Some(45), 0, 45),
            (Some(45), 2, 120),
            (Some(100_000), 0, 3600),
            (None, 9, 600),
        ] {
            let mut state = AppState::default();
            job(&mut state, "a", "leased");
            state.model_budget.outage_strikes = before;
            record_outage(&mut state, T0, 0, "a", OUTAGE, hint, &policy);
            assert_eq!(
                state.model_budget.paused_until,
                T0 + expected,
                "{hint:?} after {before}"
            );
        }
        // A later, shorter pause never shortens the one that is set.
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        record_outage(&mut state, T0, 0, "a", OUTAGE, Some(3000), &policy);
        record_outage(&mut state, T0 + 1, 0, "a", OUTAGE, Some(60), &policy);
        assert_eq!(state.model_budget.paused_until, T0 + 3000);
    }

    #[test]
    fn an_answer_clears_both_counts_and_the_jobs_waits() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        job(&mut state, "b", "leased");
        record(&mut state, T0, 0, refused(None), &policy);
        record(&mut state, T0 + 900, 0, refused(None), &policy);
        for round in 0..3 {
            record_outage(
                &mut state,
                T0 + 2000 + round * 700,
                0,
                "a",
                OUTAGE,
                None,
                &policy,
            );
        }
        record_outage(&mut state, T0 + 5000, 0, "b", OUTAGE, None, &policy);
        assert_eq!(
            (
                state.model_budget.strikes,
                state.model_budget.outage_strikes
            ),
            (2, 4)
        );
        // A failure that is not an answer changes nothing.
        let before = state.model_budget.clone();
        record(&mut state, T0 + 9000, 0, Answer::Other, &policy);
        assert_eq!(state.model_budget, before);
        // An answer ends both streaks and the job's own waits, and only its own.
        record_answer(&mut state, T0 + 9001, 0, "a", &policy);
        assert_eq!(
            (
                state.model_budget.strikes,
                state.model_budget.outage_strikes,
                state.model_budget.reason.as_str()
            ),
            (0, 0, "")
        );
        assert_eq!(
            (outage_waits(&state, "a"), outage_waits(&state, "b")),
            (0, 1)
        );
    }

    #[test]
    fn the_later_pause_names_the_cause() {
        let policy = ModelBudgetPolicy::default();
        // A credit refusal holds the gate for fifteen minutes; an outage inside that window does not take over.
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        record(&mut state, T0, 0, refused(None), &policy);
        record_outage(&mut state, T0 + 1, 0, "a", OUTAGE, None, &policy);
        assert_eq!(state.model_budget.paused_until, T0 + 900);
        assert_eq!(
            status(&state, T0 + 2, &policy).blocked_by,
            Some("provider_quota")
        );
        assert_eq!(
            count_events(&state, "worker_budget_paused", "provider_quota"),
            1
        );
        assert_eq!(
            count_events(&state, "worker_budget_paused", "provider_outage"),
            0
        );
        // An outage that has run its course and is followed by a refusal leaves the refusal as the reason.
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        record_outage(&mut state, T0, 0, "a", OUTAGE, None, &policy);
        assert_eq!(
            status(&state, T0 + 1, &policy).blocked_by,
            Some("provider_outage")
        );
        assert!(admit(&mut state, T0 + 30, &policy));
        record(&mut state, T0 + 30, 0, refused(None), &policy);
        assert_eq!(
            status(&state, T0 + 31, &policy).blocked_by,
            Some("provider_quota")
        );
        assert_eq!(
            kinds(&state),
            vec![
                (
                    "worker_budget_paused".to_owned(),
                    "provider_outage".to_owned()
                ),
                (
                    "worker_budget_resumed".to_owned(),
                    "provider_outage".to_owned()
                ),
                (
                    "worker_budget_paused".to_owned(),
                    "provider_quota".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_job_has_a_limited_number_of_free_waits() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        job(&mut state, "b", "leased");
        for hit in 1..=8u64 {
            assert!(
                record_outage(&mut state, T0 + hit * 700, 0, "a", OUTAGE, None, &policy),
                "hit {hit}"
            );
        }
        // The ninth outage and every one after it counts against the job.
        for hit in 9..=10u64 {
            assert!(
                !record_outage(&mut state, T0 + hit * 700, 0, "a", OUTAGE, None, &policy),
                "hit {hit}"
            );
        }
        assert_eq!(outage_waits(&state, "a"), 8);
        // Another job's allowance is its own, and an answer gives a job its allowance back.
        assert!(record_outage(
            &mut state,
            T0 + 11 * 700,
            0,
            "b",
            OUTAGE,
            None,
            &policy
        ));
        record_answer(&mut state, T0 + 12 * 700, 0, "a", &policy);
        assert!(record_outage(
            &mut state,
            T0 + 13 * 700,
            0,
            "a",
            OUTAGE,
            None,
            &policy
        ));
        // With no free waits every outage counts against the job (the behaviour before outages were told apart).
        let none = ModelBudgetPolicy::default()
            .with_outage(30, 600, 0)
            .unwrap();
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        assert!(!record_outage(&mut state, T0, 0, "a", OUTAGE, None, &none));
        assert_eq!(outage_waits(&state, "a"), 0);
        // The gate closes all the same.
        assert!(!admit(&mut state, T0 + 1, &none));
    }

    #[test]
    fn waits_of_jobs_that_are_gone_are_forgotten() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        job(&mut state, "b", "leased");
        record_outage(&mut state, T0, 0, "a", OUTAGE, None, &policy);
        record_outage(&mut state, T0 + 700, 0, "b", OUTAGE, None, &policy);
        assert_eq!(
            (outage_waits(&state, "a"), outage_waits(&state, "b")),
            (1, 1)
        );
        // `a` was published meanwhile; `b` is still waiting.
        state.work.get_mut("contribution:a").unwrap().state = "done".into();
        record_outage(&mut state, T0 + 1400, 0, "b", OUTAGE, None, &policy);
        assert_eq!(
            (outage_waits(&state, "a"), outage_waits(&state, "b")),
            (0, 2)
        );
        // A job that no longer exists at all is forgotten too.
        state.work.remove("contribution:b");
        record_outage(&mut state, T0 + 2100, 0, "c", OUTAGE, None, &policy);
        assert_eq!(state.model_budget.outage_waits.len(), 1);
    }

    #[test]
    fn status_names_an_outage_and_keeps_the_reason_clean() {
        let policy = ModelBudgetPolicy::default();
        let mut state = AppState::default();
        job(&mut state, "a", "leased");
        let long = "HTTP 503 ".repeat(40);
        record_outage(
            &mut state,
            T0,
            0,
            "a",
            &format!("{long}\n\u{7}\u{1b}[31m<tag>"),
            None,
            &policy,
        );
        let closed = status(&state, T0 + 1, &policy);
        assert_eq!(
            (
                closed.open,
                closed.blocked_by,
                closed.until,
                closed.outage_strikes
            ),
            (false, Some("provider_outage"), Some(T0 + 30), 1)
        );
        let reason = closed.reason.unwrap();
        assert!(reason.starts_with("HTTP 503"), "{reason}");
        assert!(reason.chars().count() <= 80, "{reason}");
        assert!(
            reason
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " ._:()/-".contains(c)),
            "{reason:?}"
        );
        // An open gate has nothing to explain, and a credit pause never shows an outage's words.
        assert_eq!(status(&state, T0 + 30, &policy).reason, None);
        let mut quota = AppState::default();
        record(&mut quota, T0, 0, refused(None), &policy);
        assert_eq!(status(&quota, T0 + 1, &policy).reason, None);
    }

    #[test]
    fn the_outage_settings_are_read_strictly() {
        let defaults = ModelBudgetPolicy::from_lookup(lookup(&[])).unwrap();
        assert_eq!(
            (
                defaults.outage_pause_secs,
                defaults.outage_pause_max_secs,
                defaults.outage_free_waits
            ),
            (30, 600, 8)
        );
        let blank = ModelBudgetPolicy::from_lookup(lookup(&[
            ("WAM_LLM_OUTAGE_PAUSE_SECS", " "),
            ("WAM_LLM_OUTAGE_FREE_WAITS", ""),
        ]))
        .unwrap();
        assert_eq!(blank, ModelBudgetPolicy::default());
        let set = ModelBudgetPolicy::from_lookup(lookup(&[
            ("WAM_LLM_OUTAGE_PAUSE_SECS", "60"),
            ("WAM_LLM_OUTAGE_PAUSE_MAX_SECS", "1200"),
            ("WAM_LLM_OUTAGE_FREE_WAITS", "0"),
        ]))
        .unwrap();
        assert_eq!(
            (
                set.outage_pause_secs,
                set.outage_pause_max_secs,
                set.outage_free_waits
            ),
            (60, 1200, 0)
        );
        for bad in [
            vec![("WAM_LLM_OUTAGE_PAUSE_SECS", "5")],
            vec![("WAM_LLM_OUTAGE_PAUSE_SECS", "86401")],
            vec![("WAM_LLM_OUTAGE_PAUSE_SECS", "soon")],
            vec![
                ("WAM_LLM_OUTAGE_PAUSE_SECS", "1200"),
                ("WAM_LLM_OUTAGE_PAUSE_MAX_SECS", "600"),
            ],
            vec![("WAM_LLM_OUTAGE_PAUSE_MAX_SECS", "20")],
            vec![("WAM_LLM_OUTAGE_FREE_WAITS", "1001")],
            vec![("WAM_LLM_OUTAGE_FREE_WAITS", "-1")],
            vec![("WAM_LLM_OUTAGE_FREE_WAITS", "many")],
        ] {
            assert!(
                matches!(
                    ModelBudgetPolicy::from_lookup(lookup(&bad)),
                    Err(ServiceError::InvalidInput(
                        "invalid_model_budget_configuration"
                    ))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_older_gate_document_is_read_as_an_open_gate() {
        // What the previous release stored: none of the outage fields.
        let stored = serde_json::json!({
            "paused_until": 0, "strikes": 2, "day": 19000, "spent_micro_usd": 5, "capped": false
        });
        let state: ModelBudgetState = serde_json::from_value(stored).unwrap();
        assert_eq!(state.strikes, 2);
        assert_eq!(
            (
                state.outage_strikes,
                state.pause_cause,
                state.reason.as_str(),
                state.outage_waits.len()
            ),
            (0, PauseCause::Quota, "", 0)
        );
        // A closed gate survives a round trip through the stored form.
        let mut closed = AppState::default();
        job(&mut closed, "a", "leased");
        record_outage(
            &mut closed,
            T0,
            0,
            "a",
            OUTAGE,
            Some(90),
            &ModelBudgetPolicy::default(),
        );
        let json = serde_json::to_string(&closed.model_budget).unwrap();
        assert!(json.contains("\"pause_cause\":\"outage\""), "{json}");
        let back: ModelBudgetState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, closed.model_budget);
    }

    #[test]
    fn resume_clears_the_outage_state() {
        let repository = ApplicationRepository::in_memory();
        repository
            .update(|state| {
                job(state, "a", "waiting_budget");
                record_outage(
                    state,
                    T0,
                    0,
                    "a",
                    OUTAGE,
                    None,
                    &ModelBudgetPolicy::default(),
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(repository.resume_model_budget(T0 + 1).unwrap(), 1);
        repository
            .read(|state| {
                let gate = &state.model_budget;
                assert_eq!(
                    (gate.paused_until, gate.strikes, gate.outage_strikes),
                    (0, 0, 0)
                );
                assert!(gate.reason.is_empty() && gate.outage_waits.is_empty());
                assert_eq!(gate.pause_cause, PauseCause::Quota);
                assert_eq!(state.work["contribution:a"].state, "queued");
                assert_eq!(count_events(state, "worker_budget_resumed", "manual"), 1);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn the_log_line_says_why_and_what_to_do() {
        let policy = ModelBudgetPolicy::new(Some(1.0), 900, 3600).unwrap();
        let mut state = AppState::default();
        job(&mut state, "a", "waiting_budget");
        assert_eq!(status(&state, T0, &policy).closed_notice(), None);
        record_outage(&mut state, T0, 0, "a", OUTAGE, None, &policy);
        let outage = status(&state, T0 + 1, &policy).closed_notice().unwrap();
        assert!(
            outage.contains("provider_outage: HTTP 503 from model provider"),
            "{outage}"
        );
        assert!(outage.contains(&format!("until {}", T0 + 30)), "{outage}");
        assert!(outage.contains("tries again by itself"), "{outage}");
        let mut state = AppState::default();
        record(&mut state, T0, 0, refused(None), &policy);
        let quota = status(&state, T0 + 1, &policy).closed_notice().unwrap();
        assert!(
            quota.contains("provider_quota") && quota.contains("adding credit"),
            "{quota}"
        );
        let mut state = AppState::default();
        record(&mut state, T0, 2_000_000, Answer::Answered, &policy);
        let capped = status(&state, T0 + 1, &policy).closed_notice().unwrap();
        assert!(
            capped.contains("daily_cap") && capped.contains("next UTC day"),
            "{capped}"
        );
    }
}
