//! The worker: queue, budget, curator and store working together (docs/test QUE-03, 06..10, FAIL-12/15/16/18).
//! The model is the scripted fake; swapping it for a CLI or an API changes nothing here.

use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeMap;
use wam_gatekeeper::budget::{BudgetConfig, ModelBudget};
use wam_gatekeeper::curator::CuratorConfig;
use wam_gatekeeper::provider::{
    LlmProvider, ModelTurn, ProviderError, ScriptStep, ScriptedProvider, TurnRequest,
};
use wam_gatekeeper::queue::{
    AccountClass, Batch, JobKind, JobQueue, JobState, NewJob, Priority, QueueBackend, QueueConfig,
    QueueError, QueueLeaseFence,
};
use wam_gatekeeper::scenario::{Scenario, builtin};
use wam_gatekeeper::store::{MemStore, Store};
use wam_gatekeeper::tools::{Job, Policy, Tools};
use wam_gatekeeper::worker::{
    BatchFormatError, CuratorBatch, Env, JobSource, MAX_CURATOR_BATCH_BYTES, Step, Worker,
};

const SCOPE: &str = "medium.com|/editor/post/:id|workflow|compose-draft|write-post";
const HOST: &str = "medium.com";
const MEMORY: &str = "web:medium-com-write-post";

struct Source {
    jobs: BTreeMap<String, Job>,
    asked: RefCell<Vec<Vec<String>>>,
}

impl Source {
    fn new(jobs: impl IntoIterator<Item = (&'static str, Job)>) -> Self {
        Self {
            jobs: jobs
                .into_iter()
                .map(|(id, job)| (id.to_owned(), job))
                .collect(),
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl JobSource for Source {
    fn curator_jobs(&self, _scope_key: &str, contribution_ids: &[String]) -> Vec<Job> {
        self.asked.borrow_mut().push(contribution_ids.to_vec());
        contribution_ids
            .iter()
            .filter_map(|id| {
                self.jobs.get(id).cloned().map(|mut job| {
                    job.id = format!("job-{id}");
                    job.contribution_ids = vec![id.clone()];
                    job
                })
            })
            .collect()
    }
}

/// The `duplicate` scenario: a candidate that adds nothing new to the memory that exists. Two jobs of the same
/// scope that differ only in id and observation time, like two contributors who did the same thing.
struct Rig {
    scenario: Scenario,
    tools: Tools,
    queue: JobQueue,
    store: MemStore,
}

impl Rig {
    fn new(window: u64) -> Self {
        let scenario = builtin("duplicate").unwrap();
        let store = scenario.build_store().unwrap();
        let queue = JobQueue::new(QueueConfig {
            coalesce_window_secs: window,
            ..QueueConfig::default()
        });
        Self {
            scenario,
            tools: Tools::load().unwrap(),
            queue,
            store,
        }
    }

    fn job_a(&self) -> Job {
        self.scenario.job.clone()
    }

    fn job_b(&self) -> Job {
        let mut job = self.scenario.job.clone();
        job.id = "job-dup-2".into();
        job.candidate.observed_at += 1;
        job
    }

    fn submit(&mut self, contributor: &str, contribution: &str, scope: &str, now: u64) -> u64 {
        self.queue
            .enqueue(
                NewJob {
                    kind: JobKind::Contribution,
                    scope_key: scope.to_owned(),
                    contributor: contributor.to_owned(),
                    device: format!("device-{contributor}"),
                    class: AccountClass::Established,
                    nonce: contribution.to_owned(),
                    contribution_id: contribution.to_owned(),
                    priority: Priority::P1,
                },
                now,
            )
            .unwrap()
    }

    fn script(&self) -> Vec<ScriptStep> {
        self.scenario
            .script
            .iter()
            .map(|c| ScriptStep::Call(c.tool.clone(), c.args.clone()))
            .collect()
    }

    fn support(&self) -> u32 {
        self.store.get(MEMORY).unwrap().unwrap().support_count
    }
}

fn tasks() -> &'static wam_core::TaskVocabulary {
    static TASKS: std::sync::OnceLock<wam_core::TaskVocabulary> = std::sync::OnceLock::new();
    TASKS.get_or_init(|| builtin("duplicate").unwrap().tasks().unwrap())
}

fn worker<'a>(tools: &'a Tools, name: &str) -> Worker<'a> {
    Worker {
        name: name.to_owned(),
        tools,
        tasks: tasks(),
        policy: Policy::default(),
        curator: CuratorConfig::default(),
        estimated_tokens: 3000,
        quota_pause_secs: 900,
    }
}

fn big_budget() -> ModelBudget {
    ModelBudget::new(
        BudgetConfig {
            requests_per_minute: 600,
            tokens_per_minute: 10_000_000,
            daily_credits: None,
        },
        0,
    )
}

fn run(
    rig: &mut Rig,
    budget: &mut ModelBudget,
    provider: &mut dyn LlmProvider,
    source: &Source,
    now: u64,
) -> Step {
    run_with_backend(
        &rig.tools,
        &mut rig.queue,
        budget,
        &mut rig.store,
        provider,
        source,
        now,
    )
}

fn run_with_backend(
    tools: &Tools,
    queue: &mut dyn QueueBackend,
    budget: &mut ModelBudget,
    store: &mut dyn Store,
    provider: &mut dyn LlmProvider,
    source: &dyn JobSource,
    now: u64,
) -> Step {
    let worker = worker(tools, "worker-1");
    let mut env = Env {
        queue,
        budget,
        store,
        provider,
        source,
    };
    worker.run_once(&mut env, now)
}

struct FailCompleteOnce<'a> {
    queue: &'a mut JobQueue,
    fail: bool,
}

