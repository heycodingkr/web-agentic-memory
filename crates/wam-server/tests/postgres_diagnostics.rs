use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
use wam_server::diagnostics::{Diagnostics, observe, random_id};
struct Schema {
    base: String,
    url: String,
    name: String,
}
impl Schema {
    fn new() -> Self {
        let base = std::env::var("WAM_TEST_DATABASE_URL").expect("use an isolated test database");
        let name = format!("wam_diag_{}_{}", std::process::id(), random_id());
        postgres::Client::connect(&base, postgres::NoTls)
            .unwrap()
            .batch_execute(&format!("CREATE SCHEMA {name}"))
            .unwrap();
        let mut url = url::Url::parse(&base).unwrap();
        url.query_pairs_mut()
            .append_pair("options", &format!("-c search_path={name}"))
            .append_pair("application_name", &name);
        let query = url.query().unwrap().replace('+', "%20");
        url.set_query(Some(&query));
        Self {
            base,
            url: url.into(),
            name,
        }
    }
    fn client(&self) -> postgres::Client {
        postgres::Client::connect(&self.url, postgres::NoTls).unwrap()
    }
}
impl Drop for Schema {
    fn drop(&mut self) {
        let _ = postgres::Client::connect(&self.base, postgres::NoTls)
            .unwrap()
            .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.name));
    }
}
#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn postgres_ingestion_reopen_dedup_redaction_and_retention() {
    let schema = Schema::new();
    let diagnostics = Diagnostics::postgres(&schema.url).unwrap();
    let app = observe(axum::Router::new(), diagnostics.clone());
    let event_id = random_id();
    let trace_id = random_id();
    let event = json!({"schema_version":1,"event_id":event_id,"trace_id":trace_id,"source":"mcp","operation":"recall","error_code":"http_error","status_code":401,"duration_ms":4});
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for _ in 0..2 {
        let response = runtime
            .block_on(
                app.clone().oneshot(
                    Request::post("/v1/telemetry/events?private-sentinel=not-stored")
                        .header("content-type", "application/json")
                        .header("x-wam-client", "mcp")
                        .header("authorization", "Bearer private-sentinel")
                        .body(Body::from(event.to_string()))
                        .unwrap(),
                ),
            )
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
    diagnostics.flush().unwrap();
    let reopened = Diagnostics::postgres(&schema.url).unwrap();
    assert_eq!(
        reopened
            .recent(100)
            .unwrap()
            .iter()
            .filter(|r| r.event_id == event_id)
            .count(),
        1
    );
    let mut client = schema.client();
    for row in client
        .query("SELECT document FROM wam_diagnostic_events", &[])
        .unwrap()
    {
        assert!(!row.get::<_, String>(0).contains("private-sentinel"));
    }
    diagnostics
        .purge(wam_server::identity::now_unix() + 8 * 86400)
        .unwrap();
    assert!(reopened.recent(100).unwrap().is_empty());
}
#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn postgres_logger_recovers_a_terminated_connection() {
    use wam_server::diagnostics::{ErrorCode, Operation, Source, StoredDiagnostic};
    let schema = Schema::new();
    let diagnostics = Diagnostics::postgres(&schema.url).unwrap();
    let mut control = schema.client();
    let pids=control.query("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND application_name=$1 AND pid<>pg_backend_pid() AND query LIKE 'DELETE FROM wam_diagnostic_events%'",&[&schema.name]).unwrap();
    assert!(!pids.is_empty());
    for row in pids {
        let pid: i32 = row.get(0);
        control
            .query_one("SELECT pg_terminate_backend($1)", &[&pid])
            .unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    let event_id = random_id();
    diagnostics.record(StoredDiagnostic {
        event_id: event_id.clone(),
        trace_id: random_id(),
        kind: "request".into(),
        source: Source::Server,
        operation: Operation::Recall,
        error_code: Some(ErrorCode::InternalError),
        method: "POST".into(),
        status_code: Some(500),
        duration_ms: 1,
        received_at: wam_server::identity::now_unix(),
    });
    for _ in 0..5 {
        if diagnostics.flush().is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(
        diagnostics
            .recent(100)
            .unwrap()
            .iter()
            .filter(|r| r.event_id == event_id)
            .count(),
        1
    );
}
