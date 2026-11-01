//! A provider that cannot answer for now is waited out, not charged to the job (docs/spec/provider-outage.md,
//! docs/test/provider-outage.md, QUE-25..QUE-33). Time is passed in, so nothing here sleeps.
mod lifecycle_support;
use lifecycle_support::*;
use std::time::Duration;
use wam_gatekeeper::provider::{LlmProvider, OpenAiCompatProvider, ScriptStep, ScriptedProvider};
use wam_server::{
    ApplicationRepository,
    durable::{ModelBudgetPolicy, WorkResult},
    ports::{ContributionStore, ScoreStore},
};

const SECRET: &str = "fixture-secret-key";
const SERVER_TEXT: &str = "text the provider sent and nobody may repeat";

fn kinds(repository: &ApplicationRepository) -> Vec<(String, String)> {
    repository
        .pending_outbox(1000)
        .unwrap()
        .into_iter()
        .map(|event| (event.kind, event.target))
        .collect()
}

fn count(events: &[(String, String)], kind: &str, target: &str) -> usize {
    events
        .iter()
        .filter(|(k, t)| k == kind && t == target)
        .count()
}

fn step(
    repository: &ApplicationRepository,
    provider: &mut dyn LlmProvider,
    policy: &ModelBudgetPolicy,
    at: u64,
) -> WorkResult {
    repository
        .process_next_budgeted(provider, &PassingReplay, policy, at)
        .unwrap()
}

fn published(result: &WorkResult) -> bool {
    matches!(result, WorkResult::Published { .. })
}

#[test]
fn an_outage_never_sends_a_job_to_review_however_long_it_lasts() {
    // QUE-25. Five failed attempts used to be enough to send the job to manual review.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let mut flaky = Recovering {
        inner: provider("high"),
        outages: 7,
        calls: 0,
    };
    let mut at = now();
    for round in 1..=7 {
        assert_eq!(
            step(&repository, &mut flaky, &policy, at),
            WorkResult::Retry,
            "outage {round}"
        );
        assert_eq!(flaky.calls, round);
        let status = repository.model_budget_status(&policy, at + 1).unwrap();
        assert!(!status.open, "outage {round}");
        assert_eq!(status.blocked_by, Some("provider_outage"));
        // The wait is not a failed attempt: the contribution is still submitted, nothing is in review, and nothing
        // runs until the gate reopens.
        assert_eq!(service.contribution(AUTHOR, &id).unwrap().status, "queued");
        assert_eq!(count(&kinds(&repository), "review_required", &id), 0);
        assert_eq!(
            step(&repository, &mut flaky, &policy, at + 1),
            WorkResult::Idle
        );
        assert_eq!(flaky.calls, round, "no call while the gate is closed");
        at = status.until.unwrap();
    }
    let result = step(&repository, &mut flaky, &policy, at);
    assert!(published(&result), "{result:?}");
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    assert_eq!(
        repository
            .model_budget_status(&policy, at)
            .unwrap()
            .blocked_by,
        None
    );
    // The gate announced every closing and every reopening once, so the counts agree after recovery.
    let events = kinds(&repository);
    assert_eq!(count(&events, "worker_budget_paused", "provider_outage"), 7);
    assert_eq!(
        count(&events, "worker_budget_resumed", "provider_outage"),
        7
    );
    assert_eq!(count(&events, "worker_budget_wait", &id), 7);
    assert_eq!(count(&events, "review_required", &id), 0);
}

#[test]
fn while_the_provider_is_down_only_one_probe_goes_out_per_window() {
    // QUE-26.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let accounts = [AUTHOR, READER, SUPPORTER];
    let ids = accounts.map(|account| contribution(&service, account));
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let mut provider = down(None);
    let t0 = now();
    let go = |provider: &mut Down, at: u64| {
        repository
            .process_next_budgeted(provider, &replay, &policy, at)
            .unwrap()
    };
    assert_eq!(go(&mut provider, t0), WorkResult::Retry);
    assert_eq!((provider.calls, replay.asked()), (1, 1));
    for at in [t0 + 1, t0 + 29] {
        assert_eq!(go(&mut provider, at), WorkResult::Idle);
    }
    assert_eq!(
        (provider.calls, replay.asked()),
        (1, 1),
        "neither the model nor a browser replay runs while the gate is closed"
    );
    let status = repository.model_budget_status(&policy, t0 + 1).unwrap();
    assert_eq!(status.blocked_by, Some("provider_outage"));
    assert_eq!(status.until, Some(t0 + 30));
    assert_eq!(status.waiting_jobs, 3);
    for (account, id) in accounts.iter().zip(&ids) {
        assert_eq!(service.contribution(account, id).unwrap().status, "queued");
    }
    assert_eq!(
        repository
            .queue_health(t0 + 1)
            .unwrap()
            .unwrap()
            .active_jobs,
        3
    );
    // When the gate reopens exactly one job goes out as the probe.
    assert_eq!(go(&mut provider, t0 + 30), WorkResult::Retry);
    assert_eq!((provider.calls, replay.asked()), (2, 2));
    assert_eq!(go(&mut provider, t0 + 31), WorkResult::Idle);
    assert_eq!((provider.calls, replay.asked()), (2, 2));
}

