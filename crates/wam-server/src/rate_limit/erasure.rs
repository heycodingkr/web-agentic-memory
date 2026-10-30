//! Account projections are addressed from an irreversible account digest, so
//! deletion retries do not need the original account ID in the durable outbox.
use super::*;
use ::redis::Script;

pub fn account_digest(account: &str) -> String {
    format!("{:x}", Sha256::digest(account.as_bytes()))
}

pub(super) fn account_fingerprint(secret: &str, account: &str) -> String {
    fingerprint(secret, &account_digest(account))
}

fn projection_fingerprint(secret: &str, digest: &str) -> Result<String, RateLimitError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(RateLimitError);
    }
    Ok(fingerprint(secret, digest))
}

pub(super) fn bucket_account(key: &str) -> Option<&str> {
    key.strip_prefix("wam:rate:account:")
        .or_else(|| key.strip_prefix("wam:rate:status-account:"))
}

pub(super) fn usage_account(key: &str) -> Option<&str> {
    key.strip_prefix("wam:usage:")?
        .split_once(':')
        .map(|(account, _)| account)
}

pub(super) fn erase_memory(
    store: &InMemoryRateLimitStore,
    secret: &str,
    digest: &str,
) -> Result<(), RateLimitError> {
    let fingerprint = projection_fingerprint(secret, digest)?;
    let mut state = store.state.lock().map_err(|_| RateLimitError)?;
    state.erased_accounts.insert(fingerprint.clone());
    state
        .buckets
        .retain(|key, _| bucket_account(key) != Some(fingerprint.as_str()));
    state
        .usage
        .retain(|key, _| usage_account(key) != Some(fingerprint.as_str()));
    Ok(())
}

const ERASE_ACCOUNT: &str = r#"
redis.call('SET', KEYS[1], '1')
local usage_keys = redis.call('SMEMBERS', KEYS[4])
for _, key in ipairs(usage_keys) do redis.call('DEL', key) end
redis.call('DEL', KEYS[2], KEYS[3], KEYS[4])
return 1
"#;

pub(super) async fn erase_redis(
    store: &RedisRateLimitStore,
    secret: &str,
    digest: &str,
) -> Result<(), RateLimitError> {
    let fingerprint = projection_fingerprint(secret, digest)?;
    let mut connection = store.connection.clone();
    let _: i64 = Script::new(ERASE_ACCOUNT)
        .key(format!("wam:deleted-account:{fingerprint}"))
        .key(format!("wam:rate:account:{fingerprint}"))
        .key(format!("wam:rate:status-account:{fingerprint}"))
        .key(format!("wam:usage-index:{fingerprint}"))
        .invoke_async(&mut connection)
        .await
        .map_err(|_| RateLimitError)?;
    Ok(())
}
