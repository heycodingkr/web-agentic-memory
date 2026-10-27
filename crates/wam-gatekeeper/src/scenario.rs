//! Curation scenarios: a seeded store, one job, a scripted correct answer, and what a good decision looks like.
//!
//! The JSON files in `scenarios/` are the seed of the labeled evaluation set (docs/test CUR-15). The same scenario
//! runs against the scripted fake (deterministic, in tests) and against any real provider (a CLI today, an HTTP API
//! later) to see whether the model decides as expected.

use crate::curator::{CuratorConfig, JobReport, Outcome, run_job};
use crate::provider::{LlmProvider, ScriptedProvider};
use crate::record::{RecordPayload, ValidationCtx, kind_str, pattern_key, validate};
use crate::store::{Evidence, MemStore, ScopeKey, Status, VersionRow};
use crate::tools::{Job, Policy, Tools};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wam_core::{TaskVocabulary, UrlMode};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeedMemory {
    pub payload: RecordPayload,
    pub evidence: Evidence,
    pub verified_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptCall {
    pub tool: String,
    pub args: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub forbidden: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    pub tasks: Vec<String>,
    pub now: u64,
    pub seed: Vec<SeedMemory>,
    pub job: Job,
    pub script: Vec<ScriptCall>,
    pub expect: Expect,
}

pub const BUILTIN: [(&str, &str); 5] = [
    ("duplicate", include_str!("../scenarios/duplicate.json")),
    ("better", include_str!("../scenarios/better.json")),
    ("new", include_str!("../scenarios/new.json")),
    ("conflict", include_str!("../scenarios/conflict.json")),
    ("injection", include_str!("../scenarios/injection.json")),
];

const EVALUATION_CONTEXTS: &str = include_str!("../scenarios/eval-contexts.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationContext {
    id: String,
    host: String,
    display: String,
    route: String,
    task: String,
    intent: String,
    memory_id: String,
    topic: String,
    workflow: String,
    keyword: String,
    localized: String,
}

pub fn all() -> Vec<Scenario> {
    BUILTIN
        .iter()
        .map(|(name, text)| {
            serde_json::from_str(text).unwrap_or_else(|error| panic!("scenario {name}: {error}"))
        })
        .collect()
}

/// Builds the starter 60-case evaluation corpus from five labeled decision archetypes and twelve explicit
/// URL/task contexts. These context variants are regression coverage, not independent human annotations.
pub fn evaluation() -> Vec<Scenario> {
    let contexts: Vec<EvaluationContext> = serde_json::from_str(EVALUATION_CONTEXTS)
        .expect("the checked-in curator evaluation contexts must be valid JSON");
    let seeds = all();
    let mut cases = Vec::with_capacity(seeds.len() * contexts.len());

    for context in &contexts {
        for seed in &seeds {
            let mut value = serde_json::to_value(seed)
                .expect("curator scenario types must remain serializable");
            replace_strings(
                &mut value,
                &[
                    ("web:medium-com-write-post", context.memory_id.as_str()),
                    ("medium.com", context.host.as_str()),
                    ("/editor/post/:id", context.route.as_str()),
                    ("compose-draft", context.task.as_str()),
                    ("write-post", context.intent.as_str()),
                    ("Medium", context.display.as_str()),
                    ("writing", context.workflow.as_str()),
                    ("초안", context.localized.as_str()),
                ],
            );
            replace_keyword_values(&mut value, context.keyword.as_str());
            let mut case: Scenario = serde_json::from_value(value)
                .expect("evaluation context substitutions must preserve scenario schema");
            case.name = format!("{}-{}", seed.name, context.id);
            case.description = format!(
                "{} ({}; {})",
                seed.description, context.display, context.topic
            );
            case.job.id = format!("eval-{}-{}", seed.name, context.id);
            for contribution_id in &mut case.job.contribution_ids {
                *contribution_id = format!("{}-{}", case.job.id, contribution_id);
            }
            cases.push(case);
        }
    }
    cases
}

fn replace_strings(value: &mut Value, replacements: &[(&str, &str)]) {
    match value {
        Value::String(text) => {
            for (from, to) in replacements {
                *text = text.replace(from, to);
            }
        }
        Value::Array(items) => {
            for item in items {
                replace_strings(item, replacements);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                replace_strings(item, replacements);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn replace_keyword_values(value: &mut Value, keyword: &str) {
    match value {
        Value::Object(fields) => {
            for (name, item) in fields {
                if name == "keywords" {
                    if let Value::Array(values) = item {
                        for value in values {
                            if value == "medium" {
                                *value = Value::String(keyword.to_owned());
                            }
                        }
                    }
                } else {
                    replace_keyword_values(item, keyword);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                replace_keyword_values(item, keyword);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

pub fn builtin(name: &str) -> Option<Scenario> {
    all().into_iter().find(|scenario| scenario.name == name)
}

impl Scenario {
    pub fn tasks(&self) -> Result<TaskVocabulary, String> {
        TaskVocabulary::new("scenario", &self.tasks)
            .map_err(|error| format!("task vocabulary: {error:?}"))
    }

    pub fn build_store(&self) -> Result<MemStore, String> {
        let tasks = self.tasks()?;
        let mut store = MemStore::new();
        for seed in &self.seed {
            let l0 = &seed.payload.l0;
            let (task, intent) = match (&l0.task_category, &l0.intent_key) {
                (Some(task), Some(intent)) => (task.clone(), intent.clone()),
                _ => return Err("seed memories need task_category and intent_key".into()),
            };
            let validated = validate(
                seed.payload.clone(),
                &ValidationCtx {
                    host: &l0.site_host,
                    task: &task,
                    intent_key: &intent,
                    tasks: &tasks,
                    mode: UrlMode::Public,
                },
            )
            .map_err(|error| format!("seed {task}/{intent}: {error}"))?;
            let scope = ScopeKey {
                host: l0.site_host.clone(),
                pattern: pattern_key(&validated.patterns[0]),
                kind: kind_str(validated.kind).to_owned(),
                task,
                intent,
            };
            store
                .insert_seed(
                    scope,
                    VersionRow {
                        version: 1,
                        payload: validated.payload,
                        fingerprint: validated.fingerprint,
                        verified_at: seed.verified_at,
                        status: Status::Active,
                        evidence: seed.evidence.clone(),
                        job_id: "seed".into(),
                    },
                    1,
                )
                .map_err(|error| format!("could not seed store: {error:?}"))?;
        }
        Ok(store)
    }

    pub fn script(&self) -> ScriptedProvider {
        ScriptedProvider::calls(
            self.script
                .iter()
                .map(|call| (call.tool.clone(), call.args.clone())),
        )
    }

    /// Does the report match what a good curator does here?
    pub fn check(&self, report: &JobReport) -> Result<(), String> {
        match &report.outcome {
            Outcome::Decided => {}
            other => return Err(format!("no decision: {other:?}")),
        }
        let kind = report
            .decision
            .as_ref()
            .map(|d| d.kind.as_str())
            .unwrap_or_default();
        if self.expect.forbidden.iter().any(|f| f == kind) {
            return Err(format!("decided {kind}, which this scenario forbids"));
        }
        if !self.expect.decisions.is_empty() && !self.expect.decisions.iter().any(|d| d == kind) {
            return Err(format!(
                "decided {kind}, expected one of {:?}",
                self.expect.decisions
            ));
        }
        Ok(())
    }
}

pub struct ScenarioRun {
    pub store: MemStore,
    pub report: JobReport,
    pub verdict: Result<(), String>,
}

/// Runs a scenario the way it is used to judge a model: the rule pass for exact duplicates is off, so the model
/// itself has to recognize the duplicate.
pub fn run_scenario(
    scenario: &Scenario,
    provider: &mut dyn LlmProvider,
) -> Result<ScenarioRun, String> {
    let tools = Tools::load()?;
    let tasks = scenario.tasks()?;
    let mut store = scenario.build_store()?;
    let report = run_job(
        &scenario.job,
        &mut store,
        provider,
        &tools,
        &Policy::default(),
        &tasks,
        &CuratorConfig {
            exact_duplicates_without_model: false,
            ..CuratorConfig::default()
        },
        scenario.now,
    );
    let verdict = scenario.check(&report);
    Ok(ScenarioRun {
        store,
        report,
        verdict,
    })
}
