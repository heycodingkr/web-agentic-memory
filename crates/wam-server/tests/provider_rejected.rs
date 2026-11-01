//! A provider that rejects the key or the model setting (HTTP 401, 403, 404) is waited out behind the budget gate and
//! not charged to the job (docs/spec/provider-outage.md R11..R14, docs/test/provider-outage.md, QUE-34..QUE-39).
//! Time is passed in, so nothing here sleeps.
mod lifecycle_support;
use lifecycle_support::*;
use std::time::Duration;
use wam_gatekeeper::provider::{LlmProvider, OpenAiCompatProvider};
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
fn a_rejected_key_parks_the_job_without_using_an_attempt_and_closes_the_gate_for_an_hour() {
    // QUE-34. Five failed attempts used to send the job to manual review within minutes of a revoked key.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let policy = ModelBudgetPolicy::default();
    let replay = CountingReplay::new();
    let mut provider = rejecting();
    let t0 = now();
    let go = |provider: &mut Rejecting, at: u64| {
        repository
            .process_next_budgeted(provider, &replay, &policy, at)
            .unwrap()
    };

    assert_eq!(go(&mut provider, t0), WorkResult::Retry);
    assert_eq!((provider.calls, replay.asked()), (1, 1));
    let status = repository.model_budget_status(&policy, t0).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.until),
        (false, Some("provider_rejected"), Some(t0 + 3600))
    );
    assert_eq!(status.rejected_strikes, 1);
    assert_eq!(
        status.reason.as_deref(),
        Some("HTTP 401 from model provider")
    );
    // The wait is not a failed attempt: the contribution is still submitted, nothing is in review, no attempt was
    // used (a used attempt would show as a `worker_retry`), and the one event says the job waits.
    assert_eq!(service.contribution(AUTHOR, &id).unwrap().status, "queued");
    let events = kinds(&repository);
    assert_eq!(count(&events, "review_required", &id), 0);
    assert_eq!(count(&events, "worker_retry", &id), 0);
    assert_eq!(count(&events, "worker_budget_wait", &id), 1);
    assert_eq!(
        count(&events, "worker_budget_paused", "provider_rejected"),
        1
    );

    // Nothing runs while the gate is closed: not the model and not a browser replay.
    for at in [t0 + 1, t0 + 1800, t0 + 3599] {
        assert_eq!(go(&mut provider, at), WorkResult::Idle, "at {}", at - t0);
    }
    assert_eq!((provider.calls, replay.asked()), (1, 1));
    // When the hour is over one probe goes out.
    assert_eq!(go(&mut provider, t0 + 3600), WorkResult::Retry);
    assert_eq!(provider.calls, 2);
}

#[test]
fn the_pause_for_a_rejection_doubles_from_an_hour_up_to_six_hours() {
    // QUE-35.
    let custom = ModelBudgetPolicy::default()
        .with_rejection(60, 120)
        .unwrap();
    let cases: [(ModelBudgetPolicy, &[u64]); 2] = [
        (
            ModelBudgetPolicy::default(),
            &[3600, 7200, 14400, 21600, 21600, 21600],
        ),
        (custom, &[60, 120, 120]),
    ];
    for (policy, waits) in cases {
        let repository = ApplicationRepository::in_memory();
        let service = prepare(&repository);
        let id = contribution(&service, AUTHOR);
        let mut provider = rejecting();
        let mut at = now();
        for (round, wait) in waits.iter().enumerate() {
            assert_eq!(
                step(&repository, &mut provider, &policy, at),
                WorkResult::Retry,
                "round {round}"
            );
            let status = repository.model_budget_status(&policy, at).unwrap();
            assert_eq!(status.until, Some(at + wait), "round {round}");
            assert_eq!(status.blocked_by, Some("provider_rejected"));
            assert_eq!(status.rejected_strikes as usize, round + 1);
            // One second early nothing runs; at the time one probe goes out.
            assert_eq!(
                step(&repository, &mut provider, &policy, at + wait - 1),
                WorkResult::Idle,
                "round {round}"
            );
            at += wait;
        }
        assert_eq!(provider.calls, waits.len());
        // However long it lasts, the job does not go to review while its free waits last.
        assert_eq!(count(&kinds(&repository), "review_required", &id), 0);
    }
}

