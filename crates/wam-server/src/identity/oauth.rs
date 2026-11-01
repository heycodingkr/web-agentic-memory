//! Public-client OAuth profile for the remote MCP resource. No client metadata URLs are fetched.
use super::{IdentityError, IdentityService, SCOPE_READ_MEMORIES, validate_scopes};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

pub const SCOPES: &[&str] = &[
    "read:memories",
    "read:dashboard",
    "write:contributions",
    "report:outcome",
];
pub(super) const ACCESS_TTL: u64 = 3600;
pub(super) const REFRESH_TTL: u64 = 30 * 24 * 3600;
pub(super) const REQUEST_TTL: u64 = 600;
pub(super) const CODE_TTL: u64 = 60;

#[derive(Clone)]
pub struct OAuthConfig {
    resource: String,
    issuer: String,
}
impl OAuthConfig {
    pub fn new(resource: &str) -> Result<Self, OAuthError> {
        let url = safe_url(resource)?;
        if url.path() != "/mcp" || url.query().is_some() {
            return Err(OAuthError::InvalidRequest);
        }
        Ok(Self {
            resource: url.to_string(),
            issuer: url.origin().ascii_serialization(),
        })
    }
    pub fn resource(&self) -> &str {
        &self.resource
    }
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn metadata_url(&self) -> String {
        format!("{}/.well-known/oauth-protected-resource/mcp", self.issuer)
    }
    pub fn resource_matches(&self, resource: &str) -> bool {
        safe_url(resource).is_ok_and(|url| url.as_str() == self.resource)
    }
    pub fn resource_metadata(&self) -> serde_json::Value {
        serde_json::json!({ "resource": self.resource, "authorization_servers": [self.issuer],
            "scopes_supported": SCOPES, "bearer_methods_supported": ["header"],
            "resource_name": "Web Agentic Memory" })
    }
    pub fn server_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "issuer": self.issuer,
            "authorization_endpoint": format!("{}/oauth/authorize", self.issuer),
            "token_endpoint": format!("{}/oauth/token", self.issuer),
            "registration_endpoint": format!("{}/oauth/register", self.issuer),
            "revocation_endpoint": format!("{}/oauth/revoke", self.issuer),
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "token_endpoint_auth_methods_supported": ["none"],
            "revocation_endpoint_auth_methods_supported": ["none"],
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": SCOPES,
            "authorization_response_iss_parameter_supported": true,
            "client_id_metadata_document_supported": false
        })
    }
    pub fn callback(
        &self,
        redirect: &str,
        state: Option<&str>,
        value: &str,
        success: bool,
    ) -> Result<String, OAuthError> {
        let mut url = safe_url(redirect)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair(if success { "code" } else { "error" }, value);
            if let Some(state) = state {
                query.append_pair("state", state);
            }
            query.append_pair("iss", &self.issuer);
        }
        Ok(url.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OAuthError {
    Disabled,
    InvalidRequest,
    InvalidClient,
    InvalidClientMetadata,
    InvalidRedirectUri,
    InvalidScope,
    InvalidGrant,
    UnsupportedGrantType,
    RateLimited,
    ServerError,
}
impl OAuthError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Disabled => "oauth_not_configured",
            Self::InvalidRequest => "invalid_request",
            Self::InvalidClient => "invalid_client",
            Self::InvalidClientMetadata => "invalid_client_metadata",
            Self::InvalidRedirectUri => "invalid_redirect_uri",
            Self::InvalidScope => "invalid_scope",
            Self::InvalidGrant => "invalid_grant",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::RateLimited => "temporarily_unavailable",
            Self::ServerError => "server_error",
        }
    }
}
impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for OAuthError {}
impl From<IdentityError> for OAuthError {
    fn from(error: IdentityError) -> Self {
        match error {
            IdentityError::Disabled => Self::Disabled,
            IdentityError::Expired | IdentityError::Conflict => Self::InvalidGrant,
            IdentityError::Invalid => Self::InvalidRequest,
            IdentityError::RateLimited | IdentityError::TooFast => Self::RateLimited,
            _ => Self::ServerError,
        }
    }
}

