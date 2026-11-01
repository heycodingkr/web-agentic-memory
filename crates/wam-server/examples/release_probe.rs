//! Synthetic-only application/Redis oracles for the owned native release runner.
#[path = "../tests/lifecycle_support/mod.rs"]
mod fixture;
mod release_support;
use release_support::{database, owned_root};

use fixture::*;
use postgres::Client;
use serde_json::{Value, json};
use std::{collections::BTreeMap, env, io::Write, sync::Arc};
use wam_contracts::{MemoryOutcome, OutcomeReportRequest, RecallRequest};
use wam_gatekeeper::record::sha256_hex;
use wam_server::{
    ApplicationRepository, FileIdentityStore, IdentityService, RateLimitStore, RedisRateLimitStore,
    WamService,
    identity::DevicePoll,
    ports::Repository,
    rate_limit::{BucketSpec, UsageRecord},
};

fn snapshot(client: &mut Client) -> Value {
    let mut counts = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    let tables: Vec<String> = client.query(
        "SELECT table_name FROM information_schema.tables WHERE table_schema=current_schema() AND table_type='BASE TABLE' ORDER BY table_name", &[])
        .unwrap().iter().map(|r| r.get(0)).collect();
    for table in tables {
        assert!(
            table
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        );
        let rows: Vec<String> = client
            .query(
                &format!("SELECT to_jsonb(t)::text FROM \"{table}\" t ORDER BY 1"),
                &[],
            )
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        counts.insert(table.clone(), rows.len());
        hashes.insert(table, sha256_hex(&serde_json::to_vec(&rows).unwrap()));
    }
    let mut schema = BTreeMap::new();
    for (name, sql) in [
        (
            "constraints",
            "SELECT conname||':'||pg_get_constraintdef(oid) FROM pg_constraint WHERE connamespace=current_schema()::regnamespace ORDER BY 1",
        ),
        (
            "indexes",
            "SELECT indexdef FROM pg_indexes WHERE schemaname=current_schema() ORDER BY 1",
        ),
        (
            "triggers",
            "SELECT pg_get_triggerdef(t.oid) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid WHERE c.relnamespace=current_schema()::regnamespace AND NOT t.tgisinternal ORDER BY 1",
        ),
    ] {
        let rows: Vec<String> = client
            .query(sql, &[])
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        schema.insert(name, sha256_hex(&serde_json::to_vec(&rows).unwrap()));
    }
    json!({"counts":counts,"table_hashes":hashes,"schema_hashes":schema})
}

fn identity(repository: &ApplicationRepository) -> IdentityService {
    IdentityService::new(
        Arc::new(FileIdentityStore::from_repository(repository.clone())),
        None,
        "http://127.0.0.1",
        false,
    )
}

