mod account_fixture;
use browser_memory::{tools, workflow::{self, Trace}};
use host::settings::Settings;
use serde_json::{json, Value};

fn config(root: &std::path::Path, consent: bool) {
    let mut cfg = Settings::default();
    cfg.web_workflow_contribution = consent;
    cfg.web_agentic_memory_enabled = false;
    cfg.web_agentic_memory_endpoint = "http://127.0.0.1:1".into();
    cfg.telemetry_upload_consent = true;
    cfg.save(&root.join("settings.toml")).unwrap();
    account_fixture::login(root,&cfg.web_agentic_memory_endpoint,"report-fixture-user");
}
fn event(trace: &mut Trace, root: &std::path::Path, name: &str) {
    trace.call(root, "PRIVATE_CALL_ID", name, "https://medium.com/private?token=PRIVATE_URL");
    trace.result(root, "PRIVATE_CALL_ID", false);
}
#[test]
fn registered_navigation_and_wait_tool_names_are_collected() {
    let root = tempfile::tempdir().unwrap();
    config(root.path(),true);
    let mut trace=Trace::default();
    event(&mut trace,root.path(),"mcp__arise__browser_navigate");
    event(&mut trace,root.path(),"browser_wait_for");
    trace.finish(root.path(),"completed");
    assert_eq!(workflow::pending_count(root.path()),1);
    let path=std::fs::read_dir(workflow::queue_dir(root.path())).unwrap().next().unwrap().unwrap().path();
    let value:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value["actions"],json!([{"tool":"browser_navigate","outcome":"ok"},{"tool":"browser_wait_for","outcome":"ok"}]));
}

