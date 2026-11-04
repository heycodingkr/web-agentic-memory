use super::*;
use crate::endpoint::canonical_public_url;
use crate::telemetry::random_id;
use std::time::Duration;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread::JoinHandle,
    time::{SystemTime, UNIX_EPOCH},
};
use wam_contracts::{RecallCacheStatus, RecallRequest};

fn serve_once(status: &str, headers: &str, body: &str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request);
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}"), handle)
}

fn temp_cache_path() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("wam-client-test-{}-{nonce}", std::process::id()))
        .join("cache.sqlite3")
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn mcp_status_reports_reauthentication_without_anonymous_retry() {
    let (endpoint, server) = serve_once(
        "401 Unauthorized",
        "Content-Type: application/json\r\n",
        "{}",
    );
    let mut client = HttpClient::new(
        endpoint.clone(),
        Some("expired-status-private-sentinel".into()),
    )
    .unwrap();
    // Avoid diagnostics network traffic in this focused response-state test.
    client.telemetry_spool = None;
    let status = client.memory_status().await.unwrap();
    assert_eq!(status.auth, wam_contracts::McpAuthState::ReauthRequired);
    assert_eq!(status.consent, wam_contracts::McpConsentState::Unknown);
    assert_eq!(status.endpoint, format!("{endpoint}/mcp"));
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("private-sentinel")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn http_failure_keeps_error_and_sends_only_closed_diagnostic_with_same_trace() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let server =
        std::thread::spawn(move || {
            for (index, response) in [
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ].into_iter().enumerate() {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            sender.send((index, request)).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        }
        });
    let mut client = HttpClient::new(endpoint, None).unwrap();
    client.set_diagnostic_source(DiagnosticSource::Mcp);
    assert_eq!(
        client.expand("private-sentinel", 2).await,
        Err(ApiError::Server)
    );
    let trace = client.last_trace_id().unwrap();
    assert_eq!(trace.len(), 32);
    let (_, first) = receiver.recv().unwrap();
    let (_, second) = receiver.recv().unwrap();
    let first_lower = first.to_ascii_lowercase();
    let second_lower = second.to_ascii_lowercase();
    assert!(first_lower.contains(&format!("x-wam-trace-id: {trace}")));
    assert!(first_lower.contains("x-wam-client: mcp"));
    assert!(second_lower.contains("post /v1/telemetry/events"));
    assert!(second_lower.contains(&format!("x-wam-trace-id: {trace}")));
    let body: serde_json::Value =
        serde_json::from_str(second.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["trace_id"], trace);
    assert_eq!(body["source"], "mcp");
    assert_eq!(body["operation"], "expand");
    assert_eq!(body["error_code"], "http_error");
    assert_eq!(body["status_code"], 503);
    assert!(!second.contains("private-sentinel"));
    server.join().unwrap();
}

#[tokio::test]
async fn server_response_trace_is_used_for_json_empty_and_recall_failures() {
    for case in 0..5 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server_trace = "c".repeat(32);
        let (status, body) = if case >= 3 {
            ("200 OK", "invalid-json")
        } else {
            ("503 Service Unavailable", "")
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nx-wam-trace-id: {server_trace}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            stream.write_all(response.as_bytes()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let diagnostic = read_http_request(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            (request, diagnostic)
        });
        let client = HttpClient::new(endpoint, Some("private-token-sentinel".into())).unwrap();
        let error = match case {
            0 | 3 => client
                .expand("private-memory-sentinel", 2)
                .await
                .unwrap_err(),
            1 => client
                .withdraw_contribution("private-contribution-sentinel")
                .await
                .unwrap_err(),
            _ => client
                .recall(RecallRequest {
                    url: "https://example.com/private-url-sentinel".into(),
                    keywords: vec![],
                    task_category: None,
                    kind: None,
                    limit: Some(1),
                })
                .await
                .unwrap_err(),
        };
        let (request, diagnostic) = server.join().unwrap();
        assert_eq!(
            error,
            if case >= 3 {
                ApiError::InvalidResponse
            } else {
                ApiError::Server
            }
        );
        assert_eq!(
            client.last_trace_id().as_deref(),
            Some(server_trace.as_str()),
            "case {case}"
        );
        let event: serde_json::Value =
            serde_json::from_str(diagnostic.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(event["trace_id"], server_trace);
        assert_eq!(
            event["error_code"],
            if case >= 3 {
                "invalid_response"
            } else {
                "http_error"
            }
        );
        assert_eq!(
            event["operation"],
            match case {
                0 | 3 => "expand",
                1 => "contribution",
                _ => "recall",
            }
        );
        assert!(!event.to_string().contains("private-"));
        assert!(request.contains("x-wam-trace-id: "));
        assert!(diagnostic.contains(&format!("x-wam-trace-id: {server_trace}")));
    }
}

#[tokio::test]
async fn malformed_server_trace_falls_back_to_request_trace() {
    for header in [
        "INVALID-private-sentinel",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 503 Service Unavailable\r\nx-wam-trace-id: {header}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            stream.write_all(response.as_bytes()).unwrap();
            request
        });
        let client = HttpClient::new(endpoint, None).unwrap();
        assert_eq!(client.expand("memory", 2).await, Err(ApiError::Server));
        let trace = client.last_trace_id().unwrap();
        assert_eq!(trace.len(), 32);
        assert!(
            trace
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        let request = server.join().unwrap();
        let outgoing_trace = request
            .lines()
            .find_map(|line| line.strip_prefix("x-wam-trace-id: "))
            .unwrap()
            .trim();
        assert_eq!(trace, outgoing_trace);
    }
}

