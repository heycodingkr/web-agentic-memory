//! The curator loop with a scripted model (docs/test CUR-02..05, CUR-09, CUR-10, CUR-14) and its handoff to wam-core.

use serde_json::json;
use wam_core::{CanonicalUrl, RecallQuery, RouteMatchLevel, UrlMode, recall};
use wam_gatekeeper::curator::{CuratorConfig, Outcome, run_job};
use wam_gatekeeper::provider::{
    LlmProvider, ModelTurn, ProviderError, ScriptStep, ScriptedProvider, TurnRequest,
};
use wam_gatekeeper::record::sha256_hex;
use wam_gatekeeper::scenario;
use wam_gatekeeper::store::{
    AuditRecord, CommitResult, DecisionCommit, MemStore, MemoryRow, ScopeKey, Store, StoreError,
};
use wam_gatekeeper::tools::{Job, Policy, Tools, Trigger};
use wam_gatekeeper::{INSTRUCTIONS, TOOLS_JSON};

#[test]
fn starter_evaluation_corpus_has_60_unique_labeled_scenarios_that_run() {
    let cases = scenario::evaluation();
    assert_eq!(cases.len(), 60);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        60,
        "case names must be unique for saved evaluation evidence"
    );
    assert_eq!(
        cases
            .iter()
            .map(|case| case.job.scope.site_host.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        12,
        "the starter corpus must cover all declared URL contexts"
    );

    for case in &cases {
        let mut provider = case.script();
        let run = scenario::run_scenario(case, &mut provider)
            .unwrap_or_else(|error| panic!("{} failed to run: {error}", case.name));
        assert!(
            run.verdict.is_ok(),
            "{} label mismatch: {:?}; report: {:?}",
            case.name,
            run.verdict,
            run.report.turns
        );
    }
}

struct FailingDecisionStore(MemStore);

impl Store for FailingDecisionStore {
    fn memories_in_host(&self, host: &str) -> Result<Vec<MemoryRow>, StoreError> {
        self.0.memories_in_host(host)
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRow>, StoreError> {
        self.0.get(id)
    }

    fn find_by_scope(&self, scope: &ScopeKey) -> Result<Option<MemoryRow>, StoreError> {
        self.0.find_by_scope(scope)
    }

    fn apply_decision(&mut self, _commit: DecisionCommit) -> Result<CommitResult, StoreError> {
        Err(StoreError::Unavailable)
    }

    fn log_audit(&mut self, audit: AuditRecord) -> Result<(), StoreError> {
        self.0.log_audit(audit)
    }

    fn decided(&self, job_id: &str, digest: &str) -> Result<bool, StoreError> {
        self.0.decided(job_id, digest)
    }
}

#[test]
fn cur_02_to_05_each_scenario_ends_in_the_expected_decision() {
    let expected = [
        ("duplicate", "duplicate_support"),
        ("better", "supersede"),
        ("new", "create_new"),
        ("conflict", "needs_human_review"),
        ("injection", "needs_human_review"),
    ];
    for (name, kind) in expected {
        let s = scenario::builtin(name).unwrap();
        let run = scenario::run_scenario(&s, &mut s.script()).unwrap();
        assert_eq!(run.verdict, Ok(()), "{name}");
        assert_eq!(run.report.decision.as_ref().unwrap().kind, kind, "{name}");
    }
}

#[test]
fn a_store_failure_during_decision_fails_closed_without_mutating_the_catalog() {
    let s = scenario::builtin("new").unwrap();
    let tasks = s.tasks().unwrap();
    let mut provider = s.script();
    let mut store = FailingDecisionStore(s.build_store().unwrap());
    let report = run_job(
        &s.job,
        &mut store,
        &mut provider,
        &Tools::load().unwrap(),
        &Policy::default(),
        &tasks,
        &CuratorConfig {
            exact_duplicates_without_model: false,
            ..CuratorConfig::default()
        },
        s.now,
    );

    assert!(matches!(report.outcome, Outcome::ProviderError(_)));
    assert_eq!(
        report.turns.last().unwrap().result["error"]["code"],
        "STORAGE_UNAVAILABLE"
    );
    assert_eq!(store.0.memories.len(), 1);
    assert!(store.0.decisions.is_empty());
    assert!(!store.decided(&s.job.id, &s.job.digest()).unwrap());
}

#[test]
fn a_replace_adds_a_version_and_supersedes_the_old_one() {
    let s = scenario::builtin("better").unwrap();
    let run = scenario::run_scenario(&s, &mut s.script()).unwrap();
    let row = run.store.get("web:medium-com-write-post").unwrap().unwrap();
    assert_eq!(row.versions.len(), 2);
    assert_eq!(
        row.versions[0].status,
        wam_gatekeeper::store::Status::Superseded
    );
    assert_eq!(row.active().unwrap().version, 2);
    assert!(
        row.active()
            .unwrap()
            .payload
            .l2
            .steps
            .iter()
            .any(|step| step.when.as_deref() == Some("resume-dialog"))
    );
}

#[test]
fn a_skip_only_adds_support_and_a_create_adds_a_memory() {
    let s = scenario::builtin("duplicate").unwrap();
    let run = scenario::run_scenario(&s, &mut s.script()).unwrap();
    let row = run.store.get("web:medium-com-write-post").unwrap().unwrap();
    assert_eq!((row.versions.len(), row.support_count), (1, 1));
    let s = scenario::builtin("new").unwrap();
    let run = scenario::run_scenario(&s, &mut s.script()).unwrap();
    assert_eq!(run.store.memories.len(), 2);
}

#[test]
fn memory_written_by_the_curator_is_what_wam_core_recall_serves() {
    let s = scenario::builtin("better").unwrap();
    let tasks = s.tasks().unwrap();
    let run = scenario::run_scenario(&s, &mut s.script()).unwrap();
    let indexes = run.store.indexes(&tasks, UrlMode::Public);
    assert_eq!(indexes.len(), 1);
    let url = CanonicalUrl::parse(
        "https://medium.com/editor/post/948203?token=x#body",
        UrlMode::Public,
    )
    .unwrap();
    let query = RecallQuery::new(
        url,
        Some(tasks.resolve("compose-draft").unwrap()),
        None,
        vec![],
    )
    .unwrap();
    let hits = recall(&indexes, &query);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id(), "web:medium-com-write-post");
    assert_eq!(hits[0].matched_on.route, RouteMatchLevel::Exact);
    assert!(hits[0].memory.version_id().ends_with(".v2"));
}

