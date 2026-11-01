//! Passwordless email verification. Delivery credentials never enter persistent state.
use super::{IdentityError, IdentityService, digest, random_secret};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;

pub const EMAIL_CODE_TTL: u64 = 600;
pub const EMAIL_RESEND_DELAY: u64 = 60;

#[async_trait]
pub trait EmailSender: Send + Sync + 'static {
    async fn send_code(
        &self,
        email: &str,
        code: &str,
        request_id: &str,
    ) -> Result<(), IdentityError>;

    async fn send_admin_link(
        &self,
        _email: &str,
        _link: &str,
        _request_id: &str,
    ) -> Result<(), IdentityError> {
        Err(IdentityError::Disabled)
    }
}

pub struct ResendEmailSender {
    client: reqwest::Client,
    api_key: String,
    from: String,
    endpoint: String,
}

impl ResendEmailSender {
    pub fn new(api_key: String, from: String) -> Result<Self, IdentityError> {
        if api_key.trim().is_empty() || api_key.contains(['\r', '\n']) {
            return Err(IdentityError::Invalid);
        }
        if from.contains(['\r', '\n']) {
            return Err(IdentityError::Invalid);
        }
        let address = if let Some((name, rest)) = from.split_once('<') {
            if name.contains(['\r', '\n', '<', '>']) {
                return Err(IdentityError::Invalid);
            }
            rest.strip_suffix('>').ok_or(IdentityError::Invalid)?
        } else {
            &from
        };
        normalize_email(address)?;
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .map_err(|_| IdentityError::Provider)?,
            api_key,
            from,
            endpoint: "https://api.resend.com/emails".into(),
        })
    }
}

#[async_trait]
impl EmailSender for ResendEmailSender {
    async fn send_admin_link(
        &self,
        email: &str,
        link: &str,
        request_id: &str,
    ) -> Result<(), IdentityError> {
        let response = self.client.post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("Idempotency-Key", format!("wam-admin/{request_id}"))
            .json(&serde_json::json!({
                "from": self.from, "to": [email],
                "subject": "Web Agentic Memory administrator sign-in",
                "text": format!("Administrator sign-in: {link}\n\nOpen this link in the browser where you requested sign-in within 10 minutes. If you did not request it, you can ignore this email."),
                "html": format!("<div lang=\"en\" style=\"font-family:Arial,sans-serif;padding:32px;color:#203b2d\"><h1>Administrator sign-in</h1><p>Sign in to the Web Agentic Memory contribution review console.</p><p><a href=\"{link}\" style=\"display:inline-block;padding:14px 24px;background:#294b39;color:#ffffff;text-decoration:none;border-radius:8px\">Sign in to the administrator console</a></p><p>Open this link in the browser where you requested sign-in within 10 minutes.<br>If you did not request it, you can ignore this email.</p></div>")
            })).send().await.map_err(|_| IdentityError::Provider)?;
        if !response.status().is_success() {
            return Err(IdentityError::Provider);
        }
        let receipt: serde_json::Value =
            response.json().await.map_err(|_| IdentityError::Provider)?;
        if receipt
            .get("id")
            .and_then(|id| id.as_str())
            .is_none_or(str::is_empty)
        {
            return Err(IdentityError::Provider);
        }
        Ok(())
    }
    async fn send_code(
        &self,
        email: &str,
        code: &str,
        request_id: &str,
    ) -> Result<(), IdentityError> {
        let response = self.client.post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("Idempotency-Key", format!("wam-email/{request_id}"))
            .json(&serde_json::json!({
                "from": self.from, "to": [email],
                "subject": "Your Web Agentic Memory sign-in code",
                "text": format!("Your Web Agentic Memory sign-in code is {code}.\n\nIt expires in 10 minutes. Enter it only in the browser where you requested it.\n\nIf you did not request this code, you can ignore this email.")
            }))
            .send().await.map_err(|_| IdentityError::Provider)?;
        if !response.status().is_success() {
            return Err(IdentityError::Provider);
        }
        // Only validate the public receipt shape; never surface the provider body.
        #[derive(serde::Deserialize)]
        struct Receipt {
            id: String,
        }
        let receipt: Receipt = response.json().await.map_err(|_| IdentityError::Provider)?;
        if receipt.id.is_empty() {
            return Err(IdentityError::Provider);
        }
        Ok(())
    }
}

pub(super) struct EmailAuth {
    pub(super) sender: Arc<dyn EmailSender>,
    pub(super) identity_secret: String,
}