fn device_fixture(repository: &ApplicationRepository, account: &str, file: &str) {
    repository.ensure_account(account).unwrap();
    let identity = identity(repository);
    let start = identity
        .start_device(
            "release-owned-fixture",
            vec![
                "read:memories".into(),
                "read:dashboard".into(),
                "write:contributions".into(),
            ],
            now(),
        )
        .unwrap();
    identity
        .approve_device(&start.user_code, account, now())
        .unwrap();
    let DevicePoll::Authorized { access_token, .. } =
        identity.poll_device(&start.device_code, now()).unwrap()
    else {
        panic!("owned fixture device authorization");
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(owned_root().join(file)).unwrap();
    output.write_all(access_token.as_bytes()).unwrap();
}

fn seed_or_verify(mode: &str) -> Value {
    let (mut client, url) = database();
    let repository = ApplicationRepository::connect(&url).unwrap();
    if mode == "bootstrap" {
        device_fixture(
            &repository,
            "github_release_fixture",
            "shutdown-device-token",
        );
        return json!({"passed":true,"identity":"owned synthetic device"});
    }
    if mode == "seed" {
        assert_eq!(
            client
                .query_one("SELECT count(*) FROM wam_memory_records", &[])
                .unwrap()
                .get::<_, i64>(0),
            0
        );
        let service = prepare(&repository);
        contribution(&service, AUTHOR);
        repository
            .process_next(&mut provider("high"), &PassingReplay, now())
            .unwrap();
        let recalled = service
            .recall(
                RecallRequest {
                    url: "https://example.org/editor".into(),
                    keywords: vec!["draft".into()],
                    task_category: Some("compose-draft".into()),
                    kind: None,
                    limit: Some(3),
                },
                Some(READER),
            )
            .unwrap();
        assert_eq!(recalled.hits.len(), 1);
        service
            .report_outcome(
                READER,
                recalled.receipt_id.as_deref().unwrap(),
                OutcomeReportRequest {
                    outcome: MemoryOutcome::Success,
                    used_version_ids: vec![recalled.hits[0].version_id.clone()],
                },
            )
            .unwrap();
        device_fixture(&repository, AUTHOR, "source-device-token");
    }
    let service = WamService::durable(repository.clone()).unwrap();
    assert_eq!(service.catalog().unwrap().memories.len(), 1);
    assert_eq!(
        service
            .my_contributions(AUTHOR)
            .unwrap()
            .contributions
            .len(),
        1
    );
    let points: i64 = client
        .query_one(
            "SELECT sum(points_delta)::bigint FROM wam_score_events WHERE beneficiary_id=$1",
            &[&AUTHOR],
        )
        .unwrap()
        .get(0);
    assert_eq!(points, 22);
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_usage_receipts", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    let token = std::fs::read_to_string(owned_root().join("source-device-token")).unwrap();
    assert_eq!(
        identity(&repository)
            .authenticate_device(&token, now())
            .unwrap()
            .account_id,
        AUTHOR
    );
    let mut transaction = client.transaction().unwrap();
    assert!(
        transaction
            .execute("UPDATE wam_memory_versions SET document='{}'::jsonb", &[])
            .is_err(),
        "immutable trigger must survive restore"
    );
    transaction.rollback().unwrap();
    let mut report = snapshot(&mut client);
    report["points"] = json!(points);
    report["passed"] = json!(true);
    report
}

async fn quota(mode: &str) -> Value {
    owned_root();
    let raw = env::var("WAM_TEST_REDIS_URL").expect("owned Redis required");
    let url = url::Url::parse(&raw).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let mut connection = redis::Client::open(raw.as_str())
        .unwrap()
        .get_connection()
        .unwrap();
    let info: String = redis::cmd("INFO")
        .arg("server")
        .query(&mut connection)
        .unwrap();
    let pid = env::var("WAM_RELEASE_REDIS_PID").unwrap();
    assert!(
        info.lines()
            .any(|line| line.trim() == format!("process_id:{pid}"))
    );
    let store = RedisRateLimitStore::connect(&raw).await.unwrap();
    let bucket = BucketSpec::per_minute("wam:verification:quota", 60, 4, 1);
    let usage = UsageRecord {
        key: "wam:verification:usage".into(),
        units: 1,
        request_class: "recall",
    };
    let clock = 1_798_000_000_000;
    if mode == "quota-seed" {
        for _ in 0..3 {
            assert!(
                store
                    .consume(std::slice::from_ref(&bucket), Some(&usage), clock)
                    .await
                    .unwrap()[0]
                    .allowed
            );
        }
    }
    let status = store.inspect(&bucket, clock).await.unwrap();
    assert_eq!(
        status.remaining_units, 1,
        "replica promotion must preserve consumed quota"
    );
    if mode == "quota-verify" {
        assert!(
            store
                .consume(std::slice::from_ref(&bucket), Some(&usage), clock)
                .await
                .unwrap()[0]
                .allowed
        );
        assert!(
            !store
                .consume(std::slice::from_ref(&bucket), Some(&usage), clock)
                .await
                .unwrap()[0]
                .allowed
        );
    }
    json!({"passed":true,"remaining_before_final_charge":status.remaining_units,"replicated_quota_preserved":mode=="quota-verify"})
}

#[tokio::main]
async fn main() {
    let mode = env::args().nth(1).unwrap_or_default();
    let result = match mode.as_str() {
        "seed" | "verify" | "bootstrap" => {
            let operation = mode.clone();
            tokio::task::spawn_blocking(move || seed_or_verify(&operation))
                .await
                .unwrap()
        }
        "quota-seed" | "quota-verify" => quota(&mode).await,
        _ => panic!("unsupported local verification mode"),
    };
    println!("{result}");
}
