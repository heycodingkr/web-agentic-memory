mod account_fixture;
use browser_memory::{tools, web, Store};
use host::settings::Settings;
use serde_json::{json, Value};
use std::{io::{Read, Write}, net::TcpListener, path::Path, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicUsize, Ordering}}, thread};

fn read_request(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        if bytes.windows(4).any(|chunk| chunk == b"\r\n\r\n") { return Ok(bytes); }
        if bytes.len() >= 16_384 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "request headers too large"));
        }
        let mut chunk = [0; 1024];
        let capacity = (16_384 - bytes.len()).min(chunk.len());
        let n = reader.read(&mut chunk[..capacity])?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "incomplete request headers"));
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
}

#[test]
fn fixture_waits_for_complete_fragmented_request_headers() {
    let request = b"GET /web-memory/v1/catalog HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer fixture\r\n\r\n";
    let mut fragments = request[..9].chain(&request[9..]);
    assert_eq!(read_request(&mut fragments).unwrap(), request);
}

#[test]
fn fixture_rejects_truncated_request_headers() {
    assert!(read_request(&mut &b"GET / HTTP/1.1\r\nHost: localhost\r\n"[..]).is_err());
}

#[test]
fn fixture_rejects_oversized_request_headers() {
    assert!(read_request(&mut vec![b'x'; 16_385].as_slice()).is_err());
}

struct Server {
    url: String, body: Arc<Mutex<(u16, String)>>, requests: Arc<Mutex<Vec<String>>>,
    calls: Arc<AtomicUsize>, stop: Arc<AtomicBool>, task: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(body: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = Arc::new(Mutex::new((200, body.to_string())));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (payload, seen, count, done) = (body.clone(), requests.clone(), calls.clone(), stop.clone());
        let task = thread::spawn(move || while !done.load(Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener.accept() {
                stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
                let Ok(bytes) = read_request(&mut stream) else { continue; };
                seen.lock().unwrap().push(String::from_utf8_lossy(&bytes).to_string());
                count.fetch_add(1, Ordering::SeqCst);
                let (status, body) = payload.lock().unwrap().clone();
                write!(stream, "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).ok();
            } else { thread::sleep(std::time::Duration::from_millis(2)); }
        });
        Self { url, body, requests, calls, stop, task: Some(task) }
    }
}
impl Drop for Server {
    fn drop(&mut self) { self.stop.store(true, Ordering::SeqCst); self.task.take().unwrap().join().unwrap(); }
}
fn note(site: &str) -> Value {
    json!({"id":"editor","kind":"workflow","site":site,"title":"Write draft","summary":"WEB_HINT: focus visible body.",
        "detail":"WEB_DETAIL: use native input.","evidence":"isolated smoke passed","tags":["draft"],
        "status":"approved","verified_at":"2026-10-16"})
}
fn catalog(entries: Value) -> Value { json!({"schema_version":1,"revision":"release-1","entries":entries}) }
fn settings(root: &Path, url: &str, enabled: bool) {
    let mut value = Settings::default();
    value.web_agentic_memory_enabled = enabled;
    value.web_agentic_memory_endpoint = url.into();
    value.telemetry_enabled = false;
    value.save(&root.join("settings.toml")).unwrap();
    account_fixture::login(root,url,"catalog-fixture-user");
}
#[test]
fn toggles_change_server_calls_and_hints_while_personal_notes_remain_usable() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(catalog(json!([note("medium.com")])));
    settings(root.path(), &server.url, false);
    let saved = tools::dispatch(root.path(), tools::SAVE, &json!({"kind":"workflow","site":"medium.com",
        "title":"Personal preference","summary":"PERSONAL_HINT","detail":"PERSONAL_PRIVATE_DETAIL"})).unwrap();
    let before = std::fs::read(Store::db_path(root.path())).unwrap();
    assert!(!web::refresh(root.path(), true));
    assert!(web::turn_context(root.path(), "medium.com", "draft").is_none());
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    assert!(tools::call_for_agent(root.path(), tools::RECALL, &json!({"site":"medium.com","query":"personal"})).unwrap().contains("PERSONAL_HINT"));
    settings(root.path(), &server.url, true);
    assert!(web::refresh(root.path(), true));
    let hints = web::turn_context(root.path(), "https://medium.com/editor?token=PRIVATE", "draft").unwrap();
    assert!(hints.contains("WEB_HINT") && hints.contains("untrusted"));
    assert!(!hints.contains("WEB_DETAIL") && !hints.contains("PRIVATE"));
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    settings(root.path(), &server.url, false);
    assert!(web::turn_context(root.path(), "medium.com", "draft").is_none());
    assert!(!web::refresh(root.path(), true));
    assert!(tools::call_for_agent(root.path(), tools::EXPAND, &json!({"id":saved["id"],"level":2})).unwrap().contains("PERSONAL_PRIVATE_DETAIL"));
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(Store::db_path(root.path())).unwrap(), before);
}
#[test]
fn corrupt_personal_store_does_not_block_web_recall_or_get_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(catalog(json!([note("medium.com")])));
    settings(root.path(), &server.url, true);
    assert!(web::refresh(root.path(),true));
    let path=Store::db_path(root.path());
    std::fs::write(&path,b"corrupt personal store sentinel").unwrap();
    let text=tools::call_for_agent(root.path(),tools::RECALL,&json!({"site":"medium.com","query":"draft"})).unwrap();
    assert!(text.contains("WEB_HINT") && text.contains("Personal memory unavailable"));
    assert!(tools::call_for_agent(root.path(),tools::RECALL,&json!({"site":"medium.com","unknown":"x"})).is_err());
    assert_eq!(std::fs::read(path).unwrap(),b"corrupt personal store sentinel");
}