impl IdentityService {
    pub fn with_email_auth(
        mut self,
        sender: Arc<dyn EmailSender>,
        identity_secret: String,
    ) -> Result<Self, IdentityError> {
        if identity_secret.trim().len() < 32 {
            return Err(IdentityError::Invalid);
        }
        let origin = url::Url::parse(&self.frontend_origin).map_err(|_| IdentityError::Invalid)?;
        if !matches!(origin.scheme(), "http" | "https")
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
        {
            return Err(IdentityError::Invalid);
        }
        self.email_auth = Some(EmailAuth {
            sender,
            identity_secret,
        });
        Ok(self)
    }

    pub fn email_auth_enabled(&self) -> bool {
        self.email_auth.is_some()
    }

    pub(super) fn email_from_env(&mut self) -> Result<(), IdentityError> {
        let key = std::env::var("RESEND_API_KEY").ok();
        let from = std::env::var("WAM_EMAIL_FROM").ok();
        let secret = std::env::var("WAM_EMAIL_IDENTITY_SECRET").ok();
        match (key, from, secret) {
            (None, None, None) => Ok(()),
            (Some(key), Some(from), Some(secret)) if secret.trim().len() >= 32 => {
                self.email_auth = Some(EmailAuth {
                    sender: Arc::new(ResendEmailSender::new(key, from)?),
                    identity_secret: secret,
                });
                // Apply the same strict origin validation as programmatic configuration.
                let origin =
                    url::Url::parse(&self.frontend_origin).map_err(|_| IdentityError::Invalid)?;
                if !origin.username().is_empty()
                    || origin.password().is_some()
                    || origin.query().is_some()
                    || origin.fragment().is_some()
                {
                    return Err(IdentityError::Invalid);
                }
                Ok(())
            }
            _ => Err(IdentityError::Invalid),
        }
    }

    pub async fn begin_email(&self, email: &str, now: u64) -> Result<String, IdentityError> {
        let auth = self.email_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let email = normalize_email(email)?;
        let subject_hash = email_subject(&auth.identity_secret, &email);
        let challenge = random_secret()?;
        let code = random_code()?;
        self.store
            .reserve_email(&subject_hash, &challenge, &code, now)?;
        if auth
            .sender
            .send_code(&email, &code, &digest(&challenge))
            .await
            .is_err()
        {
            self.store.cancel_email(&challenge)?;
            return Err(IdentityError::Provider);
        }
        Ok(challenge)
    }

    pub fn finish_email(
        &self,
        challenge: &str,
        code: &str,
        now: u64,
    ) -> Result<(String, String, String, u64), IdentityError> {
        self.email_auth.as_ref().ok_or(IdentityError::Disabled)?;
        // Malformed guesses still spend an attempt on an existing challenge.
        self.store.consume_email(challenge, code, now)
    }

    pub fn email_challenge_cookie(&self, challenge: &str) -> String {
        format!(
            "wam_email_challenge={challenge}; Path=/v1/auth/email; HttpOnly; SameSite=Lax; Max-Age={}{}",
            if challenge.is_empty() {
                0
            } else {
                EMAIL_CODE_TTL
            },
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }
}

pub(super) fn normalize_email(input: &str) -> Result<String, IdentityError> {
    if input.chars().any(char::is_control) {
        return Err(IdentityError::Invalid);
    }
    let email = input.trim().to_ascii_lowercase();
    if !email.is_ascii() || email.len() > 254 {
        return Err(IdentityError::Invalid);
    }
    let (local, domain) = email.split_once('@').ok_or(IdentityError::Invalid)?;
    if local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        || domain.len() > 253
        || !domain.contains('.')
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(IdentityError::Invalid);
    }
    Ok(email)
}

pub(super) fn email_subject(secret: &str, email: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts all key lengths");
    mac.update(b"wam-email-identity-v1:");
    mac.update(email.as_bytes());
    format!("email:{:x}", mac.finalize().into_bytes())
}

fn random_code() -> Result<String, IdentityError> {
    let mut code = String::with_capacity(6);
    while code.len() < 6 {
        let mut byte = [0];
        getrandom::fill(&mut byte).map_err(|_| IdentityError::Storage)?;
        if byte[0] < 250 {
            code.push(char::from(b'0' + byte[0] % 10));
        }
    }
    Ok(code)
}

#[cfg(test)]
mod delivery_tests;
#[cfg(test)]
mod tests;
