//! Credentials are stored only in the OS keychain, partitioned by endpoint and config directory.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, path::PathBuf};

use crate::{ApiError, normalize_endpoint};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedCredential {
    pub device_id: String,
    pub access_token: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

impl fmt::Debug for LinkedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkedCredential")
            .field("device_id", &self.device_id)
            .field("access_token", &"[redacted]")
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialError {
    Unavailable,
    Invalid,
}
impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => {
                "OS secure credential storage is unavailable; unlock it or use anonymous reads"
            }
            Self::Invalid => "stored WAM credential is invalid; unlink it and link again",
        })
    }
}
impl std::error::Error for CredentialError {}

pub trait CredentialStore {
    fn load(&self) -> Result<Option<LinkedCredential>, CredentialError>;
    fn save(&self, value: &LinkedCredential) -> Result<(), CredentialError>;
    fn delete(&self) -> Result<(), CredentialError>;
}

pub struct OsCredentialStore {
    account: String,
}

impl OsCredentialStore {
    pub fn new(endpoint: &str, config_dir: &std::path::Path) -> Result<Self, ApiError> {
        let endpoint = normalize_endpoint(endpoint)?;
        let config_dir = std::path::absolute(config_dir).map_err(|_| ApiError::InvalidInput)?;
        let mut digest = Sha256::new();
        digest.update(endpoint.as_bytes());
        digest.update([0]);
        digest.update(config_dir.as_os_str().as_encoded_bytes());
        Ok(Self {
            account: format!("wam-{:x}", digest.finalize()),
        })
    }

    pub fn for_endpoint(endpoint: &str) -> Result<Self, ApiError> {
        Self::new(endpoint, &config_dir())
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn entry(&self) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new("web-agentic-memory", &self.account)
            .map_err(|_| CredentialError::Unavailable)
    }
}

impl CredentialStore for OsCredentialStore {
    fn load(&self) -> Result<Option<LinkedCredential>, CredentialError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let secret = match self.entry()?.get_password() {
                Ok(secret) => secret,
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(_) => return Err(CredentialError::Unavailable),
            };
            let credential: LinkedCredential =
                serde_json::from_str(&secret).map_err(|_| CredentialError::Invalid)?;
            if credential.device_id.is_empty()
                || credential.access_token.is_empty()
                || credential.access_token.len() > 4096
                || !credential
                    .access_token
                    .bytes()
                    .all(|b| b.is_ascii_graphic())
            {
                return Err(CredentialError::Invalid);
            }
            Ok(Some(credential))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        Err(CredentialError::Unavailable)
    }

    fn save(&self, value: &LinkedCredential) -> Result<(), CredentialError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let secret = serde_json::to_string(value).map_err(|_| CredentialError::Invalid)?;
            self.entry()?
                .set_password(&secret)
                .map_err(|_| CredentialError::Unavailable)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = value;
            Err(CredentialError::Unavailable)
        }
    }

    fn delete(&self) -> Result<(), CredentialError> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialError::Unavailable),
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        Err(CredentialError::Unavailable)
    }
}

/// WAM_TOKEN remains an explicit development override; device linking never writes it.
pub fn token_for_endpoint(endpoint: &str) -> Result<Option<String>, CredentialError> {
    if std::env::var("WAM_ANONYMOUS").is_ok_and(|value| value == "1" || value == "true") {
        return Ok(None);
    }
    if let Ok(token) = std::env::var("WAM_TOKEN") {
        if !token.is_empty() {
            return Ok(Some(token));
        }
    }
    let store = OsCredentialStore::for_endpoint(endpoint).map_err(|_| CredentialError::Invalid)?;
    Ok(store.load()?.map(|credential| credential.access_token))
}

/// This is a namespace only; account linking never writes plaintext credential files.
pub fn config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("WAM_CONFIG_DIR").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/wam")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("wam")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("wam")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_separates_endpoint_and_config_without_containing_secrets() {
        let a = OsCredentialStore::new("https://wam.example/", "/tmp/a".as_ref()).unwrap();
        assert_eq!(
            a.account,
            OsCredentialStore::new("https://wam.example", "/tmp/a".as_ref())
                .unwrap()
                .account
        );
        assert_ne!(
            a.account,
            OsCredentialStore::new("https://wam.example", "/tmp/b".as_ref())
                .unwrap()
                .account
        );
        assert_ne!(
            a.account,
            OsCredentialStore::new("https://other.example", "/tmp/a".as_ref())
                .unwrap()
                .account
        );
    }

    #[test]
    fn relative_and_absolute_config_namespaces_match() {
        let relative = std::path::Path::new("wam-test-config");
        let absolute = std::env::current_dir().unwrap().join(relative);
        assert_eq!(
            OsCredentialStore::new("https://wam.example", relative)
                .unwrap()
                .account,
            OsCredentialStore::new("https://wam.example", &absolute)
                .unwrap()
                .account
        );
    }

    #[test]
    fn credential_debug_redacts_the_token() {
        let credential = LinkedCredential {
            device_id: "device".into(),
            access_token: "sentinel-secret".into(),
            scopes: vec![],
            expires_at: 1,
        };
        assert!(!format!("{credential:?}").contains("sentinel-secret"));
    }
}
