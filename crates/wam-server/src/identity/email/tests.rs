use super::*;
use crate::identity::{FileIdentityStore, IdentityError};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Mailer {
    messages: Mutex<Vec<(String, String)>>,
    fail: bool,
}
#[async_trait]
impl EmailSender for Mailer {
    async fn send_code(&self, email: &str, code: &str, _: &str) -> Result<(), IdentityError> {
        self.messages
            .lock()
            .unwrap()
            .push((email.into(), code.into()));
        if self.fail {
            Err(IdentityError::Provider)
        } else {
            Ok(())
        }
    }
}
struct Rig {
    service: IdentityService,
    mailer: Arc<Mailer>,
    path: std::path::PathBuf,
}
impl Rig {
    fn new(fail: bool) -> Self {
        let path = std::env::temp_dir()
            .join(format!("wam-email-{}", random_secret().unwrap()))
            .join("identity.json");
        let mailer = Arc::new(Mailer {
            fail,
            ..Default::default()
        });
        let service = IdentityService::new(
            Arc::new(FileIdentityStore::open(&path).unwrap()),
            None,
            "https://wam.example",
            true,
        )
        .with_email_auth(
            mailer.clone(),
            "fixture-secret-at-least-32-bytes-long".into(),
        )
        .unwrap();
        Self {
            service,
            mailer,
            path,
        }
    }
    fn code(&self) -> String {
        self.mailer
            .messages
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .1
            .clone()
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap());
    }
}

#[test]
fn email_validation_normalization_and_unbiased_code_shape() {
    assert_eq!(
        normalize_email(" Person+tag@Example.COM ").unwrap(),
        "person+tag@example.com"
    );
    for input in [
        "",
        "a",
        "a@@example.com",
        ".a@example.com",
        "a..b@example.com",
        "a@example..com",
        "a@-example.com",
        "a@example-.com",
        "a@example.com,b@example.com",
        "a\r\n@example.com",
        "a@example.com\n",
        "a b@example.com",
        "a@localhost",
        "한@example.com",
    ] {
        assert_eq!(
            normalize_email(input),
            Err(IdentityError::Invalid),
            "{input:?}"
        );
    }
    for _ in 0..100 {
        let code = random_code().unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|b| b.is_ascii_digit()));
    }
    assert_ne!(
        email_subject("a", "a@example.com"),
        email_subject("b", "a@example.com")
    );
}

#[tokio::test]
async fn email_round_trip_is_private_single_use_and_stable_after_restart() {
    let rig = Rig::new(false);
    let challenge = rig
        .service
        .begin_email("Person@Example.com", 1000)
        .await
        .unwrap();
    let code = rig.code();
    let before = std::fs::read_to_string(&rig.path).unwrap();
    for value in ["person@example.com", &challenge, &code] {
        assert!(!before.contains(&format!("\"{value}\"")));
    }
    assert!(
        rig.service
            .finish_email("different-browser", &code, 1001)
            .is_err()
    );
    let (account, session, csrf, expires) =
        rig.service.finish_email(&challenge, &code, 1001).unwrap();
    assert!(account.starts_with("email_"));
    assert_eq!(expires, 1001 + 43200);
    assert!(rig.service.csrf_valid(&session, &csrf, 1002));
    assert!(rig.service.finish_email(&challenge, &code, 1002).is_err());
    let reopened = IdentityService::new(
        Arc::new(FileIdentityStore::open(&rig.path).unwrap()),
        None,
        "https://wam.example",
        true,
    )
    .with_email_auth(
        rig.mailer.clone(),
        "fixture-secret-at-least-32-bytes-long".into(),
    )
    .unwrap();
    assert_eq!(reopened.session_info(&session, 1002).unwrap().0, account);
    let new_challenge = reopened
        .begin_email(" person@example.COM ", 1060)
        .await
        .unwrap();
    assert_eq!(
        reopened
            .finish_email(&new_challenge, &rig.code(), 1061)
            .unwrap()
            .0,
        account
    );
    let persisted = std::fs::read_to_string(&rig.path).unwrap();
    for value in [
        "person@example.com",
        &session,
        &csrf,
        &challenge,
        &new_challenge,
    ] {
        assert!(!persisted.contains(value));
    }
}

#[tokio::test]
async fn wrong_and_malformed_guesses_lock_the_challenge() {
    let rig = Rig::new(false);
    let challenge = rig
        .service
        .begin_email("a@example.com", 1000)
        .await
        .unwrap();
    for guess in ["", "12345", "abcdef", "1234567", "１２３４５６"] {
        assert_eq!(
            rig.service.finish_email(&challenge, guess, 1001),
            Err(IdentityError::Invalid)
        );
    }
    assert_eq!(
        rig.service.finish_email(&challenge, &rig.code(), 1001),
        Err(IdentityError::RateLimited)
    );
    // Attempt counters survive process restart.
    let stored = FileIdentityStore::open(&rig.path).unwrap();
    assert_eq!(
        stored.consume_email(&challenge, &rig.code(), 1002),
        Err(IdentityError::RateLimited)
    );
}

