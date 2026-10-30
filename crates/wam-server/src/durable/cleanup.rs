use super::ApplicationRepository;
use crate::{application::ServiceError, rate_limit::RateLimitStore};

impl ApplicationRepository {
    /// Call from a blocking port context, as the repository reads/writes are
    /// synchronous. External deletion and acknowledgment are individually
    /// idempotent, so a crash between them is safe to retry.
    pub async fn deliver_account_deletions(
        &self,
        limits: &dyn RateLimitStore,
        secret: &str,
        now: u64,
    ) -> Result<usize, ServiceError> {
        let events = self.pending_account_deletions(100)?;
        let mut delivered = 0;
        for event in events {
            limits
                .erase_account(secret, &event.target)
                .await
                .map_err(|_| ServiceError::Unavailable)?;
            self.acknowledge_outbox(&event.id, now)?;
            delivered += 1;
        }
        Ok(delivered)
    }
}