#[test]
fn ordinary_telemetry_consent_never_authorizes_workflow_capture() {
    let root = tempfile::tempdir().unwrap();
    config(root.path(), false);
    let mut trace = Trace::default();
    event(&mut trace, root.path(), "browser_type");
    trace.finish(root.path(), "completed");
    assert_eq!(workflow::pending_count(root.path()), 0);
}
#[test]
fn minimized_report_contains_only_closed_categories_and_domain() {
    let root = tempfile::tempdir().unwrap();
    config(root.path(), true);
    let mut trace = Trace::default();
    event(&mut trace, root.path(), "mcp__arise__browser_type");
    event(&mut trace, root.path(), "read_file_PRIVATE_WORKSPACE");
    trace.finish(root.path(), "completed");
    let paths: Vec<_> = std::fs::read_dir(workflow::queue_dir(root.path())).unwrap().map(|item|item.unwrap().path()).collect();
    assert_eq!(paths.len(), 1);
    let text = std::fs::read_to_string(&paths[0]).unwrap();
    assert!(!text.contains("PRIVATE") && !text.contains("token") && !text.contains("/private") && !text.contains("read_file"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value, json!({"schema_version":1,"app_version":env!("CARGO_PKG_VERSION"),
        "domain":"medium.com","terminal":"completed","actions":[{"tool":"browser_type","outcome":"ok"}]}));
}
#[test]
fn revocation_and_master_collection_switch_discard_inflight_data_and_preserve_personal_memory() {
    let root = tempfile::tempdir().unwrap();
    config(root.path(), true);
    let saved = tools::dispatch(root.path(), tools::SAVE,
        &json!({"kind":"preference","title":"personal","summary":"KEEP_PERSONAL"})).unwrap();
    let mut trace = Trace::default();
    event(&mut trace, root.path(), "browser_click");
    config(root.path(), false);
    trace.finish(root.path(), "failed");
    assert_eq!(workflow::pending_count(root.path()), 0);
    config(root.path(), true);
    event(&mut trace, root.path(), "browser_click");
    trace.finish(root.path(), "completed");
    assert_eq!(workflow::pending_count(root.path()), 1);
    let mut cfg = Settings::load(&root.path().join("settings.toml"));
    cfg.telemetry_enabled = false;
    cfg.save(&root.path().join("settings.toml")).unwrap();
    event(&mut trace, root.path(), "browser_type");
    trace.finish(root.path(), "completed");
    workflow::purge(root.path());
    assert_eq!(workflow::pending_count(root.path()), 0);
    assert!(tools::call_text(root.path(), tools::EXPAND, &json!({"id":saved["id"],"level":2})).is_ok());
}
#[test]
fn upload_rechecks_consent_and_sends_only_validated_closed_schema_json() {
    use std::{io::{Read,Write},net::TcpListener,time::{Duration,Instant}};
    let root = tempfile::tempdir().unwrap();
    config(root.path(),true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut cfg = Settings::load(&root.path().join("settings.toml"));
    cfg.web_agentic_memory_endpoint = format!("http://{}",listener.local_addr().unwrap());
    cfg.save(&root.path().join("settings.toml")).unwrap();
    account_fixture::login(root.path(),&cfg.web_agentic_memory_endpoint,"report-fixture-user");
    let receiver = std::thread::spawn(move || {
        let start = Instant::now();
        let mut stream = loop {
            if let Ok((stream,_)) = listener.accept() { break stream; }
            assert!(start.elapsed()<Duration::from_secs(6),"report was never delivered");
            std::thread::sleep(Duration::from_millis(2));
        };
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut bytes=Vec::new();
        let mut chunk=[0;4096];
        loop {
            let n=stream.read(&mut chunk).unwrap();
            assert!(n>0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end)=bytes.windows(4).position(|part|part==b"\r\n\r\n") {
                let header=String::from_utf8_lossy(&bytes[..end]);
                let size:usize=header.lines().filter_map(|line|line.split_once(':'))
                    .find(|(key,_)|key.eq_ignore_ascii_case("content-length")).unwrap().1.trim().parse().unwrap();
                if bytes.len()>=end+4+size { break; }
            }
        }
        write!(stream,"HTTP/1.1 202 Accepted\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
        String::from_utf8(bytes).unwrap()
    });
    let mut trace=Trace::default();
    event(&mut trace,root.path(),"browser_type");
    trace.finish(root.path(),"completed");
    cfg.web_workflow_contribution=false;
    cfg.save(&root.path().join("settings.toml")).unwrap();
    assert_eq!(workflow::upload(root.path()),0);
    assert_eq!(workflow::pending_count(root.path()),0);
    cfg.web_workflow_contribution=true;
    cfg.save(&root.path().join("settings.toml")).unwrap();
    event(&mut trace,root.path(),"browser_type");
    trace.finish(root.path(),"completed");
    std::fs::write(workflow::queue_dir(root.path()).join("000-invalid.json"),
        br#"{"prompt":"PRIVATE_TYPED_TEXT","password":"PRIVATE_PASSWORD"}"#).unwrap();
    assert_eq!(workflow::upload(root.path()),1);
    let request=receiver.join().unwrap();
    assert!(request.starts_with("POST /web-memory/v1/reports "));
    assert!(!request.contains("PRIVATE") && request.to_ascii_lowercase().contains("authorization: bearer member-fixture-token"));
    let body:Value=serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["domain"],"medium.com");
    assert_eq!(body["actions"],json!([{"tool":"browser_type","outcome":"ok"}]));
    assert_eq!(workflow::pending_count(root.path()),0);
}

#[test]
fn non_browser_actions_and_non_web_pages_never_become_reports() {
    let root = tempfile::tempdir().unwrap();
    config(root.path(), true);
    let mut trace = Trace::default();
    for (id,tool,site) in [("1","bash","medium.com"),("2","browser_type","file:///private/report"),
        ("3","browser_click","http://127.0.0.1/private"),("4","browser_memory_save","medium.com")] {
        trace.call(root.path(),id,tool,site);
        trace.result(root.path(),id,false);
    }
    trace.finish(root.path(),"completed");
    assert_eq!(workflow::pending_count(root.path()),0);
}