#[tokio::test]
async fn expired_and_replaced_codes_cannot_authenticate_and_sends_are_limited() {
    let rig = Rig::new(false);
    let first = rig
        .service
        .begin_email("a@example.com", 1000)
        .await
        .unwrap();
    let first_code = rig.code();
    assert_eq!(
        rig.service.begin_email("A@EXAMPLE.COM", 1059).await,
        Err(IdentityError::RateLimited)
    );
    let next = rig
        .service
        .begin_email("a@example.com", 1060)
        .await
        .unwrap();
    assert!(rig.service.finish_email(&first, &first_code, 1060).is_err());
    assert_eq!(
        rig.service.finish_email(&next, &rig.code(), 1660),
        Err(IdentityError::Expired)
    );
    for now in [1720, 1780, 1840] {
        rig.service.begin_email("a@example.com", now).await.unwrap();
    }
    assert_eq!(
        rig.service.begin_email("a@example.com", 1900).await,
        Err(IdentityError::RateLimited)
    );
    assert!(rig.service.begin_email("a@example.com", 4600).await.is_ok());
}

#[tokio::test]
async fn concurrent_verification_is_atomic_and_failed_delivery_leaves_no_usable_challenge() {
    let rig = Rig::new(false);
    let challenge = rig
        .service
        .begin_email("a@example.com", 1000)
        .await
        .unwrap();
    let code = rig.code();
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| rig.service.finish_email(&challenge, &code, 1001));
        let b = scope.spawn(|| rig.service.finish_email(&challenge, &code, 1001));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let failed = Rig::new(true);
    assert_eq!(
        failed.service.begin_email("a@example.com", 1000).await,
        Err(IdentityError::Provider)
    );
    let state: crate::identity::store::IdentityState =
        serde_json::from_slice(&std::fs::read(&failed.path).unwrap()).unwrap();
    let serialized = serde_json::to_value(state).unwrap();
    assert!(
        serialized["email_challenges"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        failed.service.begin_email("a@example.com", 1001).await,
        Err(IdentityError::RateLimited)
    );
}

#[tokio::test]
async fn deletion_purge_and_legacy_snapshots_preserve_privacy() {
    let rig = Rig::new(false);
    let challenge = rig
        .service
        .begin_email("a@example.com", 1000)
        .await
        .unwrap();
    let (account, _, _, _) = rig
        .service
        .finish_email(&challenge, &rig.code(), 1001)
        .unwrap();
    rig.service
        .begin_email("a@example.com", 1060)
        .await
        .unwrap();
    let mut state: crate::identity::store::IdentityState =
        serde_json::from_slice(&std::fs::read(&rig.path).unwrap()).unwrap();
    state.erase_account(&account, 1061);
    let value = serde_json::to_value(&state).unwrap();
    for field in ["accounts", "email_challenges", "email_sends", "sessions"] {
        assert!(value[field].as_array().unwrap().is_empty(), "{field}");
    }
    state = serde_json::from_slice(&std::fs::read(&rig.path).unwrap()).unwrap();
    state.purge(50000);
    let mut value = serde_json::to_value(&state).unwrap();
    assert!(value["email_challenges"].as_array().unwrap().is_empty());
    assert!(value["email_sends"].as_array().unwrap().is_empty());
    value.as_object_mut().unwrap().remove("email_challenges");
    value.as_object_mut().unwrap().remove("email_sends");
    assert!(serde_json::from_value::<crate::identity::store::IdentityState>(value).is_ok());
}

#[tokio::test]
async fn resend_wire_contract_rejects_errors_redirects_and_invalid_receipts() {
    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    type FixtureState = (StatusCode, Arc<Mutex<Vec<(HeaderMap, serde_json::Value)>>>);
    async fn capture(
        State((status, messages)): State<FixtureState>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        messages.lock().unwrap().push((headers, body));
        (status, Json(serde_json::json!({"id":"fixture-receipt"})))
    }
    for status in [
        StatusCode::OK,
        StatusCode::FOUND,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/emails", post(capture))
            .with_state((status, messages.clone()));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut sender =
            ResendEmailSender::new("fixture-secret".into(), "WAM <noreply@example.com>".into())
                .unwrap();
        sender.endpoint = endpoint;
        let result = sender
            .send_code("person@example.com", "012345", "fixture-request")
            .await;
        assert_eq!(result.is_ok(), status == StatusCode::OK);
        let messages = messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        let (headers, body) = &messages[0];
        assert_eq!(headers["authorization"], "Bearer fixture-secret");
        assert_eq!(headers["idempotency-key"], "wam-email/fixture-request");
        assert_eq!(body["to"], serde_json::json!(["person@example.com"]));
        assert_eq!(body["from"], "WAM <noreply@example.com>");
        assert!(body["text"].as_str().unwrap().contains("012345"));
        server.abort();
    }
}