fn run_script(
    name: &str,
    steps: Vec<ScriptStep>,
    config: &CuratorConfig,
) -> (
    wam_gatekeeper::curator::JobReport,
    wam_gatekeeper::store::MemStore,
) {
    let s = scenario::builtin(name).unwrap();
    let tools = Tools::load().unwrap();
    let tasks = s.tasks().unwrap();
    let mut store = s.build_store().unwrap();
    let mut provider = ScriptedProvider::new(steps);
    let report = run_job(
        &s.job,
        &mut store,
        &mut provider,
        &tools,
        &Policy::default(),
        &tasks,
        config,
        s.now,
    );
    (report, store)
}

fn call(tool: &str, args: serde_json::Value) -> ScriptStep {
    ScriptStep::Call(tool.to_owned(), args)
}

/// The loop with the rule pass for exact duplicates off: these tests are about what happens with the model.
fn model_only() -> CuratorConfig {
    CuratorConfig {
        exact_duplicates_without_model: false,
        ..CuratorConfig::default()
    }
}

#[test]
fn malformed_replies_are_told_what_went_wrong_and_may_recover() {
    let (report, _) = run_script(
        "conflict",
        vec![
            ScriptStep::Raw("I think we should escalate.".into()),
            call(
                "decide_escalate",
                json!({"reason_codes": ["conflicting_evidence"]}),
            ),
        ],
        &model_only(),
    );
    assert_eq!(report.outcome, Outcome::Decided);
    assert_eq!(report.turns[0].tool, "(malformed)");
    assert_eq!(report.turns[0].result["error"]["code"], "SCHEMA_INVALID");
}

#[test]
fn too_many_malformed_replies_fail_closed_with_the_store_untouched() {
    let s = scenario::builtin("better").unwrap();
    let before = s.build_store().unwrap();
    let (report, store) = run_script(
        "better",
        vec![
            ScriptStep::Raw("a".into()),
            ScriptStep::Raw("b".into()),
            ScriptStep::Raw("c".into()),
        ],
        &model_only(),
    );
    assert!(matches!(report.outcome, Outcome::NoDecision(_)));
    assert_eq!(store.memories, before.memories);
}