impl QueueBackend for FailCompleteOnce<'_> {
    fn backend_config(&self) -> &QueueConfig {
        self.queue.backend_config()
    }

    fn backend_job(&self, id: u64) -> Result<Option<wam_gatekeeper::queue::Job>, QueueError> {
        self.queue.backend_job(id)
    }

    fn backend_release_budget(&mut self, now: u64) -> Result<usize, QueueError> {
        self.queue.backend_release_budget(now)
    }

    fn backend_lease(&mut self, worker: &str, now: u64) -> Result<Option<Batch>, QueueError> {
        self.queue.backend_lease(worker, now)
    }

    fn backend_holds(&self, batch: &Batch) -> Result<bool, QueueError> {
        self.queue.backend_holds(batch)
    }

    fn backend_heartbeat(&mut self, batch: &mut Batch, now: u64) -> Result<u64, QueueError> {
        self.queue.backend_heartbeat(batch, now)
    }

    fn backend_complete(&mut self, batch: &Batch, now: u64) -> Result<(), QueueError> {
        if self.fail {
            self.fail = false;
            Err(QueueError::StorageUnavailable)
        } else {
            self.queue.backend_complete(batch, now)
        }
    }

    fn backend_fail(&mut self, batch: &Batch, now: u64, error: &str) -> Result<(), QueueError> {
        self.queue.backend_fail(batch, now, error)
    }

    fn backend_wait_budget(&mut self, batch: &Batch) -> Result<(), QueueError> {
        self.queue.backend_wait_budget(batch)
    }
}

/// Passes through to the scripted provider, but reports a cost for every turn, like a paid API.
struct Costly {
    inner: ScriptedProvider,
    credits: f64,
}

impl LlmProvider for Costly {
    fn name(&self) -> String {
        "fake:costly".into()
    }

    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        let mut turn = self.inner.next_turn(request)?;
        turn.meta = json!({"cost_usd": self.credits});
        Ok(turn)
    }
}

