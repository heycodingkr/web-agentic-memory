//! Local-only browser smoke fixture. Never run this example against user data.
use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use wam_gatekeeper::provider::{ScriptStep, ScriptedProvider};
use wam_server::{
    ApplicationRepository, DemoReviewerTokenAuthenticator, DemoTokenAuthenticator,
    FileIdentityStore, IdentityAuthenticator, IdentityService, InMemoryRateLimitStore, WamService,
    application::ServiceError,
    durable::{ReplayEvidence, ReplayRequest, ReplayVerifier, WorkResult},
    identity::{GitHubOAuthProvider, IdentityError, email::EmailSender, now_unix},
    router_with_identity_and_rate_limit_store,
};

#[path = "smoke_support/privacy.rs"]
mod privacy;

struct FixtureOAuth {
    origin: String,
}

#[async_trait]
impl GitHubOAuthProvider for FixtureOAuth {
    fn authorization_url(&self, state: &str, _challenge: &str) -> Result<String, IdentityError> {
        let mut callback = url::Url::parse(&format!("{}/v1/auth/github/callback", self.origin))
            .map_err(|_| IdentityError::Invalid)?;
        callback
            .query_pairs_mut()
            .append_pair("code", "smoke-author")
            .append_pair("state", state);
        Ok(callback.into())
    }

    async fn exchange_code(&self, code: &str, _verifier: &str) -> Result<String, IdentityError> {
        if code == "smoke-author" {
            Ok("smoke-author".into())
        } else {
            Err(IdentityError::Invalid)
        }
    }
}

struct FixtureEmailSender {
    code: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl EmailSender for FixtureEmailSender {
    async fn send_code(
        &self,
        _email: &str,
        code: &str,
        _request_id: &str,
    ) -> Result<(), IdentityError> {
        *self.code.lock().expect("fixture email code lock") = Some(code.to_owned());
        Ok(())
    }
}

// This is deliberately trusted fake evidence, only for the isolated local fixture.
struct FixtureReplay;
impl ReplayVerifier for FixtureReplay {
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

#[derive(Clone)]
struct FixtureState {
    repository: ApplicationRepository,
    service: Arc<WamService<ApplicationRepository>>,
    key: Arc<str>,
    capture: Option<PathBuf>,
    email_code: Arc<Mutex<Option<String>>>,
}

async fn read_email_code(
    State(state): State<FixtureState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if headers
        .get("x-wam-smoke-key")
        .and_then(|value| value.to_str().ok())
        != Some(state.key.as_ref())
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let code = state
        .email_code
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .clone()
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({"code":code})))
}

