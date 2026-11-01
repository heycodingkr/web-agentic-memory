//! Fixed synthetic search workload. Reports measurements, never model relevance claims.
#[path = "../tests/lifecycle_support/mod.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "smoke_support/privacy.rs"]
mod privacy;
mod release_support;

use postgres::{Client, binary_copy::BinaryCopyInWriter, types::Type};
use serde_json::{Value, json};
use std::{env, net::TcpListener, sync::Arc, time::Instant};
use wam_contracts::{RecallRequest, RecallResponse};
use wam_core::{
    KeywordVocabulary, MemoryKind, TaskVocabulary, UrlMode,
    recall::{RecallDocument, RecallQuery, recall_documents},
    url_scope::CanonicalUrl,
};
use wam_gatekeeper::{
    record::{RecordPayload, fingerprint_of, sha256_hex},
    store::{Evidence, MemoryRow, ScopeKey, Status, VersionRow},
};
use wam_server::{
    ApplicationRepository, DemoReviewerTokenAuthenticator, DemoTokenAuthenticator,
    FileIdentityStore, IdentityService, InMemoryRateLimitStore,
    ports::{MemoryRecord, Repository},
    router_with_identity_and_rate_limit_store,
};

const SQL: &str = include_str!("../src/durable/recall_candidates.sql");
const VOCAB: [&str; 5] = ["blog", "draft", "publish", "search", "write"];

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn generated(index: usize) -> (MemoryRow, VersionRow, String) {
    let hash = mix(index as u64 + 20261030);
    let id = format!("profile-{index:07}");
    let host = format!("profile-{}.example.org", index % 1000);
    let task = match index {
        41 => "compose-draft",
        42 => "navigate",
        _ => ["compose-draft", "navigate", "research"][(hash as usize >> 16) % 3],
    };
    let status = if matches!(index, 41 | 42) {
        Status::Active
    } else {
        match (hash >> 8) % 20 {
            0 => Status::Retired,
            1 => Status::NeedsReverify,
            _ => Status::Active,
        }
    };
    let mut payload: RecordPayload = serde_json::from_value(fixture::record()).unwrap();
    payload.l0.site_host = host.clone();
    payload.l0.task_category = Some(task.into());
    payload.l0.intent_key = Some(format!("intent-{index}"));
    payload.l0.keywords = VOCAB.iter().map(|word| (*word).into()).collect();
    payload.l0.url_patterns = if index % 2 == 0 {
        vec!["/editor".into()]
    } else {
        ["/editor", "/workspace/*", "/settings", "/*"]
            .map(str::to_owned)
            .into()
    };
    let version = VersionRow {
        version: 1,
        fingerprint: fingerprint_of(&payload),
        payload,
        verified_at: 1_700_000_000 + hash % 1000,
        status,
        evidence: Evidence::default(),
        job_id: id.clone(),
    };
    let memory = MemoryRow {
        id: id.clone(),
        scope: ScopeKey {
            host,
            pattern: "/editor".into(),
            kind: "workflow".into(),
            task: task.into(),
            intent: format!("intent-{index}"),
        },
        versions: vec![],
        support_count: 1,
        status,
        confidence_band: (hash % 3) as u8,
    };
    (memory, version, wam_server::durable::version_id(&id, 1))
}

