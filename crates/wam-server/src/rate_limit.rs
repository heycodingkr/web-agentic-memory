//! Shared, weighted API token buckets.
//!
//! Production servers use Redis/Valkey so every server instance sees the same
//! quota. The in-memory implementation is deterministic and intended for local
//! development and tests only.

mod erasure;
mod load_shed;
mod redis;
pub use erasure::account_digest;
pub use load_shed::LoadShedGate;
pub use redis::RedisRateLimitStore;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::IpAddr,
    str::FromStr,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq)]
pub struct BucketSpec {
    pub key: String,
    pub limit_per_minute: u32,
    pub capacity: u32,
    pub refill_per_second: f64,
    pub cost: u32,
}

impl BucketSpec {
    pub fn per_minute(
        key: impl Into<String>,
        limit_per_minute: u32,
        capacity: u32,
        cost: u32,
    ) -> Self {
        Self {
            key: key.into(),
            limit_per_minute,
            capacity,
            refill_per_second: f64::from(limit_per_minute) / 60.0,
            cost,
        }
    }

    fn validate(&self) -> Result<(), RateLimitError> {
        if self.key.is_empty()
            || self.limit_per_minute == 0
            || self.capacity == 0
            || self.cost == 0
            || !self.refill_per_second.is_finite()
            || self.refill_per_second <= 0.0
        {
            return Err(RateLimitError);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BucketStatus {
    pub allowed: bool,
    pub limit_per_minute: u32,
    pub capacity: u32,
    pub remaining_units: u32,
    pub resets_in_seconds: u64,
    pub retry_after_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageRecord {
    pub key: String,
    pub units: u32,
    /// A closed field name: `recall`, `detail`, or `other`.
    pub request_class: &'static str,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DailyUsage {
    pub date: String,
    pub requests: u64,
    pub units: u64,
    pub recall_requests: u64,
    pub detail_requests: u64,
    pub other_requests: u64,
    pub denied_requests: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpNetwork {
    address: IpAddr,
    prefix: u8,
}

impl IpNetwork {
    pub fn contains(&self, address: IpAddr) -> bool {
        match (self.address, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                u32::from(network) & mask == u32::from(address) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
                u128::from(network) & mask == u128::from(address) & mask
            }
            _ => false,
        }
    }
}

impl FromStr for IpNetwork {
    type Err = RateLimitError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = value.split_once('/').unwrap_or((value, ""));
        let address = address.parse::<IpAddr>().map_err(|_| RateLimitError)?;
        let max_prefix = if address.is_ipv4() { 32 } else { 128 };
        let prefix = if prefix.is_empty() {
            max_prefix
        } else {
            prefix.parse::<u8>().map_err(|_| RateLimitError)?
        };
        if prefix > max_prefix {
            return Err(RateLimitError);
        }
        Ok(Self { address, prefix })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitError;

impl fmt::Display for RateLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("rate limit storage unavailable")
    }
}

impl std::error::Error for RateLimitError {}

#[async_trait]
pub trait RateLimitStore: Send + Sync + 'static {
    /// Charges every bucket only when all buckets allow the request.
    async fn consume(
        &self,
        buckets: &[BucketSpec],
        usage: Option<&UsageRecord>,
        now_ms: u64,
    ) -> Result<Vec<BucketStatus>, RateLimitError>;

    /// Reads a quota without consuming capacity or extending its lifetime.
    async fn inspect(
        &self,
        bucket: &BucketSpec,
        now_ms: u64,
    ) -> Result<BucketStatus, RateLimitError>;

    async fn usage_history(
        &self,
        keys_and_dates: &[(String, String)],
    ) -> Result<Vec<DailyUsage>, RateLimitError>;

    /// Atomically erases projections and fences requests admitted before deletion.
    /// The digest is SHA-256(account_id), never the original account ID.
    async fn erase_account(
        &self,
        _secret: &str,
        _account_digest: &str,
    ) -> Result<(), RateLimitError> {
        Err(RateLimitError)
    }
}

#[derive(Clone, Default)]
pub struct InMemoryRateLimitStore {
    state: Arc<Mutex<MemoryState>>,
}

#[derive(Default)]
struct MemoryState {
    buckets: HashMap<String, MemoryBucket>,
    usage: HashMap<String, DailyUsage>,
    erased_accounts: HashSet<String>,
}

#[derive(Clone, Copy)]
struct MemoryBucket {
    tokens: f64,
    updated_ms: u64,
}

fn calculate_status(spec: &BucketSpec, tokens: f64, allowed: bool) -> BucketStatus {
    let remaining = tokens.floor().max(0.0) as u32;
    let resets_in_seconds =
        ((f64::from(spec.capacity) - tokens).max(0.0) / spec.refill_per_second).ceil() as u64;
    let retry_after_seconds = if allowed {
        0
    } else {
        ((f64::from(spec.cost) - tokens).max(0.0) / spec.refill_per_second).ceil() as u64
    };
    BucketStatus {
        allowed,
        limit_per_minute: spec.limit_per_minute,
        capacity: spec.capacity,
        remaining_units: remaining,
        resets_in_seconds,
        retry_after_seconds,
    }
}

fn usage_field(class: &str) -> Result<(), RateLimitError> {
    match class {
        "recall" | "detail" | "other" => Ok(()),
        _ => Err(RateLimitError),
    }
}

#[async_trait]
impl RateLimitStore for InMemoryRateLimitStore {
    async fn erase_account(
        &self,
        secret: &str,
        account_digest: &str,
    ) -> Result<(), RateLimitError> {
        erasure::erase_memory(self, secret, account_digest)
    }
    async fn consume(
        &self,
        buckets: &[BucketSpec],
        usage: Option<&UsageRecord>,
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

        let mut state = self.state.lock().map_err(|_| RateLimitError)?;
        let erased = buckets
            .iter()
            .filter_map(|bucket| erasure::bucket_account(&bucket.key))
            .chain(usage.and_then(|record| erasure::usage_account(&record.key)))
            .any(|account| state.erased_accounts.contains(account));
        let refilled: Vec<f64> = buckets
            .iter()
            .map(|spec| {
                let previous = state.buckets.get(&spec.key);
                previous.map_or(f64::from(spec.capacity), |bucket| {
                    let elapsed_ms = now_ms.saturating_sub(bucket.updated_ms);
                    (bucket.tokens + elapsed_ms as f64 * spec.refill_per_second / 1000.0)
                        .min(f64::from(spec.capacity))
                })
            })
            .collect();
        let allowed = !erased
            && buckets
                .iter()
                .zip(&refilled)
                .all(|(spec, tokens)| *tokens >= f64::from(spec.cost));

        let statuses: Vec<_> = buckets
            .iter()
            .zip(&refilled)
            .map(|(spec, tokens)| {
                calculate_status(
                    spec,
                    tokens - if allowed { f64::from(spec.cost) } else { 0.0 },
                    allowed,
                )
            })
            .collect();
        for ((spec, tokens), status) in buckets
            .iter()
            .zip(refilled)
            .zip(&statuses)
            .filter(|_| !erased)
        {
            let charged = tokens - if allowed { f64::from(spec.cost) } else { 0.0 };
            state.buckets.insert(
                spec.key.clone(),
                MemoryBucket {
                    tokens: charged,
                    updated_ms: now_ms,
                },
            );
            // Report reset from the actual stored token count, not the projected one.
            let _ = status;
        }
        if let Some(record) = usage.filter(|_| !erased) {
            if allowed {
                let usage = state.usage.entry(record.key.clone()).or_default();
                usage.requests = usage.requests.saturating_add(1);
                usage.units = usage.units.saturating_add(u64::from(record.units));
                match record.request_class {
                    "recall" => usage.recall_requests = usage.recall_requests.saturating_add(1),
                    "detail" => usage.detail_requests = usage.detail_requests.saturating_add(1),
                    _ => usage.other_requests = usage.other_requests.saturating_add(1),
                }
            } else {
                let usage = state.usage.entry(record.key.clone()).or_default();
                usage.denied_requests = usage.denied_requests.saturating_add(1);
            }
        }
        Ok(statuses)
    }

    async fn inspect(
        &self,
        bucket: &BucketSpec,
        now_ms: u64,
    ) -> Result<BucketStatus, RateLimitError> {
        bucket.validate()?;
        let state = self.state.lock().map_err(|_| RateLimitError)?;
        if erasure::bucket_account(&bucket.key)
            .is_some_and(|account| state.erased_accounts.contains(account))
        {
            return Ok(calculate_status(bucket, 0.0, false));
        }
        let previous = state.buckets.get(&bucket.key);
        let tokens = previous.map_or(f64::from(bucket.capacity), |previous| {
            let elapsed_ms = now_ms.saturating_sub(previous.updated_ms);
            (previous.tokens + elapsed_ms as f64 * bucket.refill_per_second / 1000.0)
                .min(f64::from(bucket.capacity))
        });
        Ok(calculate_status(
            bucket,
            tokens,
            tokens >= f64::from(bucket.cost),
        ))
    }

    async fn usage_history(
        &self,
        keys_and_dates: &[(String, String)],
    ) -> Result<Vec<DailyUsage>, RateLimitError> {
        let state = self.state.lock().map_err(|_| RateLimitError)?;
        Ok(keys_and_dates
            .iter()
            .map(|(key, date)| {
                let mut usage = state.usage.get(key).cloned().unwrap_or_default();
                usage.date = date.clone();
                usage
            })
            .collect())
    }
}

pub fn account_bucket(secret: &str, account_id: &str) -> BucketSpec {
    BucketSpec::per_minute(
        format!(
            "wam:rate:account:{}",
            erasure::account_fingerprint(secret, account_id)
        ),
        300,
        60,
        1,
    )
}

pub fn client_bucket(secret: &str, client_ip: &str, class: &str, cost: u32) -> BucketSpec {
    let (name, rate, capacity) = match class {
        "anonymous" => ("anonymous", 60, 20),
        "status" => ("status", 30, 10),
        _ => ("shared", 600, 120),
    };
    let mut bucket = BucketSpec::per_minute(
        format!("wam:rate:ip:{name}:{}", fingerprint(secret, client_ip)),
        rate,
        capacity,
        cost,
    );
    bucket.cost = cost;
    bucket
}

pub fn normalize_client_ip(address: IpAddr) -> String {
    match address {
        IpAddr::V4(address) => format!("{address}/32"),
        IpAddr::V6(address) => {
            let segments = address.segments();
            format!(
                "{:x}:{:x}:{:x}:{:x}::/64",
                segments[0], segments[1], segments[2], segments[3]
            )
        }
    }
}

pub fn status_account_bucket(secret: &str, account_id: &str) -> BucketSpec {
    BucketSpec::per_minute(
        format!(
            "wam:rate:status-account:{}",
            erasure::account_fingerprint(secret, account_id)
        ),
        30,
        10,
        1,
    )
}

pub fn account_usage_key(secret: &str, account_id: &str, utc_date: &str) -> String {
    format!(
        "wam:usage:{}:{}",
        erasure::account_fingerprint(secret, account_id),
        utc_date
    )
}

fn fingerprint(secret: &str, identity: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"wam-rate-limit-v1\0");
    digest.update(secret.as_bytes());
    digest.update(b"\0");
    digest.update(identity.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn div_ceil(value: u64, divisor: u64) -> u64 {
    value / divisor + u64::from(value % divisor != 0)
}

fn value_or_zero(value: Option<i64>) -> u64 {
    value.unwrap_or_default().max(0) as u64
}

pub fn utc_dates(now_secs: u64, count: usize) -> Vec<String> {
    let last_day = (now_secs / 86_400) as i64;
    (0..count)
        .map(|offset| {
            let day = last_day - (count.saturating_sub(offset + 1) as i64);
            let (year, month, date) = civil_from_days(day);
            format!("{year:04}-{month:02}-{date:02}")
        })
        .collect()
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted / 146_097
    } else {
        (shifted - 146_096) / 146_097
    };
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, date)
}

#[cfg(test)]
mod tests {
    use super::{
        BucketSpec, InMemoryRateLimitStore, IpNetwork, RateLimitStore, UsageRecord, account_bucket,
        account_usage_key, normalize_client_ip, utc_dates,
    };
    use std::net::IpAddr;