#[test]
fn a_leased_batch_becomes_decisions_and_the_jobs_are_done() {
    let mut rig = Rig::new(0);
    let before = rig.support();
    let ids = [
        rig.submit("alice", "c1", SCOPE, 0),
        rig.submit("bob", "c2", SCOPE, 0),
        rig.submit("alice", "c3", SCOPE, 0),
    ];
    let source = Source::new([
        ("c1", rig.job_a()),
        ("c2", rig.job_b()),
        ("c3", rig.job_a()),
    ]);
    let mut provider = ScriptedProvider::new(rig.script());
    let step = run(&mut rig, &mut big_budget(), &mut provider, &source, 5);
    let Step::Done {
        reports, replayed, ..
    } = step
    else {
        panic!("expected Done, got {step:?}")
    };
    assert_eq!((reports.len(), replayed), (1, 0));
    assert!(
        reports[0]
            .decision
            .as_ref()
            .is_some_and(|d| matches!(d.kind.as_str(), "duplicate_support" | "merge_evidence"))
    );
    assert_eq!(
        reports[0].turns.len(),
        rig.script().len(),
        "one curator interaction judged the batch"
    );
    let batch_input = &reports[0].turns[0].result["batch_candidates"];
    assert_eq!(
        batch_input.as_array().unwrap().len(),
        2,
        "repeat submissions by Alice count once"
    );
    assert_eq!(
        rig.support(),
        before + 2,
        "distinct contributors add evidence once"
    );
    assert!(
        ids.iter()
            .all(|id| rig.queue.job(*id).unwrap().state == JobState::Done)
    );
    assert_eq!(
        source.asked.borrow().len(),
        1,
        "both contributions were handed over together"
    );
    assert!(matches!(
        run(
            &mut rig,
            &mut big_budget(),
            &mut ScriptedProvider::new([]),
            &source,
            6
        ),
        Step::Idle
    ));
}

#[test]
fn curator_batch_format_is_versioned_bounded_and_digest_stable() {
    let rig = Rig::new(0);
    let jobs = vec![rig.job_a(), rig.job_b()];
    let batch = CuratorBatch::new(SCOPE, jobs.clone(), 20).unwrap();
    assert_eq!(batch.digest().len(), 64);
    let encoded = serde_json::to_value(&batch).unwrap();
    assert_eq!(encoded["format_version"], 1);
    assert_eq!(encoded["scope_key"], SCOPE);
    assert_eq!(encoded["jobs"].as_array().unwrap().len(), 2);

    let mut leased_jobs = jobs.clone();
    leased_jobs[0].lease_fence = Some(QueueLeaseFence {
        worker: "worker-1".into(),
        scope_key: SCOPE.into(),
        job_ids: vec![7, 8],
        generations: vec![2, 3],
        contribution_ids: vec![],
        require_attribution: false,
    });
    let leased = CuratorBatch::new(SCOPE, leased_jobs, 20).unwrap();
    assert_eq!(leased.digest(), batch.digest());
    assert!(
        serde_json::to_value(&leased).unwrap()["jobs"][0]
            .get("lease_fence")
            .is_none()
    );

    let decoded: CuratorBatch = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, batch);
    assert_eq!(decoded.digest(), batch.digest());
    let mut unsupported = serde_json::to_value(&batch).unwrap();
    unsupported["format_version"] = json!(2);
    let unsupported: CuratorBatch = serde_json::from_value(unsupported).unwrap();
    assert_eq!(
        unsupported.validate(20),
        Err(BatchFormatError::UnsupportedVersion)
    );
    assert!(matches!(
        CuratorBatch::new(SCOPE, jobs, 1),
        Err(BatchFormatError::TooManyCandidates)
    ));

    let mut oversized = rig.job_a();
    oversized.candidate.action_sequence = vec!["x".repeat(MAX_CURATOR_BATCH_BYTES)];
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![oversized], 20),
        Err(BatchFormatError::TooLarge)
    ));
}

