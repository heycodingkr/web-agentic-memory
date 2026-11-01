//! The model budget gate of the application worker (docs/spec/queue.md, QUE-08 and QUE-20..23).
//!
//! Model credit can run out in two ways: the provider says so (an HTTP 402 for a spent balance or an account without
//! billing, or a usage limit), or the operator's own daily ceiling is reached. Either way nothing may be lost and
//! nothing may keep calling a provider that will refuse. While the gate is closed no worker claims a job, runs a replay
//! or calls the model, so submitted contributions stay `queued` and the one job that met the refusal waits as
//! `waiting_budget` without having used an attempt. The gate lives in the durable application state, so every worker
//! and every restart sees the same pause. Recall never depends on it.

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

/// What the operator allows. `Default` is "no daily ceiling"; the provider's own quota still parks jobs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelBudgetPolicy {
    daily_cap_micro_usd: Option<u64>,
    quota_pause_secs: u64,
    quota_pause_max_secs: u64,
}

impl Default for ModelBudgetPolicy {
    fn default() -> Self {
        Self {
            daily_cap_micro_usd: None,
            // The wait docs/spec/queue.md names when the provider does not say when its quota returns.
            quota_pause_secs: 900,
            quota_pause_max_secs: 3600,
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
        })
    }

    /// `WAM_LLM_DAILY_BUDGET_USD` (decimal, optional), `WAM_LLM_QUOTA_PAUSE_SECS` (default 900) and
    /// `WAM_LLM_QUOTA_PAUSE_MAX_SECS` (default 3600). Blank counts as unset; anything unreadable is refused so a
    /// typo cannot silently remove the ceiling.
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
        Self::new(
            cap,
            secs("WAM_LLM_QUOTA_PAUSE_SECS", defaults.quota_pause_secs)?,
            secs(
                "WAM_LLM_QUOTA_PAUSE_MAX_SECS",
                defaults.quota_pause_max_secs,
            )?,
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
    /// The operator's daily ceiling is reached.
    DailyCap,
}

impl BudgetBlock {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderQuota => "provider_quota",
            Self::DailyCap => "daily_cap",
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
        events.push(("worker_budget_resumed", BudgetBlock::ProviderQuota));
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
        Answer::Answered => budget.strikes = 0,
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
            budget.paused_until = budget.paused_until.max(now.saturating_add(wait));
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

/// What an operator needs to know about the gate.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModelBudgetStatus {
    /// May a job start right now?
    pub open: bool,
    /// Why not, when it may not (`provider_quota` or `daily_cap`).
    pub blocked_by: Option<&'static str>,
    /// When the gate reopens by itself (unix seconds), when it is closed.
    pub until: Option<u64>,
    /// Consecutive provider refusals without an answer in between.
    pub strikes: u32,
    pub spent_today_usd: f64,
    pub daily_cap_usd: Option<f64>,
    /// Contribution jobs waiting to start: queued, retrying, or parked for budget.
    pub waiting_jobs: usize,
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
        (Some(BudgetBlock::ProviderQuota), Some(budget.paused_until))
    } else {
        (None, None)
    };
    ModelBudgetStatus {
        open: blocked_by.is_none(),
        blocked_by: blocked_by.map(BudgetBlock::as_str),
        until,
        strikes: budget.strikes,
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
            let was_closed = budget.paused_until != 0 || budget.strikes != 0;
            budget.paused_until = 0;
            budget.strikes = 0;
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
}
