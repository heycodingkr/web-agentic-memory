#![cfg(unix)]

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Server {
    child: Child,
    data: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigterm_closes_idle_mcp_stream_and_exits_through_graceful_shutdown() {
    let data = std::env::temp_dir().join(format!(
        "wam-shutdown-{}-{}",
        std::process::id(),
        wam_server::diagnostics::random_id()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_wam-server"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("WAM_ENV", "development")
        .env("WAM_BIND", "127.0.0.1:0")
        .env("WAM_DATA_DIR", &data)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start disposable API");
    let stderr = child.stderr.take().unwrap();
    let mut server = Server { child, data };
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(address) = line.strip_prefix("wam-server listening on ") {
                let _ = send.send(address.to_owned());
            }
        }
    });
    let address = receive
        .recv_timeout(Duration::from_secs(10))
        .expect("API bind address");
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        reqwest::Client::new()
            .get(format!("http://{address}/mcp"))
            .header("accept", "text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .send(),
    )
    .await
    .expect("SSE headers are not buffered")
    .unwrap();
    assert!(response.status().is_success());
    assert!(
        Command::new("kill")
            .args(["-TERM", &server.child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = server.child.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("shutdown must finish despite an idle SSE");
    assert!(status.success(), "API was killed abruptly: {status}");
    tokio::time::timeout(Duration::from_secs(1), response.bytes())
        .await
        .expect("SSE closes on termination")
        .expect("SSE has a complete HTTP ending");
    assert!(server.data.join("diagnostics.sqlite").is_file());
}