#[test]
fn curator_batch_format_rejects_mixed_scopes_empty_ids_and_duplicates() {
    let rig = Rig::new(0);
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![], 20),
        Err(BatchFormatError::EmptyJobs)
    ));

    let mut different_scope = rig.job_b();
    different_scope.scope.task_category = "research".into();
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![rig.job_a(), different_scope], 20),
        Err(BatchFormatError::MixedJobScopes)
    ));

    let mut different_trigger = rig.job_b();
    different_trigger.trigger = wam_gatekeeper::tools::Trigger::Reverify;
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![rig.job_a(), different_trigger], 20),
        Err(BatchFormatError::MixedTriggers)
    ));

    let mut different_route = rig.job_b();
    different_route.candidate.url_pattern = "/different-route".into();
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![rig.job_a(), different_route], 20),
        Err(BatchFormatError::MixedCandidateRoutes)
    ));

    let mut duplicate_id = rig.job_b();
    duplicate_id.id = rig.job_a().id;
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![rig.job_a(), duplicate_id], 20),
        Err(BatchFormatError::DuplicateJobId)
    ));

    let mut empty_id = rig.job_a();
    empty_id.id.clear();
    assert!(matches!(
        CuratorBatch::new(SCOPE, vec![empty_id], 20),
        Err(BatchFormatError::EmptyJobId)
    ));
    assert!(matches!(
        CuratorBatch::new("  ", vec![], 20),
        Err(BatchFormatError::EmptyScope)
    ));
}

#[test]
fn worker_rejects_a_mixed_scope_batch_before_calling_the_model() {
    let mut rig = Rig::new(0);
    let queued = [
        rig.submit("alice", "c1", SCOPE, 0),
        rig.submit("bob", "c2", SCOPE, 0),
    ];
    let mut other_scope = rig.job_b();
    other_scope.scope.task_category = "research".into();
    let source = Source::new([("c1", rig.job_a()), ("c2", other_scope)]);
    let mut provider = Unwanted::default();
    let before = rig.support();
    let step = run(&mut rig, &mut big_budget(), &mut provider, &source, 5);

    assert!(matches!(step, Step::Retry { .. }), "{step:?}");
    assert_eq!(provider.asked, 0);
    assert_eq!(rig.support(), before);
    assert!(
        queued
            .iter()
            .all(|id| rig.queue.job(*id).unwrap().state == JobState::Retry)
    );
}

#[test]
fn worker_retries_before_model_when_source_drops_an_active_contribution() {
    let mut rig = Rig::new(0);
    let queued = [
        rig.submit("alice", "c1", SCOPE, 0),
        rig.submit("bob", "c2", SCOPE, 0),
    ];
    let source = Source::new([("c1", rig.job_a())]);
    let mut provider = Unwanted::default();
    let before = rig.support();
    let step = run(&mut rig, &mut big_budget(), &mut provider, &source, 5);

    assert!(matches!(step, Step::Retry { .. }), "{step:?}");
    assert_eq!(
        provider.asked, 0,
        "incomplete evidence never reaches the model"
    );
    assert_eq!(rig.support(), before);
    assert!(
        queued
            .iter()
            .all(|id| rig.queue.job(*id).unwrap().state == JobState::Retry)
    );
}

#[test]
fn que_09_model_failures_leave_the_catalog_alone_and_end_in_dead_letter() {
    let mut rig = Rig::new(0);
    let id = rig.submit("alice", "c1", SCOPE, 0);
    let source = Source::new([("c1", rig.job_a())]);
    let catalog = rig.store.memories.clone();
    let mut now = 0;
    for attempt in 1..=5 {
        // First an unreachable model, then one that only babbles: both are retried, neither changes anything.
        let mut provider = if attempt % 2 == 1 {
            ScriptedProvider::new([])
        } else {
            ScriptedProvider::new((0..3).map(|n| ScriptStep::Raw(format!("babble {n}"))))
        };
        let step = run(&mut rig, &mut big_budget(), &mut provider, &source, now);
        let Step::Retry { error, .. } = step else {
            panic!("attempt {attempt}: expected Retry, got {step:?}")
        };
        assert!(!error.is_empty());
        assert_eq!(
            rig.store.memories, catalog,
            "attempt {attempt}: the catalog changed"
        );
        let job = rig.queue.job(id).unwrap();
        assert_eq!(job.attempts, attempt);
        if attempt < 5 {
            assert_eq!(job.state, JobState::Retry);
            now = job.not_before;
        }
    }
    assert_eq!(rig.queue.job(id).unwrap().state, JobState::Dead);
    assert_eq!(
        rig.store
            .audits
            .iter()
            .filter(|a| a.outcome != "decided")
            .count(),
        5,
        "every failure is audited"
    );
    assert_eq!(rig.store.memories, catalog);
}