fn seed(client: &mut Client, count: usize) -> String {
    let mut transaction = client.transaction().unwrap();
    let mut chain = "wam-search-profile-v1-seed-20261030".to_owned();
    {
        let sink = transaction.copy_in("COPY wam_memory_records(id,site_host,url_pattern,kind,task_category,intent_key,status,active_version,document) FROM STDIN BINARY").unwrap();
        let mut writer = BinaryCopyInWriter::new(
            sink,
            &[
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::INT4,
                Type::JSONB,
            ],
        );
        for index in 0..count {
            let (memory, version, _) = generated(index);
            let document = serde_json::to_value(&memory).unwrap();
            let status = document["status"].as_str().unwrap();
            writer
                .write(&[
                    &memory.id,
                    &memory.scope.host,
                    &memory.scope.pattern,
                    &memory.scope.kind,
                    &memory.scope.task,
                    &memory.scope.intent,
                    &status,
                    &1_i32,
                    &document,
                ])
                .unwrap();
            chain = sha256_hex(
                format!(
                    "{chain}|{}|{}",
                    document,
                    serde_json::to_string(&version).unwrap()
                )
                .as_bytes(),
            );
        }
        writer.finish().unwrap();
    }
    {
        let sink = transaction
            .copy_in("COPY wam_memory_versions(id,memory_id,version_no,document) FROM STDIN BINARY")
            .unwrap();
        let mut writer =
            BinaryCopyInWriter::new(sink, &[Type::TEXT, Type::TEXT, Type::INT4, Type::JSONB]);
        for index in 0..count {
            let (memory, version, id) = generated(index);
            writer
                .write(&[
                    &id,
                    &memory.id,
                    &1_i32,
                    &serde_json::to_value(version).unwrap(),
                ])
                .unwrap();
        }
        writer.finish().unwrap();
    }
    {
        let sink = transaction.copy_in("COPY wam_memory_index(version_id,site_host,url_pattern,kind,task_category,keyword) FROM STDIN BINARY").unwrap();
        let mut writer = BinaryCopyInWriter::new(
            sink,
            &[
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
                Type::TEXT,
            ],
        );
        for index in 0..count {
            let (_, version, id) = generated(index);
            let l0 = &version.payload.l0;
            for route in &l0.url_patterns {
                for word in &l0.keywords {
                    writer
                        .write(&[
                            &id,
                            &l0.site_host,
                            &route,
                            &l0.kind,
                            &l0.task_category.as_deref().unwrap(),
                            word,
                        ])
                        .unwrap();
                }
            }
        }
        writer.finish().unwrap();
    }
    transaction.commit().unwrap();
    client.batch_execute("ANALYZE").unwrap();
    chain
}

fn query(
    path: &str,
    task: Option<&str>,
    kind: Option<&str>,
    words: &[&str],
) -> (RecallQuery, RecallRequest) {
    let tasks = TaskVocabulary::new("profile", ["compose-draft", "navigate", "research"]).unwrap();
    let keywords = KeywordVocabulary::new("profile", VOCAB)
        .unwrap()
        .filter(words.iter().copied())
        .unwrap();
    let request = RecallRequest {
        url: path.into(),
        task_category: task.map(str::to_owned),
        kind: kind.map(str::to_owned),
        keywords: words.iter().map(|word| (*word).into()).collect(),
        limit: Some(3),
    };
    let query = RecallQuery::new(
        CanonicalUrl::parse(path, UrlMode::Public).unwrap(),
        task.map(|task| tasks.resolve(task).unwrap()),
        kind.map(|value| match value {
            "site-note" => MemoryKind::SiteNote,
            "workflow" => MemoryKind::Workflow,
            "gotcha" => MemoryKind::Gotcha,
            _ => panic!("profile kind must use the public API vocabulary"),
        }),
        keywords,
    )
    .unwrap();
    (query, request)
}

fn rank(records: &[MemoryRecord], query: &RecallQuery) -> Vec<(String, String, u16)> {
    let documents: Vec<_> = records
        .iter()
        .map(|row| {
            RecallDocument::new(&row.index, &row.title, &row.summary, &row.site, &row.domain)
        })
        .collect();
    recall_documents(&documents, query)
        .iter()
        .map(|hit| {
            (
                hit.memory.id().into(),
                hit.memory.version_id().into(),
                hit.score,
            )
        })
        .collect()
}

fn stats(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    let percentile = |p: f64| values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    json!({"n":values.len(),"p50_ms":percentile(0.50),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),
        "serial_service_rate_estimate_rps":1000.0 * values.len() as f64 / values.iter().sum::<f64>(),"raw_ms":values})
}

