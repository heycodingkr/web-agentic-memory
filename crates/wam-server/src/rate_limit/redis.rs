use super::{
    BucketSpec, BucketStatus, DailyUsage, RateLimitError, RateLimitStore, UsageRecord, div_ceil,
    usage_field, value_or_zero,
};
use ::redis::{Script, aio::ConnectionManager};
use async_trait::async_trait;

const TOKEN_BUCKET_SCRIPT: &str = r#"
local now = tonumber(ARGV[1])
local consume = tonumber(ARGV[2]) == 1
local count = tonumber(ARGV[3])
local usage = tonumber(ARGV[4]) == 1
local units = tonumber(ARGV[5])
local usage_field = ARGV[6]
local buckets = {}
local allowed = true
local retry_ms = 0
local erased = false

for i = 1, count do
  local key = KEYS[i]
  local account = string.match(key, '^wam:rate:account:(.+)$') or string.match(key, '^wam:rate:status%-account:(.+)$')
  if account and redis.call('EXISTS', 'wam:deleted-account:' .. account) == 1 then erased = true end
  local offset = 7 + (i - 1) * 3
  local capacity = tonumber(ARGV[offset])
  local refill_per_second = tonumber(ARGV[offset + 1])
  local cost = tonumber(ARGV[offset + 2])
  local stored = redis.call('HMGET', key, 'tokens', 'updated_ms')
  local tokens = tonumber(stored[1]) or capacity
  local updated_ms = tonumber(stored[2]) or now
  local elapsed_ms = math.max(0, now - updated_ms)
  tokens = math.min(capacity, tokens + elapsed_ms * refill_per_second / 1000)
  if tokens < cost then
    allowed = false
    retry_ms = math.max(retry_ms, math.ceil((cost - tokens) / refill_per_second * 1000))
  end
  buckets[i] = { key, tokens, capacity, refill_per_second, cost }
end

if usage then
  local account = string.match(KEYS[count + 1], '^wam:usage:([^:]+):')
  if account and redis.call('EXISTS', 'wam:deleted-account:' .. account) == 1 then erased = true end
end
if erased then allowed = false end
local result = { allowed and 1 or 0, retry_ms }
for i = 1, count do
  local bucket = buckets[i]
  local tokens = bucket[2]
  if consume and allowed then tokens = tokens - bucket[5] end
  local reset_ms = math.ceil((bucket[3] - tokens) / bucket[4] * 1000)
  if consume and not erased then
    redis.call('HSET', bucket[1], 'tokens', tokens, 'updated_ms', now)
    redis.call('PEXPIRE', bucket[1], math.max(120000, reset_ms * 2))
  end
  table.insert(result, math.floor(tokens))
  table.insert(result, reset_ms)
  table.insert(result, math.floor(bucket[3] * 60))
end

if consume and usage then
  local usage_key = KEYS[count + 1]
  if allowed then
    redis.call('HINCRBY', usage_key, 'requests', 1)
    redis.call('HINCRBY', usage_key, 'units', units)
    redis.call('HINCRBY', usage_key, usage_field, 1)
  elseif not erased then
    redis.call('HINCRBY', usage_key, 'denied_requests', 1)
  end
  if not erased then
    redis.call('EXPIRE', usage_key, 7776000)
    local account = string.match(usage_key, '^wam:usage:([^:]+):')
    if account then
      local index_key = 'wam:usage-index:' .. account
      redis.call('SADD', index_key, usage_key)
      redis.call('EXPIRE', index_key, 7776000)
    end
  end
end

return result
"#;

const USAGE_HISTORY_SCRIPT: &str = r#"
local result = {}
for i = 1, #KEYS do
  result[i] = redis.call('HMGET', KEYS[i], 'requests', 'units', 'recall', 'detail', 'other', 'denied_requests')
end
return result
"#;

#[derive(Clone)]
pub struct RedisRateLimitStore {
    pub(super) connection: ConnectionManager,
}

impl RedisRateLimitStore {
    pub async fn connect(url: &str) -> Result<Self, RateLimitError> {
        let client = redis::Client::open(url).map_err(|_| RateLimitError)?;
        let connection = ConnectionManager::new(client)
            .await
            .map_err(|_| RateLimitError)?;
        Ok(Self { connection })
    }

