//! The job queue's policies (docs/test QUE-02..QUE-12, QUE-14 in memory). QUE-01 (atomic accept) and QUE-13 (the
//! dashboard state) need Postgres and the web app; the worker side of QUE-09 is in tests/worker.rs.

use std::collections::BTreeMap;
use wam_gatekeeper::queue::{
    AccountClass, Batch, EnqueueError, JobKind, JobQueue, JobState, NewJob, Priority, QueueConfig,
    QueueError,
};

fn cfg() -> QueueConfig {
    QueueConfig {
        coalesce_window_secs: 0,
        ..QueueConfig::default()
    }
}

fn job(scope: &str, who: &str, nonce: &str) -> NewJob {
    NewJob {
        kind: JobKind::Contribution,
        scope_key: scope.to_owned(),
        contributor: who.to_owned(),
        device: format!("device-{who}"),
        class: AccountClass::Established,
        nonce: nonce.to_owned(),
        contribution_id: format!("c-{who}-{nonce}"),
        priority: Priority::P1,
    }
}

fn safety(scope: &str, kind: JobKind, nonce: &str) -> NewJob {
    NewJob {
        kind,
        priority: Priority::P0,
        ..job(scope, "system", nonce)
    }
}

#[test]
fn que_02_a_scope_is_held_by_one_worker_at_a_time() {
    let mut q = JobQueue::new(cfg());
    q.enqueue(job("a", "u1", "1"), 0).unwrap();
    let first = q.lease("w1", 0).unwrap();
    let late = q.enqueue(job("a", "u2", "2"), 1).unwrap();
    assert_eq!(
        q.job(late).unwrap().state,
        JobState::WaitingScope,
        "visible as soon as it is accepted"
    );
    assert!(q.lease("w2", 1).is_none(), "scope a is held");
    q.enqueue(job("b", "u3", "3"), 1).unwrap();
    assert_eq!(
        q.lease("w2", 1).unwrap().scope_key,
        "b",
        "other scopes run in parallel"
    );
    q.complete(&first, 2).unwrap();
    let next = q.lease("w3", 2).unwrap();
    assert_eq!(
        (next.scope_key.as_str(), next.job_ids.clone()),
        ("a", vec![late])
    );
}

#[test]
fn que_03_same_scope_jobs_are_judged_together_up_to_the_limit() {
    let mut q = JobQueue::new(cfg());
    for n in 0..25 {
        q.enqueue(job("a", &format!("u{n}"), "x"), 0).unwrap();
    }
    let first = q.lease("w", 0).unwrap();
    assert_eq!(first.job_ids.len(), 20);
    q.complete(&first, 1).unwrap();
    assert_eq!(q.lease("w", 1).unwrap().job_ids.len(), 5);
}

#[test]
fn same_scope_batches_do_not_mix_contribution_and_maintenance_triggers() {
    let mut q = JobQueue::new(cfg());
    let contribution = q
        .enqueue(job("same-scope", "alice", "contribution"), 0)
        .unwrap();
    let maintenance = q
        .enqueue(safety("same-scope", JobKind::Reverify, "reverify"), 0)
        .unwrap();

    let first = q.lease("worker", 0).unwrap();
    assert_eq!(first.job_ids.len(), 1);
    q.complete(&first, 1).unwrap();
    let second = q.lease("worker", 1).unwrap();
    assert_eq!(second.job_ids.len(), 1);
    assert_ne!(first.job_ids, second.job_ids);
    assert_ne!(
        q.job(first.job_ids[0]).unwrap().kind,
        q.job(second.job_ids[0]).unwrap().kind
    );
    assert_eq!(q.job(first.job_ids[0]).unwrap().state, JobState::Done);
    assert_eq!(q.job(second.job_ids[0]).unwrap().state, JobState::Leased);
    assert!([contribution, maintenance].contains(&first.job_ids[0]));
    assert!([contribution, maintenance].contains(&second.job_ids[0]));
}