#[test]
fn fixing_the_key_recovers_by_itself_and_resume_reopens_at_once() {
    // QUE-36.
    let policy = ModelBudgetPolicy::default();
    let t0 = now();

    // (a) Nobody does anything but fix the key: the next probe finds out.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    assert_eq!(
        step(&repository, &mut rejecting(), &policy, t0),
        WorkResult::Retry
    );
    let result = step(&repository, &mut provider("high"), &policy, t0 + 3600);
    assert!(published(&result), "{result:?}");
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    let status = repository.model_budget_status(&policy, t0 + 3600).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.rejected_strikes),
        (true, None, 0)
    );
    assert_eq!(status.reason, None);
    let events = kinds(&repository);
    assert_eq!(
        count(&events, "worker_budget_paused", "provider_rejected"),
        1
    );
    assert_eq!(
        count(&events, "worker_budget_resumed", "provider_rejected"),
        1
    );

    // (b) The operator mends the key and reopens the gate (`wam-worker --resume-budget`): the parked job is queued
    // again and the next worker takes it at once.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    assert_eq!(
        step(&repository, &mut rejecting(), &policy, t0),
        WorkResult::Retry
    );
    assert_eq!(
        step(&repository, &mut provider("high"), &policy, t0 + 10),
        WorkResult::Idle
    );
    assert_eq!(repository.resume_model_budget(t0 + 10).unwrap(), 1);
    let status = repository.model_budget_status(&policy, t0 + 10).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.rejected_strikes),
        (true, None, 0)
    );
    let result = step(&repository, &mut provider("high"), &policy, t0 + 10);
    assert!(published(&result), "{result:?}");
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    assert_eq!(
        count(&kinds(&repository), "worker_budget_resumed", "manual"),
        1
    );
}

#[test]
fn a_job_rejected_too_often_ends_in_review_like_any_failure() {
    // QUE-37. The waits are counted together with outages (R11): this policy allows two.
    let policy = ModelBudgetPolicy::default()
        .with_outage(30, 600, 2)
        .unwrap();
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let mut provider = rejecting();
    let mut at = now();
    for call in 1..=7usize {
        assert_eq!(
            step(&repository, &mut provider, &policy, at),
            WorkResult::Retry,
            "call {call}"
        );
        let events = kinds(&repository);
        // Two calls wait for free; every later one is charged like an ordinary failure: four retries, then review.
        assert_eq!(
            count(&events, "worker_budget_wait", &id),
            call.min(2),
            "call {call}"
        );
        assert_eq!(
            count(&events, "worker_retry", &id),
            call.saturating_sub(2).min(4),
            "call {call}"
        );
        assert_eq!(
            count(&events, "review_required", &id),
            usize::from(call == 7),
            "call {call}"
        );
        // The gate closes either way: a rejected key is everybody's problem.
        let status = repository.model_budget_status(&policy, at + 1).unwrap();
        assert_eq!(status.blocked_by, Some("provider_rejected"), "call {call}");
        at = status.until.unwrap();
    }
    // In review nothing claims it again.
    assert_eq!(
        step(&repository, &mut provider, &policy, at),
        WorkResult::Idle
    );
    assert_eq!(provider.calls, 7);

    // With no free waits a rejection is charged from the first one, and the gate still closes.
    let charged = ModelBudgetPolicy::default()
        .with_outage(30, 600, 0)
        .unwrap();
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    assert_eq!(
        step(&repository, &mut rejecting(), &charged, now()),
        WorkResult::Retry
    );
    let events = kinds(&repository);
    assert_eq!(count(&events, "worker_retry", &id), 1);
    assert_eq!(count(&events, "worker_budget_wait", &id), 0);
    assert_eq!(
        repository
            .model_budget_status(&charged, now())
            .unwrap()
            .blocked_by,
        Some("provider_rejected")
    );

    // One outage and one rejection use up the two free waits of the same job; the third trouble is charged.
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let mut at = now();
    assert_eq!(
        step(&repository, &mut down(None), &policy, at),
        WorkResult::Retry
    );
    at = repository
        .model_budget_status(&policy, at)
        .unwrap()
        .until
        .unwrap();
    assert_eq!(
        step(&repository, &mut rejecting(), &policy, at),
        WorkResult::Retry
    );
    at = repository
        .model_budget_status(&policy, at)
        .unwrap()
        .until
        .unwrap();
    let events = kinds(&repository);
    assert_eq!(
        (
            count(&events, "worker_budget_wait", &id),
            count(&events, "worker_retry", &id)
        ),
        (2, 0)
    );
    assert_eq!(
        step(&repository, &mut down(None), &policy, at),
        WorkResult::Retry
    );
    let events = kinds(&repository);
    assert_eq!(
        (
            count(&events, "worker_budget_wait", &id),
            count(&events, "worker_retry", &id)
        ),
        (2, 1),
        "the third trouble is charged as an attempt"
    );
}

