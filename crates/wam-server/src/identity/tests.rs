use super::*;
use async_trait::async_trait;
use std::{path::PathBuf, time::Duration};

struct StubGitHub;

#[async_trait]
impl GitHubOAuthProvider for StubGitHub {
    fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
    ) -> Result<String, IdentityError> {
        Ok(format!(
            "https://github.test/authorize?state={state}&code_challenge={code_challenge}&code_challenge_method=S256"
        ))
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<String, IdentityError> {
        if code == "bad" || verifier.is_empty() {
            Err(IdentityError::Provider)
        } else {
            Ok("github-subject-42".to_owned())
        }
    }
}

fn temp_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "wam-identity-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("create private test directory");
    directory.join("identity.json")
}

#[tokio::test]
async fn oauth_state_pkce_session_csrf_and_logout_survive_file_reopen() {
    let path = temp_path("oauth");
    let store = Arc::new(FileIdentityStore::open(&path).expect("open"));
    let identity = IdentityService::new(
        Arc::clone(&store),
        Some(Arc::new(StubGitHub)),
        "https://wam.example",
        true,
    );
    let (state, url) = identity.begin_oauth(100).expect("begin oauth");
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("code_challenge="));
    assert!(!url.contains("github-subject"));
    let (account_id, token, csrf, expires_at) = identity
        .finish_oauth("authorization-code", &state, &state, 101)
        .await
        .expect("complete oauth");
    assert!(account_id.starts_with("github_"));
    assert_eq!(expires_at, 101 + SESSION_TTL_SECS);
    assert!(identity.csrf_valid(&token, &csrf, 101));
    assert!(identity.session_cookie(&token, 100).contains("HttpOnly"));
    assert!(
        identity
            .session_cookie(&token, 100)
            .contains("SameSite=Lax")
    );
    assert!(identity.session_cookie(&token, 100).contains("; Secure"));

    assert_eq!(
        identity
            .finish_oauth("replay", &state, &state, 102)
            .await
            .unwrap_err(),
        IdentityError::Invalid
    );
    drop(identity);
    drop(store);

    let reopened = Arc::new(FileIdentityStore::open(&path).expect("reopen"));
    let reopened_identity = IdentityService::new(
        Arc::clone(&reopened),
        Some(Arc::new(StubGitHub)),
        "https://wam.example",
        true,
    );
    let (same_account, same_csrf, _) = reopened_identity
        .session_info(&token, 102)
        .expect("session persists");
    assert_eq!(same_account, account_id);
    assert_eq!(same_csrf, csrf);
    assert!(
        reopened_identity
            .authenticate_session(&token, 102)
            .is_some()
    );
    reopened_identity
        .revoke_session(&token, 103)
        .expect("logout revokes");
    assert!(
        reopened_identity
            .authenticate_session(&token, 103)
            .is_none()
    );
    assert_eq!(
        reopened_identity.session_info(&token, 103).unwrap_err(),
        IdentityError::Expired
    );
    let _ = fs::remove_dir_all(path.parent().expect("test directory"));
}

#[test]
fn github_device_link_return_is_state_bound_expiring_and_internal() {
    let path = temp_path("oauth-device-return");
    let store = Arc::new(FileIdentityStore::open(&path).expect("open"));
    let identity = IdentityService::new(
        Arc::clone(&store),
        Some(Arc::new(StubGitHub)),
        "https://wam.example",
        true,
    );
    let (state, authorize_url) = identity
        .begin_oauth_for_device_link("abcd-efgh", 100)
        .expect("begin device-link OAuth");
    assert!(authorize_url.contains("code_challenge_method=S256"));
    let cookie = identity
        .device_link_oauth_return_cookie("ABCD-EFGH")
        .expect("return cookie")
        .split(';')
        .next()
        .expect("cookie pair")
        .trim_start_matches("wam_oauth_return=")
        .to_owned();
    assert_eq!(cookie, "device:ABCD-EFGH");
    assert_eq!(
        identity.device_link_oauth_return(&state, &state, &cookie, 101),
        Some("https://wam.example/link?code=ABCD-EFGH".to_owned())
    );
    assert!(
        identity
            .device_link_oauth_return("another-state", &state, &cookie, 101)
            .is_none()
    );
    assert!(
        identity
            .device_link_oauth_return(&state, &state, "device:https://evil.example", 101)
            .is_none()
    );
    assert!(
        identity
            .device_link_oauth_return(&state, &state, &cookie, 401)
            .is_none()
    );
    assert!(
        identity
            .device_link_oauth_return_cookie("not-a-code")
            .is_none()
    );
    drop(identity);
    drop(store);
    let _ = fs::remove_dir_all(path.parent().expect("test directory"));
}