#[test]
fn a_model_that_never_decides_leaves_the_job_for_a_retry() {
    let steps = (0..30).map(|_| call("job_get", json!({}))).collect();
    let (report, store) = run_script(
        "better",
        steps,
        &CuratorConfig {
            max_turns: 6,
            ..model_only()
        },
    );
    assert!(matches!(report.outcome, Outcome::NoDecision(_)));
    assert_eq!(report.turns.len(), 6);
    assert_eq!(
        store.memories["web:medium-com-write-post"].versions.len(),
        1
    );
    // The ninth read was refused but a decision would still have been possible; here it never came.
    assert!(
        report
            .turns
            .iter()
            .any(|t| t.result["error"]["code"] == "BUDGET_EXCEEDED")
            || report.turns.len() < 9
    );
}

#[test]
fn an_unreachable_model_is_a_provider_error_not_a_decision() {
    // The script runs dry after one read: the provider reports Exhausted.
    let (report, store) = run_script("better", vec![call("job_get", json!({}))], &model_only());
    assert!(matches!(report.outcome, Outcome::ProviderError(_)));
    assert_eq!(report.decision, None);
    assert_eq!(
        store.memories["web:medium-com-write-post"].versions.len(),
        1
    );
    let _ = ProviderError::Exhausted;
}

#[test]
fn cur_14_every_decision_audit_names_the_instructions_and_tools_it_ran_under() {
    let s = scenario::builtin("conflict").unwrap();
    let run = scenario::run_scenario(&s, &mut s.script()).unwrap();
    let audit = run.store.audits.last().unwrap();
    assert_eq!(audit.instructions_hash, sha256_hex(INSTRUCTIONS.as_bytes()));
    assert_eq!(audit.tools_hash, sha256_hex(TOOLS_JSON.as_bytes()));
    assert_eq!(audit.provider, "fake:scripted");
    assert_eq!(audit.outcome, "decided");
    assert_eq!(audit.job_digest, s.job.digest());
    assert_eq!(audit.calls.len(), run.report.turns.len());
    let raw = serde_json::to_string(&audit).unwrap();
    assert!(
        !raw.contains("someone@"),
        "the audit carries digests and codes, not content"
    );
    let mut other = s.job.clone();
    other.candidate.independent_contributors += 1;
    assert_ne!(other.digest(), s.job.digest());
}

/// A model that must not be needed. It counts how often it was asked and always fails.
#[derive(Default)]
struct Unwanted {
    asked: u32,
}

impl LlmProvider for Unwanted {
    fn name(&self) -> String {
        "fake:unwanted".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.asked += 1;
        Err(ProviderError::Failed("the model was called".into()))
    }
}

/// The seeded memory's own steps (wait and snapshot are noise that normalization drops): an exact duplicate.
fn exact_duplicate_job() -> Job {
    let mut job = scenario::builtin("duplicate").unwrap().job;
    job.candidate.action_sequence = [
        "navigate", "click", "type", "click", "type", "key", "type", "wait", "snapshot",
    ]
    .map(String::from)
    .to_vec();
    job
}

fn judge(
    job: &Job,
    config: &CuratorConfig,
) -> (
    wam_gatekeeper::curator::JobReport,
    wam_gatekeeper::store::MemStore,
    u32,
) {
    let s = scenario::builtin("duplicate").unwrap();
    let tools = Tools::load().unwrap();
    let tasks = s.tasks().unwrap();
    let mut store = s.build_store().unwrap();
    let mut provider = Unwanted::default();
    let report = run_job(
        job,
        &mut store,
        &mut provider,
        &tools,
        &Policy::default(),
        &tasks,
        config,
        s.now,
    );
    (report, store, provider.asked)
}

#[test]
fn an_exact_duplicate_is_decided_by_rule_without_a_model_call() {
    let before = scenario::builtin("duplicate")
        .unwrap()
        .build_store()
        .unwrap()
        .memories["web:medium-com-write-post"]
        .support_count;
    let (report, store, asked) = judge(&exact_duplicate_job(), &CuratorConfig::default());
    assert_eq!(asked, 0, "no model call");
    assert_eq!(report.outcome, Outcome::Decided);
    let decision = report.decision.as_ref().unwrap();
    assert_eq!(
        (decision.kind.as_str(), decision.memory_id.as_deref()),
        ("duplicate_support", Some("web:medium-com-write-post"))
    );
    assert_eq!(
        store.memories["web:medium-com-write-post"].support_count,
        before + 1,
        "its evidence still counts"
    );
    assert_eq!(
        store.memories["web:medium-com-write-post"].versions.len(),
        1,
        "no new version"
    );
    let audit = store.audits.last().unwrap();
    assert_eq!(
        (audit.provider.as_str(), audit.outcome.as_str()),
        ("rule:exact-fingerprint", "decided")
    );
    assert_eq!(
        report.turns.len(),
        1,
        "one tool call, made by the rule through the same tool layer"
    );
}