#[test]
fn the_pause_follows_retry_after_and_otherwise_backs_off() {
    // QUE-27.
    let policy = ModelBudgetPolicy::default();
    let run = |hint: Option<u64>, expected: &[u64]| {
        let repository = ApplicationRepository::in_memory();
        let service = prepare(&repository);
        contribution(&service, AUTHOR);
        let mut provider = down(hint);
        let mut at = now();
        for (round, wait) in expected.iter().enumerate() {
            assert_eq!(
                step(&repository, &mut provider, &policy, at),
                WorkResult::Retry,
                "{hint:?} round {round}"
            );
            let status = repository.model_budget_status(&policy, at).unwrap();
            assert_eq!(status.until, Some(at + wait), "{hint:?} round {round}");
            // One second short of the wait nothing goes out; at the wait the probe does.
            assert_eq!(
                step(&repository, &mut provider, &policy, at + wait - 1),
                WorkResult::Idle
            );
            assert_eq!(provider.calls, round + 1);
            at += wait;
        }
    };
    // No hint: 30 seconds, doubling, at most ten minutes.
    run(None, &[30, 60, 120, 240, 480, 600, 600]);
    // A hint longer than the backoff wins until the backoff catches up; a shorter one cannot shorten it.
    run(Some(45), &[45, 60, 120, 240]);
    run(Some(5), &[30, 60]);
    // The provider's own time is followed up to the configured maximum for provider-named times (one hour).
    run(Some(100_000), &[3600]);
}

#[test]
fn when_the_provider_returns_everything_waiting_is_processed_and_the_gate_resets() {
    // QUE-28. A met the outage, so B and C (which did not) go first; nobody is skipped or published twice.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let jobs = [(AUTHOR, "/a"), (READER, "/b"), (SUPPORTER, "/c")];
    for (account, route) in jobs {
        contribution_on(&service, account, route);
    }
    let policy = ModelBudgetPolicy::default();
    let t0 = now();
    assert_eq!(
        step(&repository, &mut down(None), &policy, t0),
        WorkResult::Retry
    );
    let at = t0 + 30;
    for (account, route) in [(READER, "/b"), (SUPPORTER, "/c"), (AUTHOR, "/a")] {
        let mut healthy = scripted_record(record_on(route), "high");
        let result = step(&repository, &mut healthy, &policy, at);
        assert!(published(&result), "{route}: {result:?}");
        assert_eq!(repository.total_points_for(account).unwrap(), 20, "{route}");
    }
    assert_eq!(service.catalog().unwrap().memories.len(), 3);
    assert_eq!(
        step(&repository, &mut down(None), &policy, at),
        WorkResult::Idle
    );
    let status = repository.model_budget_status(&policy, at).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.strikes),
        (true, None, 0)
    );
    let events = kinds(&repository);
    assert_eq!(count(&events, "worker_budget_paused", "provider_outage"), 1);
    assert_eq!(
        count(&events, "worker_budget_resumed", "provider_outage"),
        1
    );
}

#[test]
fn a_job_the_provider_cannot_answer_does_not_hold_up_the_others_and_ends_in_review() {
    // QUE-29. P is the oldest job and the provider is down for its route only.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let poison = contribution_on(&service, AUTHOR, "/poison");
    contribution_on(&service, READER, "/h1");
    contribution_on(&service, SUPPORTER, "/h2");
    let policy = ModelBudgetPolicy::default();
    let mut provider = selective("/poison");
    let t0 = now();
    let mut at = t0;
    let mut outcomes = vec![step(&repository, &mut provider, &policy, at)];
    // After P's first outage the healthy jobs go first, in one pass, without waiting for P.
    at += 700;
    outcomes.push(step(&repository, &mut provider, &policy, at));
    outcomes.push(step(&repository, &mut provider, &policy, at));
    assert_eq!(outcomes[0], WorkResult::Retry);
    assert!(
        published(&outcomes[1]) && published(&outcomes[2]),
        "{outcomes:?}"
    );
    assert_eq!(repository.total_points_for(READER).unwrap(), 20);
    assert_eq!(repository.total_points_for(SUPPORTER).unwrap(), 20);
    // From here only P is left. It waits eight times without using an attempt, then uses five.
    for hit in 2..=13 {
        assert_eq!(
            count(&kinds(&repository), "review_required", &poison),
            0,
            "before hit {hit}"
        );
        at += 700;
        assert_eq!(
            step(&repository, &mut provider, &policy, at),
            WorkResult::Retry,
            "hit {hit}"
        );
        assert_eq!(provider.down_calls, hit);
    }
    assert_eq!(provider.down_calls, 13);
    assert_eq!(count(&kinds(&repository), "review_required", &poison), 1);
    // In review it is out of the queue for good: nothing is claimed, nothing is published or credited.
    at += 700;
    assert_eq!(
        step(&repository, &mut provider, &policy, at),
        WorkResult::Idle
    );
    assert_eq!(provider.down_calls, 13);
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    assert_eq!(service.catalog().unwrap().memories.len(), 2);
}

