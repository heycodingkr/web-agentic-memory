use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::Duration,
};
use wam_client::{
    ApiError, HttpClient, WamApi,
    device::{DeviceApi, LinkPoll},
};
use wam_contracts::{
    CONTRIBUTION_CONSENT_VERSION, CreateContributionRequest, MemoryOutcome, OutcomeReportRequest,
    ProfilePutRequest,
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

#[tokio::test]
async fn device_secret_is_not_forwarded_to_an_http_redirect_target() {
    let first = TcpListener::bind("127.0.0.1:0").unwrap();
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", first.local_addr().unwrap());
    let location = format!("http://{}/capture", target.local_addr().unwrap());
    target.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = first.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).unwrap();
        assert!(count > 0);
        socket.write_all(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
    });
    let mut client = HttpClient::new(endpoint, None).unwrap();
    client.disable_telemetry_spool();
    assert!(matches!(
        client.poll_link("synthetic-device-code").await,
        Err(ApiError::Server)
    ));
    server.join().unwrap();
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn anonymous_private_operations_and_missing_consent_never_reach_the_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client =
        HttpClient::new(format!("http://{}", listener.local_addr().unwrap()), None).unwrap();
    assert!(matches!(
        client.dashboard().await,
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        client.my_usage().await,
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        client.my_profile().await,
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        client.my_contributions().await,
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        client.withdraw_contribution("contribution-1").await,
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        client
            .save_profile(ProfilePutRequest {
                handle: "tester".into(),
                bio: vec![],
                service_links: vec![],
                public_opt_in: false,
            })
            .await,
        Err(ApiError::Unauthorized)
    ));
    assert!(matches!(
        client
            .report_outcome(
                "receipt",
                OutcomeReportRequest {
                    outcome: MemoryOutcome::Success,
                    used_version_ids: vec![],
                }
            )
            .await,
        Err(ApiError::Unauthorized)
    ));
    client.set_access_token(Some("synthetic-development-token".into()));
    assert!(matches!(
        client
            .submit_contribution(CreateContributionRequest {
                url: "https://example.com".into(),
                task_category: "research".into(),
                actions: vec![],
                consent: false,
                consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
                nonce: None,
            })
            .await,
        Err(ApiError::Forbidden)
    ));
    assert!(matches!(
        client
            .report_outcome(
                "",
                OutcomeReportRequest {
                    outcome: MemoryOutcome::Success,
                    used_version_ids: vec![],
                }
            )
            .await,
        Err(ApiError::InvalidInput)
    ));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn device_api_uses_closed_routes_and_keeps_link_credentials_out_of_urls() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let server = thread::spawn(move || {
        let replies = [
            (
                "200 OK",
                r#"{"device_code":"device-secret","user_code":"ABCD-EFGH","verification_uri":"https://wam.example/device","expires_in":600,"interval":5}"#,
            ),
            ("200 OK", r#"{"status":"pending","interval":7}"#),
            (
                "200 OK",
                r#"{"status":"authorized","device_id":"device-1","access_token":"access-secret","scopes":["read:memories"],"expires_at":1900000000}"#,
            ),
            (
                "200 OK",
                r#"{"id":"device-1","name":"WAM CLI","scopes":["read:memories"],"expires_at":1900000000}"#,
            ),
            ("204 No Content", ""),
        ];
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

    let client = HttpClient::new(endpoint, None).unwrap();
    let start = client.start_link().await.unwrap();
    assert_eq!(start.user_code, "ABCD-EFGH");
    assert_eq!(start.interval, 5);
    assert!(matches!(
        client.poll_link("device-secret").await.unwrap(),
        LinkPoll::Pending { interval: 7 }
    ));
    assert!(matches!(
        client.poll_link("device-secret").await.unwrap(),
        LinkPoll::Authorized { device_id, access_token, .. }
            if device_id == "device-1" && access_token == "access-secret"
    ));
    assert_eq!(
        client.link_status("access-secret").await.unwrap().name,
        "WAM CLI"
    );
    client.revoke_link("access-secret").await.unwrap();

    let requests: Vec<_> = (0..5).map(|_| receiver.recv().unwrap()).collect();
    assert!(requests[0].starts_with("POST /v1/link/start HTTP/1.1\r\n"));
    let start_body: serde_json::Value =
        serde_json::from_str(requests[0].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(start_body["scopes"].as_array().unwrap().len(), 4);
    assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));

    for request in &requests[1..3] {
        assert!(request.starts_with("POST /v1/link/poll HTTP/1.1\r\n"));
        assert!(!request.lines().next().unwrap().contains("device-secret"));
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["device_code"], "device-secret");
    }
    assert!(requests[3].starts_with("GET /v1/link/status HTTP/1.1\r\n"));
    assert!(
        requests[3]
            .to_ascii_lowercase()
            .contains("authorization: bearer access-secret")
    );
    assert!(requests[4].starts_with("POST /v1/link/revoke HTTP/1.1\r\n"));
    assert!(
        requests[4]
            .to_ascii_lowercase()
            .contains("authorization: bearer access-secret")
    );
    server.join().unwrap();
}

#[test]
fn link_poll_deserializer_accepts_only_known_lifecycle_states() {
    for body in [
        r#"{"status":"denied"}"#,
        r#"{"status":"expired"}"#,
        r#"{"status":"already_used"}"#,
    ] {
        assert!(serde_json::from_str::<LinkPoll>(body).is_ok(), "{body}");
    }
    assert!(serde_json::from_str::<LinkPoll>(r#"{"status":"unknown"}"#).is_err());
}