#[test]
fn a_near_duplicate_still_needs_the_model() {
    let job = scenario::builtin("duplicate").unwrap().job;
    let (report, store, asked) = judge(&job, &CuratorConfig::default());
    assert_eq!(asked, 1);
    assert!(matches!(report.outcome, Outcome::ProviderError(_)));
    assert_eq!(
        store.memories["web:medium-com-write-post"].versions.len(),
        1
    );
}

#[test]
fn the_rule_pass_can_be_switched_off_to_judge_the_model_itself() {
    let (_, _, asked) = judge(&exact_duplicate_job(), &model_only());
    assert_eq!(asked, 1);
}

#[test]
fn the_rule_pass_is_for_new_contributions_only_and_stays_inside_the_scope() {
    let mut reverify = exact_duplicate_job();
    reverify.trigger = Trigger::Reverify;
    assert_eq!(
        judge(&reverify, &CuratorConfig::default()).2,
        1,
        "reverify is the model's call"
    );
    let mut other_intent = exact_duplicate_job();
    other_intent.scope.intent_key = "write-newsletter".into();
    assert_eq!(
        judge(&other_intent, &CuratorConfig::default()).2,
        1,
        "a different intent is not a duplicate"
    );
}

/// Reports that the account's quota is spent on the first call.
struct OutOfQuota;

impl LlmProvider for OutOfQuota {
    fn name(&self) -> String {
        "fake:out-of-quota".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        Err(ProviderError::QuotaExhausted {
            why: "usage limit reached".into(),
            retry_after_secs: Some(3600),
        })
    }
}

#[test]
fn spent_quota_is_its_own_outcome_not_a_failed_attempt_and_changes_nothing() {
    let s = scenario::builtin("better").unwrap();
    let before = s.build_store().unwrap();
    let tools = Tools::load().unwrap();
    let tasks = s.tasks().unwrap();
    let mut store = s.build_store().unwrap();
    let report = run_job(
        &s.job,
        &mut store,
        &mut OutOfQuota,
        &tools,
        &Policy::default(),
        &tasks,
        &model_only(),
        s.now,
    );
    assert_eq!(
        report.outcome,
        Outcome::QuotaExhausted {
            why: "usage limit reached".into(),
            retry_after_secs: Some(3600)
        }
    );
    assert_eq!(report.decision, None);
    assert_eq!(store.memories, before.memories);
    assert_eq!(
        store.audits.last().unwrap().outcome,
        "quota_exhausted: usage limit reached"
    );
    assert!(
        !store.decided(&s.job.id, &s.job.digest()).unwrap(),
        "a parked job is not a decided one"
    );
}

/// Answers the first two turns with a read tool, then the provider goes down.
struct DownAfterTwoTurns {
    turns: usize,
}

impl LlmProvider for DownAfterTwoTurns {
    fn name(&self) -> String {
        "fake:down-after-two-turns".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.turns += 1;
        if self.turns > 2 {
            return Err(ProviderError::Unavailable {
                why: "HTTP 503 from model provider".into(),
                retry_after_secs: Some(60),
            });
        }
        Ok(ModelTurn {
            tool: "job_get".into(),
            args: json!({}),
            raw: json!({"tool": "job_get", "args": {}}).to_string(),
            meta: json!({"cost_usd": 0.01}),
        })
    }
}

/// Fails every call the way a provider does when it is not at fault for a reply.
enum Raises {
    Down,
    Failed,
    Timeout,
}

impl LlmProvider for Raises {
    fn name(&self) -> String {
        "fake:raises".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        Err(match self {
            Self::Down => ProviderError::Unavailable {
                why: "HTTP 503 from model provider".into(),
                retry_after_secs: Some(60),
            },
            Self::Failed => ProviderError::Failed("HTTP 400 from model provider".into()),
            Self::Timeout => ProviderError::Timeout(5),
        })
    }
}