/// Unknown registration metadata is ignored per RFC 7591. No supplied URL is fetched.
#[derive(Deserialize)]
pub struct RegistrationInput {
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
    pub token_endpoint_auth_method: Option<String>,
    pub grant_types: Option<Vec<String>>,
    pub response_types: Option<Vec<String>>,
    pub scope: Option<String>,
}
#[derive(Serialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_id_issued_at: u64,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: &'static str,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub scope: String,
}
#[derive(Deserialize)]
pub struct AuthorizationInput {
    pub client_id: String,
    pub redirect_uri: String,
    pub response_type: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: String,
    pub scope: Option<String>,
    pub state: Option<String>,
}
#[derive(Deserialize)]
pub struct TokenInput {
    pub grant_type: String,
    pub client_id: String,
    pub resource: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub client_secret: Option<String>,
}
#[derive(Deserialize)]
pub struct RevocationInput {
    pub client_id: String,
    pub token: String,
    pub token_type_hint: Option<String>,
}
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}
#[derive(Serialize)]
pub struct AuthorizationPreview {
    pub client_name: String,
    pub redirect_uri: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

pub(super) fn loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}
pub(super) fn safe_url(input: &str) -> Result<Url, OAuthError> {
    if input.len() > 2048 || input.chars().any(char::is_control) {
        return Err(OAuthError::InvalidRequest);
    }
    let url = Url::parse(input).map_err(|_| OAuthError::InvalidRequest)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https" || url.scheme() == "http" && loopback(&url))
    {
        return Err(OAuthError::InvalidRequest);
    }
    Ok(url)
}
pub(super) fn redirect_uri(input: &str) -> Result<String, OAuthError> {
    let url = safe_url(input).map_err(|_| OAuthError::InvalidRedirectUri)?;
    if url
        .query_pairs()
        .any(|(key, _)| matches!(key.as_ref(), "code" | "state" | "iss" | "error"))
    {
        return Err(OAuthError::InvalidRedirectUri);
    }
    Ok(url.into())
}
pub(super) fn redirect_matches(registered: &str, actual: &str) -> bool {
    let (Ok(mut registered), Ok(mut actual)) = (Url::parse(registered), Url::parse(actual)) else {
        return false;
    };
    if registered.scheme() == "http"
        && actual.scheme() == "http"
        && loopback(&registered)
        && loopback(&actual)
    {
        // RFC 8252 permits an ephemeral loopback port, but no other component may change.
        let _ = registered.set_port(None);
        let _ = actual.set_port(None);
    }
    registered == actual
}
pub(super) fn scopes(input: Option<&str>) -> Result<Vec<String>, OAuthError> {
    let input = input.unwrap_or(SCOPE_READ_MEMORIES);
    if input.len() > 256 || input.is_empty() {
        return Err(OAuthError::InvalidScope);
    }
    validate_scopes(input.split_ascii_whitespace().map(str::to_owned).collect())
        .map_err(|_| OAuthError::InvalidScope)
}
pub(super) fn valid_challenge(value: &str) -> bool {
    value.len() == 43
        && URL_SAFE_NO_PAD
            .decode(value)
            .is_ok_and(|bytes| bytes.len() == 32)
}
pub(super) fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        && super::constant_hash_eq(
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            challenge,
        )
}

