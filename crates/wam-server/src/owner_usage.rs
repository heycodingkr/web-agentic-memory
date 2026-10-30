//! Shared owner quota/history projection for HTTP and MCP transports.
use crate::rate_limit::{
    RateLimitError, RateLimitStore, account_bucket, account_usage_key, utc_dates,
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use wam_contracts::{OwnerUsageResponse, SCHEMA_VERSION, UsageDailyPoint};

#[derive(Clone)]
pub(crate) struct OwnerUsageQuery {
    limits: Arc<dyn RateLimitStore>,
    secret: Arc<str>,
}

impl OwnerUsageQuery {
    pub(crate) fn new(limits: Arc<dyn RateLimitStore>, secret: Arc<str>) -> Self {
        Self { limits, secret }
    }

    pub(crate) async fn read(&self, account: &str) -> Result<OwnerUsageResponse, RateLimitError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let quota = self
            .limits
            .inspect(&account_bucket(&self.secret, account), now_ms)
            .await?;
        let keyed_dates: Vec<_> = utc_dates(now_ms / 1000, 30)
            .into_iter()
            .map(|date| (account_usage_key(&self.secret, account, &date), date))
            .collect();
        let daily = self
            .limits
            .usage_history(&keyed_dates)
            .await?
            .into_iter()
            .map(|point| UsageDailyPoint {
                utc_date: point.date,
                requests: point.requests,
                units: point.units,
                recall_requests: point.recall_requests,
                detail_requests: point.detail_requests,
                other_requests: point.other_requests,
                denied_requests: point.denied_requests,
            })
            .collect();
        Ok(OwnerUsageResponse {
            schema_version: SCHEMA_VERSION,
            period_days: 30,
            limit_per_minute: quota.limit_per_minute,
            burst_capacity: quota.capacity,
            remaining_units: quota.remaining_units,
            resets_in_seconds: quota.resets_in_seconds,
            daily,
        })
    }
}