#[test]
fn a_provider_that_is_down_ends_the_job_without_a_decision() {
    let s = scenario::builtin("better").unwrap();
    let tools = Tools::load().unwrap();
    let tasks = s.tasks().unwrap();
    let before = s.build_store().unwrap();

    // Down on the first turn.
    let mut store = s.build_store().unwrap();
    let report = run_job(
        &s.job,
        &mut store,
        &mut Raises::Down,
        &tools,
        &Policy::default(),
        &tasks,
        &model_only(),
        s.now,
    );
    assert_eq!(
        report.outcome,
        Outcome::Unavailable {
            why: "HTTP 503 from model provider".into(),
            retry_after_secs: Some(60)
        }
    );
    assert_eq!(report.decision, None);
    assert_eq!(store.memories, before.memories);
    assert_eq!(
        store.audits.last().unwrap().outcome,
        "provider_unavailable: HTTP 503 from model provider"
    );
    assert!(
        !store.decided(&s.job.id, &s.job.digest()).unwrap(),
        "a job that met an outage is not a decided one"
    );

    // Down after two good turns: what the model already answered is kept in the report, and so is its cost.
    let mut store = s.build_store().unwrap();
    let report = run_job(
        &s.job,
        &mut store,
        &mut DownAfterTwoTurns { turns: 0 },
        &tools,
        &Policy::default(),
        &tasks,
        &model_only(),
        s.now,
    );
    assert!(matches!(report.outcome, Outcome::Unavailable { .. }));
    assert_eq!(report.turns.len(), 2);
    assert!(
        (report.cost_usd() - 0.02).abs() < 1e-9,
        "{}",
        report.cost_usd()
    );
    assert_eq!(report.decision, None);
    assert_eq!(store.memories, before.memories);

    // A reply the provider could not be blamed for is still an ordinary failure.
    for mut ordinary in [Raises::Failed, Raises::Timeout] {
        let mut store = s.build_store().unwrap();
        let report = run_job(
            &s.job,
            &mut store,
            &mut ordinary,
            &tools,
            &Policy::default(),
            &tasks,
            &model_only(),
            s.now,
        );
        assert!(
            matches!(report.outcome, Outcome::ProviderError(_)),
            "{:?}",
            report.outcome
        );
    }
}

/// Answers `good_turns` turns with a read tool, then the provider rejects the key.
struct RejectsAfter {
    good_turns: usize,
    turns: usize,
}

impl LlmProvider for RejectsAfter {
    fn name(&self) -> String {
        "fake:rejects-after".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.turns += 1;
        if self.turns > self.good_turns {
            return Err(ProviderError::Rejected {
                why: "HTTP 401 from model provider".into(),
            });
        }
        Ok(ModelTurn {
            tool: "job_get".into(),
            args: json!({}),
            raw: json!({"tool": "job_get", "args": {}}).to_string(),
            meta: json!({"cost_usd": 0.01}),
        })
    }
}

#[test]
fn a_provider_that_rejects_the_key_ends_the_job_without_a_decision() {
    // MODEL-19 a and b (docs/spec/provider-outage.md, R11).
    let s = scenario::builtin("better").unwrap();
    let tools = Tools::load().unwrap();
    let tasks = s.tasks().unwrap();
    let before = s.build_store().unwrap();

    for good_turns in [0, 2] {
        let mut store = s.build_store().unwrap();
        let report = run_job(
            &s.job,
            &mut store,
            &mut RejectsAfter {
                good_turns,
                turns: 0,
            },
            &tools,
            &Policy::default(),
            &tasks,
            &model_only(),
            s.now,
        );
        assert_eq!(
            report.outcome,
            Outcome::ProviderRejected {
                why: "HTTP 401 from model provider".into()
            },
            "{good_turns} good turns"
        );
        assert_eq!(report.decision, None);
        assert_eq!(store.memories, before.memories);
        assert_eq!(
            store.audits.last().unwrap().outcome,
            "provider_rejected: HTTP 401 from model provider"
        );
        assert!(
            !store.decided(&s.job.id, &s.job.digest()).unwrap(),
            "a job that met a rejected key is not a decided one"
        );
        // What the model answered before the key was refused stays in the report, with its cost.
        assert_eq!(report.turns.len(), good_turns);
        assert!((report.cost_usd() - 0.01 * good_turns as f64).abs() < 1e-9);
    }
}