impl IdentityService {
    pub fn with_mcp_oauth(mut self, resource: &str) -> Result<Self, OAuthError> {
        let frontend = safe_url(&self.frontend_origin)?;
        if frontend.path() != "/" || frontend.query().is_some() {
            return Err(OAuthError::InvalidRequest);
        }
        self.mcp_oauth = Some(OAuthConfig::new(resource)?);
        Ok(self)
    }
    pub fn mcp_oauth_config(&self) -> Result<&OAuthConfig, OAuthError> {
        self.mcp_oauth.as_ref().ok_or(OAuthError::Disabled)
    }
    pub fn register_mcp_client(
        &self,
        input: RegistrationInput,
        now: u64,
    ) -> Result<RegisteredClient, OAuthError> {
        self.mcp_oauth_config()?;
        self.store.register_mcp_client(input, now)
    }
    pub fn start_mcp_authorization(
        &self,
        input: AuthorizationInput,
        now: u64,
    ) -> Result<String, OAuthError> {
        let config = self.mcp_oauth_config()?;
        self.store.start_mcp_authorization(input, config, now)
    }
    pub fn preview_mcp_authorization(
        &self,
        request: &str,
        account: &str,
        now: u64,
    ) -> Result<AuthorizationPreview, OAuthError> {
        self.mcp_oauth_config()?;
        self.store.preview_mcp_authorization(request, account, now)
    }
    pub fn decide_mcp_authorization(
        &self,
        request: &str,
        account: &str,
        approve: bool,
        now: u64,
    ) -> Result<String, OAuthError> {
        let config = self.mcp_oauth_config()?;
        self.store
            .decide_mcp_authorization(request, account, approve, config, now)
    }
    pub fn exchange_mcp_token(
        &self,
        input: TokenInput,
        now: u64,
    ) -> Result<TokenResponse, OAuthError> {
        let config = self.mcp_oauth_config()?;
        self.store.exchange_mcp_token(input, config, now)
    }
    pub fn revoke_mcp_token(&self, input: RevocationInput, now: u64) -> Result<(), OAuthError> {
        self.mcp_oauth_config()?;
        self.store.revoke_mcp_token(input, now)
    }
    pub fn begin_oauth_for_mcp(
        &self,
        request: &str,
        now: u64,
    ) -> Result<(String, String), IdentityError> {
        let provider = self.provider.as_ref().ok_or(IdentityError::Disabled)?;
        if request.len() != 43 || !self.mcp_request_exists(request, now) {
            return Err(IdentityError::Invalid);
        }
        let (state, challenge) = self
            .store
            .begin_oauth_with_return(now, Some(super::digest(request)))?;
        let url = provider.authorization_url(&state, &challenge)?;
        Ok((state, url))
    }

    pub fn begin_oauth_for_device_link(
        &self,
        code: &str,
        now: u64,
    ) -> Result<(String, String), IdentityError> {
        let code = super::canonical_device_link_code(code).ok_or(IdentityError::Invalid)?;
        let provider = self.provider.as_ref().ok_or(IdentityError::Disabled)?;
        let return_hash = super::digest(&format!("device:{code}"));
        let (state, challenge) = self.store.begin_oauth_with_return(now, Some(return_hash))?;
        let url = provider.authorization_url(&state, &challenge)?;
        Ok((state, url))
    }

    pub fn device_link_oauth_return_cookie(&self, code: &str) -> Option<String> {
        let code = super::canonical_device_link_code(code)?;
        Some(format!(
            "wam_oauth_return=device:{code}; Path=/v1/auth/github; HttpOnly; SameSite=Lax; Max-Age=300{}",
            if self.cookie_secure { "; Secure" } else { "" }
        ))
    }

    pub fn device_link_oauth_return(
        &self,
        state: &str,
        cookie_state: &str,
        return_cookie: &str,
        now: u64,
    ) -> Option<String> {
        let code = return_cookie.strip_prefix("device:")?;
        let code = super::canonical_device_link_code(code)?;
        let bound_return = format!("device:{code}");
        self.store
            .oauth_return_matches(state, cookie_state, &bound_return, now)
            .then(|| format!("{}/link?code={code}", self.frontend_origin))
    }
    pub fn mcp_oauth_return(
        &self,
        state: &str,
        cookie_state: &str,
        request: &str,
        now: u64,
    ) -> Option<String> {
        (request.len() == 43
            && self
                .store
                .oauth_return_matches(state, cookie_state, request, now))
        .then(|| format!("{}/oauth/approve?request={request}", self.frontend_origin))
    }
    pub fn oauth_return_cookie(&self, request: &str) -> String {
        format!(
            "wam_oauth_return={request}; Path=/v1/auth/github; HttpOnly; SameSite=Lax; Max-Age=300{}",
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }
    pub fn clear_oauth_return_cookie(&self) -> String {
        format!(
            "wam_oauth_return=; Path=/v1/auth/github; HttpOnly; SameSite=Lax; Max-Age=0{}",
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }
    pub fn mcp_request_exists(&self, request: &str, now: u64) -> bool {
        self.store.mcp_request_exists(request, now)
    }
}