#[test]
fn device_flow_is_owner_bound_one_time_scoped_rotatable_and_revocable() {
    let path = temp_path("device");
    let store = FileIdentityStore::open(&path).expect("open");
    let started = store
        .start_device(
            "Codex workstation",
            vec![
                SCOPE_READ_MEMORIES.to_owned(),
                SCOPE_WRITE_CONTRIBUTIONS.to_owned(),
                SCOPE_REPORT_OUTCOME.to_owned(),
            ],
            100,
            "https://wam.example/link".to_owned(),
        )
        .expect("start device");
    assert_eq!(started.interval, DEVICE_POLL_INTERVAL_SECS);
    assert_eq!(
        store.poll_device(&started.device_code, 100).unwrap(),
        DevicePoll::Pending {
            interval: DEVICE_POLL_INTERVAL_SECS
        }
    );
    store
        .approve_device(&started.user_code, "github_account_a", 101)
        .expect("approve device");
    assert_eq!(
        store.poll_device(&started.device_code, 102).unwrap_err(),
        IdentityError::TooFast
    );
    let DevicePoll::Authorized {
        device_id,
        access_token,
        scopes,
        expires_at,
    } = store
        .poll_device(&started.device_code, 103)
        .expect("issue token")
    else {
        panic!("device should receive its token once");
    };
    assert!(scopes.contains(&SCOPE_READ_MEMORIES.to_owned()));
    assert!(scopes.contains(&SCOPE_WRITE_CONTRIBUTIONS.to_owned()));
    assert!(!scopes.contains(&SCOPE_READ_DASHBOARD.to_owned()));
    assert!(expires_at > 103);
    let credential = store.device_credential(&access_token, 104).unwrap();
    assert_eq!(credential.account_id, "github_account_a");
    assert!(!credential.browser_session);
    assert!(!store.device_credential(&access_token, expires_at).is_some());
    assert_eq!(
        store.poll_device(&started.device_code, 106).unwrap(),
        DevicePoll::AlreadyUsed
    );
    assert_eq!(
        store.revoke_device("github_account_b", &device_id, 106),
        Err(IdentityError::Invalid)
    );

    let (rotated_token, summary) = store
        .rotate_device("github_account_a", &device_id, 107)
        .expect("rotate token");
    assert_eq!(summary.name, "Codex workstation");
    assert!(store.device_credential(&access_token, 107).is_none());
    assert!(store.device_credential(&rotated_token, 107).is_some());
    store
        .revoke_device("github_account_a", &device_id, 108)
        .expect("revoke device");
    assert!(store.device_credential(&rotated_token, 108).is_none());
    drop(store);

    let reopened = FileIdentityStore::open(&path).expect("reopen");
    assert!(reopened.device_credential(&rotated_token, 108).is_none());
    let _ = fs::remove_dir_all(path.parent().expect("test directory"));
}

#[test]
fn device_approval_limits_guesses_and_rejects_unrecognized_scopes() {
    let path = temp_path("limits");
    let store = FileIdentityStore::open(&path).expect("open");
    for _ in 0..10 {
        assert_eq!(
            store.approve_device("not-a-code", "github_account", 100),
            Err(IdentityError::Invalid)
        );
    }
    drop(store);
    let store = FileIdentityStore::open(&path).expect("reopen after failed attempts");
    assert_eq!(
        store.approve_device("not-a-code", "github_account", 100),
        Err(IdentityError::RateLimited)
    );
    assert_eq!(
        validate_scopes(vec!["admin".to_owned()]),
        Err(IdentityError::Invalid)
    );
    let _ = fs::remove_dir_all(path.parent().expect("test directory"));
}

#[test]
fn failed_identity_store_persistence_does_not_commit_in_memory_state() {
    let path = temp_path("persist-failure");
    let store = FileIdentityStore::open(&path).expect("open");
    let parent = path.parent().expect("test directory");
    fs::remove_dir_all(parent).expect("remove store directory");

    let result = store.start_device(
        "test client",
        vec![SCOPE_READ_MEMORIES.to_owned()],
        100,
        "https://wam.example/link".to_owned(),
    );

    assert_eq!(result.unwrap_err(), IdentityError::Storage);
    assert_eq!(store.device_request_count().expect("identity state"), 0);
}

#[test]
fn scope_validation_adds_read_scope_and_deduplicates() {
    assert_eq!(
        validate_scopes(vec![
            SCOPE_WRITE_CONTRIBUTIONS.to_owned(),
            SCOPE_WRITE_CONTRIBUTIONS.to_owned(),
        ])
        .expect("valid scopes"),
        vec![
            SCOPE_READ_MEMORIES.to_owned(),
            SCOPE_WRITE_CONTRIBUTIONS.to_owned(),
        ]
    );
    assert_eq!(
        validate_scopes(Vec::new()).expect("default scope"),
        vec![SCOPE_READ_MEMORIES.to_owned()]
    );
}

#[test]
fn cookie_lookup_rejects_ambiguous_duplicate_names() {
    let mut headers = axum::http::HeaderMap::new();
    headers.append(
        header::COOKIE,
        axum::http::HeaderValue::from_static("theme=dark; wam_session=first"),
    );
    assert_eq!(
        cookie_value(&headers, "wam_session").as_deref(),
        Some("first")
    );

    headers.append(
        header::COOKIE,
        axum::http::HeaderValue::from_static("wam_session=second"),
    );
    assert_eq!(cookie_value(&headers, "wam_session"), None);
}

#[test]
fn user_codes_normalize_case_and_discard_separators() {
    assert_eq!(normalize_user_code("abcd-1234"), "ABCD1234");
    assert_eq!(normalize_user_code("ab cd.12"), "ABCD12");
}

#[test]
fn identity_store_rejects_corrupt_state_and_shared_temp_directory() {
    let path = temp_path("corrupt");
    fs::write(&path, b"not-json").expect("write invalid fixture");
    assert!(FileIdentityStore::open(&path).is_err());
    let _ = fs::remove_dir_all(path.parent().expect("test directory"));

    let shared_path = std::env::temp_dir().join("wam-identity-must-use-private-directory.json");
    assert!(FileIdentityStore::open(shared_path).is_err());
}
