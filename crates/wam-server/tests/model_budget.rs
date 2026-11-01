//! The worker's model budget gate (docs/spec/queue.md, QUE-08, QUE-20..23): when the provider's credit is spent or the
//! daily ceiling is reached, contributions wait in the queue and nothing keeps calling the model.
mod lifecycle_support;
use lifecycle_support::*;
use wam_server::{
    ApplicationRepository,
    durable::{ModelBudgetPolicy, WorkResult},
    ports::{ContributionStore, ScoreStore},
};

const DAY: u64 = 86_400;

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

#[test]
fn a_refusal_closes_the_gate_for_every_queued_job_and_nothing_runs_while_it_is_closed() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let accounts = [AUTHOR, READER, SUPPORTER];
    let ids = accounts.map(|account| contribution(&service, account));
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let mut provider = exhausted(None);
    let t0 = now();

    assert_eq!(
        repository
            .process_next_budgeted(&mut provider, &replay, &policy, t0)
            .unwrap(),
        WorkResult::Retry
    );
    assert_eq!((provider.calls, replay.asked()), (1, 1));
    // The default pause (15 minutes) holds for every job, including the two nobody has touched yet.
    for at in [t0 + 1, t0 + 60, t0 + 899] {
        assert_eq!(
            repository
                .process_next_budgeted(&mut provider, &replay, &policy, at)
                .unwrap(),
            WorkResult::Idle
        );
    }
    assert_eq!(
        (provider.calls, replay.asked()),
        (1, 1),
        "neither the model nor a browser replay runs while the gate is closed"
    );

    let status = repository.model_budget_status(&policy, t0 + 1).unwrap();
    assert!(!status.open);
    assert_eq!(status.blocked_by, Some("provider_quota"));
    assert_eq!(status.until, Some(t0 + 900));
    assert_eq!((status.strikes, status.waiting_jobs), (1, 3));
    // Contributors still see their contributions as submitted, and the operator's queue health counts all three
    // (the one parked for budget as well as the two queued behind it), so a long outage ages into an alert.
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
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
}

#[test]
fn the_pause_follows_the_providers_hint_and_otherwise_doubles_up_to_an_hour() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let mut at = now();
    // No hint: 15 minutes, then 30, then an hour, and an hour from then on.
    let mut provider = exhausted(None);
    for expected in [900, 1800, 3600, 3600] {
        assert_eq!(
            repository
                .process_next_budgeted(&mut provider, &replay, &policy, at)
                .unwrap(),
            WorkResult::Retry
        );
        let status = repository.model_budget_status(&policy, at).unwrap();
        assert_eq!(status.until, Some(at + expected));
        at += expected;
    }
    assert_eq!(provider.calls, 4, "one probe per pause, not one per poll");
    // A hint from the provider wins, within bounds: at least 30 seconds, at most an hour.
    for (hint, expected) in [(120, 120), (5, 30), (DAY, 3600)] {
        let mut provider = exhausted(Some(hint));
        assert_eq!(
            repository
                .process_next_budgeted(&mut provider, &replay, &policy, at)
                .unwrap(),
            WorkResult::Retry
        );
        let status = repository.model_budget_status(&policy, at).unwrap();
        assert_eq!(status.until, Some(at + expected), "hint {hint}");
        at += expected;
    }
}

#[test]
fn when_credit_returns_the_waiting_job_publishes_and_the_gate_resets() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let t0 = now();
    assert_eq!(
        repository
            .process_next_budgeted(&mut exhausted(None), &replay, &policy, t0)
            .unwrap(),
        WorkResult::Retry
    );
    // A contribution that arrives during the pause waits too: nothing starts for it.
    let late = contribution(&service, READER);
    let mut idle_probe = exhausted(None);
    assert_eq!(
        repository
            .process_next_budgeted(&mut idle_probe, &replay, &policy, t0 + 100)
            .unwrap(),
        WorkResult::Idle
    );
    assert_eq!(idle_probe.calls, 0);
    assert_eq!(
        service.contribution(READER, &late).unwrap().status,
        "queued"
    );

    // The pause ends and the credit is back: the queue drains. The refusal used no attempt, so a job that met it is
    // not on its way to manual review.
    let result = repository
        .process_next_budgeted(&mut provider("high"), &replay, &policy, t0 + 900)
        .unwrap();
    assert!(matches!(result, WorkResult::Published { .. }), "{result:?}");
    let approved = [(AUTHOR, &id), (READER, &late)]
        .iter()
        .filter(|(account, id)| service.contribution(account, id).unwrap().status == "approved")
        .count();
    assert_eq!(approved, 1);
    let status = repository.model_budget_status(&policy, t0 + 901).unwrap();
    assert!(status.open);
    assert_eq!(status.strikes, 0);
    let events = kinds(&repository);
    assert_eq!(count(&events, "worker_budget_paused", "provider_quota"), 1);
    assert_eq!(count(&events, "worker_budget_resumed", "provider_quota"), 1);
    assert_eq!(count(&events, "worker_budget_wait", &id), 1);
}

