#![forbid(unsafe_code)]

//! HTTP transport for command-line and MCP callers. It knows wire contracts,
//! not CLI flags, MCP protocol details, browser state, or core ranking rules.

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use std::{fmt, time::Duration};
use wam_contracts::{
    ContributionListResponse, ContributionRecordResponse, CreateContributionRequest,
    DashboardResponse, LeaderboardResponse, MemoryDetailResponse, OutcomeReportRequest,
    OutcomeReportResponse, ProfilePutRequest, ProfileResponse, RecallRequest, RecallResponse,
    VocabResponse,
};
use wam_core::url_scope::{CanonicalUrl, UrlMode};

#[derive(Clone)]
pub struct HttpClient {
    endpoint: String,
    token: Option<String>,
    client: Client,
}

impl HttpClient {
    pub fn new(endpoint: impl Into<String>, token: Option<String>) -> Result<Self, ApiError> {
        let endpoint = endpoint.into().trim_end_matches('/').to_owned();
        let parsed = reqwest::Url::parse(&endpoint).map_err(|_| ApiError::InvalidEndpoint)?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(ApiError::InvalidEndpoint);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("wam-client/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ApiError::Transport)?;
        Ok(Self {
            endpoint,
            token,
            client,
        })
    }

    async fn get<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: Option<(&str, &str)>,
    ) -> Result<T, ApiError> {
        let mut url = route_url(&self.endpoint, segments)?;
        if let Some((key, value)) = query {
            url.query_pairs_mut().append_pair(key, value);
        }
        let request = self.client.get(url);
        self.send(request).await
    }

    async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        segments: &[&str],
        body: &B,
    ) -> Result<T, ApiError> {
        let url = route_url(&self.endpoint, segments)?;
        self.send(self.client.post(url).json(body)).await
    }

    async fn send_empty(&self, request: reqwest::RequestBuilder) -> Result<(), ApiError> {
        let mut request = request;
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|_| ApiError::Transport)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(error_for_status(response.status()))
        }
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ApiError> {
        let mut request = request;
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|_| ApiError::Transport)?;
        decode(response).await
    }
}

fn canonical_public_url(input: &str) -> Result<String, ApiError> {
    let url = CanonicalUrl::parse(input, UrlMode::Public).map_err(|_| ApiError::InvalidInput)?;
    Ok(format!("https://{}{}", url.host(), url.path()))
}

fn route_url(endpoint: &str, segments: &[&str]) -> Result<reqwest::Url, ApiError> {
    let mut url = reqwest::Url::parse(endpoint).map_err(|_| ApiError::InvalidEndpoint)?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| ApiError::InvalidEndpoint)?;
        path.pop_if_empty().extend(segments.iter().copied());
    }
    Ok(url)
}

fn error_for_status(status: StatusCode) -> ApiError {
    match status {
        StatusCode::UNAUTHORIZED => ApiError::Unauthorized,
        StatusCode::FORBIDDEN => ApiError::Forbidden,
        StatusCode::NOT_FOUND => ApiError::NotFound,
        StatusCode::CONFLICT => ApiError::Conflict,
        StatusCode::BAD_REQUEST => ApiError::InvalidInput,
        _ => ApiError::Server,
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ApiError> {
    if !response.status().is_success() {
        return Err(error_for_status(response.status()));
    }
    response.json().await.map_err(|_| ApiError::InvalidResponse)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiError {
    InvalidEndpoint,
    InvalidInput,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Transport,
    Server,
    InvalidResponse,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidEndpoint => "invalid API endpoint",
            Self::InvalidInput => "request was rejected",
            Self::Unauthorized => "authentication required",
            Self::Forbidden => "permission denied",
            Self::NotFound => "resource not found",
            Self::Conflict => "resource conflict",
            Self::Transport => "API is unreachable",
            Self::Server => "API returned an error",
            Self::InvalidResponse => "API response did not match the contract",
        })
    }
}

impl std::error::Error for ApiError {}

#[async_trait]
pub trait WamApi: Send + Sync {
    async fn status(&self) -> Result<(), ApiError>;
    async fn vocab(&self) -> Result<VocabResponse, ApiError>;
    async fn recall(&self, request: RecallRequest) -> Result<RecallResponse, ApiError>;
    async fn expand(&self, id: &str, level: u8) -> Result<MemoryDetailResponse, ApiError>;
    async fn leaderboard(&self) -> Result<LeaderboardResponse, ApiError>;
    async fn dashboard(&self) -> Result<DashboardResponse, ApiError>;
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

    async fn recall(&self, mut request: RecallRequest) -> Result<RecallResponse, ApiError> {
        request.url = canonical_public_url(&request.url)?;
        let mut builder = self
            .client
            .post(format!("{}/v1/memories/recall", self.endpoint))
            .json(&request);
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        self.send(builder).await
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
        self.get(&["v1", "me", "dashboard"], None).await
    }

    async fn public_profile(&self, handle: &str) -> Result<ProfileResponse, ApiError> {
        self.get(&["v1", "profiles", handle], None).await
    }

    async fn my_profile(&self) -> Result<ProfileResponse, ApiError> {
        self.get(&["v1", "me", "profile"], None).await
    }

    async fn save_profile(&self, profile: ProfilePutRequest) -> Result<ProfileResponse, ApiError> {
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
        request.url = canonical_public_url(&request.url)?;
        self.post_json(&["v1", "me", "contributions"], &request)
            .await
    }

    async fn my_contributions(&self) -> Result<ContributionListResponse, ApiError> {
        self.get(&["v1", "me", "contributions"], None).await
    }

    async fn withdraw_contribution(&self, id: &str) -> Result<(), ApiError> {
        let url = route_url(&self.endpoint, &["v1", "me", "contributions", id])?;
        self.send_empty(self.client.delete(url)).await
    }

    async fn report_outcome(
        &self,
        receipt_id: &str,
        request: OutcomeReportRequest,
    ) -> Result<OutcomeReportResponse, ApiError> {
        if receipt_id.is_empty() || receipt_id.chars().count() > 128 {
            return Err(ApiError::InvalidInput);
        }
        self.post_json(&["v1", "receipts", receipt_id, "outcome"], &request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_endpoints_without_network_access() {
        assert!(matches!(
            HttpClient::new("file:///tmp/wam", None),
            Err(ApiError::InvalidEndpoint)
        ));
        assert!(HttpClient::new("http://127.0.0.1:3000/", None).is_ok());
    }

    #[test]
    fn public_urls_are_canonicalized_before_they_can_be_sent() {
        assert_eq!(
            canonical_public_url("https://example.com/guide?token=private#section").unwrap(),
            "https://example.com/guide"
        );
        assert!(!canonical_public_url("http://example.com/guide").is_ok());
    }

    #[test]
    fn dynamic_path_values_are_encoded_as_single_segments() {
        let url = route_url(
            "http://127.0.0.1:3000",
            &["v1", "profiles", "alice/example?private=1"],
        )
        .unwrap();
        assert_eq!(url.path(), "/v1/profiles/alice%2Fexample%3Fprivate=1");
        assert_eq!(url.query(), None);
        let dashboard = route_url("http://127.0.0.1:3000", &["v1", "me", "dashboard"]).unwrap();
        assert_eq!(dashboard.path(), "/v1/me/dashboard");
        let contribution = route_url(
            "http://127.0.0.1:3000",
            &["v1", "me", "contributions", "local/1"],
        )
        .unwrap();
        assert_eq!(contribution.path(), "/v1/me/contributions/local%2F1");
    }
}