#[test]
fn web_ids_expand_read_only_and_cannot_collide_with_personal_ids() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(catalog(json!([note("medium.com")])));
    settings(root.path(), &server.url, true);
    assert!(web::refresh(root.path(), true));
    let recall = tools::call_for_agent(root.path(), tools::RECALL, &json!({"site":"medium.com"})).unwrap();
    assert!(recall.contains("web:editor"), "{recall}");
    assert!(tools::call_for_agent(root.path(), tools::EXPAND, &json!({"id":"web:editor","level":2})).unwrap().contains("WEB_DETAIL"));
    assert!(tools::call_for_agent(root.path(), tools::FORGET, &json!({"ids":["web:editor"]})).is_err());
    assert!(tools::call_for_agent(root.path(), tools::SAVE, &json!({"kind":"workflow","title":"bad replace","summary":"x","replaces":"web:editor"})).is_err());
    assert!(!Store::db_path(root.path()).exists());
    settings(root.path(), &server.url, false);
    assert!(tools::call_for_agent(root.path(), tools::EXPAND, &json!({"id":"web:editor","level":2})).unwrap_err().contains("disabled"));
}
#[test]
fn invalid_payloads_never_replace_valid_cache_and_empty_release_revokes_bundled_notes() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(catalog(json!([note("medium.com")])));
    settings(root.path(), &server.url, true);
    assert!(web::refresh(root.path(), true));
    let mut poison = note("medium.com");
    poison["summary"] = json!("password=never-leak");
    for body in [json!({"schema_version":99,"revision":"bad","entries":[]}),
        catalog(json!([{ "id":"poison","status":"pending" }])),
        catalog(json!([poison])),
        catalog(json!([note("medium.com"),note("medium.com")]))] {
        *server.body.lock().unwrap() = (200, body.to_string());
        assert!(!web::refresh(root.path(), true));
        assert!(web::turn_context(root.path(), "medium.com", "draft").unwrap().contains("WEB_HINT"));
    }
    *server.body.lock().unwrap() = (503, "not available".into());
    assert!(!web::refresh(root.path(), true));
    assert!(web::turn_context(root.path(), "medium.com", "draft").is_some());
    *server.body.lock().unwrap() = (200, catalog(json!([])).to_string());
    assert!(web::refresh(root.path(), true));
    assert!(web::turn_context(root.path(), "medium.com", "draft").is_none());
}
#[test]
fn offline_first_use_is_bounded_domain_scoped_and_never_creates_personal_database() {
    let root = tempfile::tempdir().unwrap();
    settings(root.path(),"http://127.0.0.1:1",true);
    let hints = web::turn_context(root.path(), "https://medium.com/new-story", "draft").unwrap();
    assert!(hints.contains("Medium"));
    assert!(browser_memory::text::est_tokens(&hints) <= 400);
    for site in ["evilmedium.com","other.test","file:///tmp/editor"] {
        assert!(web::turn_context(root.path(), site, "draft").is_none());
    }
    assert!(!Store::db_path(root.path()).exists());
    assert!(!web::refresh(root.path(), true), "an unavailable loopback endpoint preserves offline member hints");
}
#[test]
fn disabling_during_a_request_discards_the_response_before_caching_or_injection() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(catalog(json!([note("medium.com")])));
    settings(root.path(), &server.url, true);
    let guard = server.body.lock().unwrap();
    let path = root.path().to_path_buf();
    let request = thread::spawn(move || web::refresh(&path, true));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while server.calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    settings(root.path(), &server.url, false);
    drop(guard);
    assert!(!request.join().unwrap());
    assert!(!web::cache_path(root.path()).exists());
    assert!(web::turn_context(root.path(), "medium.com", "draft").is_none());
}

#[test]
fn network_request_contains_no_task_text_or_page_url_and_endpoint_changes_discard_old_cache() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(catalog(json!([note("medium.com")])));
    settings(root.path(), &server.url, true);
    assert!(web::refresh(root.path(), true));
    web::turn_context(root.path(), "https://medium.com/private?token=HIDDEN", "PRIVATE ARTICLE SENTINEL");
    let request = server.requests.lock().unwrap().join("\n");
    assert!(request.starts_with("GET /web-memory/v1/catalog "));
    assert!(!request.contains("HIDDEN") && !request.contains("ARTICLE") && request.to_ascii_lowercase().contains("authorization: bearer member-fixture-token"));
    let other = Server::new(catalog(json!([])));
    settings(root.path(), &other.url, true);
    assert!(!web::turn_context(root.path(), "medium.com", "draft").unwrap_or_default().contains("WEB_HINT"));
    assert!(web::refresh(root.path(), true));
    assert!(web::turn_context(root.path(), "medium.com", "draft").is_none());
}
