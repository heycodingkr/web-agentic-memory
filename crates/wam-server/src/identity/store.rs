pub(super) mod admin;
mod device;
mod email;
mod mcp_oauth;

use super::{
    IdentityCredential, IdentityError, OAUTH_TTL_SECS, SCOPE_MANAGE_ACCOUNT, SCOPE_MANAGE_DEVICES,
    SCOPE_READ_DASHBOARD, SCOPE_READ_MEMORIES, SCOPE_REPORT_OUTCOME, SCOPE_WRITE_CONTRIBUTIONS,
    SESSION_TTL_SECS, constant_hash_eq, derive_csrf, digest, ensure_private_data_directory,
    random_secret, set_file_permissions,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use device::{ApprovalAttempt, DeviceRecord, DeviceRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::Mutex,
};
use subtle::ConstantTimeEq;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OAuthTransaction {
    state_hash: String,
    code_verifier: String,
    expires_at: u64,
    #[serde(default)]
    return_request_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AccountBinding {
    subject_hash: String,
    account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionRecord {
    token_hash: String,
    csrf_hash: String,
    account_id: String,
    expires_at: u64,
    revoked_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct IdentityState {
    schema_version: u32,
    oauth_transactions: Vec<OAuthTransaction>,
    accounts: Vec<AccountBinding>,
    sessions: Vec<SessionRecord>,
    device_requests: Vec<DeviceRequest>,
    devices: Vec<DeviceRecord>,
    approval_attempts: Vec<ApprovalAttempt>,
    #[serde(default)]
    mcp_oauth: mcp_oauth::McpOAuthState,
    #[serde(default)]
    email_challenges: Vec<email::EmailChallenge>,
    #[serde(default)]
    email_sends: Vec<email::EmailSend>,
    #[serde(default)]
    admin: admin::AdminState,
}

impl Default for IdentityState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            oauth_transactions: Vec::new(),
            accounts: Vec::new(),
            sessions: Vec::new(),
            device_requests: Vec::new(),
            devices: Vec::new(),
            approval_attempts: Vec::new(),
            mcp_oauth: mcp_oauth::McpOAuthState::default(),
            email_challenges: Vec::new(),
            email_sends: Vec::new(),
            admin: admin::AdminState::default(),
        }
    }
}

impl IdentityState {
    pub(crate) fn erase_account(&mut self, account_id: &str, now: u64) {
        self.erase_email(account_id);
        self.accounts
            .retain(|binding| binding.account_id != account_id);
        self.sessions
            .retain(|session| session.account_id != account_id);
        self.erase_devices(account_id, now);
        self.mcp_oauth.erase_account(account_id);
    }

    pub(crate) fn purge(&mut self, now: u64) {
        self.oauth_transactions
            .retain(|transaction| transaction.expires_at > now);
        self.sessions
            .retain(|session| session.expires_at > now && session.revoked_at.is_none());
        self.purge_device_history(now);
        self.mcp_oauth.purge(now);
        self.purge_email(now);
        self.admin.purge(now);
    }
}

pub struct FileIdentityStore {
    path: PathBuf,
    state: Mutex<IdentityState>,
    repository: Option<crate::durable::ApplicationRepository>,
}

impl FileIdentityStore {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            ensure_private_data_directory(parent)?;
        }
        let state = if path.exists() {
            let bytes = fs::read(&path)?;
            serde_json::from_slice(&bytes).map_err(io::Error::other)?
        } else {
            IdentityState::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
            repository: None,
        })
    }

    pub fn from_repository(repository: crate::durable::ApplicationRepository) -> Self {
        Self {
            path: PathBuf::new(),
            state: Mutex::new(IdentityState::default()),
            repository: Some(repository),
        }
    }

    fn read<T>(&self, f: impl FnOnce(&IdentityState) -> T) -> Result<T, IdentityError> {
        if let Some(repository) = &self.repository {
            return repository
                .identity_read(f)
                .map_err(|_| IdentityError::Storage);
        }
        let state = self.state.lock().map_err(|_| IdentityError::Storage)?;
        Ok(f(&state))
    }

    fn update<T>(
        &self,
        f: impl FnOnce(&mut IdentityState) -> Result<T, IdentityError>,
    ) -> Result<T, IdentityError> {
        if let Some(repository) = &self.repository {
            return repository.identity_update(f);
        }
        let mut state = self.state.lock().map_err(|_| IdentityError::Storage)?;
        let mut next = state.clone();
        // Persist the candidate before committing it in memory. A failed disk
        // write must not leave this process ahead of the durable state.
        let result = f(&mut next);
        self.persist(&next)?;
        *state = next;
        result
    }

    fn persist(&self, state: &IdentityState) -> Result<(), IdentityError> {
        let bytes = serde_json::to_vec(state).map_err(|_| IdentityError::Storage)?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| IdentityError::Storage)?;
        file.write_all(&bytes).map_err(|_| IdentityError::Storage)?;
        file.sync_all().map_err(|_| IdentityError::Storage)?;
        fs::rename(&temporary, &self.path).map_err(|_| IdentityError::Storage)?;
        set_file_permissions(&self.path).map_err(|_| IdentityError::Storage)?;
        Ok(())
    }

    pub(super) fn begin_oauth(&self, now: u64) -> Result<(String, String), IdentityError> {
        self.begin_oauth_with_return(now, None)
    }

    pub(super) fn begin_oauth_with_return(
        &self,
        now: u64,
        return_request_hash: Option<String>,
    ) -> Result<(String, String), IdentityError> {
        let state = random_secret()?;
        let verifier = random_secret()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let entry = OAuthTransaction {
            state_hash: digest(&state),
            code_verifier: verifier,
            expires_at: now.saturating_add(OAUTH_TTL_SECS),
            return_request_hash,
        };
        self.update(|stored| {
            stored
                .oauth_transactions
                .retain(|transaction| transaction.expires_at > now);
            stored.oauth_transactions.push(entry);
            Ok(())
        })?;
        Ok((state, challenge))
    }

    pub(super) fn oauth_return_matches(
        &self,
        query_state: &str,
        cookie_state: &str,
        request: &str,
        now: u64,
    ) -> bool {
        if query_state.is_empty()
            || query_state
                .as_bytes()
                .ct_eq(cookie_state.as_bytes())
                .unwrap_u8()
                != 1
        {
            return false;
        }
        self.read(|state| {
            state.oauth_transactions.iter().any(|entry| {
                entry.expires_at > now
                    && constant_hash_eq(&entry.state_hash, &digest(query_state))
                    && entry
                        .return_request_hash
                        .as_ref()
                        .is_some_and(|hash| constant_hash_eq(hash, &digest(request)))
            })
        })
        .unwrap_or(false)
    }

    pub(super) fn take_oauth_verifier(
        &self,
        query_state: &str,
        cookie_state: &str,
        now: u64,
    ) -> Result<String, IdentityError> {
        if query_state.is_empty()
            || cookie_state.is_empty()
            || query_state
                .as_bytes()
                .ct_eq(cookie_state.as_bytes())
                .unwrap_u8()
                != 1
        {
            return Err(IdentityError::Invalid);
        }
        let state_hash = digest(query_state);
        self.update(|stored| {
            let Some(index) = stored
                .oauth_transactions
                .iter()
                .position(|entry| constant_hash_eq(&entry.state_hash, &state_hash))
            else {
                return Err(IdentityError::Invalid);
            };
            let transaction = stored.oauth_transactions.remove(index);
            if transaction.expires_at <= now {
                return Err(IdentityError::Expired);
            }
            Ok(transaction.code_verifier)
        })
    }

    pub(super) fn account_for_github_subject(
        &self,
        subject: &str,
    ) -> Result<String, IdentityError> {
        let subject_hash = digest(subject);
        self.update(|state| {
            if let Some(binding) = state
                .accounts
                .iter()
                .find(|binding| constant_hash_eq(&binding.subject_hash, &subject_hash))
            {
                return Ok(binding.account_id.clone());
            }
            let account_id = format!("github_{}", &digest(&random_secret()?)[..24]);
            state.accounts.push(AccountBinding {
                subject_hash,
                account_id: account_id.clone(),
            });
            Ok(account_id)
        })
    }

    pub(super) fn create_session(
        &self,
        account_id: String,
        now: u64,
    ) -> Result<(String, String, u64), IdentityError> {
        let token = random_secret()?;
        let csrf = derive_csrf(&token);
        let expires_at = now.saturating_add(SESSION_TTL_SECS);
        self.update(|state| {
            state
                .sessions
                .retain(|session| session.expires_at > now && session.revoked_at.is_none());
            state.sessions.push(SessionRecord {
                token_hash: digest(&token),
                csrf_hash: digest(&csrf),
                account_id,
                expires_at,
                revoked_at: None,
            });
            Ok(())
        })?;
        Ok((token, csrf, expires_at))
    }

    pub(super) fn session_info(
        &self,
        token: &str,
        now: u64,
    ) -> Result<(String, String, u64), IdentityError> {
        let csrf = derive_csrf(token);
        let token_hash = digest(token);
        self.read(|state| {
            state
                .sessions
                .iter()
                .find(|session| constant_hash_eq(&session.token_hash, &token_hash))
                .filter(|session| {
                    session.revoked_at.is_none()
                        && session.expires_at > now
                        && constant_hash_eq(&session.csrf_hash, &digest(&csrf))
                })
                .map(|session| (session.account_id.clone(), csrf, session.expires_at))
                .ok_or(IdentityError::Expired)
        })?
    }

    pub(super) fn session_credential(&self, token: &str, now: u64) -> Option<IdentityCredential> {
        let token_hash = digest(token);
        self.read(|state| {
            state
                .sessions
                .iter()
                .find(|session| {
                    constant_hash_eq(&session.token_hash, &token_hash)
                        && session.revoked_at.is_none()
                        && session.expires_at > now
                })
                .map(|session| IdentityCredential {
                    account_id: session.account_id.clone(),
                    scopes: vec![
                        SCOPE_READ_MEMORIES.to_owned(),
                        SCOPE_WRITE_CONTRIBUTIONS.to_owned(),
                        SCOPE_REPORT_OUTCOME.to_owned(),
                        SCOPE_READ_DASHBOARD.to_owned(),
                        SCOPE_MANAGE_DEVICES.to_owned(),
                        SCOPE_MANAGE_ACCOUNT.to_owned(),
                    ],
                    browser_session: true,
                })
        })
        .ok()
        .flatten()
    }

    pub(super) fn csrf_valid(&self, token: &str, csrf: &str, now: u64) -> bool {
        if csrf.is_empty() {
            return false;
        }
        let token_hash = digest(token);
        let csrf_hash = digest(csrf);
        self.read(|state| {
            state.sessions.iter().any(|session| {
                constant_hash_eq(&session.token_hash, &token_hash)
                    && constant_hash_eq(&session.csrf_hash, &csrf_hash)
                    && session.revoked_at.is_none()
                    && session.expires_at > now
            })
        })
        .unwrap_or(false)
    }

    pub(super) fn revoke_session(&self, token: &str, now: u64) -> Result<(), IdentityError> {
        let token_hash = digest(token);
        self.update(|state| {
            if let Some(session) = state
                .sessions
                .iter_mut()
                .find(|session| constant_hash_eq(&session.token_hash, &token_hash))
            {
                session.revoked_at = Some(now);
            }
            Ok(())
        })
    }

    #[cfg(test)]
    pub(super) fn device_request_count(&self) -> Result<usize, IdentityError> {
        self.read(|state| state.device_requests.len())
    }
}