#[test]
fn a_rejection_tells_the_operator_what_to_fix_and_survives_a_restart() {
    // QUE-38. A file repository, so that the restart is a real reopen.
    let directory = std::env::temp_dir().join(format!(
        "wam-rejected-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = directory.join("application.json");
    let policy = ModelBudgetPolicy::default();
    let t0 = now();
    let until;
    {
        let repository = ApplicationRepository::open(&path).unwrap();
        let service = prepare(&repository);
        contribution(&service, AUTHOR);
        // An outage mends itself, so nobody has to look: the queue stays healthy.
        assert_eq!(
            step(&repository, &mut down(None), &policy, t0),
            WorkResult::Retry
        );
        assert!(!repository.queue_health(t0 + 1).unwrap().unwrap().alert);
        // A rejection waits for a person: the alert is on at once, however young the job is.
        assert_eq!(
            step(&repository, &mut rejecting(), &policy, t0 + 30),
            WorkResult::Retry
        );
        let health = repository.queue_health(t0 + 31).unwrap().unwrap();
        assert!(health.alert, "{health:?}");
        assert_eq!(health.active_jobs, 1);
        until = repository
            .model_budget_status(&policy, t0 + 31)
            .unwrap()
            .until
            .unwrap();
        assert_eq!(until, t0 + 30 + 3600);
    }

    // A new process finds the same closed gate and says what to do about it.
    let repository = ApplicationRepository::open(&path).unwrap();
    let status = repository.model_budget_status(&policy, t0 + 32).unwrap();
    assert_eq!(
        (
            status.open,
            status.blocked_by,
            status.until,
            status.rejected_strikes,
            status.outage_strikes,
            status.reason.as_deref(),
            status.waiting_jobs
        ),
        (
            false,
            Some("provider_rejected"),
            Some(until),
            1,
            1,
            Some("HTTP 401 from model provider"),
            1
        )
    );
    let notice = status.closed_notice().unwrap();
    for needle in [
        "provider_rejected",
        "HTTP 401 from model provider",
        "MODEL_API_KEY",
        "WAM_LLM_MODEL",
        "WAM_LLM_BASE_URL",
        "--resume-budget",
    ] {
        assert!(notice.contains(needle), "{needle} is missing: {notice}");
    }
    assert!(repository.queue_health(t0 + 32).unwrap().unwrap().alert);
    let mut probe = rejecting();
    assert_eq!(
        step(&repository, &mut probe, &policy, until - 1),
        WorkResult::Idle
    );
    assert_eq!(probe.calls, 0);

    // The key is mended: the probe publishes, the gate is clean and the alert goes away.
    let result = step(&repository, &mut provider("high"), &policy, until);
    assert!(published(&result), "{result:?}");
    let status = repository.model_budget_status(&policy, until).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.rejected_strikes),
        (true, None, 0)
    );
    assert!(!repository.queue_health(until).unwrap().unwrap().alert);
    drop(repository);
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_real_http_provider_rejection_and_recovery_through_the_worker() {
    // QUE-39. The provider code, the HTTP classification and the worker, joined by a loopback server. The 401 body
    // is the one dev.meta.ai/docs/error-handling documents, with the provider's own text added.
    let unauthorized = serde_json::json!({"error":{
        "message":SERVER_TEXT,"type":"authentication_error","param":null,"code":"invalid_api_key"
    }})
    .to_string();
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
        Reply::new("401 Unauthorized", "", unauthorized),
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
        (
            status.open,
            status.blocked_by,
            status.until,
            status.reason.as_deref()
        ),
        (
            false,
            Some("provider_rejected"),
            Some(t0 + 3600),
            Some("HTTP 401 from model provider")
        )
    );
    // What the operator reads: the setting to look at, and nothing of the key or of the server's text.
    let shown = format!(
        "{} {} {:?}",
        serde_json::to_string(&status).unwrap(),
        status.closed_notice().unwrap(),
        kinds(&repository)
    );
    assert!(shown.contains("MODEL_API_KEY"), "{shown}");
    assert!(
        !shown.contains(SECRET) && !shown.contains(SERVER_TEXT),
        "{shown}"
    );
    assert_eq!(
        step(&repository, &mut provider, &policy, t0 + 3599),
        WorkResult::Idle
    );
    assert_eq!(
        server.hits(),
        1,
        "nothing reaches the provider while the gate is closed"
    );

    // The key was replaced meanwhile: the probe finds out and the job is published.
    let result = step(&repository, &mut provider, &policy, t0 + 3600);
    assert!(published(&result), "{result:?}");
    assert_eq!(server.hits(), 2);
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    assert!(server.requests().iter().all(|request| {
        request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {SECRET}"))
    }));
    let status = repository.model_budget_status(&policy, t0 + 3600).unwrap();
    assert_eq!(
        (status.open, status.blocked_by, status.rejected_strikes),
        (true, None, 0)
    );
}
