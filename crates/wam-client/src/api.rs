//! Shared API contract and the HTTP endpoint bindings.

use crate::endpoint::{canonical_public_url, route_url};
use crate::{ApiError, DiagnosticError, DiagnosticOperation, HttpClient, LocalCacheStatus};
use async_trait::async_trait;
use wam_contracts::{
    ContributionListResponse, ContributionRecordResponse, CreateContributionRequest,
    DashboardResponse, LeaderboardResponse, McpAuthState, McpConsentState, McpStatusResponse,
    MemoryDetailResponse, OutcomeReportRequest, OutcomeReportResponse, OwnerUsageResponse,
    ProfilePutRequest, ProfileResponse, RecallRequest, RecallResponse, VocabResponse,
};

#[async_trait]
pub trait WamApi: Send + Sync {
    async fn report_diagnostic(
        &self,
        _operation: DiagnosticOperation,
        _error: DiagnosticError,
    ) -> Option<String> {
        None
    }
    fn last_trace_id(&self) -> Option<String> {
        None
    }
    async fn status(&self) -> Result<(), ApiError>;
    async fn memory_status(&self) -> Result<McpStatusResponse, ApiError> {
        self.status().await?;
        Ok(McpStatusResponse::new(
            "/mcp".into(),
            McpAuthState::Anonymous,
            McpConsentState::Unknown,
        ))
    }
    async fn vocab(&self) -> Result<VocabResponse, ApiError>;
    async fn recall(&self, request: RecallRequest) -> Result<RecallResponse, ApiError>;
    async fn expand(&self, id: &str, level: u8) -> Result<MemoryDetailResponse, ApiError>;
    async fn expand_version(
        &self,
        id: &str,
        level: u8,
        version_id: Option<&str>,
    ) -> Result<MemoryDetailResponse, ApiError> {
        if version_id.is_some_and(|version| version.is_empty() || version.len() > 64) {
            return Err(ApiError::InvalidInput);
        }
        let response = self.expand(id, level).await?;
        if version_id.is_some_and(|version| version != response.version_id) {
            return Err(ApiError::NotFound);
        }
        Ok(response)
    }
    async fn leaderboard(&self) -> Result<LeaderboardResponse, ApiError>;
    async fn dashboard(&self) -> Result<DashboardResponse, ApiError>;
    async fn my_usage(&self) -> Result<OwnerUsageResponse, ApiError> {
        Err(ApiError::Server)
    }
    async fn cache_status(&self) -> Result<LocalCacheStatus, ApiError> {
        Err(ApiError::CacheUnavailable)
    }
    async fn clear_cache(&self) -> Result<(), ApiError> {
        Err(ApiError::CacheUnavailable)
    }
    async fn public_profile(&self, handle: &str) -> Result<ProfileResponse, ApiError>;
    async fn my_profile(&self) -> Result<ProfileResponse, ApiError>;
    async fn save_profile(&self, profile: ProfilePutRequest) -> Result<ProfileResponse, ApiError>;
    async fn submit_contribution(
        &self,
        request: CreateContributionRequest,
    ) -> Result<ContributionRecordResponse, ApiError>;
    async fn my_contributions(&self) -> Result<ContributionListResponse, ApiError>;
    async fn withdraw_contribution(&self, id: &str) -> Result<(), ApiError>;
    async fn report_outcome(
        &self,
        receipt_id: &str,
        request: OutcomeReportRequest,
    ) -> Result<OutcomeReportResponse, ApiError>;
}

#[async_trait]
impl WamApi for HttpClient {
    async fn memory_status(&self) -> Result<McpStatusResponse, ApiError> {
        let endpoint = format!("{}/mcp", self.endpoint);
        match self
            .get::<McpStatusResponse>(&["v1", "mcp", "status"], None)
            .await
        {
            Ok(mut response) => {
                response.endpoint = endpoint;
                Ok(response)
            }
            Err(ApiError::Unauthorized) if self.token.is_some() => Ok(McpStatusResponse::new(
                endpoint,
                McpAuthState::ReauthRequired,
                McpConsentState::Unknown,
            )),
            Err(error) => Err(error),
        }
    }

