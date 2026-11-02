use super::{FileIdentityStore, device::last_seen_is_stale};
use crate::identity::oauth::{
    self, ACCESS_TTL, AuthorizationInput, AuthorizationPreview, CODE_TTL, OAuthConfig, OAuthError,
    REFRESH_TTL, REQUEST_TTL, RegisteredClient, RegistrationInput, RevocationInput, TokenInput,
    TokenResponse,
};
use crate::identity::{
    DeviceSummary, IdentityCredential, constant_hash_eq, digest, random_secret,
    sanitize_device_name,
};
use serde::{Deserialize, Serialize};

const CLIENT_TTL: u64 = 90 * 24 * 3600;
const MAX_CLIENTS: usize = 2048;
const MAX_REQUESTS: usize = 4096;
const MAX_GRANTS: usize = 10_000;
const MAX_REFRESHES: usize = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClientRecord {
    id: String,
    name: String,
    redirects: Vec<String>,
    scopes: Vec<String>,
    allow_refresh: bool,
    expires_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuthorizationRecord {
    request_hash: String,
    client_id: String,
    redirect: String,
    scopes: Vec<String>,
    challenge: String,
    resource: String,
    state: Option<String>,
    account_id: Option<String>,
    expires_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CodeRecord {
    code_hash: String,
    client_id: String,
    redirect: String,
    scopes: Vec<String>,
    challenge: String,
    resource: String,
    account_id: String,
    expires_at: u64,
    used_at: Option<u64>,
    family_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct GrantRecord {
    id: String,
    account_id: String,
    client_id: String,
    name: String,
    resource: String,
    scopes: Vec<String>,
    created_at: u64,
    expires_at: u64,
    revoked_at: Option<u64>,
    access_hash: String,
    access_expires_at: u64,
    /// Last time the client used the grant: a token issue or refresh, or an authenticated request.
    #[serde(default)]
    last_seen_at: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct RefreshRecord {
    token_hash: String,
    family_id: String,
    expires_at: u64,
    used_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct McpOAuthState {
    clients: Vec<ClientRecord>,
    requests: Vec<AuthorizationRecord>,
    codes: Vec<CodeRecord>,
    grants: Vec<GrantRecord>,
    refreshes: Vec<RefreshRecord>,
}

impl McpOAuthState {
    pub(super) fn purge(&mut self, now: u64) {
        self.clients.retain(|client| client.expires_at > now);
        self.requests.retain(|request| request.expires_at > now);
        self.grants
            .retain(|grant| grant.expires_at > now && grant.revoked_at.is_none());
        self.refreshes.retain(|token| {
            token.expires_at > now && self.grants.iter().any(|grant| grant.id == token.family_id)
        });
        // Keep consumed code digests while their grant is active, to detect replay and revoke it.
        self.codes.retain(|code| {
            code.expires_at > now
                || code
                    .family_id
                    .as_ref()
                    .is_some_and(|id| self.grants.iter().any(|grant| grant.id == *id))
        });
    }
    pub(super) fn erase_account(&mut self, account: &str) {
        self.requests
            .retain(|request| request.account_id.as_deref() != Some(account));
        self.codes.retain(|code| code.account_id != account);
        self.grants.retain(|grant| grant.account_id != account);
        self.refreshes
            .retain(|token| self.grants.iter().any(|grant| grant.id == token.family_id));
    }
    fn revoke_family(&mut self, id: &str, now: u64) {
        if let Some(grant) = self.grants.iter_mut().find(|grant| grant.id == id) {
            grant.revoked_at = Some(now);
        }
    }
    pub(super) fn revoke_device(&mut self, account: &str, id: &str, now: u64) -> bool {
        let Some(grant) = self
            .grants
            .iter_mut()
            .find(|grant| grant.id == id && grant.account_id == account)
        else {
            return false;
        };
        grant.revoked_at = Some(now);
        true
    }
    pub(super) fn revoke_token(&mut self, hash: &str, now: u64) -> bool {
        if let Some(grant) = self.grants.iter_mut().find(|grant| {
            constant_hash_eq(&grant.access_hash, hash)
                && grant.revoked_at.is_none()
                && grant.expires_at > now
                && grant.access_expires_at > now
        }) {
            grant.revoked_at = Some(now);
            true
        } else {
            false
        }
    }
    fn accepts(grant: &GrantRecord, token_hash: &str, resource: &str, now: u64) -> bool {
        constant_hash_eq(&grant.access_hash, token_hash)
            && grant.revoked_at.is_none()
            && grant.expires_at > now
            && grant.access_expires_at > now
            && grant.resource == resource
    }
    pub(super) fn authenticate(
        &self,
        token_hash: &str,
        resource: Option<&str>,
        now: u64,
    ) -> Option<IdentityCredential> {
        let resource = resource?;
        self.grants
            .iter()
            .find(|grant| Self::accepts(grant, token_hash, resource, now))
            .map(|grant| IdentityCredential {
                account_id: grant.account_id.clone(),
                scopes: grant.scopes.clone(),
                browser_session: false,
            })
    }
    /// True when this access token would authenticate and its grant's recorded use is missing or old.
    pub(super) fn last_seen_is_stale(
        &self,
        token_hash: &str,
        resource: Option<&str>,
        now: u64,
    ) -> bool {
        let Some(resource) = resource else {
            return false;
        };
        self.grants
            .iter()
            .find(|grant| Self::accepts(grant, token_hash, resource, now))
            .is_some_and(|grant| last_seen_is_stale(grant.last_seen_at, now))
    }
    /// Marks a still-valid access token's grant as used. Revoked, expired or wrong-audience tokens never count.
    pub(super) fn record_use(&mut self, token_hash: &str, resource: Option<&str>, now: u64) {
        let Some(resource) = resource else {
            return;
        };
        if let Some(grant) = self
            .grants
            .iter_mut()
            .find(|grant| Self::accepts(grant, token_hash, resource, now))
        {
            if last_seen_is_stale(grant.last_seen_at, now) {
                grant.last_seen_at = Some(now);
            }
        }
    }
    fn summary(grant: &GrantRecord, now: u64) -> DeviceSummary {
        DeviceSummary {
            id: grant.id.clone(),
            name: grant.name.clone(),
            scopes: grant.scopes.clone(),
            created_at: grant.created_at,
            expires_at: grant.expires_at,
            last_seen_at: grant.last_seen_at,
            revoked: grant.revoked_at.is_some() || grant.expires_at <= now,
            oauth_managed: true,
        }
    }
    pub(super) fn token_summary(
        &self,
        hash: &str,
        resource: Option<&str>,
        now: u64,
    ) -> Option<DeviceSummary> {
        self.authenticate(hash, resource, now)?;
        self.grants
            .iter()
            .find(|grant| constant_hash_eq(&grant.access_hash, hash))
            .map(|grant| Self::summary(grant, now))
    }
    pub(super) fn summaries(&self, account: &str, now: u64) -> Vec<DeviceSummary> {
        self.grants
            .iter()
            .filter(|grant| grant.account_id == account)
            .map(|grant| Self::summary(grant, now))
            .collect()
    }
}

impl FileIdentityStore {
    pub(in crate::identity) fn register_mcp_client(
        &self,
        input: RegistrationInput,
        now: u64,
    ) -> Result<RegisteredClient, OAuthError> {
        if input
            .token_endpoint_auth_method
            .as_deref()
            .unwrap_or("none")
            != "none"
            || input
                .response_types
                .as_ref()
                .is_some_and(|values| values.as_slice() != ["code"])
        {
            return Err(OAuthError::InvalidClientMetadata);
        }
        let grant_types = input
            .grant_types
            .unwrap_or_else(|| vec!["authorization_code".into()]);
        if !grant_types
            .iter()
            .any(|grant| grant == "authorization_code")
            || grant_types
                .iter()
                .any(|grant| !matches!(grant.as_str(), "authorization_code" | "refresh_token"))
        {
            return Err(OAuthError::UnsupportedGrantType);
        }
        if input.redirect_uris.is_empty() || input.redirect_uris.len() > 8 {
            return Err(OAuthError::InvalidRedirectUri);
        }
        let redirects = input
            .redirect_uris
            .iter()
            .map(|uri| oauth::redirect_uri(uri))
            .collect::<Result<Vec<_>, _>>()?;
        let requested_scopes = oauth::scopes(input.scope.as_deref().or(Some(
            "read:memories read:dashboard write:contributions report:outcome",
        )))?;
        let name = sanitize_device_name(input.client_name.as_deref().unwrap_or("MCP client"));
        let client_id = format!("wam_{}", random_secret()?);
        let record = ClientRecord {
            id: client_id.clone(),
            name: name.clone(),
            redirects: redirects.clone(),
            scopes: requested_scopes.clone(),
            allow_refresh: grant_types.iter().any(|grant| grant == "refresh_token"),
            expires_at: now.saturating_add(CLIENT_TTL),
        };
        self.update(|state| {
            state.mcp_oauth.purge(now);
            if state.mcp_oauth.clients.len() >= MAX_CLIENTS {
                return Ok(Err(OAuthError::RateLimited));
            }
            state.mcp_oauth.clients.push(record);
            Ok(Ok(()))
        })
        .map_err(OAuthError::from)??;
        Ok(RegisteredClient {
            client_id,
            client_id_issued_at: now,
            client_name: name,
            redirect_uris: redirects,
            token_endpoint_auth_method: "none",
            grant_types,
            response_types: vec!["code".into()],
            scope: requested_scopes.join(" "),
        })
    }

    pub(in crate::identity) fn start_mcp_authorization(
        &self,
        input: AuthorizationInput,
        config: &OAuthConfig,
        now: u64,
    ) -> Result<String, OAuthError> {
        if input.response_type != "code"
            || input.code_challenge_method != "S256"
            || !oauth::valid_challenge(&input.code_challenge)
            || !config.resource_matches(&input.resource)
            || input
                .state
                .as_ref()
                .is_some_and(|state| state.len() > 1024 || state.chars().any(char::is_control))
        {
            return Err(OAuthError::InvalidRequest);
        }
        let redirect = oauth::redirect_uri(&input.redirect_uri)?;
        let scopes = oauth::scopes(input.scope.as_deref())?;
        let request = random_secret()?;
        self.update(|state| {
            state.mcp_oauth.purge(now);
            let Some(client) = state
                .mcp_oauth
                .clients
                .iter()
                .find(|client| client.id == input.client_id && client.expires_at > now)
            else {
                return Ok(Err(OAuthError::InvalidClient));
            };
            if !client
                .redirects
                .iter()
                .any(|uri| oauth::redirect_matches(uri, &redirect))
            {
                return Ok(Err(OAuthError::InvalidRedirectUri));
            }
            if scopes.iter().any(|scope| !client.scopes.contains(scope)) {
                return Ok(Err(OAuthError::InvalidScope));
            }
            if state.mcp_oauth.requests.len() >= MAX_REQUESTS
                || state
                    .mcp_oauth
                    .requests
                    .iter()
                    .filter(|request| request.client_id == client.id)
                    .count()
                    >= 32
            {
                return Ok(Err(OAuthError::RateLimited));
            }
            state.mcp_oauth.requests.push(AuthorizationRecord {
                request_hash: digest(&request),
                client_id: input.client_id,
                redirect,
                scopes,
                challenge: input.code_challenge,
                resource: config.resource().into(),
                state: input.state,
                account_id: None,
                expires_at: now.saturating_add(REQUEST_TTL),
            });
            Ok(Ok(request))
        })
        .map_err(OAuthError::from)?
    }

    pub(in crate::identity) fn mcp_request_exists(&self, request: &str, now: u64) -> bool {
        let hash = digest(request);
        self.read(|state| {
            state.mcp_oauth.requests.iter().any(|request| {
                constant_hash_eq(&request.request_hash, &hash) && request.expires_at > now
            })
        })
        .unwrap_or(false)
    }

    pub(in crate::identity) fn preview_mcp_authorization(
        &self,
        request: &str,
        account: &str,
        now: u64,
    ) -> Result<AuthorizationPreview, OAuthError> {
        let hash = digest(request);
        self.update(|state| {
            let oauth = &mut state.mcp_oauth;
            let Some(request) = oauth.requests.iter_mut().find(|request| {
                constant_hash_eq(&request.request_hash, &hash) && request.expires_at > now
            }) else {
                return Ok(Err(OAuthError::InvalidGrant));
            };
            if request
                .account_id
                .as_deref()
                .is_some_and(|owner| owner != account)
            {
                return Ok(Err(OAuthError::InvalidGrant));
            }
            let Some(client) = oauth
                .clients
                .iter()
                .find(|client| client.id == request.client_id && client.expires_at > now)
            else {
                return Ok(Err(OAuthError::InvalidClient));
            };
            request.account_id = Some(account.to_owned());
            Ok(Ok(AuthorizationPreview {
                client_name: client.name.clone(),
                redirect_uri: request.redirect.clone(),
                resource: request.resource.clone(),
                scopes: request.scopes.clone(),
                expires_at: request.expires_at,
            }))
        })
        .map_err(OAuthError::from)?
    }

    pub(in crate::identity) fn decide_mcp_authorization(
        &self,
        request: &str,
        account: &str,
        approve: bool,
        config: &OAuthConfig,
        now: u64,
    ) -> Result<String, OAuthError> {
        let hash = digest(request);
        let code = if approve {
            random_secret()?
        } else {
            String::new()
        };
        self.update(|state| {
            let oauth = &mut state.mcp_oauth;
            let Some(index) = oauth.requests.iter().position(|request| {
                constant_hash_eq(&request.request_hash, &hash)
                    && request.expires_at > now
                    && request.account_id.as_deref() == Some(account)
            }) else {
                return Ok(Err(OAuthError::InvalidGrant));
            };
            if oauth.grants.len() >= MAX_GRANTS || oauth.codes.len() >= MAX_GRANTS * 2 {
                return Ok(Err(OAuthError::RateLimited));
            }
            let request = oauth.requests.remove(index);
            if approve {
                oauth.codes.push(CodeRecord {
                    code_hash: digest(&code),
                    client_id: request.client_id,
                    redirect: request.redirect.clone(),
                    scopes: request.scopes,
                    challenge: request.challenge,
                    resource: request.resource,
                    account_id: account.to_owned(),
                    expires_at: now.saturating_add(CODE_TTL),
                    used_at: None,
                    family_id: None,
                });
            }
            Ok(config.callback(
                &request.redirect,
                request.state.as_deref(),
                if approve { &code } else { "access_denied" },
                approve,
            ))
        })
        .map_err(OAuthError::from)?
    }

    pub(in crate::identity) fn exchange_mcp_token(
        &self,
        input: TokenInput,
        config: &OAuthConfig,
        now: u64,
    ) -> Result<TokenResponse, OAuthError> {
        if input.client_secret.is_some() {
            return Err(OAuthError::InvalidClient);
        }
        if !config.resource_matches(&input.resource) {
            return Err(OAuthError::InvalidGrant);
        }
        // Commit security state even for invalid_grant (e.g. a reused refresh token revokes its family).
        self.update(|state| Ok(state.mcp_oauth.exchange(input, config, now)))
            .map_err(OAuthError::from)?
    }

    pub(in crate::identity) fn revoke_mcp_token(
        &self,
        input: RevocationInput,
        now: u64,
    ) -> Result<(), OAuthError> {
        let hash = digest(&input.token);
        self.update(|state| {
            let oauth = &mut state.mcp_oauth;
            if !oauth
                .clients
                .iter()
                .any(|client| client.id == input.client_id && client.expires_at > now)
            {
                return Ok(Err(OAuthError::InvalidClient));
            }
            let family = oauth
                .refreshes
                .iter()
                .find(|token| constant_hash_eq(&token.token_hash, &hash))
                .map(|token| token.family_id.clone())
                .or_else(|| {
                    oauth
                        .grants
                        .iter()
                        .find(|grant| constant_hash_eq(&grant.access_hash, &hash))
                        .map(|grant| grant.id.clone())
                });
            if let Some(id) = family {
                if oauth
                    .grants
                    .iter()
                    .any(|grant| grant.id == id && grant.client_id == input.client_id)
                {
                    oauth.revoke_family(&id, now);
                }
            }
            // RFC 7009 does not disclose whether a token was known.
            Ok(Ok(()))
        })
        .map_err(OAuthError::from)?
    }
}

impl McpOAuthState {
    fn exchange(
        &mut self,
        input: TokenInput,
        config: &OAuthConfig,
        now: u64,
    ) -> Result<TokenResponse, OAuthError> {
        let client = self
            .clients
            .iter()
            .find(|client| client.id == input.client_id && client.expires_at > now)
            .cloned()
            .ok_or(OAuthError::InvalidClient)?;
        if self.refreshes.len() >= MAX_REFRESHES {
            return Err(OAuthError::RateLimited);
        }
        let access = random_secret()?;
        let refresh = client.allow_refresh.then(random_secret).transpose()?;
        let family_id = match input.grant_type.as_str() {
            "authorization_code" => {
                let code = input.code.as_deref().ok_or(OAuthError::InvalidRequest)?;
                let redirect = oauth::redirect_uri(
                    input
                        .redirect_uri
                        .as_deref()
                        .ok_or(OAuthError::InvalidRequest)?,
                )?;
                let verifier = input
                    .code_verifier
                    .as_deref()
                    .ok_or(OAuthError::InvalidRequest)?;
                let index = self
                    .codes
                    .iter()
                    .position(|record| constant_hash_eq(&record.code_hash, &digest(code)))
                    .ok_or(OAuthError::InvalidGrant)?;
                let record = self.codes[index].clone();
                if record.client_id != client.id
                    || record.redirect != redirect
                    || record.resource != config.resource()
                {
                    return Err(OAuthError::InvalidGrant);
                }
                if record.used_at.is_some() {
                    if let Some(family) = &record.family_id {
                        self.revoke_family(family, now);
                    }
                    return Err(OAuthError::InvalidGrant);
                }
                if record.expires_at <= now || !oauth::pkce_matches(verifier, &record.challenge) {
                    self.codes[index].used_at = Some(now);
                    return Err(OAuthError::InvalidGrant);
                }
                if self.grants.len() >= MAX_GRANTS {
                    return Err(OAuthError::RateLimited);
                }
                let family = format!("device_oauth_{}", random_secret()?);
                self.grants.push(GrantRecord {
                    id: family.clone(),
                    account_id: record.account_id,
                    client_id: client.id.clone(),
                    name: format!("Remote MCP: {}", client.name),
                    resource: record.resource,
                    scopes: record.scopes,
                    created_at: now,
                    expires_at: now.saturating_add(if client.allow_refresh {
                        REFRESH_TTL
                    } else {
                        ACCESS_TTL
                    }),
                    revoked_at: None,
                    access_hash: String::new(),
                    access_expires_at: 0,
                    last_seen_at: None,
                });
                self.codes[index].used_at = Some(now);
                self.codes[index].family_id = Some(family.clone());
                family
            }
            "refresh_token" => {
                if !client.allow_refresh {
                    return Err(OAuthError::UnsupportedGrantType);
                }
                let refresh = input
                    .refresh_token
                    .as_deref()
                    .ok_or(OAuthError::InvalidRequest)?;
                let index = self
                    .refreshes
                    .iter()
                    .position(|token| constant_hash_eq(&token.token_hash, &digest(refresh)))
                    .ok_or(OAuthError::InvalidGrant)?;
                let token = self.refreshes[index].clone();
                let grant = self
                    .grants
                    .iter_mut()
                    .find(|grant| grant.id == token.family_id)
                    .ok_or(OAuthError::InvalidGrant)?;
                if grant.client_id != client.id || grant.resource != config.resource() {
                    return Err(OAuthError::InvalidGrant);
                }
                if token.used_at.is_some() {
                    grant.revoked_at = Some(now);
                    return Err(OAuthError::InvalidGrant);
                }
                if token.expires_at <= now || grant.expires_at <= now || grant.revoked_at.is_some()
                {
                    return Err(OAuthError::InvalidGrant);
                }
                if let Some(scope) = input.scope.as_deref() {
                    let scopes = oauth::scopes(Some(scope))?;
                    if scopes.iter().any(|scope| !grant.scopes.contains(scope)) {
                        return Err(OAuthError::InvalidScope);
                    }
                    grant.scopes = scopes;
                }
                self.refreshes[index].used_at = Some(now);
                token.family_id
            }
            _ => return Err(OAuthError::UnsupportedGrantType),
        };
        let grant = self
            .grants
            .iter_mut()
            .find(|grant| grant.id == family_id)
            .ok_or(OAuthError::ServerError)?;
        grant.access_hash = digest(&access);
        grant.access_expires_at = now.saturating_add(ACCESS_TTL).min(grant.expires_at);
        grant.last_seen_at = Some(now);
        if let Some(refresh) = &refresh {
            self.refreshes.push(RefreshRecord {
                token_hash: digest(refresh),
                family_id,
                expires_at: grant.expires_at,
                used_at: None,
            });
        }
        if let Some(stored_client) = self
            .clients
            .iter_mut()
            .find(|stored| stored.id == client.id)
        {
            stored_client.expires_at = now.saturating_add(CLIENT_TTL);
        }
        Ok(TokenResponse {
            access_token: access,
            token_type: "Bearer",
            expires_in: grant.access_expires_at.saturating_sub(now),
            scope: grant.scopes.join(" "),
            refresh_token: refresh,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LAST_SEEN_REFRESH_SECS;

    const RESOURCE: &str = "https://wam.example/mcp";

    fn state_with_grant() -> (McpOAuthState, String) {
        let access = "access-token";
        let mut state = McpOAuthState::default();
        state.grants.push(GrantRecord {
            id: "device_oauth_1".into(),
            account_id: "account_a".into(),
            client_id: "client".into(),
            name: "Remote MCP: test".into(),
            resource: RESOURCE.into(),
            scopes: vec!["read:memories".into()],
            created_at: 100,
            expires_at: 100 + 10_000,
            revoked_at: None,
            access_hash: digest(access),
            access_expires_at: 100 + 1_000,
            last_seen_at: None,
        });
        (state, digest(access))
    }

    fn last_seen(state: &McpOAuthState) -> Option<u64> {
        state.summaries("account_a", 100)[0].last_seen_at
    }

    #[test]
    fn using_an_access_token_records_the_use_once_per_interval() {
        let (mut state, hash) = state_with_grant();
        assert_eq!(last_seen(&state), None);
        assert!(state.last_seen_is_stale(&hash, Some(RESOURCE), 150));
        state.record_use(&hash, Some(RESOURCE), 150);
        assert_eq!(last_seen(&state), Some(150));
        assert!(!state.last_seen_is_stale(&hash, Some(RESOURCE), 160));
        state.record_use(&hash, Some(RESOURCE), 160);
        assert_eq!(last_seen(&state), Some(150));
        let later = 150 + LAST_SEEN_REFRESH_SECS;
        assert!(state.last_seen_is_stale(&hash, Some(RESOURCE), later));
        state.record_use(&hash, Some(RESOURCE), later);
        assert_eq!(last_seen(&state), Some(later));
    }

    #[test]
    fn tokens_that_do_not_authenticate_never_count_as_use() {
        let (mut state, hash) = state_with_grant();
        // Wrong audience, no audience, an unknown token and an expired access token.
        state.record_use(&hash, Some("https://other.example/mcp"), 150);
        state.record_use(&hash, None, 150);
        state.record_use(&digest("unknown"), Some(RESOURCE), 150);
        state.record_use(&hash, Some(RESOURCE), 100 + 1_000);
        assert_eq!(last_seen(&state), None);
        assert!(!state.last_seen_is_stale(&hash, Some("https://other.example/mcp"), 150));
        // A revoked grant stops counting.
        assert!(state.revoke_device("account_a", "device_oauth_1", 120));
        state.record_use(&hash, Some(RESOURCE), 150);
        assert_eq!(last_seen(&state), None);
    }

    #[test]
    fn stored_grants_without_the_field_still_load() {
        let (state, _) = state_with_grant();
        let mut json = serde_json::to_value(&state).unwrap();
        json["grants"][0]
            .as_object_mut()
            .unwrap()
            .remove("last_seen_at");
        let loaded: McpOAuthState = serde_json::from_value(json).unwrap();
        assert_eq!(last_seen(&loaded), None);
    }
}
