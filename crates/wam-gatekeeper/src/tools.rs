//! The curator's tool layer (docs/spec/curator.md): four read tools and six decision tools.
//!
//! Whatever the model sends, this layer enforces the invariants: scope containment, no duplicate active memory,
//! optimistic concurrency, an evidence policy for replacing and retiring, schema and text checks on everything the
//! model writes, a read budget, and exactly one decision per job. A rejected call changes nothing.

use crate::TOOLS_JSON;
use crate::error::{ErrorCode, ToolError};
use crate::record::{
    RecordPayload, Validated, ValidationCtx, build_pattern, candidate_fingerprint, kind_str,
    normalize_actions, parse_kind, path_covers, pattern_key, sha256_hex, step_actions, validate,
};
use crate::store::{Evidence, MemoryRow, ScopeKey, Status, Store, VersionRow};
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
        let max_read_calls = raw["limits"]["max_read_calls_per_job"]
            .as_u64()
            .ok_or("max_read_calls_per_job")? as u32;
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
    #[serde(default)]
    pub contribution_ids: Vec<String>,
    #[serde(default)]
    pub replay_passed: bool,
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
                        "the read-call budget is used up; decide now with what you know, or escalate",
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
                Ok(self.job_get())
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
                self.finish("needs_human_review", None, None, a.reason_codes, None)
            }
            "decide_reject" => {
                let a: CodesArgs = parse_args(args)?;
                self.check_reason_codes(&a.reason_codes)?;
                self.finish("reject_invalid", None, None, a.reason_codes, None)
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
            .filter(|row| row.scope.host == self.host() && row.status.recallable())
            .cloned()
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
        memory_id: Option<String>,
        version: Option<u32>,
        reason_codes: Vec<String>,
        confidence_band: Option<String>,
    ) -> Result<Value, ToolError> {
        let decision = Decision {
            kind: kind.to_owned(),
            memory_id: memory_id.clone(),
            version,
            reason_codes,
            confidence_band,
        };
        self.decided = Some(decision);
        Ok(json!({"ok": true, "decision": kind, "memory_id": memory_id, "version": version}))
    }

    /// The active memory of this job's scope whose steps are exactly the candidate's, if there is one. The curator
    /// loop decides this case by rule, without a model call (docs/spec/queue.md, docs/spec/curator.md).
    pub fn exact_duplicate(&self) -> Option<String> {
        let fingerprint = candidate_fingerprint(&self.job.candidate.action_sequence);
        self.store
            .memories_in_host(self.host())
            .into_iter()
            .find(|row| {
                row.scope.task == self.job.scope.task_category
                    && row.scope.intent == self.job.scope.intent_key
                    && row.active().is_some_and(|v| v.fingerprint == fingerprint)
            })
            .map(|row| row.id.clone())
    }

    fn job_get(&self) -> Value {
        let c = &self.job.candidate;
        let exact = self.exact_duplicate();
        json!({
            "job_id": self.job.id,
            "trigger": self.job.trigger,
            "scope": self.job.scope,
            "candidate": {
                "url_pattern": c.url_pattern,
                "action_sequence": c.action_sequence,
                "normalized_steps": normalize_actions(&c.action_sequence),
                "outcomes": c.outcomes,
                "success_rate": c.evidence().success_rate(),
                "observed_traits": c.observed_traits,
                "independent_contributors": c.independent_contributors,
                "ui_fingerprint": c.ui_fingerprint,
                "observed_at": c.observed_at,
            },
            "precheck": {"exact_fingerprint_match": exact},
            "replay_passed": self.job.replay_passed,
            "read_calls_left": self.tools.max_read_calls.saturating_sub(self.read_calls),
        })
    }

    fn memory_search(&self, a: SearchArgs) -> Result<Value, ToolError> {
        let limit = match a.limit {
            None => 5,
            Some(value) if (1..=10).contains(&value) => value as usize,
            Some(_) => return Err(schema("limit must be 1 to 10")),
        };
        let kind = match a.kind.as_deref() {
            None => None,
            Some(raw) => Some(
                parse_kind(raw)
                    .ok_or_else(|| schema("kind must be workflow, site_note, or gotcha"))?,
            ),
        };
        if a.task_category
            .as_deref()
            .is_some_and(|task| !self.tasks.contains(task))
        {
            return Err(schema("unknown task_category"));
        }
        let wanted_pattern = match a.url_pattern.as_deref() {
            None => None,
            Some(route) => Some(
                build_pattern(self.host(), route, self.policy.mode)
                    .map_err(|why| schema(format!("invalid url_pattern: {why}")))?,
            ),
        };
        let wanted_keywords: Vec<String> = match a.keywords {
            Some(list) if !list.is_empty() => {
                if list.len() > 8 {
                    return Err(schema("keywords allows at most 8 items"));
                }
                let vocabulary = KeywordVocabulary::new("query", &list)
                    .map_err(|error| schema(format!("invalid or duplicate keywords: {error:?}")))?;
                list.iter()
                    .filter_map(|keyword| vocabulary.resolve(keyword))
                    .map(|keyword| keyword.as_str().to_owned())
                    .collect()
            }
            _ => Vec::new(),
        };

        let mut hits: Vec<(u32, &MemoryRow)> = Vec::new();
        for row in self.store.memories_in_host(self.host()) {
            let Some(active) = row.active() else { continue };
            let l0 = &active.payload.l0;
            if kind.is_some_and(|wanted| kind_str(wanted) != l0.kind) {
                continue;
            }
            if a.task_category
                .as_deref()
                .is_some_and(|task| l0.task_category.as_deref() != Some(task))
            {
                continue;
            }
            let mut route = 1;
            if let Some(wanted) = &wanted_pattern {
                route = l0
                    .url_patterns
                    .iter()
                    .filter_map(|r| build_pattern(self.host(), r, self.policy.mode).ok())
                    .map(|pattern| relation(wanted, &pattern))
                    .max()
                    .unwrap_or(1);
            }
            let overlap = l0
                .keywords
                .iter()
                .filter(|k| wanted_keywords.contains(k))
                .count() as u32;
            hits.push((100 * route + 10 * overlap, row));
        }
        hits.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.id.cmp(&right.1.id))
        });
        hits.truncate(limit);
        let results: Vec<Value> = hits
            .into_iter()
            .filter_map(|(score, row)| {
                let version = row.active()?;
                Some(json!({
                    "id": row.id,
                    "version": version.version,
                    "status": row.status,
                    "support_count": row.support_count,
                    "score": score,
                    "l0": version.payload.l0,
                    "l1": {"title": version.payload.l1.title, "summary": version.payload.l1.summary},
                }))
            })
            .collect();
        Ok(json!({"count": results.len(), "results": results}))
    }

    fn memory_get(&self, a: GetArgs) -> Result<Value, ToolError> {
        let row = self.visible(&a.id)?;
        let version = row.active().ok_or_else(|| {
            ToolError::new(
                ErrorCode::ScopeViolation,
                "no such memory in this job's site host",
            )
        })?;
        match a.level {
            1 => Ok(
                json!({"id": row.id, "version": version.version, "l0": version.payload.l0, "l1": version.payload.l1}),
            ),
            2 => Ok(json!({
                "id": row.id,
                "version": version.version,
                "l2": version.payload.l2,
                "evidence": version.evidence,
                "verified_at": version.verified_at,
            })),
            _ => Err(schema("level must be 1 or 2")),
        }
    }

    fn memory_compare(&self, a: CompareArgs) -> Result<Value, ToolError> {
        let row = self.visible(&a.existing_id)?;
        let active = row.active().ok_or_else(|| {
            ToolError::new(
                ErrorCode::ScopeViolation,
                "no such memory in this job's site host",
            )
        })?;
        let c = &self.job.candidate;
        let candidate_steps = normalize_actions(&c.action_sequence);
        let existing_steps = normalize_actions(&step_actions(&active.payload.l2.steps));
        let known: BTreeSet<&str> = active
            .payload
            .l2
            .structure
            .iter()
            .flat_map(|s| s.ui_traits.iter())
            .chain(active.payload.l1.pitfalls.iter())
            .chain(
                active
                    .payload
                    .l2
                    .steps
                    .iter()
                    .filter_map(|s| s.when.as_ref()),
            )
            .map(String::as_str)
            .collect();
        let new_traits: Vec<&String> = c
            .observed_traits
            .iter()
            .filter(|t| !known.contains(t.as_str()))
            .collect();
        let scope_relation = match (
            build_pattern(self.host(), &c.url_pattern, self.policy.mode),
            build_pattern(
                self.host(),
                &active.payload.l0.url_patterns[0],
                self.policy.mode,
            ),
        ) {
            (Ok(mine), Ok(theirs)) => {
                if pattern_key(&mine) == pattern_key(&theirs) {
                    "exact"
                } else if path_covers(theirs.path(), mine.path()) {
                    "narrower"
                } else if path_covers(mine.path(), theirs.path()) {
                    "broader"
                } else {
                    "disjoint"
                }
            }
            _ => "disjoint",
        };
        Ok(json!({
            "existing_id": row.id,
            "expected_version": active.version,
            "fingerprint_equal": candidate_fingerprint(&c.action_sequence) == active.fingerprint,
            "step_count": {
                "candidate": candidate_steps.len(),
                "existing": existing_steps.len(),
                "delta": candidate_steps.len() as i64 - existing_steps.len() as i64,
            },
            "new_traits": new_traits,
            "success_rate": {"candidate": c.evidence().success_rate(), "existing": active.evidence.success_rate()},
            "independent_contributors": {
                "candidate": c.independent_contributors,
                "existing": active.evidence.independent_contributors,
            },
            "freshness_days_delta": (c.observed_at as i64 - active.verified_at as i64) / 86_400,
            "ui_fingerprint_changed": c.ui_fingerprint.is_some() && c.ui_fingerprint != active.evidence.ui_fingerprint,
            "scope_relation": scope_relation,
            "existing_support_count": row.support_count,
        }))
    }

    fn decide_skip(&mut self, a: SkipArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        if a.reason != "duplicate_support" && a.reason != "merge_evidence" {
            return Err(schema("reason must be duplicate_support or merge_evidence"));
        }
        let row = self.visible(&a.existing_id)?;
        self.store
            .add_support(&row.id, self.job.candidate.independent_contributors.max(1));
        self.finish(&a.reason, Some(row.id), None, a.reason_codes, None)
    }

    fn decide_create(&mut self, a: CreateArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        let band = self.band(&a.confidence_band)?;
        let validated = self.validate_record(a.record)?;
        let scope = self.scope_of(&validated);
        if let Some(existing) = self.store.find_by_scope(&scope) {
            return Err(ToolError::new(
                ErrorCode::DuplicateExists,
                format!(
                    "an active memory already covers this scope: {}; skip or replace instead",
                    existing.id
                ),
            ));
        }
        if self.job.candidate.independent_contributors < self.policy.min_independent_create {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "not enough independent contributors to create a memory yet; escalate or skip",
            ));
        }
        let first = VersionRow {
            version: 1,
            payload: validated.payload,
            fingerprint: validated.fingerprint,
            verified_at: self.now,
            status: Status::Active,
            evidence: self.job.candidate.evidence(),
            job_id: self.job.id.clone(),
        };
        let id = self.store.create(scope, first, band);
        self.finish(
            "create_new",
            Some(id),
            Some(1),
            a.reason_codes,
            Some(a.confidence_band),
        )
    }

    fn decide_replace(&mut self, a: ReplaceArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        let band = self.band(&a.confidence_band)?;
        let existing = self.visible(&a.existing_id)?;
        let active = existing
            .active()
            .ok_or_else(|| {
                ToolError::new(
                    ErrorCode::ScopeViolation,
                    "no such memory in this job's site host",
                )
            })?
            .clone();
        if active.version != a.expected_version {
            return Err(ToolError::new(
                ErrorCode::VersionConflict,
                format!(
                    "the active version is {}, not {}; read again and decide again",
                    active.version, a.expected_version
                ),
            ));
        }
        let validated = self.validate_record(a.record)?;
        if self.scope_of(&validated) != existing.scope {
            return Err(ToolError::new(
                ErrorCode::ScopeViolation,
                "a replacement must keep the memory's scope (host, primary route, kind, task, intent)",
            ));
        }
        let c = &self.job.candidate;
        if candidate_fingerprint(&c.action_sequence) == active.fingerprint
            || validated.fingerprint == active.fingerprint
        {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "the candidate is identical to the active version; use decide_skip",
            ));
        }
        if !(self.job.replay_passed
            || c.independent_contributors >= self.policy.replace_min_independent)
        {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "replacing needs a passed replay or enough independent contributors; skip or escalate",
            ));
        }
        if c.evidence().success_rate() < active.evidence.success_rate() {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "the candidate's success rate is lower than the active version's",
            ));
        }
        let next = VersionRow {
            version: 0,
            payload: validated.payload,
            fingerprint: validated.fingerprint,
            verified_at: self.now,
            status: Status::Active,
            evidence: c.evidence(),
            job_id: self.job.id.clone(),
        };
        let number = self.store.supersede(&existing.id, next, band);
        self.finish(
            "supersede",
            Some(existing.id),
            Some(number),
            a.reason_codes,
            Some(a.confidence_band),
        )
    }

    fn decide_retire(&mut self, a: RetireArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        let existing = self.visible(&a.existing_id)?;
        let version = existing.active().map_or(0, |v| v.version);
        if version != a.expected_version {
            return Err(ToolError::new(
                ErrorCode::VersionConflict,
                format!(
                    "the active version is {version}, not {}; read again and decide again",
                    a.expected_version
                ),
            ));
        }
        let c = &self.job.candidate;
        if !(self.job.trigger == Trigger::Reverify || c.outcomes.failed > c.outcomes.success) {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "retiring needs a reverify trigger or failures that outweigh successes; escalate instead",
            ));
        }
        self.store.retire(&existing.id);
        self.finish(
            "retire",
            Some(existing.id),
            Some(version),
            a.reason_codes,
            None,
        )
    }
}
