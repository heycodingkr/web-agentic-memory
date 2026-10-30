use wam_server::{
    RateLimitStore, RedisRateLimitStore, diagnostics,
    rate_limit::{BucketSpec, UsageRecord, account_bucket, account_usage_key, utc_dates},
};

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
#[ignore = "requires disposable WAM_TEST_REDIS_URL"]
async fn redis_quota_is_shared_atomic_and_records_denials() {
    let url = std::env::var("WAM_TEST_REDIS_URL").expect("use a disposable Redis instance");
    let first = RedisRateLimitStore::connect(&url).await.unwrap();
    let second = RedisRateLimitStore::connect(&url).await.unwrap();
    let test_id = diagnostics::random_id();
    let secret = format!("redis-integration-{}-{test_id}", std::process::id());
    let account = "quota-owner";
    let mut bucket = account_bucket(&secret, account);
    bucket.capacity = 12;
    bucket.refill_per_second = 1.0;
    let date = utc_dates(unix_time(), 1).pop().unwrap();
    let usage = UsageRecord {
        key: account_usage_key(&secret, account, &date),
        units: 3,
        request_class: "recall",
    };

    let contenders = 32;
    let start = std::sync::Arc::new(tokio::sync::Barrier::new(contenders));
    let mut tasks = Vec::with_capacity(contenders);
    let now_ms = unix_time() * 1000;
    for index in 0..contenders {
        let store = if index % 2 == 0 {
            first.clone()
        } else {
            second.clone()
        };
        let bucket = bucket.clone();
        let usage = usage.clone();
        let start = start.clone();
        tasks.push(tokio::spawn(async move {
            start.wait().await;
            store
                .consume(&[bucket], Some(&usage), now_ms)
                .await
                .unwrap()
                .remove(0)
        }));
    }

    let mut admitted = 0;
    let mut denied = 0;
    for task in tasks {
        if task.await.unwrap().allowed {
            admitted += 1;
        } else {
            denied += 1;
        }
    }
    assert_eq!(admitted, 12);
    assert_eq!(denied, contenders - 12);

    let status = second.inspect(&bucket, now_ms).await.unwrap();
    assert!(!status.allowed);
    assert_eq!(status.remaining_units, 0);
    assert_eq!(status.retry_after_seconds, 1);

    let daily = first
        .usage_history(&[(usage.key, date.clone())])
        .await
        .unwrap()
        .remove(0);
    assert_eq!(daily.date, date);
    assert_eq!(daily.requests, 12);
    assert_eq!(daily.units, 36);
    assert_eq!(daily.recall_requests, 12);
    assert_eq!(daily.denied_requests, (contenders - 12) as u64);
}

#[tokio::test]
#[ignore = "requires disposable WAM_TEST_REDIS_URL"]
async fn redis_multi_bucket_denial_does_not_charge_other_buckets() {
    let url = std::env::var("WAM_TEST_REDIS_URL").expect("use a disposable Redis instance");
    let store = RedisRateLimitStore::connect(&url).await.unwrap();
    let prefix = format!("wam:test:atomic:{}", diagnostics::random_id());
    let broad = BucketSpec::per_minute(format!("{prefix}:broad"), 60, 2, 1);
    let narrow = BucketSpec::per_minute(format!("{prefix}:narrow"), 60, 1, 1);
    let now_ms = unix_time() * 1000;

    assert!(
        store
            .consume(&[broad.clone(), narrow.clone()], None, now_ms)
            .await
            .unwrap()
            .iter()
            .all(|status| status.allowed)
    );
    assert!(
        store
            .consume(&[broad.clone(), narrow], None, now_ms)
            .await
            .unwrap()
            .iter()
            .all(|status| !status.allowed)
    );

    let broad_after_denial = store.inspect(&broad, now_ms).await.unwrap();
    assert!(broad_after_denial.allowed);
    assert_eq!(broad_after_denial.remaining_units, 1);
}