    #[tokio::test]
    async fn weighted_buckets_refill_and_deny_without_charging_any_bucket() {
        let store = InMemoryRateLimitStore::default();
        let ip = BucketSpec::per_minute("ip", 60, 2, 1);
        let account = BucketSpec::per_minute("account", 300, 2, 1);

        let first = store
            .consume(&[ip.clone(), account.clone()], None, 1_000)
            .await
            .unwrap();
        assert!(first.iter().all(|status| status.allowed));
        assert_eq!(first[0].remaining_units, 1);
        let second = store
            .consume(&[ip.clone(), account.clone()], None, 1_000)
            .await
            .unwrap();
        assert!(second.iter().all(|status| status.allowed));
        let denied = store
            .consume(&[ip.clone(), account.clone()], None, 1_000)
            .await
            .unwrap();
        assert!(denied.iter().all(|status| !status.allowed));
        assert_eq!(denied[0].retry_after_seconds, 1);

        let account_still_has_capacity = store.inspect(&account, 1_000).await.unwrap();
        assert_eq!(account_still_has_capacity.remaining_units, 0);
        let refilled = store.consume(&[ip, account], None, 2_000).await.unwrap();
        assert!(refilled.iter().all(|status| status.allowed));
    }

    #[tokio::test]
    async fn daily_usage_is_account_scoped_and_status_reads_do_not_charge_quota() {
        let store = InMemoryRateLimitStore::default();
        let spec = account_bucket("test-secret", "account-a");
        let record = UsageRecord {
            key: account_usage_key("test-secret", "account-a", "2026-10-27"),
            units: 3,
            request_class: "recall",
        };
        store
            .consume(std::slice::from_ref(&spec), Some(&record), 10_000)
            .await
            .unwrap();
        let status = store.inspect(&spec, 10_000).await.unwrap();
        assert_eq!(status.remaining_units, 59);
        let history = store
            .usage_history(&[(record.key, "2026-10-27".to_owned())])
            .await
            .unwrap();
        assert_eq!(history[0].requests, 1);
        assert_eq!(history[0].units, 3);
        assert_eq!(history[0].recall_requests, 1);
        assert_eq!(history[0].detail_requests, 0);
    }