#[test]
fn que_03_the_window_opens_with_the_first_job_and_late_arrivals_join_its_batch() {
    let mut q = JobQueue::new(QueueConfig {
        coalesce_window_secs: 60,
        ..QueueConfig::default()
    });
    q.enqueue(job("a", "u1", "1"), 0).unwrap();
    assert!(q.lease("w", 30).is_none(), "still inside the window");
    q.enqueue(job("a", "u2", "2"), 40).unwrap();
    q.enqueue(job("other", "u3", "3"), 50).unwrap();
    let batch = q.lease("w", 60).unwrap();
    assert_eq!(
        (batch.scope_key.as_str(), batch.job_ids.len()),
        ("a", 2),
        "the late arrival joined the same pass"
    );
    assert!(
        q.lease("w2", 60).is_none(),
        "the other scope's own window is not over"
    );
    assert_eq!(q.lease("w2", 110).unwrap().scope_key, "other");
}

#[test]
fn que_03_a_full_batch_does_not_wait_for_the_window() {
    let mut q = JobQueue::new(QueueConfig {
        coalesce_window_secs: 60,
        ..QueueConfig::default()
    });
    for n in 0..20 {
        q.enqueue(job("a", &format!("u{n}"), "x"), 0).unwrap();
    }
    assert_eq!(q.lease("w", 1).unwrap().job_ids.len(), 20);
}

#[test]
fn que_04_a_full_queue_refuses_new_work_and_never_drops_accepted_work() {
    let mut q = JobQueue::new(QueueConfig {
        max_total: 10,
        max_per_contributor: 4,
        ..cfg()
    });
    for n in 0..4 {
        q.enqueue(job(&format!("s{n}"), "a", &n.to_string()), 0)
            .unwrap();
    }
    let over = q.enqueue(job("s9", "a", "9"), 0).unwrap_err();
    assert_eq!((over.http_status(), over.retry_after_secs()), (429, 30));
    for who in ["b", "c"] {
        for n in 0..2 {
            q.enqueue(job(&format!("{who}{n}"), who, &n.to_string()), 0)
                .unwrap();
        }
    }
    // 8 of 10 used: new accounts are limited first, established ones still fit.
    let mut newcomer = job("n", "newbie", "1");
    newcomer.class = AccountClass::New;
    let shed = q.enqueue(newcomer, 0).unwrap_err();
    assert_eq!(
        (&shed, shed.http_status()),
        (
            &EnqueueError::Shedding {
                retry_after_secs: 30
            },
            429
        )
    );
    q.enqueue(job("e1", "d", "1"), 0).unwrap();
    q.enqueue(job("e2", "d", "2"), 0).unwrap();
    let full = q.enqueue(job("e3", "e", "1"), 0).unwrap_err();
    assert_eq!(
        (&full, full.http_status()),
        (
            &EnqueueError::QueueFull {
                retry_after_secs: 30
            },
            503
        )
    );
    // Reverify and rollback work still gets in, up to 120%.
    q.enqueue(safety("r1", JobKind::Reverify, "1"), 0).unwrap();
    q.enqueue(safety("r2", JobKind::Rollback, "2"), 0).unwrap();
    assert!(
        q.enqueue(safety("r3", JobKind::Reverify, "3"), 0).is_err(),
        "even safety work stops at 120%"
    );
    // Everything that was accepted is still there and gets processed.
    let mut done = 0;
    while let Some(batch) = q.lease("w", 1) {
        done += batch.job_ids.len();
        q.complete(&batch, 2).unwrap();
    }
    assert_eq!(done, 12);
}

#[test]
fn que_04_one_device_cannot_fill_the_queue_with_many_accounts() {
    let mut q = JobQueue::new(QueueConfig {
        max_per_device: 3,
        ..cfg()
    });
    for n in 0..3 {
        let mut new = job(&format!("s{n}"), &format!("acct{n}"), "1");
        new.device = "one-laptop".to_owned();
        q.enqueue(new, 0).unwrap();
    }
    let mut fourth = job("s3", "acct3", "1");
    fourth.device = "one-laptop".to_owned();
    assert!(matches!(
        q.enqueue(fourth, 0),
        Err(EnqueueError::DeviceLimit { .. })
    ));
    q.enqueue(job("s4", "honest", "1"), 0).unwrap();
}