#[tokio::test]
async fn transport_failure_preserves_api_error_even_when_diagnostic_send_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = HttpClient::new(endpoint, None).unwrap();
    assert_eq!(client.status().await, Err(ApiError::Transport));
    assert_eq!(client.last_trace_id().unwrap().len(), 32);
}

#[tokio::test]
async fn failed_telemetry_post_does_not_report_itself() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert!(listener.accept().is_err());
        request
    });
    let client = HttpClient::new(endpoint, None).unwrap();
    assert!(
        client
            .report_diagnostic_event(DiagnosticOperation::Unknown, DiagnosticError::InvalidInput)
            .await
            .is_some()
    );
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /v1/telemetry/events"));
}

#[tokio::test]
async fn failed_delivery_is_flushed_by_next_client_after_successful_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("wam-spool-flow-{}", random_id().unwrap()));
    let (sender, receiver) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let replies = [
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}".to_owned(),
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        ];
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            sender.send(read_http_request(&mut stream)).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    let mut first = HttpClient::new(&endpoint, None).unwrap();
    first.set_telemetry_spool_dir(&root);
    assert_eq!(first.status().await, Err(ApiError::Server));
    let spool = first.telemetry_spool.as_ref().unwrap();
    let pending = spool.pending(10).unwrap();
    assert_eq!(pending.len(), 1);
    let event_id = pending[0].id().to_owned();
    drop(first);
    let mut second = HttpClient::new(&endpoint, None).unwrap();
    second.set_diagnostic_source(DiagnosticSource::Mcp);
    second.set_telemetry_spool_dir(&root);
    assert_eq!(second.status().await, Ok(()));
    assert!(
        second
            .telemetry_spool
            .as_ref()
            .unwrap()
            .pending(10)
            .unwrap()
            .is_empty()
    );
    let requests: Vec<_> = (0..4).map(|_| receiver.recv().unwrap()).collect();
    let first_report: serde_json::Value =
        serde_json::from_str(requests[1].split_once("\r\n\r\n").unwrap().1).unwrap();
    let retried: serde_json::Value =
        serde_json::from_str(requests[3].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(first_report["event_id"], event_id);
    assert_eq!(retried["event_id"], event_id);
    assert!(
        requests[3]
            .to_ascii_lowercase()
            .contains("x-wam-client: cli")
    );
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn transport_offline_event_reaches_next_successful_connection() {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let endpoint = format!("http://{address}");
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("wam-spool-offline-{}", random_id().unwrap()));
    let mut first = HttpClient::new(&endpoint, None).unwrap();
    first.set_telemetry_spool_dir(&root);
    assert_eq!(first.status().await, Err(ApiError::Transport));
    assert_eq!(
        first
            .telemetry_spool
            .as_ref()
            .unwrap()
            .pending(10)
            .unwrap()
            .len(),
        1
    );
    drop(first);
    let listener = TcpListener::bind(address).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}").unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let report = read_http_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        report
    });
    let mut second = HttpClient::new(&endpoint, None).unwrap();
    second.set_telemetry_spool_dir(&root);
    assert_eq!(second.status().await, Ok(()));
    let report = server.join().unwrap();
    assert!(report.contains("\"error_code\":\"network_error\""));
    assert!(
        second
            .telemetry_spool
            .as_ref()
            .unwrap()
            .pending(10)
            .unwrap()
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn telemetry_off_does_not_send_or_spool() {
    if std::env::var_os("WAM_TELEMETRY_TEST_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::telemetry_off_does_not_send_or_spool")
            .env("WAM_TELEMETRY_TEST_CHILD", "1")
            .env("WAM_TELEMETRY", "off")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("wam-spool-off-{}", random_id().unwrap()));
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(Duration::from_millis(40));
        assert!(listener.accept().is_err());
    });
    let mut client = HttpClient::new(endpoint, None).unwrap();
    client.set_telemetry_spool_dir(&root);
    assert_eq!(client.status().await, Err(ApiError::Server));
    server.join().unwrap();
    assert!(!root.exists());
    assert!(client.last_trace_id().is_none());
}

#[test]
fn rejects_non_http_endpoints_without_network_access() {
    assert!(matches!(
        HttpClient::new("file:///tmp/wam", None),
        Err(ApiError::InvalidEndpoint)
    ));
    assert!(HttpClient::new("http://127.0.0.1:3000/", None).is_ok());
}

