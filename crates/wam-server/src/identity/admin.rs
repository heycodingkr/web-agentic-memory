//! Dedicated, browser-bound administrator magic links. Customer credentials never grant review access.
use super::email::{EmailSender, email_subject, normalize_email};
use super::store::admin::DeliveryResult;
use super::{
    FileIdentityStore, IdentityError, IdentityService, URL_SAFE_NO_PAD, constant_hash_eq,
    cookie_value, digest, now_unix, random_secret,
};
use axum::http::{HeaderMap, header};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::{sync::Arc, time::Duration};

pub const ADMIN_LINK_TTL: u64 = 600;
pub const ADMIN_SESSION_TTL: u64 = 2 * 60 * 60;
/// How often the delivery worker looks at the outbox while a link is waiting, and while none is (another instance may
/// have queued one).
const DELIVERY_POLL_BUSY: Duration = Duration::from_secs(2);
const DELIVERY_POLL_IDLE: Duration = Duration::from_secs(30);

/// The link token of a delivery, recomputed from its stored nonce: 256 bits that nobody can produce without the
/// identity secret, which is never stored.
fn link_token(secret: &str, nonce: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts all key lengths");
    mac.update(b"wam-admin-link-v1:");
    mac.update(nonce.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// What sending the queued links needs, detached from the service so that a background task can own it.
struct DeliveryContext {
    store: Arc<FileIdentityStore>,
    sender: Arc<dyn EmailSender>,
    secret: String,
    email: String,
    origin: String,
}
impl DeliveryContext {
    /// Sends every link that is due, one at a time, and returns how many sends it made. `clock` is read before each
    /// claim and after each send, so that the next try is scheduled from when the send ended.
    async fn run_due(&self, clock: &(dyn Fn() -> u64 + Send + Sync)) -> usize {
        let mut sends = 0;
        loop {
            // The store does file or database I/O: keep it off the async worker threads.
            let (store, now) = (self.store.clone(), clock());
            let Ok(Ok(Some(claim))) =
                tokio::task::spawn_blocking(move || store.claim_admin_delivery(now)).await
            else {
                break;
            };
            let token = link_token(&self.secret, &claim.nonce);
            let link = format!("{}/admin/verify#token={token}", self.origin);
            // The token digest is the provider's idempotency key, so a send taken over after a crash is the same send.
            let result = match self
                .sender
                .send_admin_link(&self.email, &link, &claim.token_hash)
                .await
            {
                Ok(()) => DeliveryResult::Delivered,
                Err(_) => DeliveryResult::Failed,
            };
            sends += 1;
            let (store, now) = (self.store.clone(), clock());
            let booked = tokio::task::spawn_blocking(move || {
                store.finish_admin_delivery(&claim.token_hash, result, now)
            })
            .await;
            if !matches!(booked, Ok(Ok(()))) {
                // The outcome could not be booked: the lease runs out and the link is sent again, which the provider
                // answers with the same receipt.
                break;
            }
        }
        sends
    }
}

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
    fn delivery_context(&self) -> Result<DeliveryContext, IdentityError> {
        let admin = self.admin_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let mail = self.email_auth.as_ref().ok_or(IdentityError::Disabled)?;
        Ok(DeliveryContext {
            store: self.store.clone(),
            sender: mail.sender.clone(),
            secret: mail.identity_secret.clone(),
            email: admin.email.clone(),
            origin: admin.origin.clone(),
        })
    }
    /// Queues the sign-in link and returns the browser binding without waiting for the mail provider. The link goes
    /// into the durable outbox in the same transaction as the challenge, and a task sends it at once; if the process
    /// ends or the provider fails, a worker sends it later (docs/spec/admin.md, "링크 전달 outbox").
    pub fn queue_admin_link(&self, email: &str, now: u64) -> Result<String, IdentityError> {
        let admin = self.admin_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let mail = self.email_auth.as_ref().ok_or(IdentityError::Disabled)?;
        let email = normalize_email(email)?;
        if !constant_hash_eq(&digest(&email), &digest(&admin.email)) {
            return Err(IdentityError::Invalid);
        }
        let binding = random_secret()?;
        let nonce = random_secret()?;
        let token = link_token(&mail.identity_secret, &nonce);
        self.store
            .reserve_admin(&admin.subject, &binding, &token, Some(&nonce), now)?;
        if !self.admin_manual_delivery {
            let context = self.delivery_context()?;
            let wake = self.admin_wake.clone();
            tokio::spawn(async move {
                let started = std::time::Instant::now();
                context
                    .run_due(&|| now.saturating_add(started.elapsed().as_secs()))
                    .await;
                // A failed send is due again soon: let the worker know.
                wake.notify_one();
            });
        }
        Ok(binding)
    }
    /// Makes the service leave sending to the caller: queued links are not sent at once and only
    /// [`deliver_admin_links`](Self::deliver_admin_links) sends them. For tests that drive time by hand.
    pub fn with_manual_admin_delivery(mut self) -> Self {
        self.admin_manual_delivery = true;
        self
    }
    /// Sends the queued links that are due at time `now` and returns how many sends it made. A send that fails is
    /// tried again later and the last failure cancels the link; a send an instance began and did not finish is taken
    /// over once its lease ran out.
    pub async fn deliver_admin_links(&self, now: u64) -> Result<usize, IdentityError> {
        Ok(self.delivery_context()?.run_due(&|| now).await)
    }
    /// Is a sign-in link that has not expired still waiting to be sent, as of `now`?
    pub fn admin_delivery_pending(&self, now: u64) -> Result<bool, IdentityError> {
        self.store.admin_deliveries_pending(now)
    }
    /// Starts the task that sends queued sign-in links: once at start (a link a previous process did not finish), then
    /// every couple of seconds while one is waiting and every half minute otherwise. `None` when administrator
    /// sign-in is not configured. The task ends with the runtime; the outbox carries what it did not finish.
    pub fn spawn_admin_delivery(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        let context = self.delivery_context().ok()?;
        let wake = self.admin_wake.clone();
        Some(tokio::spawn(async move {
            loop {
                context.run_due(&now_unix).await;
                let wait = if context
                    .store
                    .admin_deliveries_pending(now_unix())
                    .unwrap_or(false)
                {
                    DELIVERY_POLL_BUSY
                } else {
                    DELIVERY_POLL_IDLE
                };
                tokio::select! {
                    _ = wake.notified() => {}
                    _ = tokio::time::sleep(wait) => {}
                }
            }
        }))
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
            .reserve_admin(&admin.subject, &binding, &token, None, now)?;
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
