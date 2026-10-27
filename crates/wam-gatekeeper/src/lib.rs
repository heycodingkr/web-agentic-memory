#![forbid(unsafe_code)]

//! Memory curation for Web Agentic Memory (docs/spec/curator.md, queue.md).
//!
//! The curator model proposes; this crate disposes. Every tool call goes through a deterministic layer that
//! enforces the invariants of the spec whatever the model says. The model itself sits behind
//! [`provider::LlmProvider`]: a scripted fake for tests, the locally installed Claude or Codex CLI for
//! development, and an OpenAI-compatible HTTP API such as DeepSeek or Meta Muse for deployment.
//! Switching the model is a configuration change, not a code change.

pub mod budget;
pub mod curator;
pub mod error;
pub mod postgres_queue;
pub mod postgres_store;
pub mod provider;
pub mod queue;
pub mod record;
pub mod scenario;
pub mod store;
pub mod text;
pub mod tools;
pub mod worker;

/// The instructions the curator model receives. The spec file is the single source of truth.
pub const INSTRUCTIONS: &str = include_str!("../../../docs/spec/curator/instructions.v1.md");
/// The tool definitions the curator model receives (JSON Schema, with `$defs`).
pub const TOOLS_JSON: &str = include_str!("../../../docs/spec/curator/tools.v1.json");