#[test]
fn the_pause_survives_a_restart() {
    let directory = std::env::temp_dir().join(format!(
        "wam-budget-{}-{}",
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
    {
        let repository = ApplicationRepository::open(&path).unwrap();
        let service = prepare(&repository);
        contribution(&service, AUTHOR);
        assert_eq!(
            repository
                .process_next_budgeted(&mut exhausted(None), &replay, &policy, t0)
                .unwrap(),
            WorkResult::Retry
        );
    }
    // A new process finds the gate closed: no probe goes out until the pause is over.
    let repository = ApplicationRepository::open(&path).unwrap();
    let mut probe = exhausted(None);
    assert_eq!(
        repository
            .process_next_budgeted(&mut probe, &replay, &policy, t0 + 5)
            .unwrap(),
        WorkResult::Idle
    );
    assert_eq!((probe.calls, replay.asked()), (0, 1));
    // After the pause one probe goes out; the credit is still spent, so the pause doubles.
    assert_eq!(
        repository
            .process_next_budgeted(&mut probe, &replay, &policy, t0 + 900)
            .unwrap(),
        WorkResult::Retry
    );
    assert_eq!(probe.calls, 1);
    assert_eq!(
        repository
            .model_budget_status(&policy, t0 + 900)
            .unwrap()
            .until,
        Some(t0 + 900 + 1800)
    );
    drop(repository);
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn the_daily_ceiling_stops_new_jobs_until_the_next_utc_day() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let first = contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::new(Some(0.05), 900, 3600).unwrap();
    let replay = CountingReplay::new();
    // Queue entries already exist; use the next UTC midnight so every same-day
    // probe below stays in that day even when this test runs after 23:00 UTC.
    let start = (now() / DAY + 1) * DAY;
    let tomorrow = (start / DAY + 1) * DAY;

    // The first job costs more than the whole ceiling; it still finishes (the ceiling is checked before a job).
    let mut costly_first = Costly {
        inner: provider("high"),
        cost_usd: 0.06,
        calls: 0,
    };
    let result = repository
        .process_next_budgeted(&mut costly_first, &replay, &policy, start)
        .unwrap();
    assert!(matches!(result, WorkResult::Published { .. }), "{result:?}");
    assert_eq!(
        service.contribution(AUTHOR, &first).unwrap().status,
        "approved"
    );
    // A published job is replayed twice: before the model and again on its decision.
    let replays_of_the_first_job = replay.asked();
    let status = repository.model_budget_status(&policy, start + 1).unwrap();
    assert!(!status.open);
    assert_eq!(status.blocked_by, Some("daily_cap"));
    assert_eq!(status.until, Some(tomorrow));
    assert!((status.spent_today_usd - 0.06).abs() < 1e-9);
    assert_eq!(status.daily_cap_usd, Some(0.05));

    // Another contribution arrives, on another route so that it is not an exact duplicate (which the rule pass
    // settles without the model). It waits for tomorrow: nothing is claimed, replayed or sent to the model.
    let second = contribution_on(&service, READER, "/notes");
    let mut costly_second = Costly {
        inner: scripted_record(record_on("/notes"), "high"),
        cost_usd: 0.06,
        calls: 0,
    };
    for at in [start + 2, start + 3600, tomorrow - 1] {
        assert_eq!(
            repository
                .process_next_budgeted(&mut costly_second, &replay, &policy, at)
                .unwrap(),
            WorkResult::Idle
        );
    }
    assert_eq!(
        (costly_second.calls, replay.asked()),
        (0, replays_of_the_first_job)
    );
    assert_eq!(
        service.contribution(READER, &second).unwrap().status,
        "queued"
    );

    // A new UTC day starts with nothing spent, and the waiting job runs.
    let result = repository
        .process_next_budgeted(&mut costly_second, &replay, &policy, tomorrow)
        .unwrap();
    assert!(matches!(result, WorkResult::Published { .. }), "{result:?}");
    assert_eq!(costly_second.calls, 1);
    assert_eq!(
        service.contribution(READER, &second).unwrap().status,
        "approved"
    );
    let today = repository.model_budget_status(&policy, tomorrow).unwrap();
    assert!(
        (today.spent_today_usd - 0.06).abs() < 1e-9,
        "yesterday's spend does not carry over: {}",
        today.spent_today_usd
    );
    let events = kinds(&repository);
    assert_eq!(count(&events, "worker_budget_resumed", "daily_cap"), 1);
    assert_eq!(count(&events, "worker_budget_paused", "daily_cap"), 2);
}

#[test]
fn an_exact_duplicate_is_settled_without_the_model_so_it_costs_nothing() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let replay = CountingReplay::new();
    let policy = ModelBudgetPolicy::new(Some(0.05), 900, 3600).unwrap();
    let start = now();
    contribution(&service, AUTHOR);
    let mut costly = Costly {
        inner: provider("high"),
        cost_usd: 0.01,
        calls: 0,
    };
    let result = repository
        .process_next_budgeted(&mut costly, &replay, &policy, start)
        .unwrap();
    assert!(matches!(result, WorkResult::Published { .. }), "{result:?}");
    assert_eq!(costly.calls, 1);
    // The same steps again on the same route: the rule pass supports the memory and the model is not called.
    contribution(&service, READER);
    let result = repository
        .process_next_budgeted(&mut costly, &replay, &policy, start + 1)
        .unwrap();
    assert!(
        !matches!(result, WorkResult::Idle),
        "the duplicate was processed: {result:?}"
    );
    assert_eq!(costly.calls, 1, "no model call, so no cost");
    let status = repository.model_budget_status(&policy, start + 2).unwrap();
    assert!((status.spent_today_usd - 0.01).abs() < 1e-9);
}

#[test]
fn an_operator_can_reopen_the_gate_as_soon_as_credit_is_added() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let t0 = now();
    assert_eq!(
        repository
            .process_next_budgeted(&mut exhausted(None), &replay, &policy, t0)
            .unwrap(),
        WorkResult::Retry
    );
    let mut probe = exhausted(None);
    assert_eq!(
        repository
            .process_next_budgeted(&mut probe, &replay, &policy, t0 + 5)
            .unwrap(),
        WorkResult::Idle
    );
    assert_eq!(probe.calls, 0);

    // Credit was added: reopen now instead of waiting out the pause.
    assert_eq!(repository.resume_model_budget(t0 + 6).unwrap(), 1);
    let status = repository.model_budget_status(&policy, t0 + 6).unwrap();
    assert_eq!((status.open, status.strikes), (true, 0));
    let result = repository
        .process_next_budgeted(&mut provider("high"), &replay, &policy, t0 + 7)
        .unwrap();
    assert!(matches!(result, WorkResult::Published { .. }), "{result:?}");
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    // Reopening a gate that is open is harmless.
    assert_eq!(repository.resume_model_budget(t0 + 8).unwrap(), 0);
    assert_eq!(
        count(&kinds(&repository), "worker_budget_resumed", "manual"),
        1
    );
}

#[test]
fn withdrawing_consent_cancels_a_job_that_waits_for_budget() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let t0 = now();
    assert_eq!(
        repository
            .process_next_budgeted(&mut exhausted(None), &replay, &policy, t0)
            .unwrap(),
        WorkResult::Retry
    );
    assert!(repository.withdraw_for_account(AUTHOR, &id).unwrap());
    // The pause ends, but the withdrawn contribution must not reach the model.
    let mut probe = exhausted(None);
    assert_eq!(
        repository
            .process_next_budgeted(&mut probe, &replay, &policy, t0 + 900)
            .unwrap(),
        WorkResult::Idle
    );
    assert_eq!(probe.calls, 0);
}
