//! Disposable-PostgreSQL integration tests. Set WAM_TEST_DATABASE_URL and pass `--ignored` to run.

use postgres::{Client, NoTls};
use std::time::{SystemTime, UNIX_EPOCH};
use wam_gatekeeper::curator::{CuratorConfig, Outcome, run_job};
use wam_gatekeeper::postgres_store::PostgresStore;
use wam_gatekeeper::provider::{ScriptStep, ScriptedProvider};
use wam_gatekeeper::scenario;
use wam_gatekeeper::store::{DecisionCommit, MemoryMutation, Store, StoreError};
use wam_gatekeeper::tools::{Policy, Tools};

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
        let schema = format!("wam_gatekeeper_test_{}_{}", std::process::id(), timestamp);
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

    fn store(&self) -> PostgresStore {
        PostgresStore::from_client(self.connect()).expect("apply Gatekeeper migrations")
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

#[test]
#[ignore = "requires WAM_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
fn postgres_store_commits_supersede_and_decision_idempotency_across_reopen() {
    let schema = TestSchema::create();
    let mut store = schema.store();
    let scenario = scenario::builtin("better").unwrap();
    let seeded = scenario.build_store().unwrap();
    let seed = seeded.memories.values().next().unwrap();
    let version = seed.versions.first().unwrap().clone();
    store
        .apply_decision(DecisionCommit {
            job_id: "fixture-seed".into(),
            job_digest: "fixture-seed-v1".into(),
            kind: "fixture_seed".into(),
            reason_codes: vec![],
            confidence_band: Some(seed.confidence_band),
            lease_fence: None,
            mutation: MemoryMutation::Create {
                scope: seed.scope.clone(),
                version,
                confidence_band: seed.confidence_band,
            },
            at: scenario.now,
        })
        .unwrap();

    let tools = Tools::load().unwrap();
    let tasks = scenario.tasks().unwrap();
    let mut provider = ScriptedProvider::new(
        scenario
            .script
            .iter()
            .map(|call| ScriptStep::Call(call.tool.clone(), call.args.clone())),
    );
    let report = run_job(
        &scenario.job,
        &mut store,
        &mut provider,
        &tools,
        &Policy::default(),
        &tasks,
        &CuratorConfig::default(),
        scenario.now,
    );
    assert_eq!(report.outcome, Outcome::Decided);
    assert_eq!(report.decision.as_ref().unwrap().version, Some(2));
    let before = store.get("web:medium-com-write-post").unwrap().unwrap();
    assert_eq!(before.versions.len(), 2);
    assert_eq!(
        before.versions[0].status,
        wam_gatekeeper::store::Status::Superseded
    );
    assert!(
        store
            .decided(&scenario.job.id, &scenario.job.digest())
            .unwrap()
    );
    assert_eq!(store.audit_count(&scenario.job.id).unwrap(), 1);

    let replay = store
        .apply_decision(DecisionCommit {
            job_id: scenario.job.id.clone(),
            job_digest: scenario.job.digest(),
            kind: "supersede".into(),
            reason_codes: report.decision.as_ref().unwrap().reason_codes.clone(),
            confidence_band: Some(1),
            lease_fence: None,
            mutation: MemoryMutation::Supersede {
                memory_id: before.id.clone(),
                expected_version: 1,
                next: before.versions[1].clone(),
                confidence_band: 1,
            },
            at: scenario.now,
        })
        .unwrap();
    assert_eq!(replay.memory_id.as_deref(), Some(before.id.as_str()));
    assert_eq!(replay.version, Some(2));
    assert_eq!(store.get(&before.id).unwrap().unwrap().versions.len(), 2);
    assert_eq!(store.audit_count(&scenario.job.id).unwrap(), 1);

    let mut stale_next = before.active().unwrap().clone();
    stale_next.version = 0;
    let stale = store.apply_decision(DecisionCommit {
        job_id: "stale-worker".into(),
        job_digest: "stale-worker-v1".into(),
        kind: "supersede".into(),
        reason_codes: vec!["works_after_ui_change".into()],
        confidence_band: Some(1),
        lease_fence: None,
        mutation: MemoryMutation::Supersede {
            memory_id: before.id.clone(),
            expected_version: 1,
            next: stale_next,
            confidence_band: 1,
        },
        at: scenario.now + 1,
    });
    assert_eq!(stale, Err(StoreError::Conflict));
    assert!(!store.decided("stale-worker", "stale-worker-v1").unwrap());
    assert_eq!(store.get(&before.id).unwrap().unwrap().versions.len(), 2);

    drop(store);
    let reopened = schema.store();
    let after = reopened.get(&before.id).unwrap().unwrap();
    assert_eq!(after.versions.len(), 2);
    assert_eq!(after.active().unwrap().version, 2);
    assert!(
        reopened
            .decided(&scenario.job.id, &scenario.job.digest())
            .unwrap()
    );
    assert_eq!(reopened.audit_count(&scenario.job.id).unwrap(), 1);
}
