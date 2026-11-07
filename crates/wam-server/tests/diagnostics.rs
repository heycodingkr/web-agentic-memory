use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{
    ApplicationRepository, DemoTokenAuthenticator, WamService,
    diagnostics::{
        Diagnostics, ErrorCode, Operation, Source, StoredDiagnostic, observe, random_id,
    },
};
fn request_event() -> StoredDiagnostic {
    StoredDiagnostic {
        event_id: random_id(),
        trace_id: random_id(),
        kind: "request".into(),
        source: Source::Server,
        operation: Operation::Unknown,
        error_code: Some(ErrorCode::HttpError),
        error_reason: None,
        method: "GET".into(),
        status_code: Some(404),
        duration_ms: 1,
        received_at: wam_server::identity::now_unix(),
        received_at_micros: 0,
    }
}
fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "wam-diagnostics-{}-{}",
        std::process::id(),
        random_id()
    ))
}
fn payload() -> Value {
    json!({"schema_version":1,"event_id":random_id(),"trace_id":random_id(),"source":"cli",
        "operation":"recall","error_code":"network_error","duration_ms":20})
}
#[tokio::test]
async fn mcp_idle_sse_reaches_the_client_without_diagnostics_buffering_the_body() {
    let diagnostics = Diagnostics::in_memory();
    let app = observe(
        wam_server::router(
            Arc::new(WamService::durable(ApplicationRepository::in_memory()).unwrap()),
            Arc::new(DemoTokenAuthenticator::default()),
        ),
        diagnostics.clone(),
    );
    let response = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        app.oneshot(
            Request::get("/mcp")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("diagnostics must return streaming headers without awaiting EOF")
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    assert!(response.headers().contains_key("x-wam-trace-id"));
    let mut body = response.into_body();
    let frame = tokio::time::timeout(std::time::Duration::from_millis(250), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        std::str::from_utf8(frame.data_ref().unwrap())
            .unwrap()
            .contains("connected")
    );
    drop(body);
    diagnostics.flush().unwrap();
    let records = diagnostics.recent(10).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].method, "GET");
    assert_eq!(records[0].status_code, Some(200));
    assert_eq!(records[0].source, Source::Mcp);
}
#[tokio::test]
async fn correlation_covers_errors_without_storing_private_request_content() {
    let diagnostics = Diagnostics::in_memory();
    let app = observe(
        wam_server::router(
            Arc::new(WamService::durable(ApplicationRepository::in_memory()).unwrap()),
            Arc::new(DemoTokenAuthenticator::default()),
        ),
        diagnostics.clone(),
    );
    let response=app.oneshot(Request::post("/v1/memories/recall?secret=private-sentinel")
        .header("x-wam-client","mcp").header("x-wam-trace-id","private-sentinel")
        .header("authorization","Bearer private-sentinel")
        .header("content-type","application/json")
        .body(Body::from(r#"{"url":"https://example.org/editor?secret=private-sentinel","unexpected":"private-sentinel"}"#)).unwrap()).await.unwrap();
    assert!(response.status().is_client_error());
    let trace = response.headers()["x-wam-trace-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(trace.len(), 32);
    diagnostics.flush().unwrap();
    let rows = diagnostics.recent(100).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].trace_id, trace);
    assert_eq!(rows[0].source, Source::Mcp);
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("private-sentinel")
    );
}
#[tokio::test]
async fn closed_client_intake_deduplicates_and_rejects_untrusted_fields() {
    let diagnostics = Diagnostics::in_memory();
    let app = observe(axum::Router::new(), diagnostics.clone());
    let event = payload();
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::post("/v1/telemetry/events")
                    .header("content-type", "application/json")
                    .body(Body::from(event.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
    let mut invalid = event.clone();
    invalid["message"] = json!("private-sentinel");
    let response = app
        .oneshot(
            Request::post("/v1/telemetry/events")
                .header("content-type", "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&bytes).contains("private-sentinel"));
    diagnostics.flush().unwrap();
    let rows = diagnostics.recent(100).unwrap();
    assert_eq!(rows.iter().filter(|r| r.kind == "client_error").count(), 1);
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("private-sentinel")
    );
}
#[tokio::test]
async fn json_rejections_never_reflect_untrusted_field_names_or_enum_values() {
    let app = wam_server::router(
        Arc::new(WamService::durable(ApplicationRepository::in_memory()).unwrap()),
        Arc::new(DemoTokenAuthenticator::default()),
    );
    let sentinel = "private-canary-must-not-echo";
    for (path, body) in [
        (
            "/v1/memories/recall",
            json!({"url":"https://example.org/editor", (sentinel): "unexpected"}),
        ),
        (
            "/v1/me/contributions",
            json!({"url":"https://example.org/editor", "task_category":"compose-draft", "actions":[sentinel], "consent":true,"consent_version":"public-sharing-v1"}),
        ),
        (
            "/v1/memories/recall",
            json!({"url":1,"keywords":[sentinel]}),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_client_error());
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["content-type"], "application/json");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            !String::from_utf8_lossy(&bytes).contains(sentinel),
            "JSON extractor reflected private input"
        );
    }
    for (body, content_type, status) in [
        (
            format!("{{\"url\":\"{sentinel}\""),
            "application/json",
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("{{\"url\":\"{sentinel}\"}}"),
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            format!("{{\"url\":\"{sentinel}\"}}{}", " ".repeat(2 * 1024 * 1024)),
            "application/json",
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/v1/memories/recall")
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let error: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            error,
            json!({"error": if status == StatusCode::PAYLOAD_TOO_LARGE { "payload_too_large" } else { "invalid_json" }})
        );
    }
}

#[tokio::test]
async fn remote_tool_errors_expose_and_store_the_same_trace_for_json_and_sse() {
    for accept in [
        "application/json, text/event-stream;q=0.5",
        "application/json;q=0.5, text/event-stream",
    ] {
        let diagnostics = Diagnostics::in_memory();
        let app = observe(
            wam_server::router(
                Arc::new(WamService::durable(ApplicationRepository::in_memory()).unwrap()),
                Arc::new(DemoTokenAuthenticator::default()),
            ),
            diagnostics.clone(),
        );
        let trace = random_id();
        let response = app.oneshot(Request::post("/mcp")
            .header("content-type", "application/json")
            .header("accept", accept)
            .header("mcp-protocol-version", "2025-11-25")
            .header("x-wam-trace-id", &trace)
            .body(Body::from(json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":"web_memory_expand","arguments":{"memory_id":"private-sentinel", "level":999}}
            }).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-wam-trace-id"], trace);
        let sse = response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        let json_text = if sse {
            text.lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap()
        } else {
            text
        };
        let value: Value = serde_json::from_str(json_text).unwrap();
        assert_eq!(value["result"]["isError"], true);
        assert_eq!(value["result"]["_meta"]["trace_id"], trace);
        diagnostics.flush().unwrap();
        let rows = diagnostics.recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trace_id, trace);
        assert_eq!(rows[0].operation, Operation::Expand);
        assert_eq!(rows[0].error_code, Some(ErrorCode::InvalidInput));
        assert!(
            !serde_json::to_string(&rows)
                .unwrap()
                .contains("private-sentinel")
        );
    }
}

#[tokio::test]
async fn recall_rejections_preserve_closed_reasons_for_json_and_sse() {
    let repository = ApplicationRepository::in_memory();
    repository.ensure_examples().unwrap();
    let service = Arc::new(WamService::durable(repository).unwrap());
    let keywords: Vec<_> = service
        .try_vocab()
        .unwrap()
        .keywords
        .into_iter()
        .take(9)
        .collect();
    assert_eq!(keywords.len(), 9);
    let cases = [
        (
            json!({"keywords":["private-sentinel"]}),
            Some("invalid_arguments"),
        ),
        (
            json!({"url":"https://example.org", "private-sentinel":true}),
            Some("invalid_arguments"),
        ),
        (json!({"url":17}), Some("invalid_arguments")),
        (
            json!({"url":"file:///private-sentinel"}),
            Some("invalid_url"),
        ),
        (
            json!({"url":"https://example.org", "task_category":"private-sentinel"}),
            Some("invalid_task_category"),
        ),
        (
            json!({"url":"https://example.org", "keywords":keywords}),
            Some("invalid_keywords"),
        ),
        (
            json!({"url":"https://example.org", "kind":"private-sentinel"}),
            Some("invalid_kind"),
        ),
        (
            json!({"url":"https://example.org", "limit":0}),
            Some("invalid_limit"),
        ),
        (
            json!({"url":"https://example.org", "limit":4}),
            Some("invalid_limit"),
        ),
        (json!({"url":"https://example.org", "limit":1}), None),
    ];
    for accept in [
        "application/json, text/event-stream;q=0.5",
        "application/json;q=0.5, text/event-stream",
    ] {
        let diagnostics = Diagnostics::in_memory();
        for (arguments, expected) in &cases {
            // Each validation case gets its own admission bucket.
            let app = observe(
                wam_server::router(service.clone(), Arc::new(DemoTokenAuthenticator::default())),
                diagnostics.clone(),
            );
            let trace = random_id();
            let response = app
                .clone()
                .oneshot(
                    Request::post("/mcp")
                        .header("content-type", "application/json")
                        .header("accept", accept)
                        .header("mcp-protocol-version", "2025-11-25")
                        .header("x-wam-trace-id", &trace)
                        .body(Body::from(
                            json!({
                                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                                "params":{"name":"web_memory_recall","arguments":arguments}
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["x-wam-trace-id"], trace);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let text = std::str::from_utf8(&bytes).unwrap();
            let body = text
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap_or(text);
            let value: Value = serde_json::from_str(body).unwrap();
            assert_eq!(value["result"]["isError"], expected.is_some());
            diagnostics.flush().unwrap();
            let rows = diagnostics.recent(100).unwrap();
            let row = rows.iter().find(|row| row.trace_id == trace).unwrap();
            assert_eq!(row.operation, Operation::Recall);
            let stored = serde_json::to_value(row).unwrap();
            if let Some(reason) = expected {
                assert_eq!(value["result"]["_meta"]["trace_id"], trace);
                assert_eq!(value["result"]["_meta"]["error_reason"], *reason);
                assert_eq!(stored["error_reason"], *reason);
                assert_eq!(row.error_code, Some(ErrorCode::InvalidInput));
            } else {
                assert!(value["result"]["_meta"]["error_reason"].is_null());
                assert!(stored["error_reason"].is_null());
                assert_eq!(row.error_code, None);
            }
            assert!(!stored.to_string().contains("private-sentinel"));
            assert!(!body.contains("private-sentinel"));
        }
    }
}

#[test]
fn stored_recall_reasons_are_closed_and_legacy_documents_stay_readable() {
    let legacy = serde_json::to_value(request_event()).unwrap();
    let parsed: StoredDiagnostic = serde_json::from_value(legacy.clone()).unwrap();
    assert!(parsed.valid());
    assert!(
        serde_json::to_value(&parsed)
            .unwrap()
            .get("error_reason")
            .is_none()
    );

    let mut document = legacy;
    document["source"] = json!("mcp");
    document["operation"] = json!("recall");
    document["error_code"] = json!("invalid_input");
    document["status_code"] = json!(200);
    document["error_reason"] = json!("invalid_url");
    let parsed: StoredDiagnostic = serde_json::from_value(document.clone()).unwrap();
    assert!(parsed.valid());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("diagnostics.sqlite3");
    {
        let store = Diagnostics::sqlite(&path).unwrap();
        assert!(store.record(parsed));
        store.flush().unwrap();
    }
    let store = Diagnostics::sqlite(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&store.recent(10).unwrap()[0]).unwrap()["error_reason"],
        "invalid_url"
    );

    document["error_reason"] = json!("private-sentinel");
    assert!(serde_json::from_value::<StoredDiagnostic>(document.clone()).is_err());
    let mut intake = payload();
    intake["error_reason"] = json!("invalid_url");
    assert!(serde_json::from_value::<wam_server::diagnostics::DiagnosticEvent>(intake).is_err());
}

#[tokio::test]
async fn remote_parse_errors_include_the_request_trace_in_the_rpc_body() {
    let diagnostics = Diagnostics::in_memory();
    let app = observe(
        wam_server::router(
            Arc::new(WamService::durable(ApplicationRepository::in_memory()).unwrap()),
            Arc::new(DemoTokenAuthenticator::default()),
        ),
        diagnostics.clone(),
    );
    let trace = random_id();
    let response = app
        .oneshot(
            Request::post("/mcp")
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("x-wam-trace-id", &trace)
                .body(Body::from("{private-sentinel"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"]["code"], -32700);
    assert_eq!(value["error"]["data"]["trace_id"], trace);
    diagnostics.flush().unwrap();
    assert_eq!(diagnostics.recent(10).unwrap()[0].trace_id, trace);
}

#[tokio::test]
async fn remote_mcp_json_errors_are_traced_even_when_http_status_is_ok() {
    let diagnostics = Diagnostics::in_memory();
    let app=observe(axum::Router::new().route("/mcp",get(||async{axum::Json(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"private-sentinel"}}))})),diagnostics.clone());
    let trace = random_id();
    let response = app
        .oneshot(
            Request::get("/mcp")
                .header("x-wam-trace-id", &trace)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-wam-trace-id"], trace);
    diagnostics.flush().unwrap();
    let rows = diagnostics.recent(10).unwrap();
    assert_eq!(rows[0].error_code, Some(ErrorCode::InvalidInput));
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("private-sentinel")
    );
}
#[test]
fn sqlite_reopen_retention_private_files_and_pending_storage_recovery() {
    let directory = temp_dir();
    let path = directory.join("diagnostics.sqlite");
    let diagnostics = Diagnostics::sqlite(&path).unwrap();
    let event = request_event();
    diagnostics.record(event.clone());
    diagnostics.record(event.clone());
    diagnostics.flush().unwrap();
    assert_eq!(diagnostics.recent(10).unwrap().len(), 1);
    let reopened = Diagnostics::sqlite(&path).unwrap();
    assert_eq!(reopened.recent(10).unwrap()[0].trace_id, event.trace_id);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE wam_diagnostic_events")
        .unwrap();
    let next = request_event();
    diagnostics.record(next.clone());
    assert!(diagnostics.flush().is_err());
    connection
        .execute_batch(include_str!("../src/diagnostics/schema.sql"))
        .unwrap();
    diagnostics.flush().unwrap();
    assert_eq!(diagnostics.recent(10).unwrap()[0].trace_id, next.trace_id);
    diagnostics.purge(next.received_at + 8 * 86_400).unwrap();
    assert!(diagnostics.recent(10).unwrap().is_empty());
    drop(connection);
    drop(diagnostics);
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn active_sqlite_reader_does_not_block_diagnostic_ingestion() {
    let directory = temp_dir();
    let path = directory.join("diagnostics.sqlite");
    let diagnostics = Diagnostics::sqlite(&path).unwrap();
    diagnostics.record(request_event());
    diagnostics.flush().unwrap();
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let before: i64 = reader
        .query_row("SELECT count(*) FROM wam_diagnostic_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(before, 1);
    diagnostics.record(request_event());
    diagnostics.flush().unwrap();
    assert_eq!(diagnostics.recent(100).unwrap().len(), 2);
    let snapshot: i64 = reader
        .query_row("SELECT count(*) FROM wam_diagnostic_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(snapshot, 1);
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM wam_diagnostic_events", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        2
    );
    drop(reader);
    drop(diagnostics);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn oversized_intake_and_peer_flood_are_bounded() {
    let app = observe(axum::Router::new(), Diagnostics::in_memory());
    let oversized = app
        .clone()
        .oneshot(
            Request::post("/v1/telemetry/events")
                .header("content-type", "application/json")
                .body(Body::from(" ".repeat(4096)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    for _ in 0..59 {
        let response = app
            .clone()
            .oneshot(
                Request::post("/v1/telemetry/events")
                    .header("content-type", "application/json")
                    .body(Body::from(payload().to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
    let response = app
        .oneshot(
            Request::post("/v1/telemetry/events")
                .header("content-type", "application/json")
                .body(Body::from(payload().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("x-wam-trace-id"));
}

#[test]
fn invalid_enums_bounds_and_symlinks_cannot_enter_storage() {
    for change in [
        "source",
        "error_code",
        "status_code",
        "duration_ms",
        "trace_id",
    ] {
        let mut value = payload();
        value[change] = match change {
            "status_code" => json!(999),
            "duration_ms" => json!(600001),
            "trace_id" => json!("private-sentinel"),
            _ => json!("private-sentinel"),
        };
        if let Ok(event) = serde_json::from_value::<wam_server::diagnostics::DiagnosticEvent>(value)
        {
            assert!(!event.valid())
        }
    }
    #[cfg(unix)]
    {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target");
        std::fs::write(&target, "private-sentinel").unwrap();
        let link = dir.join("diagnostics.sqlite");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(Diagnostics::sqlite(link).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "private-sentinel");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
