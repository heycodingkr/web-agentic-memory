use clap::Parser;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use wam_cli::Cli;

#[test]
fn recall_accepts_stdin_without_putting_a_url_in_argv() {
    assert!(Cli::try_parse_from(["wam", "recall", "--stdin", "--json"]).is_ok());
    for flags in [
        vec!["--stdin", "https://example.com"],
        vec!["--stdin", "--task", "research"],
        vec!["--stdin", "--limit", "2"],
    ] {
        assert!(Cli::try_parse_from(["wam", "recall"].into_iter().chain(flags)).is_err());
    }
}

struct IsolatedCommand {
    command: Command,
    directory: std::path::PathBuf,
}
impl std::ops::Deref for IsolatedCommand {
    type Target = Command;
    fn deref(&self) -> &Command {
        &self.command
    }
}
impl std::ops::DerefMut for IsolatedCommand {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.command
    }
}
impl Drop for IsolatedCommand {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn command() -> IsolatedCommand {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "wam-cli-input-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut command = Command::new(env!("CARGO_BIN_EXE_wam"));
    command
        .env("WAM_CACHE_PATH", directory.join("cache.sqlite3"))
        .env("WAM_CONFIG_DIR", directory.join("config"))
        .env("HOME", &directory);
    command
        .env("WAM_TELEMETRY", "off")
        .env("WAM_ANONYMOUS", "1")
        .env_remove("WAM_TOKEN");
    IsolatedCommand { command, directory }
}

fn request_fixture(status: &str, body: &str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("fixture request missing: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 2048];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= length {
                    break;
                }
            }
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (endpoint, handle)
}

fn with_stdin(args: &[&str], body: &[u8]) -> std::process::Output {
    let mut command = command();
    let mut child = command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A bounded reader may close its pipe before an overlarge writer finishes.
    let _ = child.stdin.take().unwrap().write_all(body);
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_recall_sends_the_same_canonical_request_and_one_json_document() {
    let (endpoint, server) = request_fixture(
        "200 OK",
        r#"{"schema_version":1,"reason":"no_matching_memories","hits":[]}"#,
    );
    let output = with_stdin(&["--endpoint", &endpoint, "--anonymous", "recall", "--stdin", "--json"],
        br#"{"url":"https://example.com/guide?sentinel-secret=1#private","keywords":["blog"],"limit":2}"#);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = server.join().unwrap();
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value["url"], "https://example.com/guide");
    assert_eq!(value["limit"], 2);
    assert!(!request.contains("sentinel-secret"));
    let _: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("sentinel-secret"));
}

#[test]
fn malformed_unknown_multiple_or_overlarge_stdin_is_rejected_without_echoing_it() {
    let inputs = [
        br#"{"url":"sentinel-secret","raw_prompt":"sentinel-secret"}"#.to_vec(),
        br#"{"url":"sentinel-secret"} {}"#.to_vec(),
        br#"{"url":"sentinel-secret","limit":0}"#.to_vec(),
        b"sentinel-secret".repeat(wam_cli::MAX_STDIN_BYTES),
    ];
    for input in inputs {
        let output = with_stdin(&["recall", "--stdin", "--json"], &input);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("sentinel-secret"));
    }
}

#[test]
fn rate_limits_use_the_documented_exit_code() {
    let (endpoint, server) =
        request_fixture("429 Too Many Requests", r#"{"error":"rate_limited"}"#);
    let output = command()
        .args(["--endpoint", &endpoint, "--anonymous", "status"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6));
    server.join().unwrap();
}

#[test]
fn anonymous_private_commands_make_no_network_write() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let output = command()
        .args([
            "--endpoint",
            &format!("http://{}", listener.local_addr().unwrap()),
            "--anonymous",
            "propose",
            "https://example.com",
            "--task",
            "research",
            "--action",
            "navigate",
            "--consent",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn invalid_endpoint_has_input_error_exit_code() {
    let output = command()
        .args(["--endpoint", "invalid", "status"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