    async fn evaluate(
        &self,
        buckets: &[BucketSpec],
        usage: Option<&UsageRecord>,
        consume: bool,
        now_ms: u64,
    ) -> Result<Vec<BucketStatus>, RateLimitError> {
        if buckets.is_empty() {
            return Err(RateLimitError);
        }
        for bucket in buckets {
            bucket.validate()?;
        }
        if let Some(usage) = usage {
            usage_field(usage.request_class)?;
            if usage.key.is_empty() || usage.units == 0 {
                return Err(RateLimitError);
            }
        }
        let script = Script::new(TOKEN_BUCKET_SCRIPT);
        let mut invocation = script.prepare_invoke();
        for bucket in buckets {
            invocation.key(&bucket.key);
        }
        if let Some(usage) = usage.filter(|_| consume) {
            invocation.key(&usage.key);
        }
        invocation
            .arg(now_ms)
            .arg(u8::from(consume))
            .arg(buckets.len())
            .arg(u8::from(usage.is_some() && consume))
            .arg(usage.map_or(0, |record| record.units))
            .arg(usage.map_or("other", |record| record.request_class));
        for bucket in buckets {
            invocation
                .arg(bucket.capacity)
                .arg(bucket.refill_per_second)
                .arg(bucket.cost);
        }
        let mut connection = self.connection.clone();
        let values: Vec<i64> = invocation
            .invoke_async(&mut connection)
            .await
            .map_err(|_| RateLimitError)?;
        if values.len() != 2 + 3 * buckets.len() {
            return Err(RateLimitError);
        }
        let allowed = values[0] == 1;
        let retry_after_seconds = div_ceil(values[1].max(0) as u64, 1000);
        Ok(buckets
            .iter()
            .enumerate()
            .map(|(index, bucket)| {
                let offset = 2 + index * 3;
                BucketStatus {
                    allowed,
                    limit_per_minute: bucket.limit_per_minute,
                    capacity: bucket.capacity,
                    remaining_units: values[offset].max(0) as u32,
                    resets_in_seconds: div_ceil(values[offset + 1].max(0) as u64, 1000),
                    retry_after_seconds,
                }
            })
            .collect())
    }
}

#[async_trait]
impl RateLimitStore for RedisRateLimitStore {
    async fn erase_account(
        &self,
        secret: &str,
        account_digest: &str,
    ) -> Result<(), RateLimitError> {
        super::erasure::erase_redis(self, secret, account_digest).await
    }
    async fn consume(
        &self,
        buckets: &[BucketSpec],
        usage: Option<&UsageRecord>,
        now_ms: u64,
    ) -> Result<Vec<BucketStatus>, RateLimitError> {
        self.evaluate(buckets, usage, true, now_ms).await
    }

    async fn inspect(
        &self,
        bucket: &BucketSpec,
        now_ms: u64,
    ) -> Result<BucketStatus, RateLimitError> {
        self.evaluate(std::slice::from_ref(bucket), None, false, now_ms)
            .await?
            .into_iter()
            .next()
            .ok_or(RateLimitError)
    }

    async fn usage_history(
        &self,
        keys_and_dates: &[(String, String)],
    ) -> Result<Vec<DailyUsage>, RateLimitError> {
        if keys_and_dates.is_empty() {
            return Ok(Vec::new());
        }
        let script = Script::new(USAGE_HISTORY_SCRIPT);
        let mut invocation = script.prepare_invoke();
        for (key, _) in keys_and_dates {
            invocation.key(key);
        }
        let mut connection = self.connection.clone();
        let rows: Vec<Vec<Option<i64>>> = invocation
            .invoke_async(&mut connection)
            .await
            .map_err(|_| RateLimitError)?;
        if rows.len() != keys_and_dates.len() {
            return Err(RateLimitError);
        }
        rows.into_iter()
            .zip(keys_and_dates)
            .map(|(values, (_, date))| {
                if values.len() != 6 {
                    return Err(RateLimitError);
                }
                Ok(DailyUsage {
                    date: date.clone(),
                    requests: value_or_zero(values[0]),
                    units: value_or_zero(values[1]),
                    recall_requests: value_or_zero(values[2]),
                    detail_requests: value_or_zero(values[3]),
                    other_requests: value_or_zero(values[4]),
                    denied_requests: value_or_zero(values[5]),
                })
            })
            .collect()
    }
}
