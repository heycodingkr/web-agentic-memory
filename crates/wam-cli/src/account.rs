//! Local account lifecycle. The store is injectable so tests never touch real credentials.
use std::time::Duration;

use serde::Serialize;
use wam_client::{
    ApiError,
    credentials::{CredentialError, CredentialStore, LinkedCredential},
    device::{DeviceApi, LinkPoll},
};

#[derive(Debug)]
pub enum AccountError {
    Api(ApiError),
    Store(CredentialError),
    AlreadyLinked,
    Denied,
    Expired,
    StorageRollbackFailed,
}
impl From<ApiError> for AccountError {
    fn from(value: ApiError) -> Self {
        Self::Api(value)
    }
}
impl From<CredentialError> for AccountError {
    fn from(value: CredentialError) -> Self {
        Self::Store(value)
    }
}
impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => write!(f, "{error}"),
            Self::Store(error) => write!(f, "{error}"),
            Self::AlreadyLinked => f.write_str("a credential is already stored for this endpoint; unlink it before linking again"),
            Self::Denied => f.write_str("device authorization was denied"),
            Self::Expired => f.write_str("device authorization expired; run wam link again"),
            Self::StorageRollbackFailed => f.write_str("credential storage failed and the new device could not be revoked; revoke it in the web device settings"),
        }
    }
}
impl std::error::Error for AccountError {}

#[derive(Serialize)]
pub struct LinkedReport {
    pub schema_version: u8,
    pub status: &'static str,
    pub device_id: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

#[derive(Serialize)]
pub struct UnlinkedReport {
    pub schema_version: u8,
    pub local_removed: bool,
    pub server_revoked: bool,
}

/// Public instructions contain the user code, never the device secret or access token.
pub async fn link(
    api: &dyn DeviceApi,
    store: &dyn CredentialStore,
    instructions: impl FnOnce(&str, &str),
) -> Result<LinkedReport, AccountError> {
    if store.load()?.is_some() {
        return Err(AccountError::AlreadyLinked);
    }
    let start = api.start_link().await?;
    let uri = wam_client::normalize_endpoint(&start.verification_uri)?;
    if start.user_code.is_empty()
        || start.user_code.len() > 32
        || !start
            .user_code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || start.device_code.is_empty()
        || start.device_code.len() > 128
        || !(1..=600).contains(&start.expires_in)
        || !(1..=60).contains(&start.interval)
    {
        return Err(ApiError::InvalidResponse.into());
    }
    instructions(&uri, &start.user_code);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(start.expires_in);
    let mut interval = start.interval;
    loop {
        let wake = tokio::time::Instant::now() + Duration::from_secs(interval);
        if wake >= deadline {
            return Err(AccountError::Expired);
        }
        tokio::time::sleep_until(wake).await;
        let reply = tokio::time::timeout_at(deadline, api.poll_link(&start.device_code))
            .await
            .map_err(|_| AccountError::Expired)??;
        match reply {
            LinkPoll::Pending { interval: next } => {
                if !(1..=60).contains(&next) {
                    return Err(ApiError::InvalidResponse.into());
                }
                // A server may slow polling, but must never cause us to speed up.
                interval = interval.max(next);
            }
            LinkPoll::Denied => return Err(AccountError::Denied),
            LinkPoll::Expired | LinkPoll::AlreadyUsed => return Err(AccountError::Expired),
            LinkPoll::Authorized {
                device_id,
                access_token,
                scopes,
                expires_at,
            } => {
                let credential = LinkedCredential {
                    device_id,
                    access_token,
                    scopes,
                    expires_at,
                };
                let valid = !credential.device_id.is_empty()
                    && credential.device_id.len() <= 128
                    && !credential.access_token.is_empty()
                    && credential.access_token.len() <= 4096
                    && credential
                        .access_token
                        .bytes()
                        .all(|b| b.is_ascii_graphic())
                    && credential.scopes.iter().any(|s| s == "read:memories")
                    && credential.scopes.iter().all(|s| {
                        matches!(
                            s.as_str(),
                            "read:memories"
                                | "read:dashboard"
                                | "write:contributions"
                                | "report:outcome"
                        )
                    })
                    && credential.expires_at > unix_seconds();
                let failure = if !valid {
                    Some(AccountError::Api(ApiError::InvalidResponse))
                } else {
                    store.save(&credential).err().map(AccountError::Store)
                };
                if let Some(error) = failure {
                    if !matches!(
                        api.revoke_link(&credential.access_token).await,
                        Ok(()) | Err(ApiError::Unauthorized)
                    ) {
                        return Err(AccountError::StorageRollbackFailed);
                    }
                    return Err(error);
                }
                return Ok(LinkedReport {
                    schema_version: 1,
                    status: "linked",
                    device_id: credential.device_id,
                    scopes: credential.scopes,
                    expires_at: credential.expires_at,
                });
            }
        }
    }
}

pub async fn unlink(
    api: &dyn DeviceApi,
    store: &dyn CredentialStore,
    local_only: bool,
) -> Result<UnlinkedReport, AccountError> {
    let credential = match store.load() {
        Ok(credential) => credential,
        Err(CredentialError::Invalid) if local_only => {
            store.delete()?;
            return Ok(UnlinkedReport {
                schema_version: 1,
                local_removed: true,
                server_revoked: false,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let Some(credential) = credential else {
        return Ok(UnlinkedReport {
            schema_version: 1,
            local_removed: false,
            server_revoked: false,
        });
    };
    if !local_only {
        match api.revoke_link(&credential.access_token).await {
            Ok(()) | Err(ApiError::Unauthorized) => {}
            Err(error) => return Err(error.into()), // Keep it so an offline unlink can be retried.
        }
    }
    store.delete()?;
    Ok(UnlinkedReport {
        schema_version: 1,
        local_removed: true,
        server_revoked: !local_only,
    })
}

pub fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