async fn curate(
    State(state): State<FixtureState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if headers
        .get("x-wam-smoke-key")
        .and_then(|value| value.to_str().ok())
        != Some(state.key.as_ref())
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let existing = state
        .service
        .catalog()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .memories
        .into_iter()
        .find(|memory| memory.domain == "example.org" && memory.task_category == "compose-draft");
    let (tool, args) = if let Some(memory) = existing {
        let version = (1..=100)
            .find(|version| {
                wam_server::durable::version_id(&memory.memory_id, *version) == memory.version_id
            })
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        (
            "decide_replace",
            json!({
                "existing_id": memory.memory_id, "expected_version": version,
                "record": record(true), "reason_codes": ["works_after_ui_change"], "confidence_band": "high"
            }),
        )
    } else {
        (
            "decide_create",
            json!({
                "record": record(false), "reason_codes": ["no_existing_in_scope"], "confidence_band": "high"
            }),
        )
    };
    let mut model = privacy::CaptureProvider {
        inner: ScriptedProvider::new([ScriptStep::Call(tool.into(), args)]),
        path: state.capture.clone(),
    };
    let repository = state.repository.clone();
    let result = tokio::task::spawn_blocking(move || {
        repository.process_next(&mut model, &FixtureReplay, now_unix())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let body = match result {
        WorkResult::Published {
            memory_id,
            version_id,
        } => json!({"status":"published","memory_id":memory_id,"version_id":version_id}),
        WorkResult::Supported { memory_id } => json!({"status":"supported","memory_id":memory_id}),
        WorkResult::Retired { memory_id } => json!({"status":"retired","memory_id":memory_id}),
        WorkResult::Idle => json!({"status":"idle"}),
        WorkResult::NeedsReview => json!({"status":"needs_review"}),
        WorkResult::Rejected => json!({"status":"rejected"}),
        WorkResult::Retry => json!({"status":"retry"}),
    };
    Ok(Json(body))
}

fn record(updated: bool) -> Value {
    json!({
        "l0":{"kind":"workflow","site_host":"example.org","url_patterns":["/editor"],"task_category":"compose-draft","intent_key":"compose-draft","keywords":["draft","write"]},
        "l1":{"title":"Save an editor draft","summary": if updated { "Save the draft using the updated editor order." } else { "Open the editor, enter the draft, and save it." },"key_points":[],"pitfalls":[]},
        "l2":{"steps": if updated { json!([{"action":"navigate"},{"action":"click"},{"action":"type"}]) } else { json!([{"action":"navigate"},{"action":"type"},{"action":"click"}]) }}
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind: SocketAddr = std::env::var("WAM_SMOKE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3105".into())
        .parse()?;
    if !bind.ip().is_loopback() {
        return Err("WAM_SMOKE_BIND must be loopback".into());
    }
    let data_dir = PathBuf::from(
        std::env::var("WAM_SMOKE_DATA_DIR").map_err(|_| "WAM_SMOKE_DATA_DIR is required")?,
    );
    let canonical_data_dir = fs::canonicalize(&data_dir)?;
    let canonical_temp_dir = fs::canonicalize(std::env::temp_dir())?;
    if !data_dir.is_absolute()
        || !canonical_data_dir.starts_with(&canonical_temp_dir)
        || canonical_data_dir == canonical_temp_dir
    {
        return Err(
            "WAM_SMOKE_DATA_DIR must be an absolute path under the OS temp directory".into(),
        );
    }
    if fs::read_dir(&data_dir)?.next().is_some() {
        return Err("WAM_SMOKE_DATA_DIR must be empty".into());
    }
    let key = std::env::var("WAM_SMOKE_KEY").map_err(|_| "WAM_SMOKE_KEY is required")?;
    if key.len() < 24 {
        return Err("WAM_SMOKE_KEY must be at least 24 bytes".into());
    }
    let origin =
        std::env::var("WAM_SMOKE_WEB_ORIGIN").unwrap_or_else(|_| "http://127.0.0.1:5175".into());
    let parsed = url::Url::parse(&origin)?;
    if parsed.scheme() != "http"
        || !parsed.host().is_some_and(|host| match host {
            url::Host::Ipv4(ip) => ip.is_loopback(),
            url::Host::Ipv6(ip) => ip.is_loopback(),
            url::Host::Domain(name) => name == "localhost",
        })
    {
        return Err("WAM_SMOKE_WEB_ORIGIN must be loopback HTTP".into());
    }
    let repository = ApplicationRepository::open(data_dir.join("application.json"))?;
    // This opt-in only exists in the owned, empty-directory loopback fixture.
    // It captures the actual fake-provider boundary and raises fixture quotas;
    // these runs never establish production rate-limit or live-model acceptance.
    let privacy_capture = (std::env::var("WAM_SMOKE_PRIVACY_CAPTURE").as_deref() == Ok("1"))
        .then(|| data_dir.join("gatekeeper.jsonl"));
    let service = Arc::new(WamService::durable(repository.clone())?);
    let email_code = Arc::new(Mutex::new(None));
    let identity = IdentityService::new(
        Arc::new(FileIdentityStore::from_repository(repository.clone())),
        Some(Arc::new(FixtureOAuth { origin })),
        parsed.origin().ascii_serialization(),
        false,
    )
    .with_email_auth(
        Arc::new(FixtureEmailSender {
            code: email_code.clone(),
        }),
        "wam-smoke-fixture-identity-secret-over-32-bytes".into(),
    )?;
    let identity = match std::env::var("WAM_SMOKE_MCP_RESOURCE") {
        Ok(resource) => {
            let url = url::Url::parse(&resource)?;
            if url.origin() != parsed.origin() {
                return Err("fixture MCP resource must share the loopback web origin".into());
            }
            identity.with_mcp_oauth(&resource)?
        }
        Err(_) => identity,
    };
    let identity = Arc::new(identity);
    let auth = Arc::new(IdentityAuthenticator::new(
        DemoTokenAuthenticator::default(),
        Arc::clone(&identity),
    ));
    let key: Arc<str> = key.into();
    let fixture_state = FixtureState {
        repository: repository.clone(),
        service: service.clone(),
        key: key.clone(),
        capture: privacy_capture.clone(),
        email_code: email_code.clone(),
    };
    let app = router_with_identity_and_rate_limit_store(
        Arc::clone(&service),
        auth,
        Arc::new(DemoReviewerTokenAuthenticator::default()),
        identity,
        Arc::new(privacy::FixtureRateLimits {
            inner: InMemoryRateLimitStore::default(),
            bulk: privacy_capture.is_some(),
        }),
        "wam-smoke-rate-limit-only",
        Vec::new(),
    )
    .route(
        "/__smoke/email-code",
        get(read_email_code).with_state(fixture_state.clone()),
    )
    .route("/__smoke/curate", post(curate).with_state(fixture_state));
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("wam smoke fixture listening on {}", listener.local_addr()?);
    let diagnostics =
        wam_server::diagnostics::Diagnostics::sqlite(data_dir.join("diagnostics.sqlite"))?;
    let app = wam_server::diagnostics::observe(app, diagnostics);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
