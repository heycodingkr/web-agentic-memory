use super::{FileIdentityStore, IdentityState};
use crate::identity::{
    DEVICE_CODE_TTL_SECS, DEVICE_POLL_INTERVAL_SECS, DEVICE_TTL_SECS, DeviceApprovalPreview,
    DevicePoll, DeviceStart, DeviceSummary, IdentityCredential, IdentityError, constant_hash_eq,
    digest, normalize_user_code, random_secret, random_user_code, sanitize_device_name,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DeviceRequest {
    device_code_hash: String,
    user_code_hash: String,
    requested_scopes: Vec<String>,
    device_name: String,
    status: String,
    account_id: Option<String>,
    expires_at: u64,
    interval_secs: u64,
    last_polled_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DeviceRecord {
    id: String,
    account_id: String,
    name: String,
    token_hash: String,
    scopes: Vec<String>,
    created_at: u64,
    expires_at: u64,
    revoked_at: Option<u64>,
    last_seen_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ApprovalAttempt {
    account_id: String,
    window_started_at: u64,
    attempts: u8,
}

impl IdentityState {
    pub(crate) fn erase_devices(&mut self, account_id: &str, now: u64) {
        self.devices.retain(|device| device.account_id != account_id);
        self.device_requests.retain(|request| request.account_id.as_deref() != Some(account_id));
        self.approval_attempts.retain(|attempt| attempt.account_id != account_id);
        self.device_requests.retain(|request| request.expires_at > now);
    }

    pub(crate) fn purge_device_history(&mut self, now: u64) {
        self.devices.retain(|device| device.expires_at > now && device.revoked_at.is_none());
        self.device_requests.retain(|request| request.expires_at > now && !matches!(request.status.as_str(), "consumed" | "denied" | "expired"));
        self.approval_attempts.retain(|attempt| now.saturating_sub(attempt.window_started_at) < 3600);
    }
}

impl FileIdentityStore {
    pub(in crate::identity) fn start_device(
        &self,
        name: &str,
        requested_scopes: Vec<String>,
        now: u64,
        verification_uri: String,
    ) -> Result<DeviceStart, IdentityError> {
        let device_code = random_secret()?;
        let user_code = random_user_code()?;
        let device_name = sanitize_device_name(name);
        let request = DeviceRequest {
            device_code_hash: digest(&device_code),
            user_code_hash: digest(&normalize_user_code(&user_code)),
            requested_scopes,
            device_name,
            status: "pending".to_owned(),
            account_id: None,
            expires_at: now.saturating_add(DEVICE_CODE_TTL_SECS),
            interval_secs: DEVICE_POLL_INTERVAL_SECS,
            last_polled_at: None,
        };
        self.update(|state| {
            state
                .device_requests
                .retain(|request| request.expires_at > now && request.status != "consumed");
            state.device_requests.push(request);
            Ok(())
        })?;
        Ok(DeviceStart {
            device_code,
            user_code,
            verification_uri,
            expires_in: DEVICE_CODE_TTL_SECS,
            interval: DEVICE_POLL_INTERVAL_SECS,
        })
    }

    pub(in crate::identity) fn approve_device(
        &self,
        user_code: &str,
        account_id: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        let normalized = normalize_user_code(user_code);
        let user_hash = digest(&normalized);
        self.update(|state| {
            let index = device_request_index(state, &user_hash, account_id, now)?;
            let request = &mut state.device_requests[index];
            if request.expires_at <= now {
                request.status = "expired".to_owned();
                return Err(IdentityError::Expired);
            }
            if request.status != "pending" {
                return Err(IdentityError::Conflict);
            }
            request.status = "approved".to_owned();
            request.account_id = Some(account_id.to_owned());
            state
                .approval_attempts
                .retain(|attempt| attempt.account_id != account_id);
            Ok(())
        })
    }

    pub(in crate::identity) fn preview_device(
        &self,
        user_code: &str,
        account_id: &str,
        now: u64,
    ) -> Result<DeviceApprovalPreview, IdentityError> {
        let normalized = normalize_user_code(user_code);
        let user_hash = digest(&normalized);
        self.update(|state| {
            let index = device_request_index(state, &user_hash, account_id, now)?;
            let request = &state.device_requests[index];
            if request.expires_at <= now {
                return Err(IdentityError::Expired);
            }
            if request.status != "pending" {
                return Err(IdentityError::Conflict);
            }
            Ok(DeviceApprovalPreview {
                name: request.device_name.clone(),
                scopes: request.requested_scopes.clone(),
                expires_at: request.expires_at,
            })
        })
    }

    pub(in crate::identity) fn deny_device(
        &self,
        user_code: &str,
        account_id: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        let normalized = normalize_user_code(user_code);
        let user_hash = digest(&normalized);
        self.update(|state| {
            let index = device_request_index(state, &user_hash, account_id, now)?;
            let request = &mut state.device_requests[index];
            if request.expires_at <= now {
                request.status = "expired".to_owned();
                return Err(IdentityError::Expired);
            }
            if request.status != "pending" {
                return Err(IdentityError::Conflict);
            }
            request.status = "denied".to_owned();
            request.account_id = Some(account_id.to_owned());
            state
                .approval_attempts
                .retain(|attempt| attempt.account_id != account_id);
            Ok(())
        })
    }

    pub(in crate::identity) fn poll_device(
        &self,
        device_code: &str,
        now: u64,
    ) -> Result<DevicePoll, IdentityError> {
        let token = random_secret()?;
        let device_id = format!("device_{}", random_secret()?);
        let device_code_hash = digest(device_code);
        self.update(|state| {
            let request = state
                .device_requests
                .iter_mut()
                .find(|request| constant_hash_eq(&request.device_code_hash, &device_code_hash))
                .ok_or(IdentityError::Invalid)?;
            if request.expires_at <= now {
                request.status = "expired".to_owned();
                return Ok(DevicePoll::Expired);
            }
            if request
                .last_polled_at
                .is_some_and(|last| now.saturating_sub(last) < request.interval_secs)
            {
                return Err(IdentityError::TooFast);
            }
            request.last_polled_at = Some(now);
            match request.status.as_str() {
                "pending" => Ok(DevicePoll::Pending {
                    interval: request.interval_secs,
                }),
                "denied" => Ok(DevicePoll::Denied),
                "expired" => Ok(DevicePoll::Expired),
                "consumed" => Ok(DevicePoll::AlreadyUsed),
                "approved" => {
                    let account_id = request.account_id.clone().ok_or(IdentityError::Storage)?;
                    let scopes = request.requested_scopes.clone();
                    let expires_at = now.saturating_add(DEVICE_TTL_SECS);
                    let name = request.device_name.clone();
                    request.status = "consumed".to_owned();
                    state.devices.push(DeviceRecord {
                        id: device_id.clone(),
                        account_id,
                        name,
                        token_hash: digest(&token),
                        scopes: scopes.clone(),
                        created_at: now,
                        expires_at,
                        revoked_at: None,
                        last_seen_at: Some(now),
                    });
                    Ok(DevicePoll::Authorized {
                        device_id,
                        access_token: token,
                        scopes,
                        expires_at,
                    })
                }
                _ => Err(IdentityError::Storage),
            }
        })
    }

    pub(in crate::identity) fn device_credential(
        &self,
        token: &str,
        now: u64,
    ) -> Option<IdentityCredential> {
        let token_hash = digest(token);
        self.read(|state| {
            state
                .devices
                .iter()
                .find(|device| {
                    constant_hash_eq(&device.token_hash, &token_hash)
                        && device.revoked_at.is_none()
                        && device.expires_at > now
                })
                .map(|device| IdentityCredential {
                    account_id: device.account_id.clone(),
                    scopes: device.scopes.clone(),
                    browser_session: false,
                })
        })
        .ok()
        .flatten()
    }

    pub(in crate::identity) fn list_devices(
        &self,
        account_id: &str,
        now: u64,
    ) -> Result<Vec<DeviceSummary>, IdentityError> {
        self.read(|state| {
            state
                .devices
                .iter()
                .filter(|device| device.account_id == account_id)
                .map(|device| DeviceSummary {
                    id: device.id.clone(),
                    name: device.name.clone(),
                    scopes: device.scopes.clone(),
                    created_at: device.created_at,
                    expires_at: device.expires_at,
                    last_seen_at: device.last_seen_at,
                    revoked: device.revoked_at.is_some() || device.expires_at <= now,
                })
                .collect()
        })
    }

    pub(in crate::identity) fn revoke_device(
        &self,
        account_id: &str,
        id: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        self.update(|state| {
            let device = state
                .devices
                .iter_mut()
                .find(|device| device.id == id && device.account_id == account_id)
                .ok_or(IdentityError::Invalid)?;
            if device.revoked_at.is_some() {
                return Err(IdentityError::Conflict);
            }
            device.revoked_at = Some(now);
            Ok(())
        })
    }

    pub(in crate::identity) fn rotate_device(
        &self,
        account_id: &str,
        id: &str,
        now: u64,
    ) -> Result<(String, DeviceSummary), IdentityError> {
        let token = random_secret()?;
        self.update(|state| {
            let device = state
                .devices
                .iter_mut()
                .find(|device| device.id == id && device.account_id == account_id)
                .ok_or(IdentityError::Invalid)?;
            if device.revoked_at.is_some() || device.expires_at <= now {
                return Err(IdentityError::Expired);
            }
            device.token_hash = digest(&token);
            device.created_at = now;
            device.expires_at = now.saturating_add(DEVICE_TTL_SECS);
            let summary = DeviceSummary {
                id: device.id.clone(),
                name: device.name.clone(),
                scopes: device.scopes.clone(),
                created_at: device.created_at,
                expires_at: device.expires_at,
                last_seen_at: device.last_seen_at,
                revoked: false,
            };
            Ok((token, summary))
        })
    }
}

fn device_request_index(
    state: &mut IdentityState,
    user_hash: &str,
    account_id: &str,
    now: u64,
) -> Result<usize, IdentityError> {
    match state
        .device_requests
        .iter()
        .position(|request| constant_hash_eq(&request.user_code_hash, user_hash))
    {
        Some(index) => Ok(index),
        None => {
            increment_approval_attempt(state, account_id, now)?;
            Err(IdentityError::Invalid)
        }
    }
}

fn increment_approval_attempt(
    state: &mut IdentityState,
    account_id: &str,
    now: u64,
) -> Result<(), IdentityError> {
    let attempt = state
        .approval_attempts
        .iter_mut()
        .find(|attempt| attempt.account_id == account_id);
    match attempt {
        Some(attempt) if now.saturating_sub(attempt.window_started_at) < 60 => {
            attempt.attempts = attempt.attempts.saturating_add(1);
            if attempt.attempts > 10 {
                return Err(IdentityError::RateLimited);
            }
        }
        Some(attempt) => {
            attempt.window_started_at = now;
            attempt.attempts = 1;
        }
        None => state.approval_attempts.push(ApprovalAttempt {
            account_id: account_id.to_owned(),
            window_started_at: now,
            attempts: 1,
        }),
    }
    Ok(())
}
