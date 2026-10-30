use crate::{
    Command,
    account::{self, AccountError},
};
use serde::Serialize;
use wam_client::{
    ApiError, HttpClient, WamApi,
    credentials::{CredentialStore, config_dir},
    device::DeviceApi,
};

pub fn exit_code(error: ApiError) -> i32 {
    match error {
        ApiError::InvalidInput | ApiError::InvalidEndpoint => 2,
        ApiError::Unauthorized => 3,
        ApiError::Forbidden | ApiError::Conflict => 4,
        ApiError::Transport | ApiError::Server => 5,
        ApiError::RateLimited { .. } => 6,
        _ => 1,
    }
}

pub fn requires_auth(command: &Command) -> bool {
    matches!(
        command,
        Command::Rank { .. }
            | Command::Usage { .. }
            | Command::Propose { .. }
            | Command::Contributions { .. }
            | Command::Withdraw { .. }
            | Command::Outcome { .. }
    )
}

/// Local commands return their output and diagnostic exit code together.
pub async fn execute(
    client: &HttpClient,
    store: &dyn CredentialStore,
    command: &Command,
) -> Result<Option<(String, i32)>, AccountError> {
    match command {
        Command::Link { no_browser, json } => {
            let report = account::link(client, store, |uri, code| {
                eprintln!("Open {uri} and enter code {code}");
                if !no_browser && !open_browser(uri) {
                    eprintln!("Browser could not be opened; use the URL above.");
                }
            })
            .await?;
            let output = if *json {
                crate::render_json(&report)?
            } else {
                format!(
                    "Device linked. Credential saved in OS secure storage.\nScopes: {}",
                    report.scopes.join(", ")
                )
            };
            Ok(Some((output, 0)))
        }
        Command::Unlink { local_only, json } => {
            let report = account::unlink(client, store, *local_only).await?;
            let output = if *json {
                crate::render_json(&report)?
            } else if !report.local_removed {
                "No saved device credential for this endpoint.".into()
            } else if report.server_revoked {
                "Device revoked and local credential removed.".into()
            } else {
                "Local credential removed. Revoke the device in the web device settings.".into()
            };
            Ok(Some((output, 0)))
        }
        Command::Doctor { json } => {
            let api_result = client.status().await;
            let cache = client.cache_status().await.ok();
            let mut code = api_result.err().map_or(0, exit_code);
            let auth = match store.load() {
                Ok(None) => "anonymous",
                Ok(Some(credential)) => match client.link_status(&credential.access_token).await {
                    Ok(_) => "linked",
                    Err(ApiError::Unauthorized) => {
                        code = 3;
                        "reauth_required"
                    }
                    Err(error) => {
                        code = exit_code(error);
                        "unverified"
                    }
                },
                Err(_) => {
                    if code == 0 {
                        code = 1;
                    }
                    "storage_unavailable"
                }
            };
            #[derive(Serialize)]
            struct Doctor<'a> {
                schema_version: u8,
                api: &'a str,
                authentication: &'a str,
                config_dir: String,
                cache: &'a str,
                credential_storage: &'a str,
            }
            let report = Doctor {
                schema_version: 1,
                api: if api_result.is_ok() {
                    "reachable"
                } else {
                    "unreachable"
                },
                authentication: auth,
                config_dir: config_dir().display().to_string(),
                cache: if cache.is_some() {
                    "available"
                } else {
                    "unavailable"
                },
                credential_storage: "OS secure store",
            };
            let output = if *json {
                crate::render_json(&report)?
            } else {
                format!(
                    "API: {}\nAuthentication: {}\nCredential storage: {}\nConfig directory: {}\nCache: {}",
                    report.api,
                    report.authentication,
                    report.credential_storage,
                    report.config_dir,
                    report.cache
                )
            };
            Ok(Some((output, code)))
        }
        _ => Ok(None),
    }
}

