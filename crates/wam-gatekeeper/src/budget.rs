//! Model budget (docs/spec/queue.md): a per-minute request and token bucket plus a daily credit ceiling.
//!
//! The curator is the only paid, rate-limited part of the pipeline. When the budget is spent the queue parks jobs
//! (`waiting_budget`) instead of failing them, and recall is never affected. Time is passed in, in seconds.

const DAY_SECS: u64 = 86_400;

#[derive(Clone, Debug)]
pub struct BudgetConfig {
    pub requests_per_minute: u32,
    pub tokens_per_minute: u64,
    /// Credits per UTC day (the provider's own unit: dollars for the CLIs, credits for Muse). `None`: no ceiling.
    pub daily_credits: Option<f64>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 20,
            tokens_per_minute: 200_000,
            daily_credits: None,
        }
    }
}

/// When to try again, and whether the daily ceiling (not the per-minute rate) is the reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetWait {
    pub retry_after_secs: u64,
    pub daily: bool,
}

pub struct ModelBudget {
    cfg: BudgetConfig,
    requests: f64,
    tokens: f64,
    refilled_at: u64,
    day: u64,
    spent: f64,
    /// No model call before this time: set when the provider itself says its quota is spent.
    paused_until: u64,
}

impl ModelBudget {
    pub fn new(cfg: BudgetConfig, now: u64) -> Self {
        Self {
            requests: f64::from(cfg.requests_per_minute),
            tokens: cfg.tokens_per_minute as f64,
            cfg,
            refilled_at: now,
            day: now / DAY_SECS,
            spent: 0.0,
            paused_until: 0,
        }
    }

    /// The provider says its quota is spent: no call before `until`, whatever the buckets hold. A pause never
    /// shortens one that is already set.
    pub fn pause_until(&mut self, until: u64) {
        self.paused_until = self.paused_until.max(until);
    }

    fn refill(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.refilled_at) as f64;
        self.requests = (self.requests + f64::from(self.cfg.requests_per_minute) * elapsed / 60.0)
            .min(f64::from(self.cfg.requests_per_minute));
        self.tokens = (self.tokens + self.cfg.tokens_per_minute as f64 * elapsed / 60.0)
            .min(self.cfg.tokens_per_minute as f64);
        self.refilled_at = self.refilled_at.max(now);
        if now / DAY_SECS > self.day {
            self.day = now / DAY_SECS;
            self.spent = 0.0;
        }
    }

    fn daily_exhausted(&self) -> bool {
        self.cfg.daily_credits.is_some_and(|cap| self.spent >= cap)
    }

    /// Reserves one model pass of about `tokens` tokens, or says when to come back. A pass larger than the whole
    /// per-minute bucket is charged the bucket, so that it can run at all.
    pub fn try_acquire(&mut self, now: u64, tokens: u64) -> Result<(), BudgetWait> {
        self.refill(now);
        let pause_wait = self.paused_until.saturating_sub(now);
        if self.daily_exhausted() {
            let daily_wait = (self.day + 1) * DAY_SECS - now;
            return Err(BudgetWait {
                retry_after_secs: daily_wait.max(pause_wait),
                daily: daily_wait >= pause_wait,
            });
        }
        if pause_wait > 0 {
            return Err(BudgetWait {
                retry_after_secs: pause_wait,
                daily: false,
            });
        }
        let need = (tokens as f64).min(self.cfg.tokens_per_minute as f64);
        let request_wait =
            (1.0 - self.requests).max(0.0) * 60.0 / f64::from(self.cfg.requests_per_minute.max(1));
        let token_wait =
            (need - self.tokens).max(0.0) * 60.0 / self.cfg.tokens_per_minute.max(1) as f64;
        let wait = request_wait.max(token_wait);
        if wait > 0.0 {
            return Err(BudgetWait {
                retry_after_secs: wait.ceil().max(1.0) as u64,
                daily: false,
            });
        }
        self.requests -= 1.0;
        self.tokens -= need;
        Ok(())
    }

    /// Is there room for a call right now? (Does not reserve anything.)
    pub fn has_room(&mut self, now: u64) -> bool {
        self.refill(now);
        !self.daily_exhausted()
            && now >= self.paused_until
            && self.requests >= 1.0
            && self.tokens >= 1.0
    }

    /// What a finished pass really cost. The ceiling is checked before a pass, so the last one may overshoot it.
    pub fn record_cost(&mut self, now: u64, credits: f64) {
        self.refill(now);
        self.spent += credits.max(0.0);
    }

    pub fn spent_today(&self) -> f64 {
        self.spent
    }
}
