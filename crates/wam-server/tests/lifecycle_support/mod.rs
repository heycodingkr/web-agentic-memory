#![allow(dead_code)]
use serde_json::{Value, json};
use std::cell::Cell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use wam_contracts::{CONTRIBUTION_CONSENT_VERSION, ContributionAction, CreateContributionRequest};
use wam_gatekeeper::provider::{
    LlmProvider, Message, ModelTurn, ProviderError, ScriptStep, ScriptedProvider, TurnRequest,
};
use wam_server::{
    ApplicationRepository, WamService,
    application::ServiceError,
    durable::{ReplayEvidence, ReplayRequest, ReplayVerifier},
    ports::Repository,
};
pub const AUTHOR: &str = "github_author";
pub const READER: &str = "github_reader";
pub const SUPPORTER: &str = "github_supporter";
pub fn prepare(repository: &ApplicationRepository) -> WamService<ApplicationRepository> {
    for account in [AUTHOR, READER, SUPPORTER] {
        repository.ensure_account(account).unwrap();
    }
    WamService::durable(repository.clone()).unwrap()
}
pub fn contribution(service: &WamService<ApplicationRepository>, account: &str) -> String {
    service
        .create_contribution(
            account,
            CreateContributionRequest {
                url: "https://example.org/editor?private-sentinel=not-stored#secret".into(),
                task_category: "compose-draft".into(),
                actions: vec![
                    ContributionAction::Navigate,
                    ContributionAction::Edit,
                    ContributionAction::Save,
                ],
                consent: true,
                consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
                nonce: None,
            },
        )
        .unwrap()
        .id
}
/// A contribution on `route` of the example site (the default helper uses `/editor`).
pub fn contribution_on(
    service: &WamService<ApplicationRepository>,
    account: &str,
    route: &str,
) -> String {
    service
        .create_contribution(
            account,
            CreateContributionRequest {
                url: format!("https://example.org{route}"),
                task_category: "compose-draft".into(),
                actions: vec![
                    ContributionAction::Navigate,
                    ContributionAction::Edit,
                    ContributionAction::Save,
                ],
                consent: true,
                consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
                nonce: None,
            },
        )
        .unwrap()
        .id
}
/// The record `record()` describes, for `route`.
pub fn record_on(route: &str) -> Value {
    let mut payload = record();
    payload["l0"]["url_patterns"] = json!([route]);
    payload
}
pub fn record() -> Value {
    json!({
        "l0":{"kind":"workflow","site_host":"example.org","url_patterns":["/editor"],"task_category":"compose-draft","intent_key":"compose-draft","keywords":["draft","write"]},
        "l1":{"title":"Save an editor draft","summary":"Open the editor, enter the draft, and save it.","key_points":[],"pitfalls":[]},
        "l2":{"steps":[{"action":"navigate"},{"action":"type"},{"action":"click"}]}
    })
}
pub fn provider(band: &str) -> ScriptedProvider {
    scripted_record(record(), band)
}
pub fn scripted_record(record: Value, band: &str) -> ScriptedProvider {
    ScriptedProvider::new([ScriptStep::Call(
        "decide_create".into(),
        json!({
            "record":record,"reason_codes":["no_existing_in_scope"],"confidence_band":band,
        }),
    )])
}
pub fn replacement(existing: &str, expected: u32) -> ScriptedProvider {
    let mut payload = record();
    payload["l1"]["summary"] = "Save the draft using the updated editor order.".into();
    payload["l2"]["steps"] = json!([{"action":"navigate"},{"action":"click"},{"action":"type"}]);
    ScriptedProvider::new([ScriptStep::Call(
        "decide_replace".into(),
        json!({
            "existing_id":existing,"expected_version":expected,"record":payload,
            "reason_codes":["works_after_ui_change"],"confidence_band":"high",
        }),
    )])
}
pub fn improvement(service: &WamService<ApplicationRepository>, account: &str) -> String {
    service
        .create_contribution(
            account,
            CreateContributionRequest {
                url: "https://example.org/editor".into(),
                task_category: "compose-draft".into(),
                actions: vec![
                    ContributionAction::Navigate,
                    ContributionAction::Save,
                    ContributionAction::Edit,
                ],
                consent: true,
                consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
                nonce: None,
            },
        )
        .unwrap()
        .id
}
pub struct PassingReplay;
impl ReplayVerifier for PassingReplay {
    fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError> {
        Ok(ReplayEvidence {
            input_digest: request.input_digest.clone(),
            samples: 3,
            baseline_successes: 3,
            candidate_successes: 3,
            baseline_tool_calls: 30,
            candidate_tool_calls: 15,
        })
    }
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A provider whose quota or credit is spent: every call is refused, and the calls are counted.
pub struct Exhausted {
    pub calls: usize,
    pub retry_after_secs: Option<u64>,
}
pub fn exhausted(retry_after_secs: Option<u64>) -> Exhausted {
    Exhausted {
        calls: 0,
        retry_after_secs,
    }
}
impl LlmProvider for Exhausted {
    fn name(&self) -> String {
        "test:exhausted".into()
    }
    fn next_turn(&mut self, _: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        Err(ProviderError::QuotaExhausted {
            why: "Billing account issue".into(),
            retry_after_secs: self.retry_after_secs,
        })
    }
}

/// Wraps a provider so that every turn says what it cost, as a provider with a price list does.
pub struct Costly<P> {
    pub inner: P,
    pub cost_usd: f64,
    pub calls: usize,
}
impl<P: LlmProvider> LlmProvider for Costly<P> {
    fn name(&self) -> String {
        self.inner.name()
    }
    fn reports_cost(&self) -> bool {
        true
    }
    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        let mut turn = self.inner.next_turn(request)?;
        turn.meta = json!({"cost_usd": self.cost_usd});
        Ok(turn)
    }
}