#[test]
fn que_09_a_failed_batch_is_retried_and_then_succeeds() {
    let mut rig = Rig::new(0);
    let id = rig.submit("alice", "c1", SCOPE, 0);
    let source = Source::new([("c1", rig.job_a())]);
    let before = rig.support();
    assert!(matches!(
        run(
            &mut rig,
            &mut big_budget(),
            &mut ScriptedProvider::new([]),
            &source,
            0
        ),
        Step::Retry { .. }
    ));
    let retry_at = rig.queue.job(id).unwrap().not_before;
    assert!(matches!(
        run(
            &mut rig,
            &mut big_budget(),
            &mut ScriptedProvider::new([]),
            &source,
            retry_at - 1
        ),
        Step::Idle
    ));
    let mut provider = ScriptedProvider::new(rig.script());
    assert!(matches!(
        run(
            &mut rig,
            &mut big_budget(),
            &mut provider,
            &source,
            retry_at
        ),
        Step::Done { .. }
    ));
    assert_eq!(
        (rig.queue.job(id).unwrap().state, rig.support()),
        (JobState::Done, before + 1)
    );
}

#[test]
fn que_06_committed_scope_decision_replays_after_queue_completion_failure() {
    let mut rig = Rig::new(0);
    let ids = [
        rig.submit("alice", "c1", SCOPE, 0),
        rig.submit("bob", "c2", SCOPE, 0),
    ];
    let source = Source::new([("c1", rig.job_a()), ("c2", rig.job_b())]);
    let before = rig.support();
    let mut provider = ScriptedProvider::new(rig.script());
    let first = {
        let mut queue = FailCompleteOnce {
            queue: &mut rig.queue,
            fail: true,
        };
        run_with_backend(
            &rig.tools,
            &mut queue,
            &mut big_budget(),
            &mut rig.store,
            &mut provider,
            &source,
            0,
        )
    };
    assert!(matches!(first, Step::Unavailable { .. }));
    assert_eq!(rig.support(), before + 2);

    let retry = run(
        &mut rig,
        &mut big_budget(),
        &mut ScriptedProvider::new([]),
        &source,
        301,
    );
    let Step::Done {
        reports, replayed, ..
    } = retry
    else {
        panic!("expected idempotent replay, got {retry:?}")
    };
    assert_eq!((reports.len(), replayed), (0, 1));
    assert_eq!(rig.support(), before + 2, "support was not applied twice");
    assert!(
        ids.iter()
            .all(|id| rig.queue.job(*id).unwrap().state == JobState::Done)
    );
}

#[test]
fn que_08_an_empty_request_bucket_parks_the_batch_until_the_bucket_refills() {
    let mut rig = Rig::new(0);
    let first = rig.submit("alice", "c1", "scope-1", 0);
    let second = rig.submit("bob", "c2", "scope-2", 0);
    let source = Source::new([("c1", rig.job_a()), ("c2", rig.job_b())]);
    let mut budget = ModelBudget::new(
        BudgetConfig {
            requests_per_minute: 1,
            tokens_per_minute: 10_000_000,
            daily_credits: None,
        },
        0,
    );
    let mut provider = ScriptedProvider::new(rig.script());
    assert!(matches!(
        run(&mut rig, &mut budget, &mut provider, &source, 0),
        Step::Done { .. }
    ));
    let mut provider = ScriptedProvider::new(rig.script());
    let step = run(&mut rig, &mut budget, &mut provider, &source, 1);
    let Step::Parked {
        retry_after_secs,
        daily,
        ..
    } = step
    else {
        panic!("expected Parked, got {step:?}")
    };
    assert!(
        (1..=60).contains(&retry_after_secs) && !daily,
        "{retry_after_secs} {daily}"
    );
    let parked = rig.queue.job(second).unwrap();
    assert_eq!(
        (parked.state, parked.attempts),
        (JobState::WaitingBudget, 0),
        "parked, and no attempt charged"
    );
    assert_eq!(rig.queue.job(first).unwrap().state, JobState::Done);
    // Recall is unaffected while the budget is spent: the catalog stays readable.
    assert!(!rig.store.memories_in_host(HOST).unwrap().is_empty());
    let mut provider = ScriptedProvider::new(rig.script());
    assert!(matches!(
        run(&mut rig, &mut budget, &mut provider, &source, 62),
        Step::Done { .. }
    ));
    assert_eq!(rig.queue.job(second).unwrap().state, JobState::Done);
}

