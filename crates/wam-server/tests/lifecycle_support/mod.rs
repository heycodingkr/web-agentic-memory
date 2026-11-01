#![allow(dead_code)]
use serde_json::{Value, json};
use std::cell::Cell;
use wam_contracts::{CONTRIBUTION_CONSENT_VERSION, ContributionAction, CreateContributionRequest};
use wam_gatekeeper::provider::{
    LlmProvider, ModelTurn, ProviderError, ScriptStep, ScriptedProvider, TurnRequest,
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
