//! Disposable-PostgreSQL integration tests. Set WAM_TEST_DATABASE_URL and pass `--ignored` to run.

use postgres::{Client, NoTls};
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wam_gatekeeper::postgres_queue::PostgresQueue;
use wam_gatekeeper::postgres_store::PostgresStore;
use wam_gatekeeper::queue::{
    AccountClass, JobKind, JobState, NewJob, Priority, QueueBackend, QueueConfig, QueueError,
};
use wam_gatekeeper::scenario;
use wam_gatekeeper::store::{DecisionCommit, MemoryMutation, Store, StoreError};

struct TestSchema {
    database_url: String,
    schema: String,
}

impl TestSchema {
    fn create() -> Self {
        let database_url = std::env::var("WAM_TEST_DATABASE_URL")
            .expect("set WAM_TEST_DATABASE_URL to a disposable PostgreSQL database");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let schema = format!(
            "wam_gatekeeper_queue_test_{}_{}",
            std::process::id(),
            timestamp
        );
        let mut client =
            Client::connect(&database_url, NoTls).expect("connect disposable database");
        client
            .batch_execute(&format!("CREATE SCHEMA \"{schema}\""))
            .expect("create isolated schema");
        Self {
            database_url,
            schema,
        }
    }

    fn connect(&self) -> Client {
        let mut client = Client::connect(&self.database_url, NoTls).expect("connect test schema");
        client
            .batch_execute(&format!("SET search_path TO \"{}\"", self.schema))
            .expect("select isolated schema");
        client
    }

    fn queue(&self, config: QueueConfig) -> PostgresQueue {
        PostgresQueue::from_client(self.connect(), config).expect("apply Gatekeeper migrations")
    }
}

impl Drop for TestSchema {
    fn drop(&mut self) {
        if let Ok(mut client) = Client::connect(&self.database_url, NoTls) {
            let _ = client.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS \"{}\" CASCADE",
                self.schema
            ));
        }
    }
}

fn job(scope: &str, contributor: &str, nonce: &str, contribution: &str) -> NewJob {
    NewJob {
        kind: JobKind::Contribution,
        scope_key: scope.to_owned(),
        contributor: contributor.to_owned(),
        device: format!("device-{contributor}"),
        class: AccountClass::Established,
        nonce: nonce.to_owned(),
        contribution_id: contribution.to_owned(),
        priority: Priority::P1,
    }
}

