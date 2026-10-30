//! Device linking transport; codes and tokens never appear in URLs or diagnostics.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{ApiError, HttpClient, credentials::LinkedCredential};

#[derive(Deserialize)]
pub struct LinkStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LinkPoll {
    Pending {
        interval: u64,
    },
    Authorized {
        device_id: String,
        access_token: String,
        scopes: Vec<String>,
        expires_at: u64,
    },
    Denied,
    Expired,
    AlreadyUsed,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LinkStatus {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

#[async_trait]
pub trait DeviceApi: Send + Sync {
    async fn start_link(&self) -> Result<LinkStart, ApiError>;
    async fn poll_link(&self, code: &str) -> Result<LinkPoll, ApiError>;
    async fn link_status(&self, token: &str) -> Result<LinkStatus, ApiError>;
    async fn revoke_link(&self, token: &str) -> Result<(), ApiError>;
}

impl HttpClient {
    pub fn with_credential(&self, credential: &LinkedCredential) -> Self {
        let mut client = self.clone();
        client.token = Some(credential.access_token.clone());
        client
    }
}

#[async_trait]
impl DeviceApi for HttpClient {
    async fn start_link(&self) -> Result<LinkStart, ApiError> {
        self.post_json(&["v1", "link", "start"], &serde_json::json!({
            "name": "WAM CLI / local MCP",
            "scopes": ["read:memories", "read:dashboard", "write:contributions", "report:outcome"]
        })).await
    }

    async fn poll_link(&self, code: &str) -> Result<LinkPoll, ApiError> {
        self.post_json(
            &["v1", "link", "poll"],
            &serde_json::json!({ "device_code": code }),
        )
        .await
    }

    async fn link_status(&self, token: &str) -> Result<LinkStatus, ApiError> {
        let mut client = self.clone();
        client.token = Some(token.to_owned());
        client.get(&["v1", "link", "status"], None).await
    }

    async fn revoke_link(&self, token: &str) -> Result<(), ApiError> {
        let mut client = self.clone();
        client.token = Some(token.to_owned());
        let url = crate::route_url(&self.endpoint, &["v1", "link", "revoke"])?;
        client.send_empty(client.client.post(url)).await
    }
}