fn main() {
    let count: usize = env::args().nth(1).unwrap().parse().unwrap();
    let samples: usize = env::args().nth(2).unwrap_or("10".into()).parse().unwrap();
    let repeats: usize = env::args().nth(3).unwrap_or("5".into()).parse().unwrap();
    assert!(matches!(count, 1000 | 10_000 | 100_000 | 1_000_000));
    assert!((2..=100).contains(&samples) && (2..=10).contains(&repeats));
    if cfg!(debug_assertions) {
        eprintln!("search profiles require an optimized release build");
        std::process::exit(2);
    }
    let (mut client, url) = release_support::database();
    let repository = ApplicationRepository::connect(&url).unwrap();
    let service = Arc::new(fixture::prepare(&repository));
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_memory_records", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let started = Instant::now();
    let corpus_hash = seed(&mut client, count);
    let seed_seconds = started.elapsed().as_secs_f64();
    let index_rows: i64 = client
        .query_one("SELECT count(*) FROM wam_memory_index", &[])
        .unwrap()
        .get(0);
    assert_eq!(index_rows, count as i64 * 25 / 2);
    let load = Instant::now();
    let all = repository.try_memories().unwrap();
    let active = (0..count)
        .filter(|index| generated(*index).0.status.recallable())
        .count();
    assert_eq!(all.len(), active);
    let exhaustive_load_ms = load.elapsed().as_secs_f64() * 1000.0;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server_repository = repository.clone();
    let app_service = service.clone();
    let server_origin = origin.clone();
    let server = std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let identity = Arc::new(IdentityService::new(
                    Arc::new(FileIdentityStore::from_repository(server_repository)),
                    None,
                    server_origin,
                    false,
                ));
                let app = router_with_identity_and_rate_limit_store(
                    app_service,
                    Arc::new(DemoTokenAuthenticator::default()),
                    Arc::new(DemoReviewerTokenAuthenticator::default()),
                    identity,
                    Arc::new(privacy::FixtureRateLimits {
                        inner: InMemoryRateLimitStore::default(),
                        bulk: true,
                    }),
                    "wam-profile-fixture-quota-only",
                    vec![],
                );
                axum::serve(
                    tokio::net::TcpListener::from_std(listener).unwrap(),
                    app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
            });
    });
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let cases = [
        (
            "exact",
            query(
                "https://profile-41.example.org/editor",
                Some("compose-draft"),
                None,
                &["draft"],
            ),
        ),
        (
            "prefix",
            query(
                "https://profile-41.example.org/workspace/notes",
                Some("compose-draft"),
                None,
                &["search"],
            ),
        ),
        (
            "exact-other-task",
            query(
                "https://profile-42.example.org/editor",
                Some("navigate"),
                None,
                &["write"],
            ),
        ),
        (
            "missing-route",
            query(
                "https://profile-42.example.org/unrelated",
                Some("compose-draft"),
                None,
                &[],
            ),
        ),
        (
            "missing-host",
            query("https://absent.example.org/editor", None, None, &[]),
        ),
        (
            "missing-kind",
            query(
                "https://profile-41.example.org/editor",
                None,
                Some("site-note"),
                &[],
            ),
        ),
        (
            "hot-host",
            query(
                "https://profile-41.example.org/editor",
                None,
                None,
                &["publish", "draft"],
            ),
        ),
    ];
    let mut results = Vec::new();
    let mut total_errors = 0;
    for (name, (query, request)) in cases {
        let expected = rank(&all, &query);
        if name.starts_with("missing-") {
            assert!(expected.is_empty());
        } else {
            assert!(!expected.is_empty(), "positive workload must actually hit");
        }
        let mut first = None;
        let mut trials = Vec::new();
        for trial in 0..repeats {
            let mut core_ms = Vec::new();
            let mut db_ms = Vec::new();
            let mut http_ms = Vec::new();
            for sample in 0..=samples {
                let timer = Instant::now();
                let core = rank(&all, &query);
                let core_elapsed = timer.elapsed().as_secs_f64() * 1000.0;
                let timer = Instant::now();
                let candidates = repository.try_recall_candidates(&query).unwrap();
                let db_elapsed = timer.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(
                    rank(&candidates, &query),
                    expected,
                    "database candidate parity"
                );
                assert_eq!(core, expected);
                let timer = Instant::now();
                let response = http
                    .post(format!("{origin}/v1/memories/recall"))
                    .json(&request)
                    .send()
                    .unwrap();
                if !response.status().is_success() {
                    total_errors += 1;
                }
                assert!(
                    response.status().is_success(),
                    "HTTP profile failed: {}",
                    response.status()
                );
                let response: RecallResponse = response.json().unwrap();
                let http_elapsed = timer.elapsed().as_secs_f64() * 1000.0;
                let observed: Vec<_> = response
                    .hits
                    .iter()
                    .map(|hit| (hit.memory_id.clone(), hit.version_id.clone(), hit.score))
                    .collect();
                assert_eq!(observed, expected, "HTTP/core parity");
                if trial == 0 && sample == 0 {
                    first = Some(
                        json!({"core_ms":core_elapsed,"db_ms":db_elapsed,"http_ms":http_elapsed,"candidate_count":candidates.len()}),
                    );
                }
                if sample > 0 {
                    core_ms.push(core_elapsed);
                    db_ms.push(db_elapsed);
                    http_ms.push(http_elapsed);
                }
            }
            trials.push(json!({"trial":trial,"core":stats(core_ms),"database":stats(db_ms),"http":stats(http_ms)}));
        }
        let task = request.task_category.as_deref();
        let kind = query.kind().map(wam_gatekeeper::record::kind_str);
        let explain: Value = client
            .query_one(
                &format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {SQL}"),
                &[&query.url().host(), &query.url().path(), &task, &kind],
            )
            .unwrap()
            .get(0);
        results.push(json!({"case":name,"expected":expected,"first_observed":first,"trials":trials,"explain":explain}));
    }
    let _ = stop.send(());
    server.join().unwrap();
    let pg: String = client.query_one("SELECT version()", &[]).unwrap().get(0);
    let bytes: i64 = client
        .query_one("SELECT pg_database_size(current_database())", &[])
        .unwrap()
        .get(0);
    println!(
        "{}",
        json!({"schema_version":1,"passed":true,"synthetic_only":true,"human_reviewed":false,"profile":"release",
        "seed":20261030,"generator":"search-profile-v1","records":count,"exhaustive_active_records":active,"index_rows":index_rows,"corpus_sha256":corpus_hash,
        "seed_seconds":seed_seconds,"exhaustive_load_ms":exhaustive_load_ms,"postgres":pg,"database_bytes":bytes,
        "fixture_distribution":"1000 host shards; workflow only; three tasks; active/needs-reverify/retired; 1/4 routes and 5 keywords",
        "http_quota":"in-memory x100; not a Redis/topology throughput result","cache":"warm repeated workload; first observation separately; OS cache not flushed",
        "core_scope":"exhaustive preloaded records plus document-reference projection",
        "database_scope":"actual repository candidate query, row decoding and dedicated-thread dispatch",
        "concurrency":1,"samples_per_trial":samples,"repeats":repeats,"errors":total_errors,"cases":results})
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_rows_obey_the_real_core_index_contract_and_have_positive_controls() {
        let tasks = TaskVocabulary::new("test", ["compose-draft", "navigate", "research"]).unwrap();
        for index in 0..1000 {
            let (memory, version, id) = generated(index);
            let payload = &version.payload;
            let keywords = KeywordVocabulary::new("test", VOCAB)
                .unwrap()
                .filter(VOCAB)
                .unwrap();
            let patterns = payload
                .l0
                .url_patterns
                .iter()
                .map(|pattern| {
                    wam_gatekeeper::record::build_pattern(
                        &payload.l0.site_host,
                        pattern,
                        UrlMode::Public,
                    )
                    .unwrap()
                })
                .collect();
            assert!(
                wam_core::MemoryIndex::new(
                    &memory.id,
                    id,
                    patterns,
                    tasks
                        .resolve(payload.l0.task_category.as_deref().unwrap())
                        .unwrap(),
                    MemoryKind::Workflow,
                    payload.l0.intent_key.as_deref().unwrap(),
                    keywords,
                    memory.confidence_band,
                    wam_core::MemoryStatus::Active,
                    version.verified_at
                )
                .is_ok()
            );
        }
        assert_eq!(generated(41).0.scope.task, "compose-draft");
        assert_eq!(generated(42).0.scope.task, "navigate");
        assert!(generated(41).0.status.recallable() && generated(42).0.status.recallable());
    }
}