#[test]
#[ignore = "requires WAM_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
fn postgres_queue_persists_nonce_scope_lease_and_fencing_across_reopen() {
    let schema = TestSchema::create();
    let config = QueueConfig {
        coalesce_window_secs: 0,
        ..QueueConfig::default()
    };
    let mut store = PostgresStore::from_client(schema.connect()).unwrap();
    let scenario = scenario::builtin("better").unwrap();
    let seeded = scenario.build_store().unwrap();
    let seed = seeded.memories.values().next().unwrap();
    let memory_id = seed.id.clone();
    store
        .apply_decision(DecisionCommit {
            job_id: "queue-test-seed".into(),
            job_digest: "queue-test-seed-v1".into(),
            kind: "fixture_seed".into(),
            reason_codes: vec![],
            confidence_band: Some(seed.confidence_band),
            mutation: MemoryMutation::Create {
                scope: seed.scope.clone(),
                version: seed.versions[0].clone(),
                confidence_band: seed.confidence_band,
            },
            lease_fence: None,
            at: scenario.now,
        })
        .unwrap();
    let mut first = schema.queue(config.clone());
    let original_id = first
        .enqueue(job("site:write", "alice", "nonce-1", "contribution-1"), 0)
        .unwrap();

    let mut second_process = schema.queue(config.clone());
    assert_eq!(
        second_process
            .enqueue(job("different-scope", "alice", "nonce-1", "replay"), 1)
            .unwrap(),
        original_id,
        "the contributor nonce remains idempotent across processes"
    );
    let stale_batch = first.backend_lease("worker-old", 0).unwrap().unwrap();
    assert_eq!(stale_batch.job_ids, [original_id]);

    let late_id = second_process
        .enqueue(job("site:write", "bob", "nonce-2", "contribution-2"), 2)
        .unwrap();
    assert_eq!(
        second_process.job(late_id).unwrap().unwrap().state,
        JobState::WaitingScope
    );
    assert!(
        second_process
            .backend_lease("worker-other", 3)
            .unwrap()
            .is_none()
    );

    drop(first);
    drop(second_process);
    let mut after_restart = schema.queue(config.clone());
    let recovered = after_restart
        .backend_lease("worker-new", 301)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.scope_key, "site:write");
    assert_eq!(recovered.job_ids, [original_id, late_id]);
    assert_eq!(
        after_restart.job(original_id).unwrap().unwrap().attempts,
        1,
        "an expired lease consumes one attempt before recovery"
    );
    assert_eq!(
        after_restart.backend_complete(&stale_batch, 302),
        Err(QueueError::StaleLease),
        "the pre-restart worker cannot commit with the old generation"
    );
    let no_fence = DecisionCommit {
        job_id: "candidate-missing-fence".into(),
        job_digest: "candidate-missing-fence-v1".into(),
        kind: "support".into(),
        reason_codes: vec![],
        confidence_band: None,
        mutation: MemoryMutation::AddSupport {
            memory_id: memory_id.clone(),
            count: 1,
        },
        lease_fence: None,
        at: 302,
    };
    assert_eq!(store.apply_decision(no_fence), Err(StoreError::Conflict));
    assert!(
        !store
            .decided("candidate-missing-fence", "candidate-missing-fence-v1")
            .unwrap()
    );
    let stale_commit = DecisionCommit {
        job_id: "candidate-stale-fence".into(),
        job_digest: "candidate-stale-fence-v1".into(),
        kind: "support".into(),
        reason_codes: vec![],
        confidence_band: None,
        mutation: MemoryMutation::AddSupport {
            memory_id: memory_id.clone(),
            count: 1,
        },
        lease_fence: Some(stale_batch.fence()),
        at: 302,
    };
    assert_eq!(
        store.apply_decision(stale_commit),
        Err(StoreError::Conflict),
        "the memory transaction rejects a lease generation that has been replaced"
    );
    assert!(
        !store
            .decided("candidate-stale-fence", "candidate-stale-fence-v1")
            .unwrap()
    );
    // Hold the same queue row lock as a consent withdrawal while the publisher attempts its decision.
    let application_name = format!("wam_consent_race_{}", std::process::id());
    let mut racing_store_client = schema.connect();
    racing_store_client
        .query_one(
            "SELECT set_config('application_name', $1, false)",
            &[&application_name],
        )
        .unwrap();
    let mut racing_store = PostgresStore::from_client(racing_store_client).unwrap();
    let mut withdrawal_client = schema.connect();
    let mut withdrawal_tx = withdrawal_client.transaction().unwrap();
    assert_eq!(
        withdrawal_tx
            .execute(
                "UPDATE wam_gatekeeper_jobs SET attribution_withdrawn = TRUE WHERE id = $1 AND state = 'leased'",
                &[&i64::try_from(late_id).unwrap()],
            )
            .unwrap(),
        1
    );
    let mut withdrawn_fence = recovered.fence();
    withdrawn_fence.contribution_ids = vec!["contribution-1".into(), "contribution-2".into()];
    withdrawn_fence.require_attribution = true;
    let withdrawn_commit = DecisionCommit {
        job_id: "candidate-withdrawn-fence".into(),
        job_digest: "candidate-withdrawn-fence-v1".into(),
        kind: "support".into(),
        reason_codes: vec![],
        confidence_band: None,
        mutation: MemoryMutation::AddSupport {
            memory_id: memory_id.clone(),
            count: 1,
        },
        lease_fence: Some(withdrawn_fence),
        at: 302,
    };
    let (started_tx, started_rx) = mpsc::channel();
    let publisher = thread::spawn(move || {
        started_tx.send(()).unwrap();
        racing_store.apply_decision(withdrawn_commit)
    });
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("publisher starts its decision attempt");
    let mut monitor = schema.connect();
    let wait_deadline = Instant::now() + Duration::from_secs(3);
    let publisher_blocked_on_withdrawal = loop {
        let waiting: i64 = monitor
            .query_one(
                "SELECT COUNT(*) FROM pg_stat_activity WHERE application_name = $1 AND wait_event_type = 'Lock'",
                &[&application_name],
            )
            .unwrap()
            .get(0);
        if waiting > 0 {
            break true;
        }
        if Instant::now() >= wait_deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(10));
    };
    withdrawal_tx.commit().unwrap();
    assert!(
        publisher_blocked_on_withdrawal,
        "decision transaction must serialize behind the uncommitted withdrawal"
    );
    assert_eq!(
        publisher.join().unwrap(),
        Err(StoreError::Conflict),
        "a withdrawal committed before publish rejects the curator decision"
    );
    assert!(
        !store
            .decided("candidate-withdrawn-fence", "candidate-withdrawn-fence-v1")
            .unwrap()
    );
    assert_eq!(store.get(&memory_id).unwrap().unwrap().support_count, 0);

    let mut current_fence = recovered.fence();
    current_fence.contribution_ids = vec!["contribution-1".into()];
    current_fence.require_attribution = true;
    let valid_commit = DecisionCommit {
        job_id: "candidate-current-fence".into(),
        job_digest: "candidate-current-fence-v1".into(),
        kind: "support".into(),
        reason_codes: vec![],
        confidence_band: None,
        mutation: MemoryMutation::AddSupport {
            memory_id: memory_id.clone(),
            count: 1,
        },
        lease_fence: Some(current_fence),
        at: 302,
    };
    store.apply_decision(valid_commit).unwrap();
    assert_eq!(store.get(&memory_id).unwrap().unwrap().support_count, 1);
    after_restart.backend_complete(&recovered, 303).unwrap();

    drop(after_restart);
    let reopened = schema.queue(config.clone());
    assert_eq!(
        reopened.job(original_id).unwrap().unwrap().state,
        JobState::Done
    );
    assert_eq!(
        reopened.job(late_id).unwrap().unwrap().state,
        JobState::Done
    );
    assert!(matches!(
        PostgresQueue::from_client(
            schema.connect(),
            QueueConfig {
                max_total: config.max_total + 1,
                ..config
            }
        ),
        Err(QueueError::StorageUnavailable)
    ));
}

