//! The curator's tool layer (docs/spec/curator.md): four read tools and six decision tools.
//!
//! Whatever the model sends, this layer enforces the invariants: scope containment, no duplicate active memory,
//! optimistic concurrency, an evidence policy for replacing and retiring, schema and text checks on everything the
//! model writes, a read budget, and exactly one decision per job. A rejected call changes nothing.

mod decisions;
mod reads;

use crate::TOOLS_JSON;
use crate::error::{ErrorCode, ToolError};
use crate::queue::QueueLeaseFence;
use crate::record::{
    RecordPayload, Validated, ValidationCtx, build_pattern, candidate_fingerprint, kind_str,
    normalize_actions, parse_kind, path_covers, pattern_key, sha256_hex, step_actions, validate,
};
use crate::store::{
    DecisionCommit, Evidence, MemoryMutation, MemoryRow, ScopeKey, Status, Store, StoreError,
    VersionRow,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use wam_core::{KeywordVocabulary, TaskVocabulary, UrlMode, UrlPattern};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    Read,
    Decision,
}

#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: String,
    pub kind: ToolKind,
    pub description: String,
    /// JSON Schema with every `$ref` inlined, ready for a provider.
    pub input_schema: Value,
}

#[derive(Clone, Debug)]
pub struct Tools {
    pub specs: Vec<ToolSpec>,
    pub reason_codes: BTreeSet<String>,
    pub max_read_calls: u32,
    pub hash: String,
}

fn inline_refs(value: &Value, defs: &Value) -> Value {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if let Some(target) = reference
                    .strip_prefix("#/$defs/")
                    .and_then(|name| defs.get(name))
                {
                    let mut resolved = inline_refs(target, defs);
                    if let Value::Object(object) = &mut resolved {
                        for (key, sibling) in map.iter().filter(|(key, _)| key.as_str() != "$ref") {
                            object.insert(key.clone(), inline_refs(sibling, defs));
                        }
                    }
                    return resolved;
                }
            }
            Value::Object(
                map.iter()
                    .map(|(key, item)| (key.clone(), inline_refs(item, defs)))
                    .collect(),
            )
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| inline_refs(item, defs)).collect())
        }
        other => other.clone(),
    }
}

