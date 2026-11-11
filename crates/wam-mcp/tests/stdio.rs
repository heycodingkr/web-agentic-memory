use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Session {
    child: Child,
    messages: mpsc::Receiver<Value>,
    directory: std::path::PathBuf,
}
impl Session {
    fn start(endpoint: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "wam-stdio-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut child = Command::new(env!("CARGO_BIN_EXE_wam-mcp"))
            .env("WAM_ENDPOINT", endpoint)
            .env("WAM_ANONYMOUS", "1")
            .env("WAM_TELEMETRY", "off")
            .env("WAM_CACHE_PATH", directory.join("cache.sqlite3"))
            .env("WAM_CONFIG_DIR", directory.join("config"))
            .env_remove("WAM_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(value) = serde_json::from_str(&line) else {
                    break;
                };
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            messages,
            directory,
        }
    }
    fn send(&mut self, message: Value) {
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }
    fn receive(&self, timeout: Duration) -> Value {
        self.messages
            .recv_timeout(timeout)
            .expect("stdio response deadline")
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
#[test]
fn slow_cancelled_read_does_not_block_ping_or_return_a_success() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (started, entered) = mpsc::channel();
    let server = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(value) => break value,
                Err(_) if std::time::Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0; 4096];
        let _ = stream.read(&mut buffer).unwrap();
        started.send(()).unwrap();
        thread::sleep(Duration::from_millis(1200));
        let body = r#"{"schema_version":1,"reason":"ok","hits":[]}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    let mut session = Session::start(&endpoint);
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}));
    assert_eq!(session.receive(Duration::from_secs(3))["id"], 1);
    session.send(json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"web_memory_recall","arguments":{"url":"https://example.com/guide"}}}));
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    session.send(
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":11}}),
    );
    session.send(json!({"jsonrpc":"2.0","id":12,"method":"ping"}));
    assert_eq!(session.receive(Duration::from_millis(600))["id"], 12);
    server.join().unwrap();
    assert!(
        session
            .messages
            .recv_timeout(Duration::from_millis(150))
            .is_err()
    );
}
#[test]
fn oversized_frame_is_rejected_and_following_ping_still_works() {
    let mut session = Session::start("http://127.0.0.1:1");
    session.send(
        json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{"padding":"x".repeat(70 * 1024)}}),
    );
    assert_eq!(
        session.receive(Duration::from_secs(3))["error"]["code"],
        -32600
    );
    session.send(json!({"jsonrpc":"2.0","id":2,"method":"ping"}));
    assert_eq!(session.receive(Duration::from_secs(3))["id"], 2);
}
#[test]
fn negotiated_versions_preserve_behavior_hints_and_legacy_omissions() {
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let mut session = Session::start("http://127.0.0.1:1");
        session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}}));
        assert_eq!(
            session.receive(Duration::from_secs(3))["result"]["protocolVersion"],
            version
        );
        session.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let response = session.receive(Duration::from_secs(3));
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 10);
        for tool in tools {
            if version == "2024-11-05" {
                assert!(tool.get("annotations").is_none());
            } else {
                for hint in [
                    "readOnlyHint",
                    "destructiveHint",
                    "idempotentHint",
                    "openWorldHint",
                ] {
                    assert!(
                        tool["annotations"][hint].is_boolean(),
                        "{}: missing boolean {hint} for {version}",
                        tool["name"]
                    );
                }
            }
            if matches!(version, "2024-11-05" | "2025-03-26") {
                assert!(tool.get("outputSchema").is_none());
            }
        }
    }
}