#[test]
fn que_04_resubmitting_the_same_nonce_returns_the_same_job() {
    let mut q = JobQueue::new(cfg());
    let a = q.enqueue(job("a", "u", "n1"), 0).unwrap();
    assert_eq!(q.enqueue(job("a", "u", "n1"), 5).unwrap(), a);
    assert_eq!(q.jobs().count(), 1);
}

#[test]
fn que_05_a_flood_from_one_contributor_does_not_starve_the_others() {
    let mut q = JobQueue::new(cfg());
    for n in 0..30 {
        q.enqueue(job(&format!("flood{n}"), "spammer", &n.to_string()), 0)
            .unwrap();
    }
    q.enqueue(job("calm1", "alice", "1"), 1).unwrap();
    q.enqueue(job("calm2", "bob", "1"), 1).unwrap();
    let mut order = Vec::new();
    for _ in 0..4 {
        let batch = q.lease("w", 2).unwrap();
        order.push(q.job(batch.job_ids[0]).unwrap().contributor.clone());
        q.complete(&batch, 2).unwrap();
    }
    assert!(
        order[..3].contains(&"alice".to_owned()) && order[..3].contains(&"bob".to_owned()),
        "{order:?}"
    );
}

#[test]
fn que_05_everyone_is_served_in_turn_even_after_a_long_quiet_stretch() {
    let mut q = JobQueue::new(cfg());
    for round in 0..3 {
        for who in ["a", "b"] {
            q.enqueue(job(&format!("{who}{round}"), who, &round.to_string()), 0)
                .unwrap();
        }
    }
    let mut order = String::new();
    while let Some(batch) = q.lease("w", 1) {
        order.push_str(&q.job(batch.job_ids[0]).unwrap().contributor);
        q.complete(&batch, 1).unwrap();
    }
    assert_eq!(order, "ababab", "round robin, not first-come-first-served");
}

#[test]
fn que_11_safety_work_goes_before_routine_work() {
    let mut q = JobQueue::new(cfg());
    q.enqueue(job("routine", "u", "1"), 0).unwrap();
    q.enqueue(safety("urgent", JobKind::Rollback, "1"), 5)
        .unwrap();
    assert_eq!(q.lease("w", 6).unwrap().scope_key, "urgent");
}

#[test]
fn que_11_urgent_work_does_not_wait_for_the_coalescing_window() {
    let mut q = JobQueue::new(QueueConfig {
        coalesce_window_secs: 60,
        ..QueueConfig::default()
    });
    q.enqueue(safety("urgent", JobKind::Reverify, "1"), 0)
        .unwrap();
    assert_eq!(q.lease("w", 0).unwrap().scope_key, "urgent");
}

#[test]
fn que_06_failures_back_off_then_dead_letter_and_never_run_early() {
    let mut q = JobQueue::new(cfg());
    let id = q.enqueue(job("a", "u", "1"), 0).unwrap();
    let mut now = 0;
    let mut delays = Vec::new();
    for attempt in 1..=5 {
        let batch = q.lease("w", now).unwrap();
        q.fail(&batch, now, "model timeout").unwrap();
        let state = q.job(id).unwrap().state;
        if attempt < 5 {
            assert_eq!(state, JobState::Retry);
            let wait = q.job(id).unwrap().not_before - now;
            delays.push(wait);
            assert!(
                q.lease("w", now + wait - 1).is_none(),
                "attempt {attempt}: too early"
            );
            now += wait;
        } else {
            assert_eq!(state, JobState::Dead);
        }
    }
    assert!(
        delays.windows(2).all(|w| w[1] > w[0]),
        "exponential: {delays:?}"
    );
    assert!(
        delays.iter().all(|d| *d <= 1800 + 900),
        "capped with jitter: {delays:?}"
    );
    assert_eq!(
        q.job(id).unwrap().last_error.as_deref(),
        Some("model timeout")
    );
    assert!(
        q.lease("w", now + 10_000).is_none(),
        "a dead job is not retried"
    );
}

