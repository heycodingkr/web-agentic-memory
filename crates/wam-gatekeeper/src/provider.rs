//! The model seam. The curator loop talks to [`LlmProvider`] and nothing else.
//!
//! * `ScriptedProvider`: deterministic fake for tests and for the scripted scenario runs.
//! * `CliProvider`: the locally installed Claude Code or Codex CLI, used as a plain text completer with no tools,
//!   for local development (it reuses the user's own login, so no API key is needed). Tool calls travel as a JSON
//!   turn protocol (`{"tool": ..., "args": ...}`), the fallback docs/spec/curator.md describes for models without
//!   native tool calling.
//! * `openai-compat` (DeepSeek, Meta Muse): the deployment HTTP provider, selected by configuration.

use crate::tools::ToolSpec;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Job(String),
    Call { tool: String, args: Value },
    Result { tool: String, value: Value },
}

pub struct TurnRequest<'a> {
    pub system: &'a str,
    pub tools: &'a [ToolSpec],
    pub transcript: &'a [Message],
}

#[derive(Clone, Debug)]
pub struct ModelTurn {
    pub tool: String,
    pub args: Value,
    pub raw: String,
    /// Provider-specific accounting (cost, tokens) when known.
    pub meta: Value,
}

#[derive(Debug)]
pub enum ProviderError {
    Spawn(String),
    Timeout(u64),
    Failed(String),
    Configuration(String),
    Malformed {
        raw: String,
        why: String,
    },
    NotBuilt(String),
    Exhausted,
    /// The account's usage limit or credit is spent. Trying again at once cannot help, so the queue parks the job
    /// (`waiting_budget`) and the worker pauses the budget instead of charging an attempt. `retry_after_secs` is
    /// what the provider said about when the quota returns, if it said anything.
    QuotaExhausted {
        why: String,
        retry_after_secs: Option<u64>,
    },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(why) => write!(f, "could not start the model process: {why}"),
            Self::Timeout(seconds) => write!(f, "the model call timed out after {seconds}s"),
            Self::Failed(why) => write!(f, "the model call failed: {why}"),
            Self::Configuration(why) => write!(f, "model provider configuration: {why}"),
            Self::Malformed { why, .. } => write!(f, "the model reply was not a tool call: {why}"),
            Self::NotBuilt(why) => write!(f, "{why}"),
            Self::Exhausted => write!(f, "the scripted provider has no more turns"),
            Self::QuotaExhausted { why, .. } => write!(f, "the model quota is exhausted: {why}"),
        }
    }
}

impl std::error::Error for ProviderError {}

pub trait LlmProvider {
    fn name(&self) -> String;
    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError>;
    /// Does every turn carry what it cost, in US dollars, as `meta["cost_usd"]`? A worker that enforces a spending
    /// ceiling needs this: a provider that does not say what it cost is never counted against the ceiling.
    fn reports_cost(&self) -> bool {
        false
    }
}

/// One step of a scripted run.
#[derive(Clone, Debug)]
pub enum ScriptStep {
    Call(String, Value),
    /// A reply that is not a tool call (exercises the loop's handling of malformed output).
    Raw(String),
}

pub struct ScriptedProvider {
    steps: VecDeque<ScriptStep>,
}

impl ScriptedProvider {
    pub fn new(steps: impl IntoIterator<Item = ScriptStep>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
        }
    }

    pub fn calls(calls: impl IntoIterator<Item = (String, Value)>) -> Self {
        Self::new(
            calls
                .into_iter()
                .map(|(tool, args)| ScriptStep::Call(tool, args)),
        )
    }
}

impl LlmProvider for ScriptedProvider {
    fn name(&self) -> String {
        "fake:scripted".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        match self.steps.pop_front().ok_or(ProviderError::Exhausted)? {
            ScriptStep::Call(tool, args) => Ok(ModelTurn {
                raw: json!({"tool": tool, "args": args}).to_string(),
                tool,
                args,
                meta: Value::Null,
            }),
            ScriptStep::Raw(raw) => Err(ProviderError::Malformed {
                why: "scripted malformed reply".into(),
                raw,
            }),
        }
    }
}

mod cli;
mod factory;
mod openai_compat;
mod protocol;

pub use cli::{CliKind, CliProvider};
pub use factory::build_provider;
pub use openai_compat::{OpenAiCompatProvider, Pricing};
pub use protocol::{extract_turn, looks_like_quota_error, protocol_text, render_transcript};