#[test]
fn que_08_the_daily_credit_ceiling_parks_work_until_the_next_day() {
    let mut rig = Rig::new(0);
    rig.submit("alice", "c1", "scope-1", 0);
    let second = rig.submit("bob", "c2", "scope-2", 0);
    let source = Source::new([("c1", rig.job_a()), ("c2", rig.job_b())]);
    let mut budget = ModelBudget::new(
        BudgetConfig {
            requests_per_minute: 600,
            tokens_per_minute: 10_000_000,
            daily_credits: Some(0.05),
        },
        1_000,
    );
    // Four turns at 0.02 credits each overshoot the ceiling of 0.05.
    let mut provider = Costly {
        inner: ScriptedProvider::new(rig.script()),
        credits: 0.02,
    };
    assert!(matches!(
        run(&mut rig, &mut budget, &mut provider, &source, 1_000),
        Step::Done { .. }
    ));
    assert!(budget.spent_today() > 0.05);
    let mut provider = ScriptedProvider::new(rig.script());
    let step = run(&mut rig, &mut budget, &mut provider, &source, 2_000);
    let Step::Parked {
        retry_after_secs,
        daily,
        ..
    } = step
    else {
        panic!("expected Parked, got {step:?}")
    };
    assert!(daily);
    assert_eq!(retry_after_secs, 86_400 - 2_000, "until the day rolls over");
    assert_eq!(
        rig.queue.job(second).unwrap().state,
        JobState::WaitingBudget
    );
    let mut provider = ScriptedProvider::new(rig.script());
    assert!(matches!(
        run(&mut rig, &mut budget, &mut provider, &source, 86_400 + 1),
        Step::Done { .. }
    ));
}

#[test]
fn fail_15_16_a_worker_that_lost_its_lease_applies_nothing() {
    let mut rig = Rig::new(0);
    let id = rig.submit("alice", "c1", SCOPE, 0);
    let source = Source::new([("c1", rig.job_a())]);
    let slow = rig.queue.lease("slow-worker", 0).unwrap();
    // The lease expires; another worker takes the job over.
    let takeover = rig.queue.lease("fast-worker", 400).unwrap();
    assert_eq!(takeover.job_ids, vec![id]);
    let before = (rig.store.memories.clone(), rig.support());
    let tools = Tools::load().unwrap();
    let worker = worker(&tools, "slow-worker");
    let mut provider = ScriptedProvider::new(rig.script());
    let mut budget = big_budget();
    let mut env = Env {
        queue: &mut rig.queue,
        budget: &mut budget,
        store: &mut rig.store,
        provider: &mut provider,
        source: &source,
    };
    let step = worker.process(&mut env, slow, 401);
    assert!(matches!(step, Step::Fenced { .. }), "{step:?}");
    assert_eq!(
        (rig.store.memories.clone(), rig.support()),
        before,
        "nothing was applied"
    );
    assert_eq!(
        rig.queue.job(id).unwrap().state,
        JobState::Leased,
        "the new holder still has it"
    );
    assert!(
        rig.store.audits.is_empty(),
        "the stale worker never even called the model"
    );
}