#[test]
#[ignore = "requires WAM_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
fn postgres_queue_cancels_resolves_purges_and_reports_state() {
    let schema = TestSchema::create();
    let mut queue = schema.queue(QueueConfig {
        coalesce_window_secs: 0,
        ..QueueConfig::default()
    });
    let leased_id = queue
        .enqueue(job("scope-a", "alice", "nonce-a", "contribution-a"), 0)
        .unwrap();
    let leased = queue.backend_lease("worker", 0).unwrap().unwrap();
    let waiting_id = queue
        .enqueue(job("scope-b", "alice", "nonce-b", "contribution-b"), 1)
        .unwrap();
    let active_id = queue
        .enqueue(job("scope-c", "bob", "nonce-c", "contribution-c"), 2)
        .unwrap();

    assert_eq!(
        queue.cancel_contribution("contribution-b", 3).unwrap(),
        wam_gatekeeper::queue::CancelReport {
            canceled: 1,
            still_leased: 0,
        }
    );
    assert_eq!(
        queue.cancel_contributor("alice", 4).unwrap(),
        wam_gatekeeper::queue::CancelReport {
            canceled: 0,
            still_leased: 1,
        }
    );
    assert!(queue.job(leased_id).unwrap().unwrap().attribution_withdrawn);
    assert_eq!(
        queue.job(waiting_id).unwrap().unwrap().state,
        JobState::Canceled
    );
    let stats = queue.stats(10).unwrap();
    assert_eq!((stats.active, stats.oldest_age_secs), (2, 8));

    assert_eq!(
        queue
            .resolve_contribution("contribution-a", true, 10)
            .unwrap(),
        1
    );
    assert_eq!(
        queue.backend_complete(&leased, 11),
        Err(QueueError::StaleLease)
    );
    assert_eq!(queue.stats(11).unwrap().active, 1);
    assert_eq!(queue.purge_finished(9, 10).unwrap(), 0);
    assert_eq!(queue.purge_finished(11, 0).unwrap(), 2);
    assert!(queue.job(leased_id).unwrap().is_none());
    assert!(queue.job(waiting_id).unwrap().is_none());
    assert_eq!(
        queue.job(active_id).unwrap().unwrap().state,
        JobState::Queued
    );
    let replay_after_purge = queue
        .enqueue(job("scope-a", "alice", "nonce-a", "contribution-a"), 12)
        .unwrap();
    assert_ne!(replay_after_purge, leased_id);
}

#[test]
#[ignore = "requires WAM_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
fn postgres_queue_two_workers_cannot_claim_the_same_scope() {
    let schema = TestSchema::create();
    let config = QueueConfig {
        coalesce_window_secs: 0,
        ..QueueConfig::default()
    };
    let queue = schema.queue(config.clone());
    queue
        .enqueue(job("same-scope", "alice", "nonce", "contribution"), 0)
        .unwrap();
    let mut maintenance = job("same-scope", "system", "reverify", "reverify-source");
    maintenance.kind = JobKind::Reverify;
    maintenance.priority = Priority::P0;
    queue.enqueue(maintenance, 0).unwrap();
    drop(queue);

    let barrier = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for worker_name in ["worker-a", "worker-b"] {
        let database_url = schema.database_url.clone();
        let schema_name = schema.schema.clone();
        let config = config.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let mut client = Client::connect(&database_url, NoTls).unwrap();
            client
                .batch_execute(&format!("SET search_path TO \"{schema_name}\""))
                .unwrap();
            let mut queue = PostgresQueue::from_client(client, config).unwrap();
            barrier.wait();
            queue.backend_lease(worker_name, 0).unwrap()
        }));
    }
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);
    assert!(
        results
            .iter()
            .flatten()
            .all(|batch| batch.scope_key == "same-scope" && batch.job_ids.len() == 1),
        "one scope claim includes only one job kind"
    );
}