#[test]
fn a_real_http_provider_outage_and_recovery_through_the_worker() {
    // QUE-32. The provider code, the HTTP classification and the worker, joined by a loopback server.
    let error = |code: &str| {
        serde_json::json!({"error":{"type":"server_error","code":code,"message":SERVER_TEXT}})
            .to_string()
    };
    let answer = serde_json::json!({
        "tool":"decide_create",
        "args":{"record":record(),"reason_codes":["no_existing_in_scope"],"confidence_band":"high"}
    })
    .to_string();
    let success = serde_json::json!({
        "output":[{"type":"message","content":[{"type":"output_text","text":answer}]}]
    })
    .to_string();
    let server = scripted_server(vec![
        Reply::new(
            "503 Service Unavailable",
            "Retry-After: 45\r\n",
            error("service_overloaded"),
        ),
        Reply::new("429 Too Many Requests", "", error("rate_limit_exceeded")),
        Reply::new("200 OK", "", success),
    ]);
    let mut provider = OpenAiCompatProvider::new(
        "muse",
        &server.base_url,
        SECRET,
        "fixture-model",
        Duration::from_secs(5),
    )
    .unwrap();
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let t0 = now();

    assert_eq!(
        step(&repository, &mut provider, &policy, t0),
        WorkResult::Retry
    );
    assert_eq!(server.hits(), 1);
    let status = repository.model_budget_status(&policy, t0).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.until),
        (false, Some("provider_outage"), Some(t0 + 45)),
        "the provider's Retry-After is followed"
    );
    assert_eq!(
        step(&repository, &mut provider, &policy, t0 + 44),
        WorkResult::Idle
    );
    assert_eq!(
        server.hits(),
        1,
        "nothing reaches the provider while the gate is closed"
    );

    // A rate limit without a hint backs off: the second outage in a row waits 60 seconds.
    assert_eq!(
        step(&repository, &mut provider, &policy, t0 + 45),
        WorkResult::Retry
    );
    assert_eq!(server.hits(), 2);
    let status = repository.model_budget_status(&policy, t0 + 45).unwrap();
    assert_eq!(status.until, Some(t0 + 45 + 60));
    assert_eq!(
        step(&repository, &mut provider, &policy, t0 + 45 + 59),
        WorkResult::Idle
    );
    assert_eq!(server.hits(), 2);

    let result = step(&repository, &mut provider, &policy, t0 + 45 + 60);
    assert!(published(&result), "{result:?}");
    assert_eq!(server.hits(), 3);
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);

    // The key went out on every request and appears nowhere the operator reads; neither does the server's text.
    assert!(server.requests().iter().all(|request| {
        request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {SECRET}"))
    }));
    let status = repository
        .model_budget_status(&policy, t0 + 45 + 60)
        .unwrap();
    let shown = format!(
        "{} {:?}",
        serde_json::to_string(&status).unwrap(),
        kinds(&repository)
    );
    assert!(
        !shown.contains(SECRET) && !shown.contains(SERVER_TEXT),
        "{shown}"
    );
}

/// Five claims that each fail the way `make` makes them fail, `700` seconds apart (longer than any backoff or pause).
/// Returns the gate as it stood right after the fifth failure.
fn five_failed_claims(
    make: &mut dyn FnMut() -> Box<dyn LlmProvider>,
    policy: &ModelBudgetPolicy,
) -> wam_server::durable::ModelBudgetStatus {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let mut at = now();
    let mut gate = None;
    for attempt in 1..=5usize {
        assert_eq!(
            step(&repository, make().as_mut(), policy, at),
            WorkResult::Retry,
            "attempt {attempt}"
        );
        let events = kinds(&repository);
        // Four retries, then the fifth failure puts the job in review and says so.
        assert_eq!(
            count(&events, "worker_retry", &id),
            attempt.min(4),
            "attempt {attempt}"
        );
        assert_eq!(
            count(&events, "review_required", &id),
            usize::from(attempt == 5),
            "attempt {attempt}"
        );
        gate = Some(repository.model_budget_status(policy, at + 1).unwrap());
        at += 700;
    }
    // In review nothing claims the job any more.
    let mut probe = Rejected { calls: 0 };
    assert_eq!(step(&repository, &mut probe, policy, at), WorkResult::Idle);
    assert_eq!(probe.calls, 0);
    gate.unwrap()
}

