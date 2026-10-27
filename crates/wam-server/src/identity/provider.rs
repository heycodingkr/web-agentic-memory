use super::IdentityError;
use async_trait::async_trait;
use reqwest::{Client, header};
use serde::Deserialize;
use url::Url;

#[async_trait]
pub trait GitHubOAuthProvider: Send + Sync + 'static {
    fn authorization_url(&self, state: &str, code_challenge: &str)
    -> Result<String, IdentityError>;
    async fn exchange_code(&self, code: &str, code_verifier: &str)
    -> Result<String, IdentityError>;
}

pub(super) struct GitHubOAuthClient {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    client: Client,
}

impl GitHubOAuthClient {
    pub(super) fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    ) -> Result<Self, IdentityError> {
        let client = Client::builder()
            .user_agent(concat!("wam-server/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|_| IdentityError::Provider)?;
        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            client,
        })
    }
}

#[derive(Deserialize)]
struct GitHubTokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct GitHubUserResponse {
    id: u64,
}

#[async_trait]
impl GitHubOAuthProvider for GitHubOAuthClient {
    fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
    ) -> Result<String, IdentityError> {
        let mut url = Url::parse("https://github.com/login/oauth/authorize")
            .map_err(|_| IdentityError::Provider)?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", "read:user")
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<String, IdentityError> {
        let response = self
            .client
            .post("https://github.com/login/oauth/access_token")
            .header(header::ACCEPT, "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .map_err(|_| IdentityError::Provider)?;
        if !response.status().is_success() {
            return Err(IdentityError::Provider);
        }
        let token: GitHubTokenResponse =
            response.json().await.map_err(|_| IdentityError::Provider)?;
        let access_token = token.access_token.ok_or(IdentityError::Provider)?;
        let response = self
            .client
            .get("https://api.github.com/user")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| IdentityError::Provider)?;
        if !response.status().is_success() {
            return Err(IdentityError::Provider);
        }
        let user: GitHubUserResponse =
            response.json().await.map_err(|_| IdentityError::Provider)?;
        Ok(user.id.to_string())
    }
}