#[test]
fn que_10_a_contributor_who_withdraws_mid_flight_is_left_out_of_the_batch() {
    let mut rig = Rig::new(0);
    let alice = rig.submit("alice", "c1", SCOPE, 0);
    let bob = rig.submit("bob", "c2", SCOPE, 0);
    let source = Source::new([("c1", rig.job_a()), ("c2", rig.job_b())]);
    let batch = rig.queue.lease("worker-1", 1).unwrap();
    assert_eq!(batch.job_ids.len(), 2);
    let report = rig.queue.cancel_contributor("alice", 2);
    assert_eq!((report.canceled, report.still_leased), (0, 1));
    let before = rig.support();
    let tools = Tools::load().unwrap();
    let worker = worker(&tools, "worker-1");
    let mut provider = ScriptedProvider::new(rig.script());
    let mut budget = big_budget();
    let mut env = Env {
        queue: &mut rig.queue,
        budget: &mut budget,
        store: &mut rig.store,
        provider: &mut provider,
        source: &source,
    };
    let step = worker.process(&mut env, batch, 3);
    let Step::Done { reports, .. } = step else {
        panic!("expected Done, got {step:?}")
    };
    assert_eq!(reports.len(), 1, "only bob's contribution was judged");
    assert_eq!(source.asked.borrow().as_slice(), [vec!["c2".to_owned()]]);
    assert_eq!(rig.support(), before + 1, "alice's evidence did not count");
    assert!(rig.queue.job(alice).unwrap().attribution_withdrawn);
    assert_eq!(rig.queue.job(alice).unwrap().state, JobState::Done);
    assert_eq!(rig.queue.job(bob).unwrap().state, JobState::Done);
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

/// Answers the scripted calls, then reports that the account's quota is spent, like a CLI at its usage limit.
struct RunsOut {
    inner: ScriptedProvider,
    after_calls: u32,
    calls: u32,
    retry_after_secs: Option<u64>,
}

impl LlmProvider for RunsOut {
    fn name(&self) -> String {
        "fake:runs-out".into()
    }

    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        if self.calls > self.after_calls {
            return Err(ProviderError::QuotaExhausted {
                why: "You've hit your usage limit".into(),
                retry_after_secs: self.retry_after_secs,
            });
        }
        self.inner.next_turn(request)
    }
}

#[test]
fn que_08_a_provider_that_runs_out_of_quota_parks_the_batch_and_nobody_calls_it_meanwhile() {
    let mut rig = Rig::new(0);
    let first = rig.submit("alice", "c1", "scope-1", 0);
    let second = rig.submit("bob", "c2", "scope-2", 0);
    let source = Source::new([("c1", rig.job_a()), ("c2", rig.job_b())]);
    let catalog = rig.store.memories.clone();
    let mut budget = big_budget();
    let mut provider = RunsOut {
        inner: ScriptedProvider::new([]),
        after_calls: 0,
        calls: 0,
        retry_after_secs: None,
    };
    let step = run(&mut rig, &mut budget, &mut provider, &source, 100);
    let Step::Parked {
        retry_after_secs,
        daily,
        provider_said,
    } = step
    else {
        panic!("expected Parked, got {step:?}")
    };
    assert_eq!((retry_after_secs, daily), (900, false), "the default pause");
    assert!(provider_said.unwrap().contains("usage limit"));
    let parked = rig.queue.job(first).unwrap();
    assert_eq!(
        (parked.state, parked.attempts),
        (JobState::WaitingBudget, 0),
        "parked, and no attempt charged"
    );
    assert_eq!(rig.store.memories, catalog, "the catalog is untouched");
    assert!(
        rig.store.audits[0].outcome.starts_with("quota_exhausted"),
        "{}",
        rig.store.audits[0].outcome
    );
    // While the pause lasts the model is not called at all: the next batch is parked as well, and then nothing
    // is left to try.
    let asked = provider.calls;
    let mut idle = Unwanted::default();
    let step = run(&mut rig, &mut budget, &mut idle, &source, 101);
    assert!(
        matches!(
            step,
            Step::Parked {
                retry_after_secs: 899,
                provider_said: None,
                ..
            }
        ),
        "{step:?}"
    );
    for now in [500, 999] {
        let step = run(&mut rig, &mut budget, &mut idle, &source, now);
        assert!(matches!(step, Step::Idle), "t={now}: {step:?}");
    }
    assert_eq!(idle.asked, 0, "the provider was not asked during the pause");
    assert_eq!(provider.calls, asked);
    assert_eq!(
        rig.queue.job(second).unwrap().state,
        JobState::WaitingBudget
    );
    // After the pause everything is released and processed.
    let mut provider = ScriptedProvider::new(rig.script().into_iter().chain(rig.script()));
    let step = run(&mut rig, &mut budget, &mut provider, &source, 1_000);
    assert!(matches!(step, Step::Done { .. }), "{step:?}");
    let step = run(&mut rig, &mut budget, &mut provider, &source, 1_001);
    assert!(matches!(step, Step::Done { .. }), "{step:?}");
    assert!(
        [first, second]
            .iter()
            .all(|id| rig.queue.job(*id).unwrap().state == JobState::Done)
    );
}

