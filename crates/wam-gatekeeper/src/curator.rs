//! The curator loop: instructions + tools + one provider = one decision per job (docs/spec/curator.md).
//!
//! The loop owns the budget and fails closed: a model that never decides, keeps replying with garbage, or cannot be
//! reached leaves the store untouched and the job to be retried by the queue.

use crate::INSTRUCTIONS;
use crate::provider::{LlmProvider, Message, ProviderError, TurnRequest};
use crate::record::sha256_hex;
use crate::store::{AuditRecord, Store};
use crate::tools::{Decision, Job, Policy, ToolCtx, Tools, Trigger};
use serde_json::{Value, json};
use wam_core::TaskVocabulary;

#[derive(Clone, Debug)]
pub struct CuratorConfig {
    /// Model turns per job (reads, retries, and the decision).
    pub max_turns: u32,
    /// Replies that were not a tool call before the job is given up.
    pub max_malformed: u32,
    /// A contribution whose steps are exactly those of the active memory in its scope is decided by rule (a
    /// duplicate, its evidence added) without calling the model. Turn it off to evaluate the model's own judgement.
    pub exact_duplicates_without_model: bool,
}

impl Default for CuratorConfig {
    fn default() -> Self {
        Self {
            max_turns: 14,
            max_malformed: 2,
            exact_duplicates_without_model: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Decided,
    /// No decision; the store is unchanged. The reason says why (turn limit, malformed replies).
    NoDecision(String),
    /// The model could not be reached or failed; retry later (docs/spec/queue.md).
    ProviderError(String),
    /// The provider's quota or credit is spent: park the job (`waiting_budget`), do not charge an attempt.
    QuotaExhausted {
        why: String,
        retry_after_secs: Option<u64>,
    },
    /// The provider could not answer for now (rate limited, overloaded, unreachable, timed out). The job is not at
    /// fault: the application worker parks it behind the budget gate and gives its attempt back, within a per-job
    /// limit (docs/spec/provider-outage.md). The store is unchanged.
    Unavailable {
        why: String,
        retry_after_secs: Option<u64>,
    },
}

#[derive(Clone, Debug)]
pub struct TurnLog {
    pub tool: String,
    pub args: Value,
    pub result: Value,
    pub raw: String,
    pub meta: Value,
}

#[derive(Clone, Debug)]
pub struct JobReport {
    pub outcome: Outcome,
    pub decision: Option<Decision>,
    pub turns: Vec<TurnLog>,
    pub audit: AuditRecord,
}

impl JobReport {
    /// What the model passes of this job cost in US dollars, as far as the provider said (`meta["cost_usd"]`). A turn
    /// without a reported cost counts as free, so a provider that never reports one is never counted against a ceiling.
    pub fn cost_usd(&self) -> f64 {
        self.turns
            .iter()
            .filter_map(|turn| turn.meta["cost_usd"].as_f64())
            .sum()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_job(
    job: &Job,
    store: &mut dyn Store,
    provider: &mut dyn LlmProvider,
    tools: &Tools,
    policy: &Policy,
    tasks: &TaskVocabulary,
    config: &CuratorConfig,
    now: u64,
) -> JobReport {
    let mut provider_name = provider.name();
    let mut transcript = vec![Message::Job("Begin the job.".to_owned())];
    let mut turns: Vec<TurnLog> = Vec::new();
    let mut malformed = 0u32;
    let mut outcome = Outcome::NoDecision("the turn limit was reached".into());

    let (decision, calls) = {
        let mut ctx = ToolCtx::new(job, store, tools, policy, tasks, now);
        let mut turn_budget = config.max_turns;
        if config.exact_duplicates_without_model && job.trigger == Trigger::NewContribution {
            match ctx.exact_duplicate() {
                Ok(Some(existing_id)) => {
                    let args = json!({
                        "existing_id": existing_id,
                        "reason": "duplicate_support",
                        "reason_codes": ["same_fingerprint"],
                    });
                    // The rule goes through the same tool layer as the model would, so every check still applies.
                    let result = match ctx.call("decide_skip", &args) {
                        Ok(value) => value,
                        Err(error) => error.to_value(),
                    };
                    turns.push(TurnLog {
                        tool: "decide_skip".into(),
                        args,
                        result,
                        raw: "(decided by rule: exact fingerprint match)".into(),
                        meta: Value::Null,
                    });
                    if ctx.decision().is_some() {
                        outcome = Outcome::Decided;
                        provider_name = "rule:exact-fingerprint".into();
                        turn_budget = 0;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    outcome = Outcome::ProviderError(error.to_string());
                    turn_budget = 0;
                }
            }
        }
        for _ in 0..turn_budget {
            let request = TurnRequest {
                system: INSTRUCTIONS,
                tools: &tools.specs,
                transcript: &transcript,
            };
            let turn = match provider.next_turn(&request) {
                Ok(turn) => turn,
                Err(ProviderError::Malformed { raw, why }) => {
                    malformed += 1;
                    let value = json!({"error": {"code": "SCHEMA_INVALID", "message": format!("your last reply was not a valid tool call JSON object: {why}")}});
                    turns.push(TurnLog {
                        tool: "(malformed)".into(),
                        args: Value::Null,
                        result: value.clone(),
                        raw,
                        meta: Value::Null,
                    });
                    transcript.push(Message::Result {
                        tool: "(your last reply)".into(),
                        value,
                    });
                    if malformed > config.max_malformed {
                        outcome =
                            Outcome::NoDecision("too many replies that were not tool calls".into());
                        break;
                    }
                    continue;
                }
                Err(ProviderError::QuotaExhausted {
                    why,
                    retry_after_secs,
                }) => {
                    outcome = Outcome::QuotaExhausted {
                        why,
                        retry_after_secs,
                    };
                    break;
                }
                Err(ProviderError::Unavailable {
                    why,
                    retry_after_secs,
                }) => {
                    outcome = Outcome::Unavailable {
                        why,
                        retry_after_secs,
                    };
                    break;
                }
                Err(error) => {
                    outcome = Outcome::ProviderError(error.to_string());
                    break;
                }
            };
            let result = match ctx.call(&turn.tool, &turn.args) {
                Ok(value) => value,
                Err(error) => error.to_value(),
            };
            transcript.push(Message::Call {
                tool: turn.tool.clone(),
                args: turn.args.clone(),
            });
            transcript.push(Message::Result {
                tool: turn.tool.clone(),
                value: result.clone(),
            });
            turns.push(TurnLog {
                tool: turn.tool,
                args: turn.args,
                result,
                raw: turn.raw,
                meta: turn.meta,
            });
            if ctx.decision().is_some() {
                outcome = Outcome::Decided;
                break;
            }
        }
        (ctx.decision().cloned(), ctx.log.clone())
    };

    let mut audit = AuditRecord {
        job_id: job.id.clone(),
        provider: provider_name,
        instructions_hash: sha256_hex(INSTRUCTIONS.as_bytes()),
        tools_hash: tools.hash.clone(),
        job_digest: job.digest(),
        calls,
        decision: decision.as_ref().and_then(|d| serde_json::to_value(d).ok()),
        outcome: match &outcome {
            Outcome::Decided => "decided".into(),
            Outcome::NoDecision(why) => format!("no_decision: {why}"),
            Outcome::ProviderError(why) => format!("provider_error: {why}"),
            Outcome::QuotaExhausted { why, .. } => format!("quota_exhausted: {why}"),
            Outcome::Unavailable { why, .. } => format!("provider_unavailable: {why}"),
        },
        at: now,
    };
    if store.log_audit(audit.clone()).is_err() {
        outcome = Outcome::ProviderError("memory store could not persist the audit record".into());
        audit.outcome = "audit_storage_failed".into();
    }
    JobReport {
        outcome,
        decision,
        turns,
        audit,
    }
}