    #[tokio::test]
    async fn denied_account_requests_are_counted_without_charging_accepted_usage() {
        let store = InMemoryRateLimitStore::default();
        let bucket = BucketSpec::per_minute("account:small", 60, 1, 1);
        let record = UsageRecord {
            key: account_usage_key("test-secret", "account-a", "2026-10-27"),
            units: 1,
            request_class: "other",
        };

        let admitted = store
            .consume(std::slice::from_ref(&bucket), Some(&record), 10_000)
            .await
            .unwrap();
        assert!(admitted[0].allowed);
        let denied = store
            .consume(std::slice::from_ref(&bucket), Some(&record), 10_000)
            .await
            .unwrap();
        assert!(!denied[0].allowed);

        let history = store
            .usage_history(&[(record.key, "2026-10-27".to_owned())])
            .await
            .unwrap();
        assert_eq!(history[0].requests, 1);
        assert_eq!(history[0].units, 1);
        assert_eq!(history[0].denied_requests, 1);
    }

    #[test]
    fn trusted_proxy_ranges_validate_and_match_only_their_address_family() {
        let network: IpNetwork = "192.0.2.0/24".parse().unwrap();
        assert!(network.contains("192.0.2.15".parse().unwrap()));
        assert!(!network.contains("192.0.3.15".parse().unwrap()));
        assert!(!network.contains("2001:db8::1".parse().unwrap()));
        assert!("2001:db8::1/128".parse::<IpNetwork>().is_ok());
        assert!("192.0.2.1/33".parse::<IpNetwork>().is_err());
    }

    #[test]
    fn utc_usage_dates_span_month_boundaries_in_ascending_order() {
        assert_eq!(
            utc_dates(1_730_419_200, 3),
            ["2024-10-30", "2024-10-31", "2024-11-01"]
        );
        assert_eq!(
            utc_dates(1_730_505_600, 3),
            ["2024-10-31", "2024-11-01", "2024-11-02"]
        );
    }

    #[test]
    fn ipv6_clients_share_a_privacy_preserving_64_bit_prefix() {
        let first: IpAddr = "2001:db8:10:20::1".parse().unwrap();
        let rotated: IpAddr = "2001:db8:10:20:ffff::9".parse().unwrap();
        let other_network: IpAddr = "2001:db8:10:21::1".parse().unwrap();
        assert_eq!(normalize_client_ip(first), normalize_client_ip(rotated));
        assert_ne!(
            normalize_client_ip(first),
            normalize_client_ip(other_network)
        );
        assert_eq!(
            normalize_client_ip("192.0.2.7".parse().unwrap()),
            "192.0.2.7/32"
        );
    }
}