#[test]
fn ordinary_failures_still_use_attempts_and_the_old_behaviour_can_be_restored() {
    // QUE-30 (and QUE-06 as a regression guard).
    let policy = ModelBudgetPolicy::default();
    // (a) The provider rejects the request: not an outage, no gate, five attempts.
    let status = five_failed_claims(&mut || Box::new(Rejected { calls: 0 }), &policy);
    assert_eq!(
        (status.open, status.blocked_by, status.outage_strikes),
        (true, None, 0)
    );
    // (b) The provider answers but the model never decides: also five attempts, and the answers end any outage.
    let status = five_failed_claims(
        &mut || {
            Box::new(ScriptedProvider::new(
                (0..3).map(|_| ScriptStep::Raw("this is not a tool call".into())),
            ))
        },
        &policy,
    );
    assert_eq!((status.open, status.blocked_by), (true, None));
    // (c) With no free waits an outage is charged to the job like any failure, but the gate still closes for everyone.
    let charged = ModelBudgetPolicy::default()
        .with_outage(30, 600, 0)
        .unwrap();
    let status = five_failed_claims(&mut || Box::new(down(None)), &charged);
    assert_eq!(
        (status.open, status.blocked_by),
        (false, Some("provider_outage"))
    );
    assert_eq!(status.outage_strikes, 5);
}

#[test]
fn an_outage_wait_survives_a_restart_and_a_withdrawal_cancels_it() {
    // QUE-31.
    let directory = std::env::temp_dir().join(format!(
        "wam-outage-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = directory.join("application.json");
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let t0 = now();
    let first;
    {
        let repository = ApplicationRepository::open(&path).unwrap();
        let service = prepare(&repository);
        first = contribution_on(&service, AUTHOR, "/first");
        contribution_on(&service, READER, "/second");
        assert_eq!(
            repository
                .process_next_budgeted(&mut down(Some(120)), &replay, &policy, t0)
                .unwrap(),
            WorkResult::Retry
        );
    }
    // A new process finds the same closed gate, and says why.
    let repository = ApplicationRepository::open(&path).unwrap();
    let mut probe = down(None);
    assert_eq!(
        repository
            .process_next_budgeted(&mut probe, &replay, &policy, t0 + 5)
            .unwrap(),
        WorkResult::Idle
    );
    assert_eq!(probe.calls, 0);
    let status = repository.model_budget_status(&policy, t0 + 5).unwrap();
    assert_eq!(
        (
            status.open,
            status.blocked_by,
            status.until,
            status.outage_strikes,
            status.reason.as_deref(),
            status.waiting_jobs
        ),
        (
            false,
            Some("provider_outage"),
            Some(t0 + 120),
            1,
            Some("HTTP 503 from model provider"),
            2
        )
    );
    // The contributor withdraws the contribution that was waiting; when the gate reopens only the other one runs.
    let service = wam_server::WamService::durable(repository.clone()).unwrap();
    assert!(repository.withdraw_for_account(AUTHOR, &first).unwrap());
    assert_eq!(
        repository
            .model_budget_status(&policy, t0 + 5)
            .unwrap()
            .waiting_jobs,
        1
    );
    let result = repository
        .process_next_budgeted(
            &mut scripted_record(record_on("/second"), "high"),
            &replay,
            &policy,
            t0 + 120,
        )
        .unwrap();
    assert!(published(&result), "{result:?}");
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    assert_eq!(repository.total_points_for(READER).unwrap(), 20);
    assert_eq!(service.catalog().unwrap().memories.len(), 1);
    drop(repository);
    std::fs::remove_dir_all(&directory).ok();

    // The operator reopens the gate by hand (`wam-worker --resume-budget`): the parked job is queued again and the
    // next worker takes it at once instead of waiting out the pause.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    assert_eq!(
        repository
            .process_next_budgeted(&mut down(None), &replay, &policy, t0)
            .unwrap(),
        WorkResult::Retry
    );
    assert_eq!(repository.resume_model_budget(t0 + 1).unwrap(), 1);
    let status = repository.model_budget_status(&policy, t0 + 1).unwrap();
    assert_eq!(
        (status.open, status.outage_strikes, status.reason.clone()),
        (true, 0, None)
    );
    let result = repository
        .process_next_budgeted(&mut provider("high"), &replay, &policy, t0 + 1)
        .unwrap();
    assert!(published(&result), "{result:?}");
    assert_eq!(
        count(&kinds(&repository), "worker_budget_resumed", "manual"),
        1
    );
}
