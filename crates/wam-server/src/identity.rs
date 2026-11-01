//! GitHub web sessions and device authorization for the local WAM server.
//! Secrets are returned once to their client and persisted only as SHA-256 digests.

pub mod admin;
pub mod email;
pub mod oauth;
mod provider;
pub(crate) mod store;
#[cfg(test)]
mod tests;

use provider::GitHubOAuthClient;
pub use provider::GitHubOAuthProvider;
pub use store::FileIdentityStore;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill as random_fill;
use reqwest::header;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use url::Url;

pub const SCOPE_READ_MEMORIES: &str = "read:memories";
pub const SCOPE_WRITE_CONTRIBUTIONS: &str = "write:contributions";
pub const SCOPE_REPORT_OUTCOME: &str = "report:outcome";
pub const SCOPE_READ_DASHBOARD: &str = "read:dashboard";
pub const SCOPE_MANAGE_DEVICES: &str = "manage:devices";
pub const SCOPE_MANAGE_ACCOUNT: &str = "manage:account";
const SESSION_TTL_SECS: u64 = 12 * 60 * 60;
const DEVICE_TTL_SECS: u64 = 90 * 24 * 60 * 60;
/// A device's "last used" time is refreshed at most this often, so a busy client costs one identity write per
/// interval instead of one per request.
const LAST_SEEN_REFRESH_SECS: u64 = 5 * 60;
const OAUTH_TTL_SECS: u64 = 5 * 60;
const DEVICE_CODE_TTL_SECS: u64 = 10 * 60;
const DEVICE_POLL_INTERVAL_SECS: u64 = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityError {
    Disabled,
    Invalid,
    Expired,
    Conflict,
    RateLimited,
    TooFast,
    Provider,
    Storage,
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Disabled => "identity provider is not configured",
            Self::Invalid => "authorization request is invalid",
            Self::Expired => "authorization request expired",
            Self::Conflict => "authorization request is no longer pending",
            Self::RateLimited => "authorization attempts are rate limited",
            Self::TooFast => "polling interval has not elapsed",
            Self::Provider => "identity provider request failed",
            Self::Storage => "identity store is unavailable",
        })
    }
}