impl Tools {
    /// Parses the embedded `docs/spec/curator/tools.v1.json`.
    pub fn load() -> Result<Self, String> {
        let raw: Value =
            serde_json::from_str(TOOLS_JSON).map_err(|error| format!("tools.v1.json: {error}"))?;
        let defs = raw.get("$defs").cloned().unwrap_or(Value::Null);
        let mut specs = Vec::new();
        for tool in raw["tools"].as_array().ok_or("tools must be an array")? {
            let kind = match tool["kind"].as_str() {
                Some("read") => ToolKind::Read,
                Some("decision") => ToolKind::Decision,
                other => return Err(format!("unknown tool kind {other:?}")),
            };
            specs.push(ToolSpec {
                name: tool["name"].as_str().ok_or("tool name")?.to_owned(),
                kind,
                description: tool["description"].as_str().unwrap_or_default().to_owned(),
                input_schema: inline_refs(&tool["input_schema"], &defs),
            });
        }
        let reason_codes = defs["reason_codes"]["items"]["enum"]
            .as_array()
            .ok_or("reason_codes enum")?
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect();
        let max_read_calls = raw["limits"]["max_read_calls_per_scope_batch"]
            .as_u64()
            .ok_or("max_read_calls_per_scope_batch")? as u32;
        Ok(Self {
            specs,
            reason_codes,
            max_read_calls,
            hash: sha256_hex(TOOLS_JSON.as_bytes()),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    NewContribution,
    Reverify,
    Withdrawal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcomes {
    pub success: u32,
    #[serde(default)]
    pub partial: u32,
    #[serde(default)]
    pub failed: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub url_pattern: String,
    pub action_sequence: Vec<String>,
    pub outcomes: Outcomes,
    #[serde(default)]
    pub observed_traits: Vec<String>,
    pub independent_contributors: u32,
    #[serde(default)]
    pub ui_fingerprint: Option<String>,
    pub observed_at: u64,
}

impl Candidate {
    pub fn evidence(&self) -> Evidence {
        Evidence {
            independent_contributors: self.independent_contributors,
            success: self.outcomes.success,
            partial: self.outcomes.partial,
            failed: self.outcomes.failed,
            ui_fingerprint: self.ui_fingerprint.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobScope {
    pub site_host: String,
    pub task_category: String,
    pub intent_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: String,
    pub trigger: Trigger,
    pub scope: JobScope,
    pub candidate: Candidate,
    /// Original candidate traces reviewed together by one scope-level curator pass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub batch_candidates: Vec<Candidate>,
    #[serde(default)]
    pub contribution_ids: Vec<String>,
    #[serde(default)]
    pub replay_passed: bool,
    /// Runtime-only queue lease token; it must not change the immutable curator input digest.
    #[serde(skip)]
    pub lease_fence: Option<QueueLeaseFence>,
}

impl Job {
    pub fn digest(&self) -> String {
        sha256_hex(serde_json::to_string(self).unwrap_or_default().as_bytes())
    }
}

#[derive(Clone, Debug)]
pub struct Policy {
    /// Independent contributors needed before a memory is created.
    pub min_independent_create: u32,
    /// Independent contributors that stand in for a passed replay when replacing.
    pub replace_min_independent: u32,
    pub mode: UrlMode,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            min_independent_create: 1,
            replace_min_independent: 2,
            mode: UrlMode::Public,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// The decision enum of docs/spec/dedup.md: duplicate_support, merge_evidence, create_new, supersede, retire,
    /// needs_human_review, reject_invalid.
    pub kind: String,
    pub memory_id: Option<String>,
    pub version: Option<u32>,
    pub reason_codes: Vec<String>,
    pub confidence_band: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    url_pattern: Option<String>,
    keywords: Option<Vec<String>>,
    kind: Option<String>,
    task_category: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetArgs {
    id: String,
    level: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompareArgs {
    existing_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkipArgs {
    existing_id: String,
    reason: String,
    reason_codes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceArgs {
    existing_id: String,
    expected_version: u32,
    record: RecordPayload,
    reason_codes: Vec<String>,
    confidence_band: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    record: RecordPayload,
    reason_codes: Vec<String>,
    confidence_band: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetireArgs {
    existing_id: String,
    expected_version: u32,
    reason_codes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodesArgs {
    reason_codes: Vec<String>,
}

fn parse_args<T: DeserializeOwned>(args: &Value) -> Result<T, ToolError> {
    serde_json::from_value(args.clone()).map_err(|error| {
        ToolError::new(
            ErrorCode::SchemaInvalid,
            format!("invalid arguments: {error}"),
        )
    })
}

fn schema(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::SchemaInvalid, message)
}

fn storage_unavailable(_error: StoreError) -> ToolError {
    ToolError::new(
        ErrorCode::StorageUnavailable,
        "the memory store is temporarily unavailable; make no decision and retry later",
    )
}

fn store_commit_error(error: StoreError, kind: &str) -> ToolError {
    match error {
        StoreError::Unavailable => storage_unavailable(error),
        StoreError::Missing => ToolError::new(
            ErrorCode::ScopeViolation,
            "no such memory in this job's site host",
        ),
        StoreError::Conflict if kind == "create_new" => ToolError::new(
            ErrorCode::DuplicateExists,
            "an active memory already covers this scope; skip or replace instead",
        ),
        StoreError::Conflict if matches!(kind, "supersede" | "retire") => ToolError::new(
            ErrorCode::VersionConflict,
            "the memory changed while this decision was being committed; read again and decide again",
        ),
        StoreError::Conflict => ToolError::new(
            ErrorCode::ScopeViolation,
            "the target memory is no longer active in this job's site host",
        ),
    }
}

/// 3 same route, 2 one covers the other, 1 same host only.
fn relation(wanted: &UrlPattern, other: &UrlPattern) -> u32 {
    if pattern_key(wanted) == pattern_key(other) {
        3
    } else if path_covers(wanted.path(), other.path()) || path_covers(other.path(), wanted.path()) {
        2
    } else {
        1
    }
}

pub struct ToolCtx<'a> {
    pub job: &'a Job,
    pub store: &'a mut dyn Store,
    pub tools: &'a Tools,
    pub policy: &'a Policy,
    pub tasks: &'a TaskVocabulary,
    pub now: u64,
    read_calls: u32,
    decided: Option<Decision>,
    pub log: Vec<Value>,
}

impl<'a> ToolCtx<'a> {
    pub fn new(
        job: &'a Job,
        store: &'a mut dyn Store,
        tools: &'a Tools,
        policy: &'a Policy,
        tasks: &'a TaskVocabulary,
        now: u64,
    ) -> Self {
        Self {
            job,
            store,
            tools,
            policy,
            tasks,
            now,
            read_calls: 0,
            decided: None,
            log: Vec::new(),
        }
    }

    pub fn decision(&self) -> Option<&Decision> {
        self.decided.as_ref()
    }

    pub fn read_calls(&self) -> u32 {
        self.read_calls
    }

    /// Runs one tool call and records it. Errors are values for the model, never panics.
    pub fn call(&mut self, tool: &str, args: &Value) -> Result<Value, ToolError> {
        let result = self.dispatch(tool, args);
        self.log.push(json!({
            "tool": tool,
            "args_digest": sha256_hex(args.to_string().as_bytes()),
            "error": result.as_ref().err().map(|error| error.code.as_str()),
        }));
        result
    }

    fn dispatch(&mut self, tool: &str, args: &Value) -> Result<Value, ToolError> {
        let kind = self
            .tools
            .specs
            .iter()
            .find(|spec| spec.name == tool)
            .map(|spec| spec.kind)
            .ok_or_else(|| schema(format!("unknown tool {tool:?}")))?;
        match kind {
            ToolKind::Read => {
                self.read_calls += 1;
                if self.read_calls > self.tools.max_read_calls {
                    return Err(ToolError::new(
                        ErrorCode::BudgetExceeded,
                        "the scope-batch read-call budget is used up; decide now with what you know, or escalate",
                    ));
                }
            }
            ToolKind::Decision => {
                if self.decided.is_some() {
                    return Err(ToolError::new(
                        ErrorCode::AlreadyDecided,
                        "this job already has a decision",
                    ));
                }
            }
        }
        match tool {
            "job_get" => {
                parse_args::<NoArgs>(args)?;
                self.job_get()
            }
            "memory_search" => self.memory_search(parse_args(args)?),
            "memory_get" => self.memory_get(parse_args(args)?),
            "memory_compare" => self.memory_compare(parse_args(args)?),
            "decide_skip" => self.decide_skip(parse_args(args)?),
            "decide_replace" => self.decide_replace(parse_args(args)?),
            "decide_create" => self.decide_create(parse_args(args)?),
            "decide_retire" => self.decide_retire(parse_args(args)?),
            "decide_escalate" => {
                let a: CodesArgs = parse_args(args)?;
                self.check_reason_codes(&a.reason_codes)?;
                self.finish(
                    "needs_human_review",
                    a.reason_codes,
                    None,
                    MemoryMutation::None,
                )
            }
            "decide_reject" => {
                let a: CodesArgs = parse_args(args)?;
                self.check_reason_codes(&a.reason_codes)?;
                self.finish("reject_invalid", a.reason_codes, None, MemoryMutation::None)
            }
            other => Err(schema(format!("unknown tool {other:?}"))),
        }
    }

    fn host(&self) -> &str {
        &self.job.scope.site_host
    }

    fn check_reason_codes(&self, codes: &[String]) -> Result<(), ToolError> {
        if codes.is_empty() || codes.len() > 5 {
            return Err(schema("reason_codes needs 1 to 5 items"));
        }
        match codes
            .iter()
            .find(|code| !self.tools.reason_codes.contains(*code))
        {
            Some(_) => Err(schema("reason_codes must come from the allowed list")),
            None => Ok(()),
        }
    }

    fn band(&self, band: &str) -> Result<u8, ToolError> {
        match band {
            "low" => Ok(0),
            "medium" => Ok(1),
            "high" => Ok(2),
            _ => Err(schema("confidence_band must be low, medium, or high")),
        }
    }

    /// A memory of this job's host. Unknown ids and other hosts' ids look the same (no existence oracle).
    fn visible(&self, id: &str) -> Result<MemoryRow, ToolError> {
        self.store
            .get(id)
            .map_err(storage_unavailable)?
            .filter(|row| row.scope.host == self.host() && row.status.recallable())
            .ok_or_else(|| {
                ToolError::new(
                    ErrorCode::ScopeViolation,
                    "no such memory in this job's site host",
                )
            })
    }

    fn validate_record(&self, record: RecordPayload) -> Result<Validated, ToolError> {
        validate(
            record,
            &ValidationCtx {
                host: self.host(),
                task: &self.job.scope.task_category,
                intent_key: &self.job.scope.intent_key,
                tasks: self.tasks,
                mode: self.policy.mode,
            },
        )
    }

    fn scope_of(&self, validated: &Validated) -> ScopeKey {
        ScopeKey {
            host: self.job.scope.site_host.clone(),
            pattern: pattern_key(&validated.patterns[0]),
            kind: kind_str(validated.kind).to_owned(),
            task: self.job.scope.task_category.clone(),
            intent: self.job.scope.intent_key.clone(),
        }
    }

    fn finish(
        &mut self,
        kind: &str,
        reason_codes: Vec<String>,
        confidence_band: Option<String>,
        mutation: MemoryMutation,
    ) -> Result<Value, ToolError> {
        let band = confidence_band
            .as_deref()
            .map(|value| self.band(value))
            .transpose()?;
        let committed = self
            .store
            .apply_decision(DecisionCommit {
                job_id: self.job.id.clone(),
                job_digest: self.job.digest(),
                kind: kind.to_owned(),
                reason_codes: reason_codes.clone(),
                confidence_band: band,
                mutation,
                lease_fence: self.job.lease_fence.clone(),
                at: self.now,
            })
            .map_err(|error| store_commit_error(error, kind))?;
        let decision = Decision {
            kind: kind.to_owned(),
            memory_id: committed.memory_id.clone(),
            version: committed.version,
            reason_codes,
            confidence_band,
        };
        self.decided = Some(decision);
        Ok(json!({
            "ok": true,
            "decision": kind,
            "memory_id": committed.memory_id,
            "version": committed.version
        }))
    }

    /// The active memory of this job's scope whose steps are exactly the candidate's, if there is one. The curator
    /// loop decides this case by rule, without a model call (docs/spec/queue.md, docs/spec/curator.md).
    pub fn exact_duplicate(&self) -> Result<Option<String>, ToolError> {
        let fingerprint = if self.job.batch_candidates.is_empty() {
            candidate_fingerprint(&self.job.candidate.action_sequence)
        } else {
            let mut fingerprints = self
                .job
                .batch_candidates
                .iter()
                .map(|candidate| candidate_fingerprint(&candidate.action_sequence));
            let first = fingerprints.next().expect("batch candidates are non-empty");
            if fingerprints.any(|candidate| candidate != first) {
                return Ok(None);
            }
            first
        };
        let memories = self
            .store
            .memories_in_host(self.host())
            .map_err(storage_unavailable)?;
        Ok(memories
            .into_iter()
            .find(|row| {
                row.scope.task == self.job.scope.task_category
                    && row.scope.intent == self.job.scope.intent_key
                    && row.active().is_some_and(|v| v.fingerprint == fingerprint)
            })
            .map(|row| row.id))
    }
}