#[test]
fn que_06_retries_of_different_jobs_do_not_line_up() {
    let mut q = JobQueue::new(cfg());
    let ids: Vec<u64> = (0..6)
        .map(|n| {
            q.enqueue(job(&format!("s{n}"), "u", &n.to_string()), 0)
                .unwrap()
        })
        .collect();
    while let Some(batch) = q.lease("w", 0) {
        q.fail(&batch, 0, "boom").unwrap();
    }
    let waits: std::collections::BTreeSet<u64> = ids
        .iter()
        .map(|id| q.job(*id).unwrap().not_before)
        .collect();
    assert!(waits.len() > 1, "jitter spreads the retries: {waits:?}");
}

#[test]
fn que_07_a_crashed_worker_loses_its_lease_and_cannot_commit_late() {
    let mut q = JobQueue::new(cfg());
    let id = q.enqueue(job("a", "u", "1"), 0).unwrap();
    let mut old = q.lease("w1", 0).unwrap();
    assert_eq!(
        q.heartbeat(&mut old, 200).unwrap(),
        500,
        "a heartbeat extends the lease"
    );
    assert_eq!(old.lease_expires_at, 500);
    let new = q.lease("w2", 501).unwrap();
    assert_eq!(new.job_ids, vec![id]);
    assert_eq!(q.job(id).unwrap().attempts, 1);
    assert!(
        new.generations[0] > old.generations[0],
        "fencing token grows"
    );
    assert_eq!(q.complete(&old, 502), Err(QueueError::StaleLease));
    assert_eq!(q.fail(&old, 502, "late"), Err(QueueError::StaleLease));
    assert_eq!(q.wait_budget(&old), Err(QueueError::StaleLease));
    let mut stale = old.clone();
    assert_eq!(q.heartbeat(&mut stale, 502), Err(QueueError::StaleLease));
    q.complete(&new, 503).unwrap();
    assert_eq!(q.job(id).unwrap().state, JobState::Done);
    assert!(q.lease("w3", 600).is_none(), "done once, never again");
}

#[test]
fn que_07_a_worker_that_took_the_job_back_under_the_same_name_is_still_fenced() {
    let mut q = JobQueue::new(cfg());
    q.enqueue(job("a", "u", "1"), 0).unwrap();
    let before_restart = q.lease("worker-1", 0).unwrap();
    let after_restart = q.lease("worker-1", 301).unwrap();
    assert_eq!(
        q.complete(&before_restart, 302),
        Err(QueueError::StaleLease)
    );
    q.complete(&after_restart, 302).unwrap();
}

#[test]
fn que_07_a_slow_worker_nobody_replaced_may_still_finish() {
    let mut q = JobQueue::new(cfg());
    let id = q.enqueue(job("a", "u", "1"), 0).unwrap();
    let batch = q.lease("w1", 0).unwrap();
    q.complete(&batch, 400).unwrap();
    assert_eq!(
        (q.job(id).unwrap().state, q.job(id).unwrap().attempts),
        (JobState::Done, 0)
    );
}

#[test]
fn que_07_a_job_that_keeps_killing_its_worker_ends_up_dead() {
    let mut q = JobQueue::new(cfg());
    let id = q.enqueue(job("poison", "u", "1"), 0).unwrap();
    let mut now = 0;
    for _ in 0..5 {
        assert!(q.lease("w", now).is_some());
        now += 301;
    }
    assert!(q.lease("w", now).is_none());
    let j = q.job(id).unwrap();
    assert_eq!(
        (j.state, j.attempts, j.last_error.as_deref()),
        (JobState::Dead, 5, Some("lease expired"))
    );
}