impl std::error::Error for IdentityError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityCredential {
    pub account_id: String,
    pub scopes: Vec<String>,
    pub browser_session: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceApprovalPreview {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_at: u64,
    pub expires_at: u64,
    pub last_seen_at: Option<u64>,
    pub revoked: bool,
    pub oauth_managed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePoll {
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

pub struct IdentityService {
    store: Arc<FileIdentityStore>,
    provider: Option<Arc<dyn GitHubOAuthProvider>>,
    frontend_origin: String,
    cookie_secure: bool,
    mcp_oauth: Option<oauth::OAuthConfig>,
    email_auth: Option<email::EmailAuth>,
    admin_auth: Option<admin::AdminAuth>,
}

impl IdentityService {
    pub fn new(
        store: Arc<FileIdentityStore>,
        provider: Option<Arc<dyn GitHubOAuthProvider>>,
        frontend_origin: impl Into<String>,
        cookie_secure: bool,
    ) -> Self {
        Self {
            store,
            provider,
            frontend_origin: frontend_origin.into().trim_end_matches('/').to_owned(),
            cookie_secure,
            mcp_oauth: None,
            email_auth: None,
            admin_auth: None,
        }
    }

    pub fn from_env(store: Arc<FileIdentityStore>) -> Result<Self, IdentityError> {
        let frontend_origin = std::env::var("WAM_PUBLIC_ORIGIN")
            .unwrap_or_else(|_| "http://127.0.0.1:5173".to_owned());
        let origin = Url::parse(&frontend_origin).map_err(|_| IdentityError::Invalid)?;
        if !matches!(origin.scheme(), "http" | "https")
            || origin.host_str().is_none()
            || !origin.path().is_empty() && origin.path() != "/"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(IdentityError::Invalid);
        }
        let production = std::env::var("WAM_ENV").is_ok_and(|value| value == "production");
        let host = origin.host_str().ok_or(IdentityError::Invalid)?;
        let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
        if origin.scheme() == "http" && (!loopback || production) {
            return Err(IdentityError::Invalid);
        }
        let cookie_secure = origin.scheme() == "https";
        if production && !cookie_secure {
            return Err(IdentityError::Invalid);
        }
        if production && std::env::var_os("WAM_DEMO_TOKEN").is_some() {
            return Err(IdentityError::Invalid);
        }
        let client_id = std::env::var("WAM_GITHUB_CLIENT_ID").ok();
        let client_secret = std::env::var("WAM_GITHUB_CLIENT_SECRET").ok();
        let provider = match (client_id, client_secret) {
            (Some(client_id), Some(client_secret))
                if !client_id.is_empty() && !client_secret.is_empty() =>
            {
                let redirect_uri = std::env::var("WAM_GITHUB_REDIRECT_URI")
                    .unwrap_or_else(|_| "http://127.0.0.1:3000/v1/auth/github/callback".to_owned());
                let callback = Url::parse(&redirect_uri).map_err(|_| IdentityError::Invalid)?;
                let callback_host = callback.host_str().ok_or(IdentityError::Invalid)?;
                let callback_loopback = matches!(callback_host, "localhost" | "127.0.0.1" | "::1");
                if callback.path() != "/v1/auth/github/callback"
                    || !matches!(callback.scheme(), "http" | "https")
                    || callback.scheme() == "http" && (!callback_loopback || production)
                    || !callback.username().is_empty()
                    || callback.password().is_some()
                    || callback.query().is_some()
                    || callback.fragment().is_some()
                {
                    return Err(IdentityError::Invalid);
                }
                Some(Arc::new(GitHubOAuthClient::new(
                    client_id,
                    client_secret,
                    redirect_uri,
                )?) as Arc<dyn GitHubOAuthProvider>)
            }
            (None, None) => None,
            _ => return Err(IdentityError::Invalid),
        };
        let mut service = Self::new(store, provider, frontend_origin, cookie_secure);
        service.email_from_env()?;
        service.admin_from_env()?;
        if production && !service.oauth_enabled() && !service.email_auth_enabled() {
            return Err(IdentityError::Disabled);
        }
        if let Ok(resource) = std::env::var("WAM_MCP_PUBLIC_URL") {
            if !service.oauth_enabled() && !service.email_auth_enabled() {
                return Err(IdentityError::Disabled);
            }
            if production && !resource.starts_with("https://") {
                return Err(IdentityError::Invalid);
            }
            service = service
                .with_mcp_oauth(&resource)
                .map_err(|_| IdentityError::Invalid)?;
        }
        Ok(service)
    }

    pub fn oauth_enabled(&self) -> bool {
        self.provider.is_some()
    }

    pub fn begin_oauth(&self, now: u64) -> Result<(String, String), IdentityError> {
        let provider = self.provider.as_ref().ok_or(IdentityError::Disabled)?;
        let (state, challenge) = self.store.begin_oauth(now)?;
        let url = provider.authorization_url(&state, &challenge)?;
        Ok((state, url))
    }

    pub async fn finish_oauth(
        &self,
        code: &str,
        query_state: &str,
        cookie_state: &str,
        now: u64,
    ) -> Result<(String, String, String, u64), IdentityError> {
        let provider = self.provider.as_ref().ok_or(IdentityError::Disabled)?;
        if code.is_empty() || code.len() > 2048 {
            return Err(IdentityError::Invalid);
        }
        let verifier = self
            .store
            .take_oauth_verifier(query_state, cookie_state, now)?;
        let subject = provider.exchange_code(code, &verifier).await?;
        let account_id = self.store.account_for_github_subject(&subject)?;
        let (session, csrf, expires_at) = self.store.create_session(account_id.clone(), now)?;
        Ok((account_id, session, csrf, expires_at))
    }

    pub fn session_info(
        &self,
        session_token: &str,
        now: u64,
    ) -> Result<(String, String, u64), IdentityError> {
        self.store.session_info(session_token, now)
    }

    pub fn revoke_session(&self, session_token: &str, now: u64) -> Result<(), IdentityError> {
        self.store.revoke_session(session_token, now)
    }

    pub fn authenticate_session(&self, token: &str, now: u64) -> Option<IdentityCredential> {
        self.store.session_credential(token, now)
    }

    pub fn authenticate_device(&self, token: &str, now: u64) -> Option<IdentityCredential> {
        match &self.mcp_oauth {
            Some(config) => {
                self.store
                    .device_credential_for_resource(token, Some(config.resource()), now)
            }
            None => self.store.device_credential(token, now),
        }
    }

    pub fn authenticate_headers(
        &self,
        headers: &axum::http::HeaderMap,
        now: u64,
    ) -> Option<IdentityCredential> {
        if headers.contains_key(header::AUTHORIZATION) {
            let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
            let token = raw.strip_prefix("Bearer ")?;
            return self.authenticate_device(token, now);
        }
        let token = cookie_value(headers, "wam_session")?;
        self.authenticate_session(&token, now)
    }

    pub fn has_session_cookie(&self, headers: &axum::http::HeaderMap) -> bool {
        cookie_value(headers, "wam_session").is_some()
    }

    pub fn csrf_valid_for_headers(&self, headers: &axum::http::HeaderMap, now: u64) -> bool {
        let Some(session_token) = cookie_value(headers, "wam_session") else {
            return true;
        };
        let Some(csrf) = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        self.csrf_valid(&session_token, csrf, now)
    }

    pub fn csrf_valid(&self, token: &str, csrf: &str, now: u64) -> bool {
        self.store.csrf_valid(token, csrf, now)
    }

    pub fn start_device(
        &self,
        name: &str,
        scopes: Vec<String>,
        now: u64,
    ) -> Result<DeviceStart, IdentityError> {
        let scopes = validate_scopes(scopes)?;
        self.store
            .start_device(name, scopes, now, format!("{}/link", self.frontend_origin))
    }

    pub fn approve_device(
        &self,
        user_code: &str,
        account_id: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        self.store.approve_device(user_code, account_id, now)
    }

    pub fn preview_device(
        &self,
        user_code: &str,
        account_id: &str,
        now: u64,
    ) -> Result<DeviceApprovalPreview, IdentityError> {
        self.store.preview_device(user_code, account_id, now)
    }

    pub fn deny_device(
        &self,
        user_code: &str,
        account_id: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        self.store.deny_device(user_code, account_id, now)
    }

    pub fn poll_device(&self, code: &str, now: u64) -> Result<DevicePoll, IdentityError> {
        self.store.poll_device(code, now)
    }

    pub fn current_device(&self, token: &str, now: u64) -> Result<DeviceSummary, IdentityError> {
        self.store.current_device(
            token,
            self.mcp_oauth.as_ref().map(|config| config.resource()),
            now,
        )
    }

    pub fn revoke_current_device(&self, token: &str, now: u64) -> Result<(), IdentityError> {
        self.current_device(token, now)?;
        self.store.revoke_current_device(token, now)
    }

    pub fn devices(&self, account_id: &str, now: u64) -> Result<Vec<DeviceSummary>, IdentityError> {
        self.store.list_devices(account_id, now)
    }

    pub fn revoke_device(&self, account_id: &str, id: &str, now: u64) -> Result<(), IdentityError> {
        self.store.revoke_device(account_id, id, now)
    }

    pub fn rotate_device(
        &self,
        account_id: &str,
        id: &str,
        now: u64,
    ) -> Result<(String, DeviceSummary), IdentityError> {
        self.store.rotate_device(account_id, id, now)
    }

    pub fn frontend_origin(&self) -> &str {
        &self.frontend_origin
    }

    pub fn session_cookie(&self, token: &str, max_age: u64) -> String {
        format!(
            "wam_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }

    pub fn clear_session_cookie(&self) -> String {
        format!(
            "wam_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }

    pub fn oauth_state_cookie(&self, state: &str) -> String {
        format!(
            "wam_oauth_state={state}; Path=/v1/auth/github; HttpOnly; SameSite=Lax; Max-Age={OAUTH_TTL_SECS}{}",
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }

    pub fn clear_oauth_state_cookie(&self) -> String {
        format!(
            "wam_oauth_state=; Path=/v1/auth/github; HttpOnly; SameSite=Lax; Max-Age=0{}",
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }

    pub fn oauth_redirect(&self) -> String {
        format!("{}/dashboard", self.frontend_origin)
    }

    pub fn login_error_redirect(&self) -> String {
        format!("{}/login?auth_error=oauth_failed", self.frontend_origin)
    }
}

pub fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let mut found = None;
    for raw in headers.get_all(header::COOKIE).iter() {
        let raw = raw.to_str().ok()?;
        for pair in raw.split(';').map(str::trim) {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if key == name {
                if found.is_some() {
                    return None;
                }
                found = Some(value.to_owned());
            }
        }
    }
    found.filter(|value| !value.is_empty())
}

fn validate_scopes(scopes: Vec<String>) -> Result<Vec<String>, IdentityError> {
    let mut clean = Vec::new();
    for scope in scopes {
        if !matches!(
            scope.as_str(),
            SCOPE_READ_MEMORIES
                | SCOPE_READ_DASHBOARD
                | SCOPE_WRITE_CONTRIBUTIONS
                | SCOPE_REPORT_OUTCOME
        ) {
            return Err(IdentityError::Invalid);
        }
        if !clean.contains(&scope) {
            clean.push(scope);
        }
    }
    if !clean.contains(&SCOPE_READ_MEMORIES.to_owned()) {
        clean.insert(0, SCOPE_READ_MEMORIES.to_owned());
    }
    Ok(clean)
}

fn sanitize_device_name(name: &str) -> String {
    let clean = name
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_owned();
    if clean.is_empty() {
        "WAM client".to_owned()
    } else {
        clean
    }
}

fn normalize_user_code(code: &str) -> String {
    code.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

fn random_user_code() -> Result<String, IdentityError> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 8];
    random_fill(&mut bytes).map_err(|_| IdentityError::Storage)?;
    let code: String = bytes
        .iter()
        .map(|byte| ALPHABET[usize::from(*byte) % ALPHABET.len()] as char)
        .collect();
    Ok(format!("{}-{}", &code[..4], &code[4..]))
}

fn canonical_device_link_code(input: &str) -> Option<String> {
    const ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let input = input.trim().to_ascii_uppercase();
    let compact = match input.as_bytes() {
        bytes if bytes.len() == 8 => input.as_str(),
        bytes if bytes.len() == 9 && bytes[4] == b'-' => {
            return canonical_device_link_code(&format!("{}{}", &input[..4], &input[5..]));
        }
        _ => return None,
    };
    if !compact
        .chars()
        .all(|character| ALPHABET.contains(character))
    {
        return None;
    }
    Some(format!("{}-{}", &compact[..4], &compact[4..]))
}

pub(crate) fn random_secret() -> Result<String, IdentityError> {
    let mut bytes = [0u8; 32];
    random_fill(&mut bytes).map_err(|_| IdentityError::Storage)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn derive_csrf(session_token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(
        format!("wam-csrf-v1:{session_token}").as_bytes(),
    ))
}

fn digest(value: &str) -> String {
    let bytes = Sha256::digest(value.as_bytes());
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn constant_hash_eq(left: &str, right: &str) -> bool {
    left.len() == right.len() && left.as_bytes().ct_eq(right.as_bytes()).unwrap_u8() == 1
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn ensure_private_data_directory(path: &Path) -> io::Result<()> {
    let canonical = fs::canonicalize(path)?;
    let shared_temp = std::env::temp_dir().canonicalize().unwrap_or_default();
    let shared_tmp = Path::new("/tmp").canonicalize().unwrap_or_default();
    if canonical == Path::new("/") || canonical == shared_temp || canonical == shared_tmp {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "identity store requires a private application data directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