    async fn expand_version(
        &self,
        id: &str,
        level: u8,
        version_id: Option<&str>,
    ) -> Result<MemoryDetailResponse, ApiError> {
        if !matches!(level, 1 | 2)
            || version_id.is_some_and(|version| version.is_empty() || version.len() > 64)
        {
            return Err(ApiError::InvalidInput);
        }
        let mut url = route_url(&self.endpoint, &["v1", "memories", id])?;
        url.query_pairs_mut()
            .append_pair("level", &level.to_string());
        if let Some(version) = version_id {
            url.query_pairs_mut().append_pair("version_id", version);
        }
        let response: MemoryDetailResponse = self.send(self.client.get(url)).await?;
        if version_id.is_some_and(|version| version != response.version_id) {
            return Err(ApiError::NotFound);
        }
        Ok(response)
    }

    async fn report_diagnostic(
        &self,
        operation: DiagnosticOperation,
        error: DiagnosticError,
    ) -> Option<String> {
        self.report_diagnostic_event(operation, error).await
    }

    fn last_trace_id(&self) -> Option<String> {
        HttpClient::last_trace_id(self)
    }

    async fn status(&self) -> Result<(), ApiError> {
        #[derive(serde::Deserialize)]
        struct Health {
            status: String,
        }
        let health: Health = self.get(&["healthz"], None).await?;
        if health.status == "ok" {
            Ok(())
        } else {
            Err(ApiError::InvalidResponse)
        }
    }

    async fn vocab(&self) -> Result<VocabResponse, ApiError> {
        self.get(&["v1", "memories", "vocab"], None).await
    }

    async fn recall(&self, request: RecallRequest) -> Result<RecallResponse, ApiError> {
        self.recall_with_cache(request).await
    }

    async fn expand(&self, id: &str, level: u8) -> Result<MemoryDetailResponse, ApiError> {
        let level = level.to_string();
        self.get(&["v1", "memories", id], Some(("level", &level)))
            .await
    }

    async fn leaderboard(&self) -> Result<LeaderboardResponse, ApiError> {
        self.get(&["v1", "leaderboard"], None).await
    }

    async fn dashboard(&self) -> Result<DashboardResponse, ApiError> {
        self.require_auth()?;
        self.get(&["v1", "me", "dashboard"], None).await
    }

    async fn my_usage(&self) -> Result<OwnerUsageResponse, ApiError> {
        self.require_auth()?;
        self.get(&["v1", "me", "usage"], None).await
    }

    async fn cache_status(&self) -> Result<LocalCacheStatus, ApiError> {
        self.local_cache_status()
    }

    async fn clear_cache(&self) -> Result<(), ApiError> {
        self.clear_local_cache()
    }

    async fn public_profile(&self, handle: &str) -> Result<ProfileResponse, ApiError> {
        self.get(&["v1", "profiles", handle], None).await
    }

    async fn my_profile(&self) -> Result<ProfileResponse, ApiError> {
        self.require_auth()?;
        self.get(&["v1", "me", "profile"], None).await
    }

    async fn save_profile(&self, profile: ProfilePutRequest) -> Result<ProfileResponse, ApiError> {
        self.require_auth()?;
        let mut builder = self
            .client
            .put(format!("{}/v1/me/profile", self.endpoint))
            .json(&profile);
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        self.send(builder).await
    }

    async fn submit_contribution(
        &self,
        mut request: CreateContributionRequest,
    ) -> Result<ContributionRecordResponse, ApiError> {
        self.require_auth()?;
        if !request.consent {
            return Err(ApiError::Forbidden);
        }
        request.url = canonical_public_url(&request.url)?;
        self.post_json(&["v1", "me", "contributions"], &request)
            .await
    }

    async fn my_contributions(&self) -> Result<ContributionListResponse, ApiError> {
        self.require_auth()?;
        self.get(&["v1", "me", "contributions"], None).await
    }

    async fn withdraw_contribution(&self, id: &str) -> Result<(), ApiError> {
        self.require_auth()?;
        let url = route_url(&self.endpoint, &["v1", "me", "contributions", id])?;
        self.send_empty(self.client.delete(url)).await
    }

    async fn report_outcome(
        &self,
        receipt_id: &str,
        request: OutcomeReportRequest,
    ) -> Result<OutcomeReportResponse, ApiError> {
        self.require_auth()?;
        if receipt_id.is_empty() || receipt_id.chars().count() > 128 {
            return Err(ApiError::InvalidInput);
        }
        self.post_json(&["v1", "receipts", receipt_id, "outcome"], &request)
            .await
    }
}