#[test]
fn public_urls_are_canonicalized_before_they_can_be_sent() {
    assert_eq!(
        canonical_public_url("https://example.com/guide?token=private#section").unwrap(),
        "https://example.com/guide"
    );
    assert!(!canonical_public_url("http://example.com/guide").is_ok());
}

#[test]
fn dynamic_path_values_are_encoded_as_single_segments() {
    let url = route_url(
        "http://127.0.0.1:3000",
        &["v1", "profiles", "alice/example?private=1"],
    )
    .unwrap();
    assert_eq!(url.path(), "/v1/profiles/alice%2Fexample%3Fprivate=1");
    assert_eq!(url.query(), None);
    let dashboard = route_url("http://127.0.0.1:3000", &["v1", "me", "dashboard"]).unwrap();
    assert_eq!(dashboard.path(), "/v1/me/dashboard");
    let contribution = route_url(
        "http://127.0.0.1:3000",
        &["v1", "me", "contributions", "local/1"],
    )
    .unwrap();
    assert_eq!(contribution.path(), "/v1/me/contributions/local%2F1");
}

#[tokio::test]
async fn rate_limit_errors_keep_retry_after_for_cli_and_mcp_callers() {
    let (endpoint, server) = serve_once(
        "429 Too Many Requests",
        "Content-Type: application/json\r\nRetry-After: 12\r\n",
        r#"{"error":"rate_limited","retry_after_seconds":12}"#,
    );
    let client = HttpClient::new(endpoint, None).unwrap();
    let error = client.status().await.unwrap_err();
    assert_eq!(
        error,
        ApiError::RateLimited {
            retry_after_seconds: Some(12)
        }
    );
    assert_eq!(error.to_string(), "rate limit reached; retry in 12 seconds");
    server.join().unwrap();
}

#[tokio::test]
async fn only_server_opted_in_anonymous_recall_is_reused_from_local_sqlite() {
    let body = r#"{"schema_version":1,"reason":"ok","hits":[]}"#;
    let (endpoint, server) = serve_once(
        "200 OK",
        "Content-Type: application/json\r\nX-WAM-Client-Cache: anonymous-recall; max-age=300; stale-if-error=900\r\n",
        body,
    );
    let path = temp_cache_path();
    let mut client = HttpClient::new(endpoint, None).unwrap();
    client.enable_local_cache_at(&path).unwrap();
    let request = RecallRequest {
        url: "https://example.com/guide?private=value".to_owned(),
        ..RecallRequest::default()
    };
    let network = client.recall(request.clone()).await.unwrap();
    assert_eq!(network.cache_status, Some(RecallCacheStatus::Network));
    server.join().unwrap();
    let cached = client.recall(request).await.unwrap();
    assert_eq!(cached.cache_status, Some(RecallCacheStatus::Fresh));
    let status = client.cache_status().await.unwrap();
    assert_eq!(status.entries, 1);
    drop(client);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[tokio::test]
async fn anonymous_recall_uses_a_bounded_stale_entry_only_after_transport_failure() {
    let body = r#"{"schema_version":1,"reason":"ok","hits":[]}"#;
    let (endpoint, server) = serve_once(
        "200 OK",
        "Content-Type: application/json\r\nX-WAM-Client-Cache: anonymous-recall; max-age=1; stale-if-error=900\r\n",
        body,
    );
    let path = temp_cache_path();
    let mut client = HttpClient::new(endpoint, None).unwrap();
    client.enable_local_cache_at(&path).unwrap();
    let request = RecallRequest {
        url: "https://example.com/guide".to_owned(),
        ..RecallRequest::default()
    };
    assert_eq!(
        client.recall(request.clone()).await.unwrap().cache_status,
        Some(RecallCacheStatus::Network)
    );
    server.join().unwrap();
    std::thread::sleep(Duration::from_millis(1_100));
    let stale = client.recall(request).await.unwrap();
    assert_eq!(stale.cache_status, Some(RecallCacheStatus::Stale));
    drop(client);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[tokio::test]
async fn authenticated_recall_bypasses_local_cache_even_if_server_opts_in() {
    let body = r#"{"schema_version":1,"reason":"ok","hits":[],"receipt_id":"receipt-a"}"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-WAM-Client-Cache: anonymous-recall; max-age=300; stale-if-error=900\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let path = temp_cache_path();
    let mut client =
        HttpClient::new(format!("http://{address}"), Some("device-token".into())).unwrap();
    client.enable_local_cache_at(&path).unwrap();
    let request = RecallRequest {
        url: "https://example.com/guide".to_owned(),
        ..RecallRequest::default()
    };
    for _ in 0..2 {
        let response = client.recall(request.clone()).await.unwrap();
        assert_eq!(response.receipt_id.as_deref(), Some("receipt-a"));
        assert_eq!(response.cache_status, None);
    }
    server.join().unwrap();
    assert_eq!(client.cache_status().await.unwrap().entries, 0);
    drop(client);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
