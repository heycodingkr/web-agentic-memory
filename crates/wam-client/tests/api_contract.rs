use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};
use wam_client::{HttpClient, WamApi};
use wam_contracts::{
    CONTRIBUTION_CONSENT_VERSION, ContributionAction, CreateContributionRequest, MemoryOutcome,
    OutcomeReportRequest, ProfilePutRequest,
};

fn read_request(stream: &mut TcpStream) -> String {
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
            let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = headers
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

fn serve(replies: Vec<(&'static str, String)>) -> (String, mpsc::Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            sender.send(read_request(&mut stream)).unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, receiver, server)
}

#[tokio::test]
async fn version_pinning_is_encoded_and_rejects_a_server_returning_another_version() {
    let response = serde_json::json!({"schema_version":1,"memory_id":"memory/one","version_id":"v1","level":2,"content":"approved details"}).to_string();
    let (endpoint, requests, server) =
        serve(vec![("200 OK", response.clone()), ("200 OK", response)]);
    let client = HttpClient::new(endpoint, None).unwrap();
    assert_eq!(
        client
            .expand_version("memory/one", 2, Some("v1"))
            .await
            .unwrap()
            .version_id,
        "v1"
    );
    assert_eq!(
        client.expand_version("memory/one", 2, Some("v2")).await,
        Err(wam_client::ApiError::NotFound)
    );
    assert!(
        requests
            .recv()
            .unwrap()
            .starts_with("GET /v1/memories/memory%2Fone?level=2&version_id=v1 HTTP/1.1")
    );
    assert!(
        requests
            .recv()
            .unwrap()
            .starts_with("GET /v1/memories/memory%2Fone?level=2&version_id=v2 HTTP/1.1")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn owner_api_uses_safe_routes_and_canonicalizes_contribution_urls() {
    let contribution = serde_json::json!({
        "id": "contribution-1",
        "url_pattern": "https://example.com/guide",
        "task_category": "compose-draft",
        "actions": ["navigate", "save"],
        "status": "received",
        "submitted_at": 100
    })
    .to_string();
    let outcome = serde_json::json!({
        "receipt_id": "receipt/1",
        "outcome": "success",
        "recorded": true
    })
    .to_string();
    let profile = serde_json::json!({
        "schema_version": 1,
        "handle": "writer",
        "bio": [],
        "service_links": [],
        "public_opt_in": true
    })
    .to_string();
    let (endpoint, requests, server) = serve(vec![
        ("201 Created", contribution),
        ("200 OK", outcome),
        ("204 No Content", String::new()),
        ("200 OK", profile.clone()),
        ("200 OK", profile),
    ]);
    let client = HttpClient::new(endpoint, Some("synthetic-access-token".into())).unwrap();

    let created = client
        .submit_contribution(CreateContributionRequest {
            url: "https://example.com/guide?token=private#section".into(),
            task_category: "compose-draft".into(),
            actions: vec![ContributionAction::Navigate, ContributionAction::Save],
            consent: true,
            consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
            nonce: Some("retry-1".into()),
        })
        .await
        .unwrap();
    assert_eq!(created.url_pattern, "https://example.com/guide");

    let reported = client
        .report_outcome(
            "receipt/1",
            OutcomeReportRequest {
                outcome: MemoryOutcome::Success,
                used_version_ids: vec!["memory-v1".into()],
            },
        )
        .await
        .unwrap();
    assert!(reported.recorded);
    client
        .withdraw_contribution("contribution/1")
        .await
        .unwrap();
    let saved = client
        .save_profile(ProfilePutRequest {
            handle: "writer".into(),
            bio: vec![],
            service_links: vec![],
            public_opt_in: true,
        })
        .await
        .unwrap();
    assert_eq!(saved.handle, "writer");
    assert_eq!(
        client.public_profile("writer/branch").await.unwrap().handle,
        "writer"
    );

    let requests: Vec<_> = (0..5).map(|_| requests.recv().unwrap()).collect();
    assert!(requests[0].starts_with("POST /v1/me/contributions HTTP/1.1\r\n"));
    assert!(
        requests[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer synthetic-access-token")
    );
    let body: serde_json::Value =
        serde_json::from_str(requests[0].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["url"], "https://example.com/guide");
    assert!(!body.to_string().contains("private"));

    assert!(requests[1].starts_with("POST /v1/receipts/receipt%2F1/outcome HTTP/1.1\r\n"));
    assert!(requests[2].starts_with("DELETE /v1/me/contributions/contribution%2F1 HTTP/1.1\r\n"));
    assert!(requests[3].starts_with("PUT /v1/me/profile HTTP/1.1\r\n"));
    assert!(
        requests[3]
            .to_ascii_lowercase()
            .contains("authorization: bearer synthetic-access-token")
    );
    assert!(requests[4].starts_with("GET /v1/profiles/writer%2Fbranch HTTP/1.1\r\n"));
    server.join().unwrap();
}