#[test]
fn que_08_the_providers_own_reset_time_wins_over_the_default_pause() {
    let mut rig = Rig::new(0);
    let id = rig.submit("alice", "c1", SCOPE, 0);
    let source = Source::new([("c1", rig.job_a())]);
    let mut budget = big_budget();
    let mut provider = RunsOut {
        inner: ScriptedProvider::new([]),
        after_calls: 0,
        calls: 0,
        retry_after_secs: Some(120),
    };
    let step = run(&mut rig, &mut budget, &mut provider, &source, 0);
    assert!(
        matches!(
            step,
            Step::Parked {
                retry_after_secs: 120,
                ..
            }
        ),
        "{step:?}"
    );
    let mut provider = ScriptedProvider::new(rig.script());
    assert!(matches!(
        run(&mut rig, &mut budget, &mut provider, &source, 119),
        Step::Idle
    ));
    assert!(matches!(
        run(&mut rig, &mut budget, &mut provider, &source, 120),
        Step::Done { .. }
    ));
    assert_eq!(rig.queue.job(id).unwrap().state, JobState::Done);
}

#[test]
fn que_08_quota_running_out_mid_scope_pass_parks_without_partial_decisions() {
    let mut rig = Rig::new(0);
    let ids = [
        rig.submit("alice", "c1", SCOPE, 0),
        rig.submit("bob", "c2", SCOPE, 0),
    ];
    let source = Source::new([("c1", rig.job_a()), ("c2", rig.job_b())]);
    let before = rig.support();
    let mut budget = big_budget();
    // The quota runs out before the scope-level curator pass can produce a decision.
    let mut provider = RunsOut {
        inner: ScriptedProvider::new(rig.script()),
        after_calls: 3,
        calls: 0,
        retry_after_secs: Some(60),
    };
    let step = run(&mut rig, &mut budget, &mut provider, &source, 0);
    assert!(matches!(step, Step::Parked { .. }), "{step:?}");
    assert_eq!(
        rig.support(),
        before,
        "a partial scope pass applies nothing"
    );
    assert!(
        ids.iter()
            .all(|id| rig.queue.job(*id).unwrap().state == JobState::WaitingBudget)
    );
    let mut provider = ScriptedProvider::new(rig.script());
    let step = run(&mut rig, &mut budget, &mut provider, &source, 60);
    let Step::Done {
        reports, replayed, ..
    } = step
    else {
        panic!("expected Done, got {step:?}")
    };
    assert_eq!((reports.len(), replayed), (1, 0));
    assert_eq!(rig.support(), before + 2, "counted once each");
    assert!(
        ids.iter()
            .all(|id| rig.queue.job(*id).unwrap().attempts == 0),
        "no attempt was charged"
    );
}
