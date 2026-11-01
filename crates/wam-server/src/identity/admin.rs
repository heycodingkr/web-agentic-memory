//! Dedicated, browser-bound administrator magic links. Customer credentials never grant review access.
use super::email::{email_subject, normalize_email};
use super::{
    IdentityError, IdentityService, constant_hash_eq, cookie_value, digest, random_secret,
};
use axum::http::{HeaderMap, header};
use serde::Serialize;

pub const ADMIN_LINK_TTL: u64 = 600;
pub const ADMIN_SESSION_TTL: u64 = 2 * 60 * 60;

pub(super) struct AdminAuth {
    email: String,
    origin: String,
    authority: String,
    subject: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct AdminSession {
    pub csrf_token: String,
    pub expires_at: u64,
}
impl IdentityService {
    pub fn with_admin_auth(mut self, email: &str, origin: &str) -> Result<Self, IdentityError> {
        self.admin_auth = Some(self.admin_configuration(email, origin)?);
        Ok(self)
    }
    fn admin_configuration(&self, email: &str, origin: &str) -> Result<AdminAuth, IdentityError> {
        let email = normalize_email(email)?;
        let url = url::Url::parse(origin).map_err(|_| IdentityError::Invalid)?;
        if url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
        {
            return Err(IdentityError::Invalid);
        }
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || (url.scheme() == "https" && !self.cookie_secure)
        {
            return Err(IdentityError::Invalid);
        }
        let mail = self.email_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let origin = url.origin().ascii_serialization();
        if origin == self.frontend_origin {
            return Err(IdentityError::Invalid);
        }
        let authority = origin
            .split_once("://")
            .ok_or(IdentityError::Invalid)?
            .1
            .to_owned();
        let subject = email_subject(&mail.identity_secret, &format!("admin:{origin}:{email}"));
        Ok(AdminAuth {
            email,
            origin,
            authority,
            subject,
        })
    }
    pub(super) fn admin_from_env(&mut self) -> Result<(), IdentityError> {
        match (
            std::env::var("WAM_ADMIN_EMAIL").ok(),
            std::env::var("WAM_ADMIN_ORIGIN").ok(),
        ) {
            (None, None) => Ok(()),
            (Some(email), Some(origin)) => {
                self.admin_auth = Some(self.admin_configuration(&email, &origin)?);
                Ok(())
            }
            _ => Err(IdentityError::Invalid),
        }
    }
    pub fn admin_enabled(&self) -> bool {
        self.admin_auth.is_some()
    }
    pub fn admin_request_valid(&self, headers: &HeaderMap, write: bool) -> bool {
        let Some(admin) = &self.admin_auth else {
            return false;
        };
        let mut hosts = headers.get_all(header::HOST).iter();
        if hosts.next().and_then(|value| value.to_str().ok()) != Some(admin.authority.as_str())
            || hosts.next().is_some()
        {
            return false;
        }
        let mut origins = headers.get_all(header::ORIGIN).iter();
        match origins.next() {
            Some(origin) if origin.to_str().ok() == Some(admin.origin.as_str()) => {}
            None if !write => {}
            _ => return false,
        }
        if origins.next().is_some() || headers.contains_key(header::AUTHORIZATION) {
            return false;
        }
        if let Some(site) = headers.get("sec-fetch-site") {
            if !matches!(site.to_str().ok(), Some("same-origin" | "none")) {
                return false;
            }
        }
        true
    }
    pub(crate) fn unusable_admin_binding(&self) -> Result<String, IdentityError> {
        random_secret()
    }
    pub(crate) fn queue_admin_link(&self, email: &str, now: u64) -> Result<String, IdentityError> {
        let admin = self.admin_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let email = normalize_email(email)?;
        if !constant_hash_eq(&digest(&email), &digest(&admin.email)) {
            return Err(IdentityError::Invalid);
        }
        let binding = random_secret()?;
        let token = random_secret()?;
        self.store
            .reserve_admin(&admin.subject, &binding, &token, now)?;
        let link = format!("{}/admin/verify#token={token}", admin.origin);
        let store = self.store.clone();
        let sender = self
            .email_auth
            .as_ref()
            .ok_or(IdentityError::Disabled)?
            .sender
            .clone();
        tokio::spawn(async move {
            if sender
                .send_admin_link(&email, &link, &digest(&token))
                .await
                .is_err()
            {
                let _ = store.cancel_admin(&token);
            }
        });
        Ok(binding)
    }
    pub async fn begin_admin(&self, email: &str, now: u64) -> Result<String, IdentityError> {
        let admin = self.admin_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let email = normalize_email(email)?;
        if !constant_hash_eq(&digest(&email), &digest(&admin.email)) {
            return Err(IdentityError::Invalid);
        }
        let binding = random_secret()?;
        let token = random_secret()?;
        self.store
            .reserve_admin(&admin.subject, &binding, &token, now)?;
        let link = format!("{}/admin/verify#token={token}", admin.origin);
        if self
            .email_auth
            .as_ref()
            .ok_or(IdentityError::Disabled)?
            .sender
            .send_admin_link(&admin.email, &link, &digest(&token))
            .await
            .is_err()
        {
            self.store.cancel_admin(&token)?;
            return Err(IdentityError::Provider);
        }
        Ok(binding)
    }
    pub fn finish_admin(
        &self,
        binding: &str,
        token: &str,
        now: u64,
    ) -> Result<(String, AdminSession), IdentityError> {
        let admin = self.admin_auth.as_ref().ok_or(IdentityError::Disabled)?;
        self.store
            .consume_admin(&admin.subject, binding, token, now)
    }
    pub fn authenticate_admin(
        &self,
        headers: &HeaderMap,
        now: u64,
    ) -> Result<AdminSession, IdentityError> {
        let admin = self.admin_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let token =
            cookie_value(headers, self.admin_cookie_name(false)).ok_or(IdentityError::Invalid)?;
        self.store.admin_session(&admin.subject, &token, now)
    }
    pub fn admin_csrf_valid(&self, headers: &HeaderMap, session: &AdminSession) -> bool {
        let mut csrf = headers.get_all("x-csrf-token").iter();
        let valid = csrf
            .next()
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| constant_hash_eq(&digest(value), &digest(&session.csrf_token)));
        valid && csrf.next().is_none()
    }
    pub fn logout_admin(&self, headers: &HeaderMap) -> Result<(), IdentityError> {
        let token =
            cookie_value(headers, self.admin_cookie_name(false)).ok_or(IdentityError::Invalid)?;
        self.store.revoke_admin(&token)
    }
    pub fn admin_cookie_name(&self, pending: bool) -> &'static str {
        match (self.cookie_secure, pending) {
            (true, true) => "__Host-wam_admin_pending",
            (true, false) => "__Host-wam_admin_session",
            (false, true) => "wam_admin_pending",
            (false, false) => "wam_admin_session",
        }
    }
    pub fn admin_cookie(&self, value: &str, pending: bool) -> String {
        format!(
            "{}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
            self.admin_cookie_name(pending),
            if value.is_empty() {
                0
            } else if pending {
                ADMIN_LINK_TTL
            } else {
                ADMIN_SESSION_TTL
            },
            if self.cookie_secure { "; Secure" } else { "" }
        )
    }
}