#[test]
fn que_08_an_exhausted_model_budget_parks_jobs_without_losing_or_charging_them() {
    let mut q = JobQueue::new(cfg());
    let id = q.enqueue(job("a", "u", "1"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    q.wait_budget(&batch).unwrap();
    assert_eq!(q.job(id).unwrap().state, JobState::WaitingBudget);
    assert_eq!(q.job(id).unwrap().attempts, 0);
    assert!(
        q.lease("w", 100_000).is_none(),
        "parked until the budget is back"
    );
    assert_eq!(q.release_budget(100_001), 1);
    let again = q.lease("w", 100_001).unwrap();
    q.complete(&again, 100_002).unwrap();
}

#[test]
fn que_10_withdrawal_cancels_waiting_jobs_and_strips_attribution_from_running_ones() {
    let mut q = JobQueue::new(cfg());
    let running = q.enqueue(job("a", "alice", "1"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    let waiting = q.enqueue(job("a", "alice", "2"), 1).unwrap();
    let other_scope = q.enqueue(job("b", "alice", "3"), 1).unwrap();
    let other_user = q.enqueue(job("b", "bob", "1"), 1).unwrap();
    let report = q.cancel_contributor("alice", 2);
    assert_eq!((report.canceled, report.still_leased), (2, 1));
    for id in [waiting, other_scope] {
        let j = q.job(id).unwrap();
        assert_eq!(
            (j.state, j.contribution_id.clone()),
            (JobState::Canceled, None),
            "payload reference purged"
        );
    }
    assert!(q.job(running).unwrap().attribution_withdrawn);
    assert_eq!(
        q.job(other_user).unwrap().state,
        JobState::Queued,
        "other contributors are untouched"
    );
    q.complete(&batch, 3).unwrap();
}

#[test]
fn que_10_a_delayed_replay_of_a_withdrawn_request_does_not_bring_the_job_back() {
    let mut q = JobQueue::new(cfg());
    let id = q.enqueue(job("a", "alice", "1"), 0).unwrap();
    q.cancel_contributor("alice", 1);
    assert_eq!(
        q.enqueue(job("a", "alice", "1"), 2).unwrap(),
        id,
        "same nonce: same (canceled) job"
    );
    assert_eq!(q.job(id).unwrap().state, JobState::Canceled);
    assert!(q.lease("w", 3).is_none());
    let fresh = q.enqueue(job("a", "alice", "2"), 4).unwrap();
    assert_ne!(fresh, id, "a new request is a new job");
}

#[test]
fn que_10_withdrawal_reaches_parked_and_retrying_jobs_too() {
    let mut q = JobQueue::new(cfg());
    let parked = q.enqueue(job("a", "alice", "1"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    q.wait_budget(&batch).unwrap();
    let retrying = q.enqueue(job("b", "alice", "2"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    q.fail(&batch, 0, "boom").unwrap();
    assert_eq!(q.cancel_contributor("alice", 1).canceled, 2);
    for id in [parked, retrying] {
        assert_eq!(q.job(id).unwrap().state, JobState::Canceled);
    }
}

#[test]
fn que_10_withdrawing_one_contribution_does_not_cancel_the_contributors_other_jobs() {
    let mut q = JobQueue::new(cfg());
    let running = q.enqueue(job("a", "alice", "1"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    let waiting = q.enqueue(job("a", "alice", "2"), 1).unwrap();
    let other_user = q.enqueue(job("b", "bob", "1"), 1).unwrap();

    let report = q.cancel_contribution("c-alice-1", 2);
    assert_eq!((report.canceled, report.still_leased), (0, 1));
    assert!(q.job(running).unwrap().attribution_withdrawn);
    assert_eq!(q.job(waiting).unwrap().state, JobState::WaitingScope);
    assert_eq!(q.job(other_user).unwrap().state, JobState::Queued);

    let report = q.cancel_contribution("c-alice-2", 2);
    assert_eq!((report.canceled, report.still_leased), (1, 0));
    assert_eq!(q.job(waiting).unwrap().state, JobState::Canceled);
    assert_eq!(q.job(other_user).unwrap().state, JobState::Queued);
    q.complete(&batch, 3).unwrap();
}

#[test]
fn que_10_review_decision_resolves_only_its_job_and_fences_a_leased_worker() {
    let mut q = JobQueue::new(cfg());
    let reviewed = q.enqueue(job("a", "alice", "1"), 0).unwrap();
    let batch = q.lease("worker", 0).unwrap();
    let other = q.enqueue(job("b", "bob", "1"), 1).unwrap();

    assert_eq!(q.resolve_contribution("c-alice-1", true, 2), 1);
    assert_eq!(q.job(reviewed).unwrap().state, JobState::Done);
    assert_eq!(q.job(other).unwrap().state, JobState::Queued);
    assert_eq!(q.complete(&batch, 3), Err(QueueError::StaleLease));
}

#[test]
fn que_12_the_queue_reports_depth_and_age_and_alerts_when_work_is_stuck() {
    let mut q = JobQueue::new(cfg());
    q.enqueue(job("a", "u", "1"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    q.wait_budget(&batch).unwrap();
    let ok = q.stats(600);
    assert_eq!((ok.active, ok.oldest_age_secs, ok.alert), (1, 600, false));
    let stuck = q.stats(901);
    assert!(stuck.alert);
    assert_eq!(stuck.by_state[&JobState::WaitingBudget], 1);
}

/// 10 jobs/minute for 5 minutes on a handful of sites, two workers, 20 s per model pass (docs/test/metrics.md).
#[test]
fn que_12_load_at_the_planned_arrival_rate_stays_inside_the_latency_budget() {
    let mut q = JobQueue::new(QueueConfig {
        max_total: 1000,
        ..QueueConfig::default()
    });
    let service_secs = 20;
    let mut busy: Vec<Option<(u64, Batch)>> = vec![None, None];
    let mut done_at: BTreeMap<u64, u64> = BTreeMap::new();
    let mut enqueued_at: BTreeMap<u64, u64> = BTreeMap::new();
    let mut max_depth = 0;
    for now in 0..2400u64 {
        if now < 300 && now % 6 == 0 {
            let n = now / 6;
            let who = format!("user{}", n % 17);
            let id = q
                .enqueue(job(&format!("site{}", n % 5), &who, &n.to_string()), now)
                .unwrap();
            enqueued_at.insert(id, now);
        }
        for (w, slot) in busy.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(|(finish, _)| *finish <= now) {
                let (_, batch) = slot.take().unwrap();
                q.complete(&batch, now).unwrap();
                for id in &batch.job_ids {
                    done_at.insert(*id, now);
                }
            }
            if slot.is_none() {
                if let Some(batch) = q.lease(&format!("w{w}"), now) {
                    *slot = Some((now + service_secs, batch));
                }
            }
        }
        max_depth = max_depth.max(q.stats(now).active);
    }
    assert_eq!(
        done_at.len(),
        enqueued_at.len(),
        "nothing lost, nothing stuck"
    );
    let mut latencies: Vec<u64> = enqueued_at
        .iter()
        .map(|(id, at)| done_at[id] - at)
        .collect();
    latencies.sort_unstable();
    let p95 = latencies[latencies.len() * 95 / 100];
    assert!(
        p95 <= 600,
        "p95 {p95}s over the 10 minute budget; depth peaked at {max_depth}"
    );
    assert!(q.stats(2400).oldest_age_secs == 0 && q.stats(2400).active == 0);
}

#[test]
fn que_14_purge_drops_only_old_finished_jobs_and_their_idempotency_keys() {
    let mut q = JobQueue::new(cfg());
    let finished = q.enqueue(job("a", "u", "1"), 0).unwrap();
    q.enqueue(job("b", "u", "2"), 0).unwrap();
    let batch = q.lease("w", 0).unwrap();
    assert_eq!(batch.job_ids, vec![finished]);
    q.complete(&batch, 10).unwrap();
    assert_eq!(q.purge_finished(100, 1000), 0);
    assert_eq!(q.purge_finished(1010, 1000), 1);
    assert_eq!(q.jobs().count(), 1, "the unfinished job stays");
    assert_ne!(
        q.enqueue(job("a", "u", "1"), 1011).unwrap(),
        finished,
        "the old key is gone with the job"
    );
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn que_random_operation_sequences_never_break_the_queue() {
    for seed in 1..=80u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut q = JobQueue::new(QueueConfig {
            max_total: 40,
            max_per_contributor: 8,
            max_per_device: 8,
            lease_secs: 50,
            coalesce_window_secs: rng.below(30),
            ..cfg()
        });
        let mut batches: Vec<Option<Batch>> = vec![None, None, None];
        let mut now = 0u64;
        let mut accepted = 0usize;
        let mut finished: BTreeMap<u64, JobState> = BTreeMap::new();
        let mut generations: BTreeMap<u64, u64> = BTreeMap::new();
        for step in 0..300 {
            now += rng.below(20);
            match rng.below(9) {
                0..=2 => {
                    let mut new = job(
                        &format!("s{}", rng.below(5)),
                        &format!("u{}", rng.below(4)),
                        &step.to_string(),
                    );
                    new.class = if rng.below(3) == 0 {
                        AccountClass::New
                    } else {
                        AccountClass::Established
                    };
                    if rng.below(8) == 0 {
                        new.priority = Priority::P0;
                    }
                    if q.enqueue(new, now).is_ok() {
                        accepted += 1;
                    }
                }
                3 | 4 => {
                    let w = rng.below(3) as usize;
                    if batches[w].is_none() {
                        batches[w] = q.lease(&format!("w{w}"), now);
                    }
                }
                5 => {
                    let w = rng.below(3) as usize;
                    if let Some(batch) = batches[w].take() {
                        let _ = q.complete(&batch, now);
                    }
                }
                6 => {
                    let w = rng.below(3) as usize;
                    if let Some(batch) = batches[w].take() {
                        let _ = if rng.below(2) == 0 {
                            q.fail(&batch, now, "boom")
                        } else {
                            q.wait_budget(&batch)
                        };
                    }
                }
                7 => {
                    let w = rng.below(3) as usize;
                    if let Some(batch) = batches[w].as_mut() {
                        let _ = q.heartbeat(batch, now);
                    }
                    if rng.below(5) == 0 {
                        q.release_budget(now);
                    }
                }
                _ => {
                    if rng.below(6) == 0 {
                        q.cancel_contributor(&format!("u{}", rng.below(4)), now);
                    }
                }
            }
            let mut holders: BTreeMap<&str, &str> = BTreeMap::new();
            let mut total = 0;
            for j in q.jobs() {
                total += 1;
                assert!(
                    j.attempts <= q.config().max_attempts,
                    "{seed}/{step}: attempts"
                );
                if let Some(old) = finished.get(&j.id) {
                    assert_eq!(*old, j.state, "{seed}/{step}: a finished job changed state");
                }
                if j.state.terminal() {
                    finished.insert(j.id, j.state);
                }
                let seen = generations.insert(j.id, j.lease_generation).unwrap_or(0);
                assert!(
                    j.lease_generation >= seen,
                    "{seed}/{step}: the fencing token went backwards"
                );
                if j.state == JobState::Leased {
                    let holder = j
                        .lease_holder
                        .as_deref()
                        .expect("a leased job has a holder");
                    let previous = holders.insert(j.scope_key.as_str(), holder);
                    assert!(
                        previous.is_none_or(|p| p == holder),
                        "{seed}/{step}: two workers hold scope {}",
                        j.scope_key
                    );
                } else {
                    assert!(
                        j.lease_holder.is_none(),
                        "{seed}/{step}: only leased jobs have a holder"
                    );
                }
                if j.state == JobState::Canceled {
                    assert!(
                        j.contribution_id.is_none(),
                        "{seed}/{step}: a canceled job kept its payload reference"
                    );
                }
            }
            assert_eq!(
                total, accepted,
                "{seed}/{step}: an accepted job disappeared"
            );
        }
    }
}
