mod lifecycle_support;
use lifecycle_support::{AUTHOR, READER, contribution, now, prepare};
use std::sync::atomic::{AtomicBool, Ordering};
use wam_server::{
    ApplicationRepository, InMemoryRateLimitStore, RateLimitStore, RedisRateLimitStore,
    application::ServiceError,
    rate_limit::{
        BucketSpec, BucketStatus, DailyUsage, RateLimitError, UsageRecord, account_bucket,
        account_digest, account_usage_key,
    },
};

#[derive(Default)]
struct FlakyStore {
    inner: InMemoryRateLimitStore,
    unavailable: AtomicBool,
}
#[async_trait::async_trait]
impl RateLimitStore for FlakyStore {
    async fn consume(
        &self,
        buckets: &[BucketSpec],
        usage: Option<&UsageRecord>,
        at: u64,
    ) -> Result<Vec<BucketStatus>, RateLimitError> {
        self.inner.consume(buckets, usage, at).await
    }
    async fn inspect(&self, bucket: &BucketSpec, at: u64) -> Result<BucketStatus, RateLimitError> {
        self.inner.inspect(bucket, at).await
    }
    async fn usage_history(
        &self,
        keys: &[(String, String)],
    ) -> Result<Vec<DailyUsage>, RateLimitError> {
        self.inner.usage_history(keys).await
    }
    async fn erase_account(&self, secret: &str, digest: &str) -> Result<(), RateLimitError> {
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(RateLimitError);
        }
        self.inner.erase_account(secret, digest).await
    }
}
fn usage(secret: &str, account: &str) -> UsageRecord {
    UsageRecord {
        key: account_usage_key(secret, account, "2026-10-27"),
        units: 3,
        request_class: "recall",
    }
}
async fn charge(store: &dyn RateLimitStore, secret: &str, account: &str) {
    assert!(
        store
            .consume(
                &[account_bucket(secret, account)],
                Some(&usage(secret, account)),
                now() * 1000
            )
            .await
            .unwrap()[0]
            .allowed
    );
}
async fn requests(store: &dyn RateLimitStore, secret: &str, account: &str) -> u64 {
    store
        .usage_history(&[(usage(secret, account).key, "2026-10-27".into())])
        .await
        .unwrap()[0]
        .requests
}

#[tokio::test]
async fn failed_projection_cleanup_survives_restart_and_does_not_erase_another_account() {
    let directory = std::env::temp_dir().join(format!(
        "wam-cleanup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = directory.join("application.json");
    let repository = ApplicationRepository::open(&path).unwrap();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let limits = FlakyStore::default();
    charge(&limits, "test-secret", AUTHOR).await;
    charge(&limits, "test-secret", READER).await;
    repository.delete_account(AUTHOR, now()).unwrap();
    assert_ne!(
        repository.pending_outbox(1).unwrap()[0].kind,
        "account_deleted"
    );
    assert_eq!(repository.pending_account_deletions(1).unwrap().len(), 1);
    drop(service);
    drop(repository);
    let reopened = ApplicationRepository::open(&path).unwrap();
    limits.unavailable.store(true, Ordering::SeqCst);
    assert_eq!(
        reopened
            .deliver_account_deletions(&limits, "test-secret", now())
            .await,
        Err(ServiceError::Unavailable)
    );
    assert_eq!(reopened.pending_account_deletions(1).unwrap().len(), 1);
    assert_eq!(requests(&limits, "test-secret", AUTHOR).await, 1);
    limits.unavailable.store(false, Ordering::SeqCst);
    assert_eq!(
        reopened
            .deliver_account_deletions(&limits, "test-secret", now())
            .await
            .unwrap(),
        1
    );
    assert!(reopened.pending_account_deletions(1).unwrap().is_empty());
    assert_eq!(
        reopened
            .deliver_account_deletions(&limits, "test-secret", now())
            .await
            .unwrap(),
        0
    );
    assert_eq!(requests(&limits, "test-secret", AUTHOR).await, 0);
    assert_eq!(requests(&limits, "test-secret", READER).await, 1);
    assert!(
        !limits
            .consume(
                &[account_bucket("test-secret", AUTHOR)],
                Some(&usage("test-secret", AUTHOR)),
                now() * 1000
            )
            .await
            .unwrap()[0]
            .allowed
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
#[ignore = "requires disposable WAM_TEST_REDIS_URL"]
async fn redis_erasure_is_atomic_shared_idempotent_and_fences_late_requests() {
    let url = std::env::var("WAM_TEST_REDIS_URL").expect("use a disposable Redis instance");
    let first = RedisRateLimitStore::connect(&url).await.unwrap();
    let second = RedisRateLimitStore::connect(&url).await.unwrap();
    let secret = format!(
        "test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    charge(&first, &secret, AUTHOR).await;
    charge(&first, &secret, READER).await;
    assert_eq!(requests(&second, &secret, AUTHOR).await, 1);
    second
        .erase_account(&secret, &account_digest(AUTHOR))
        .await
        .unwrap();
    first
        .erase_account(&secret, &account_digest(AUTHOR))
        .await
        .unwrap();
    assert_eq!(requests(&first, &secret, AUTHOR).await, 0);
    assert_eq!(requests(&first, &secret, READER).await, 1);
    assert!(
        !first
            .consume(
                &[account_bucket(&secret, AUTHOR)],
                Some(&usage(&secret, AUTHOR)),
                now() * 1000
            )
            .await
            .unwrap()[0]
            .allowed
    );
    assert!(
        !second
            .inspect(&account_bucket(&secret, AUTHOR), now() * 1000)
            .await
            .unwrap()
            .allowed
    );
    assert_eq!(requests(&second, &secret, AUTHOR).await, 0);
}