/// A replay verifier that passes and counts how often it was asked.
pub struct CountingReplay(pub Cell<usize>);
impl CountingReplay {
    pub fn new() -> Self {
        Self(Cell::new(0))
    }
    pub fn asked(&self) -> usize {
        self.0.get()
    }
}
impl ReplayVerifier for CountingReplay {
    fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError> {
        self.0.set(self.0.get() + 1);
        PassingReplay.verify(request)
    }
}

/// A provider that is down for now: every call fails the way an overloaded or unreachable provider does, and the
/// calls are counted (docs/spec/provider-outage.md).
pub struct Down {
    pub calls: usize,
    pub retry_after_secs: Option<u64>,
}
pub fn down(retry_after_secs: Option<u64>) -> Down {
    Down {
        calls: 0,
        retry_after_secs,
    }
}
fn outage(retry_after_secs: Option<u64>) -> ProviderError {
    ProviderError::Unavailable {
        why: "HTTP 503 from model provider".into(),
        retry_after_secs,
    }
}
impl LlmProvider for Down {
    fn name(&self) -> String {
        "test:down".into()
    }
    fn next_turn(&mut self, _: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        Err(outage(self.retry_after_secs))
    }
}

/// Down for the first `outages` calls, then it behaves as `inner`.
pub struct Recovering<P> {
    pub inner: P,
    pub outages: usize,
    pub calls: usize,
}
impl<P: LlmProvider> LlmProvider for Recovering<P> {
    fn name(&self) -> String {
        self.inner.name()
    }
    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        if self.calls <= self.outages {
            Err(outage(None))
        } else {
            self.inner.next_turn(request)
        }
    }
}

fn turn(tool: &str, args: Value) -> ModelTurn {
    ModelTurn {
        raw: json!({"tool": tool, "args": args}).to_string(),
        tool: tool.into(),
        args,
        meta: Value::Null,
    }
}

/// A provider that is down for one route only. It reads the job first (`job_get`), then either fails like an
/// overloaded provider or creates the record for the job's route, so a job it cannot answer sits among jobs it can.
pub struct Selective {
    pub down_for: &'static str,
    /// Calls that were failed on purpose.
    pub down_calls: usize,
}
pub fn selective(down_for: &'static str) -> Selective {
    Selective {
        down_for,
        down_calls: 0,
    }
}
impl LlmProvider for Selective {
    fn name(&self) -> String {
        "test:selective".into()
    }
    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        let route = request
            .transcript
            .iter()
            .rev()
            .find_map(|message| match message {
                Message::Result { tool, value } if tool == "job_get" => {
                    value["candidate"]["url_pattern"]
                        .as_str()
                        .map(str::to_owned)
                }
                _ => None,
            });
        let Some(route) = route else {
            return Ok(turn("job_get", json!({})));
        };
        if route == self.down_for {
            self.down_calls += 1;
            return Err(outage(None));
        }
        Ok(turn(
            "decide_create",
            json!({
                "record": record_on(&route),
                "reason_codes": ["no_existing_in_scope"],
                "confidence_band": "high",
            }),
        ))
    }
}

/// One canned HTTP reply of a [`ScriptedServer`].
pub struct Reply {
    pub status: &'static str,
    pub headers: &'static str,
    pub body: String,
}
impl Reply {
    pub fn new(status: &'static str, headers: &'static str, body: impl Into<String>) -> Self {
        Self {
            status,
            headers,
            body: body.into(),
        }
    }
}

/// A loopback server that answers the n-th request with the n-th reply (the last one repeats) and keeps what it
/// received. Dropping it stops the thread.
pub struct ScriptedServer {
    pub base_url: String,
    hits: Arc<AtomicUsize>,
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    address: std::net::SocketAddr,
    handle: Option<JoinHandle<()>>,
}
impl ScriptedServer {
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
    /// The requests received so far, headers and body.
    pub fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}
impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break;
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
pub fn scripted_server(replies: Vec<Reply>) -> ScriptedServer {
    assert!(!replies.is_empty());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = {
        let (hits, seen, stop) = (hits.clone(), seen.clone(), stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let request = read_request(&mut stream);
                let index = hits.fetch_add(1, Ordering::SeqCst).min(replies.len() - 1);
                seen.lock().unwrap().push(request);
                let reply = &replies[index];
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
                        reply.status,
                        reply.body.len(),
                        reply.headers,
                        reply.body
                    )
                    .as_bytes(),
                );
            }
        })
    };
    ScriptedServer {
        base_url: format!("http://{address}/v1"),
        hits,
        seen,
        stop,
        address,
        handle: Some(handle),
    }
}

/// A provider that rejects every call: the credentials or the request are wrong (HTTP 401). Waiting does not help and
/// the job may be the cause, so this is an ordinary failure, not an outage.
pub struct Rejected {
    pub calls: usize,
}
impl LlmProvider for Rejected {
    fn name(&self) -> String {
        "test:rejected".into()
    }
    fn next_turn(&mut self, _: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        Err(ProviderError::Failed("HTTP 401 from model provider".into()))
    }
}
