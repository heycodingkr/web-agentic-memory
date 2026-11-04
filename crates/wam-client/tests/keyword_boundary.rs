use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use wam_client::{ApiError, HttpClient, WamApi};
use wam_contracts::RecallRequest;

struct Fixture {
    endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(vocab_available: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                let (status, body) = if request.starts_with("GET /v1/memories/vocab ") {
                    if vocab_available {
                        (
                            "200 OK",
                            r#"{"schema_version":1,"version":"review-1","task_categories":["research"],"keywords":["blog"]}"#,
                        )
                    } else {
                        ("503 Service Unavailable", "{}")
                    }
                } else if request.starts_with("POST /v1/telemetry/events ") {
                    ("204 No Content", "")
                } else {
                    ("200 OK", r#"{"schema_version":1,"reason":"ok","hits":[]}"#)
                };
                recorded.lock().unwrap().push(request);
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn captured(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
#[tokio::test]
async fn recall_filters_private_unknown_keywords_before_transport() {
    let fixture = Fixture::new(true);
    let client = HttpClient::new(&fixture.endpoint, None).unwrap();
    client
        .recall(RecallRequest {
            url: "https://example.com/guide?private-review-sentinel=1".into(),
            keywords: vec![
                "  ＢＬＯＧ  ".into(),
                "private-review-sentinel".into(),
                "blog".into(),
            ],
            task_category: Some("research".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let requests = fixture.captured();
    assert!(!requests.join("\n").contains("private-review-sentinel"));
    let request = requests
        .iter()
        .find(|r| r.starts_with("POST /v1/memories/recall "))
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["keywords"], serde_json::json!(["blog"]));
    assert_eq!(body["url"], "https://example.com/guide");
}
#[tokio::test]
async fn recall_never_falls_back_to_raw_keywords_when_vocab_is_unavailable() {
    let fixture = Fixture::new(false);
    let client = HttpClient::new(&fixture.endpoint, None).unwrap();
    let result = client
        .recall(RecallRequest {
            url: "https://example.com/guide".into(),
            keywords: vec!["private-review-sentinel".into()],
            ..Default::default()
        })
        .await;
    assert_eq!(result, Err(ApiError::Server));
    let requests = fixture.captured().join("\n");
    assert!(!requests.contains("private-review-sentinel"));
    assert!(!requests.contains("POST /v1/memories/recall "));
}
#[tokio::test]
async fn concurrent_request_failures_keep_their_own_trace_references() {
    let fixture = Fixture::new(true);
    let client = HttpClient::new(&fixture.endpoint, None).unwrap();
    let first = client.for_request();
    let second = client.for_request();
    let (a, b) = tokio::join!(first.status(), second.status());
    assert_eq!(a, Err(ApiError::InvalidResponse));
    assert_eq!(b, Err(ApiError::InvalidResponse));
    let first_trace = first.last_trace_id().unwrap();
    let second_trace = second.last_trace_id().unwrap();
    assert_ne!(first_trace, second_trace);
    assert!(client.last_trace_id().is_none());
    let captured = fixture.captured().join("\n").to_ascii_lowercase();
    assert!(captured.contains(&format!("x-wam-trace-id: {first_trace}")));
    assert!(captured.contains(&format!("x-wam-trace-id: {second_trace}")));
}

#[tokio::test]
async fn unknown_task_is_rejected_locally_without_transmitting_its_value() {
    let fixture = Fixture::new(true);
    let client = HttpClient::new(&fixture.endpoint, None).unwrap();
    assert_eq!(
        client
            .recall(RecallRequest {
                url: "https://example.com/guide".into(),
                task_category: Some("private-review-sentinel".into()),
                ..Default::default()
            })
            .await,
        Err(ApiError::InvalidInput)
    );
    assert!(
        !fixture
            .captured()
            .join("\n")
            .contains("private-review-sentinel")
    );
}
