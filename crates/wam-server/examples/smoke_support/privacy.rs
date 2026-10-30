//! Instrumentation for synthetic-only privacy sweeps of the loopback smoke fixture.
use async_trait::async_trait;
use serde_json::json;
use std::{fs::OpenOptions, io::Write, path::PathBuf};
use wam_gatekeeper::provider::{
    LlmProvider, ModelTurn, ProviderError, ScriptedProvider, TurnRequest, protocol_text,
    render_transcript,
};
use wam_server::{
    InMemoryRateLimitStore,
    rate_limit::{
        BucketSpec, BucketStatus, DailyUsage, RateLimitError, RateLimitStore, UsageRecord,
    },
};

pub struct CaptureProvider {
    pub inner: ScriptedProvider,
    pub path: Option<PathBuf>,
}

impl LlmProvider for CaptureProvider {
    fn name(&self) -> String {
        self.inner.name()
    }

    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        let turn = self.inner.next_turn(request)?;
        if let Some(path) = &self.path {
            let mut options = OpenOptions::new();
            options.append(true).create(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let row = json!({
                "system": request.system,
                "tools": protocol_text(request.tools),
                "transcript": render_transcript(request.transcript),
                "reply": turn.raw,
            });
            let mut file = options.open(path).map_err(|_| {
                ProviderError::Failed("synthetic provider capture unavailable".into())
            })?;
            writeln!(file, "{row}").map_err(|_| {
                ProviderError::Failed("synthetic provider capture unavailable".into())
            })?;
        }
        Ok(turn)
    }
}

pub struct FixtureRateLimits {
    pub inner: InMemoryRateLimitStore,
    pub bulk: bool,
}

impl FixtureRateLimits {
    fn bucket(&self, source: &BucketSpec) -> BucketSpec {
        let mut bucket = source.clone();
        if self.bulk {
            bucket.limit_per_minute *= 100;
            bucket.capacity *= 100;
            bucket.refill_per_second *= 100.0;
        }
        bucket
    }
}

#[async_trait]
impl RateLimitStore for FixtureRateLimits {
    async fn consume(
        &self,
        buckets: &[BucketSpec],
        usage: Option<&UsageRecord>,
        now_ms: u64,
    ) -> Result<Vec<BucketStatus>, RateLimitError> {
        let buckets: Vec<_> = buckets.iter().map(|b| self.bucket(b)).collect();
        self.inner.consume(&buckets, usage, now_ms).await
    }

    async fn inspect(
        &self,
        bucket: &BucketSpec,
        now_ms: u64,
    ) -> Result<BucketStatus, RateLimitError> {
        self.inner.inspect(&self.bucket(bucket), now_ms).await
    }

    async fn usage_history(
        &self,
        keys_and_dates: &[(String, String)],
    ) -> Result<Vec<DailyUsage>, RateLimitError> {
        self.inner.usage_history(keys_and_dates).await
    }

    async fn erase_account(
        &self,
        secret: &str,
        account_digest: &str,
    ) -> Result<(), RateLimitError> {
        self.inner.erase_account(secret, account_digest).await
    }
}
