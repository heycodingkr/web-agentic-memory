use async_trait::async_trait;
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use wam_cli::account::{self, AccountError};
use wam_client::{
    ApiError,
    credentials::{CredentialError, CredentialStore, LinkedCredential},
    device::{DeviceApi, LinkPoll, LinkStart, LinkStatus},
};

#[derive(Default)]
struct Store {
    value: Mutex<Option<LinkedCredential>>,
    fail_save: bool,
    unavailable: bool,
}
impl CredentialStore for Store {
    fn load(&self) -> Result<Option<LinkedCredential>, CredentialError> {
        if self.unavailable {
            Err(CredentialError::Unavailable)
        } else {
            Ok(self.value.lock().unwrap().clone())
        }
    }
    fn save(&self, value: &LinkedCredential) -> Result<(), CredentialError> {
        if self.fail_save {
            return Err(CredentialError::Unavailable);
        }
        *self.value.lock().unwrap() = Some(value.clone());
        Ok(())
    }
    fn delete(&self) -> Result<(), CredentialError> {
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}
struct Api {
    replies: Mutex<VecDeque<LinkPoll>>,
    starts: AtomicUsize,
    revokes: AtomicUsize,
    revoke_error: Option<ApiError>,
}
impl Api {
    fn new(replies: Vec<LinkPoll>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            starts: AtomicUsize::new(0),
            revokes: AtomicUsize::new(0),
            revoke_error: None,
        }
    }
}
#[async_trait]
impl DeviceApi for Api {
    async fn start_link(&self) -> Result<LinkStart, ApiError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(LinkStart {
            device_code: "private-device-code".into(),
            user_code: "ABCD-1234".into(),
            verification_uri: "https://wam.example/link".into(),
            expires_in: 60,
            interval: 1,
        })
    }
    async fn poll_link(&self, code: &str) -> Result<LinkPoll, ApiError> {
        assert_eq!(code, "private-device-code");
        Ok(self.replies.lock().unwrap().pop_front().unwrap())
    }
    async fn link_status(&self, _: &str) -> Result<LinkStatus, ApiError> {
        unreachable!()
    }
    async fn revoke_link(&self, token: &str) -> Result<(), ApiError> {
        assert_eq!(token, "private-access-token");
        self.revokes.fetch_add(1, Ordering::SeqCst);
        self.revoke_error.map_or(Ok(()), Err)
    }
}
fn authorized() -> LinkPoll {
    LinkPoll::Authorized {
        device_id: "device_test".into(),
        access_token: "private-access-token".into(),
        scopes: vec!["read:memories".into(), "read:dashboard".into()],
        expires_at: u64::MAX,
    }
}
fn stored() -> Store {
    Store {
        value: Mutex::new(Some(LinkedCredential {
            device_id: "device_test".into(),
            access_token: "private-access-token".into(),
            scopes: vec![],
            expires_at: u64::MAX,
        })),
        ..Store::default()
    }
}

#[tokio::test(start_paused = true)]
async fn link_waits_for_approval_stores_only_in_injected_store_and_redacts_output() {
    let api = Api::new(vec![LinkPoll::Pending { interval: 3 }, authorized()]);
    let store = Store::default();
    let started = tokio::time::Instant::now();
    let report = account::link(&api, &store, |uri, code| {
        assert_eq!(uri, "https://wam.example/link");
        assert_eq!(code, "ABCD-1234");
    })
    .await
    .unwrap();
    assert_eq!(started.elapsed().as_secs(), 4);
    assert!(store.load().unwrap().is_some());
    let output = serde_json::to_string(&report).unwrap();
    assert!(!output.contains("private-access-token"));
    assert!(!output.contains("private-device-code"));
    assert!(matches!(
        account::link(&api, &store, |_, _| panic!()).await,
        Err(AccountError::AlreadyLinked)
    ));
    assert_eq!(api.starts.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn storage_failure_revokes_the_just_issued_device() {
    let api = Api::new(vec![authorized()]);
    let store = Store {
        fail_save: true,
        ..Store::default()
    };
    assert!(matches!(
        account::link(&api, &store, |_, _| {}).await,
        Err(AccountError::Store(_))
    ));
    assert!(store.load().unwrap().is_none());
    assert_eq!(api.revokes.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn storage_and_revocation_failure_explains_required_web_cleanup() {
    let mut api = Api::new(vec![authorized()]);
    api.revoke_error = Some(ApiError::Transport);
    let store = Store {
        fail_save: true,
        ..Store::default()
    };
    let error = account::link(&api, &store, |_, _| {}).await.err().unwrap();
    assert!(matches!(error, AccountError::StorageRollbackFailed));
    assert!(error.to_string().contains("web device settings"));
    assert!(!error.to_string().contains("private-access-token"));
}

#[tokio::test(start_paused = true)]
async fn denial_expiration_and_unavailable_store_never_create_a_credential() {
    for reply in [LinkPoll::Denied, LinkPoll::Expired, LinkPoll::AlreadyUsed] {
        let api = Api::new(vec![reply]);
        let store = Store::default();
        assert!(account::link(&api, &store, |_, _| {}).await.is_err());
        assert!(store.load().unwrap().is_none());
        assert_eq!(api.revokes.load(Ordering::SeqCst), 0);
    }
    let api = Api::new(vec![]);
    let store = Store {
        unavailable: true,
        ..Store::default()
    };
    assert!(account::link(&api, &store, |_, _| panic!()).await.is_err());
    assert_eq!(api.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn offline_unlink_retains_credential_for_a_retry_then_revokes_before_deleting() {
    let mut api = Api::new(vec![]);
    api.revoke_error = Some(ApiError::Transport);
    let store = stored();
    assert!(account::unlink(&api, &store, false).await.is_err());
    assert!(store.load().unwrap().is_some());
    api.revoke_error = None;
    let report = account::unlink(&api, &store, false).await.unwrap();
    assert!(report.local_removed && report.server_revoked);
    assert!(store.load().unwrap().is_none());
}

#[tokio::test]
async fn expired_credentials_can_be_removed_and_local_only_never_claims_revocation() {
    let mut api = Api::new(vec![]);
    api.revoke_error = Some(ApiError::Unauthorized);
    let store = stored();
    assert!(
        account::unlink(&api, &store, false)
            .await
            .unwrap()
            .server_revoked
    );
    let store = stored();
    let before = api.revokes.load(Ordering::SeqCst);
    let report = account::unlink(&api, &store, true).await.unwrap();
    assert!(report.local_removed && !report.server_revoked);
    assert_eq!(api.revokes.load(Ordering::SeqCst), before);
}