fn open_browser(uri: &str) -> bool {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(uri);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", uri]);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(uri);
        command
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide::GuideAction;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread::{self, JoinHandle},
        time::Duration,
    };
    use wam_client::credentials::{CredentialError, LinkedCredential};

    struct TestStore(Result<Option<LinkedCredential>, CredentialError>);

    impl CredentialStore for TestStore {
        fn load(&self) -> Result<Option<LinkedCredential>, CredentialError> {
            self.0.clone()
        }

        fn save(&self, _: &LinkedCredential) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }

        fn delete(&self) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }
    }

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

    fn serve(
        replies: Vec<(&'static str, &'static str)>,
    ) -> (String, mpsc::Receiver<String>, JoinHandle<()>) {
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

    fn doctor() -> Command {
        Command::Doctor { json: true }
    }

    #[test]
    fn local_exit_codes_keep_input_auth_and_throttle_classes_distinct() {
        for (error, expected) in [
            (ApiError::InvalidInput, 2),
            (ApiError::InvalidEndpoint, 2),
            (ApiError::Unauthorized, 3),
            (ApiError::Forbidden, 4),
            (ApiError::Conflict, 4),
            (ApiError::Transport, 5),
            (ApiError::Server, 5),
            (
                ApiError::RateLimited {
                    retry_after_seconds: Some(9),
                },
                6,
            ),
            (ApiError::NotFound, 1),
            (ApiError::InvalidResponse, 1),
            (ApiError::CacheUnavailable, 1),
        ] {
            assert_eq!(exit_code(error), expected, "{error:?}");
        }
    }

    #[tokio::test]
    async fn doctor_reports_anonymous_health_without_touching_a_real_credential_store() {
        let (endpoint, requests, server) = serve(vec![("200 OK", r#"{"status":"ok"}"#)]);
        let client = HttpClient::new(endpoint, None).unwrap();
        let store = TestStore(Ok(None));

        let (output, code) = execute(&client, &store, &doctor()).await.unwrap().unwrap();
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(code, 0);
        assert_eq!(report["api"], "reachable");
        assert_eq!(report["authentication"], "anonymous");
        assert_eq!(report["cache"], "unavailable");
        assert!(
            requests
                .recv()
                .unwrap()
                .starts_with("GET /healthz HTTP/1.1\r\n")
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn doctor_marks_rejected_saved_credentials_for_reauthentication() {
        let (endpoint, requests, server) = serve(vec![
            ("200 OK", r#"{"status":"ok"}"#),
            ("401 Unauthorized", r#"{"error":"unauthorized"}"#),
            ("204 No Content", ""),
        ]);
        let client = HttpClient::new(endpoint, None).unwrap();
        let store = TestStore(Ok(Some(LinkedCredential {
            device_id: "device-1".into(),
            access_token: "fixture-access-token".into(),
            scopes: vec!["read:dashboard".into()],
            expires_at: 1_900_000_000,
        })));

        let (output, code) = execute(&client, &store, &doctor()).await.unwrap().unwrap();
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(code, 3);
        assert_eq!(report["api"], "reachable");
        assert_eq!(report["authentication"], "reauth_required");
        let health = requests.recv().unwrap();
        let status = requests.recv().unwrap();
        let diagnostic = requests.recv().unwrap();
        assert!(health.starts_with("GET /healthz HTTP/1.1\r\n"));
        assert!(status.starts_with("GET /v1/link/status HTTP/1.1\r\n"));
        assert!(
            status
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-access-token")
        );
        assert!(diagnostic.starts_with("POST /v1/telemetry/events HTTP/1.1\r\n"));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn doctor_reports_secure_store_failure_without_leaking_it_as_auth_state() {
        let (endpoint, requests, server) = serve(vec![("200 OK", r#"{"status":"ok"}"#)]);
        let client = HttpClient::new(endpoint, None).unwrap();
        let store = TestStore(Err(CredentialError::Unavailable));

        let (output, code) = execute(&client, &store, &doctor()).await.unwrap().unwrap();
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(code, 1);
        assert_eq!(report["authentication"], "storage_unavailable");
        assert_eq!(report["credential_storage"], "OS secure store");
        assert!(
            requests
                .recv()
                .unwrap()
                .starts_with("GET /healthz HTTP/1.1\r\n")
        );
        server.join().unwrap();
    }

    #[test]
    fn local_auth_gate_is_limited_to_owner_commands() {
        assert!(requires_auth(&Command::Usage { json: false }));
        assert!(requires_auth(&Command::Rank { json: false }));
        assert!(!requires_auth(&Command::Leaderboard { json: false }));
        assert!(!requires_auth(&Command::Guide {
            action: GuideAction::Print {
                host: crate::install::Host::Codex,
                write: false,
            },
        }));
    }
}
