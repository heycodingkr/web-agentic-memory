use super::*;
use crate::identity::admin::{ADMIN_LINK_TTL, ADMIN_SESSION_TTL, AdminSession};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct AdminState {
    challenges: Vec<AdminChallenge>,
    sessions: Vec<AdminSessionRecord>,
    sends: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct AdminChallenge {
    subject: String,
    binding_hash: String,
    token_hash: String,
    expires_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct AdminSessionRecord {
    subject: String,
    token_hash: String,
    expires_at: u64,
}
impl AdminState {
    pub(super) fn purge(&mut self, now: u64) {
        self.challenges.retain(|entry| entry.expires_at > now);
        self.sessions.retain(|entry| entry.expires_at > now);
        self.sends.retain(|sent| sent.saturating_add(3600) > now);
    }
}
impl FileIdentityStore {
    pub(in crate::identity) fn reserve_admin(
        &self,
        subject: &str,
        binding: &str,
        token: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        self.update(|state| {
            let admin = &mut state.admin;
            admin.purge(now);
            if admin.sends.len() >= 5
                || admin.sends.iter().any(|sent| sent.saturating_add(60) > now)
            {
                return Err(IdentityError::RateLimited);
            }
            admin.challenges.clear();
            admin.challenges.push(AdminChallenge {
                subject: subject.into(),
                binding_hash: digest(binding),
                token_hash: digest(token),
                expires_at: now.saturating_add(ADMIN_LINK_TTL),
            });
            admin.sends.push(now);
            Ok(())
        })
    }
    pub(in crate::identity) fn cancel_admin(&self, token: &str) -> Result<(), IdentityError> {
        self.update(|state| {
            state
                .admin
                .challenges
                .retain(|entry| !constant_hash_eq(&entry.token_hash, &digest(token)));
            Ok(())
        })
    }
    pub(in crate::identity) fn consume_admin(
        &self,
        subject: &str,
        binding: &str,
        token: &str,
        now: u64,
    ) -> Result<(String, AdminSession), IdentityError> {
        if binding.len() != 43 || token.len() != 43 {
            return Err(IdentityError::Invalid);
        }
        let session_token = random_secret()?;
        let expires_at = now.saturating_add(ADMIN_SESSION_TTL);
        self.update(|state| {
            state.admin.purge(now);
            let index = state
                .admin
                .challenges
                .iter()
                .position(|entry| {
                    constant_hash_eq(&entry.subject, subject)
                        && constant_hash_eq(&entry.binding_hash, &digest(binding))
                        && constant_hash_eq(&entry.token_hash, &digest(token))
                })
                .ok_or(IdentityError::Invalid)?;
            state.admin.challenges.remove(index);
            // Bound stored sessions, while allowing the administrator to use several browsers.
            if state.admin.sessions.len() >= 10 {
                state.admin.sessions.remove(0);
            }
            state.admin.sessions.push(AdminSessionRecord {
                subject: subject.into(),
                token_hash: digest(&session_token),
                expires_at,
            });
            Ok((
                session_token.clone(),
                AdminSession {
                    csrf_token: derive_csrf(&session_token),
                    expires_at,
                },
            ))
        })
    }
    pub(in crate::identity) fn admin_session(
        &self,
        subject: &str,
        token: &str,
        now: u64,
    ) -> Result<AdminSession, IdentityError> {
        if token.len() != 43 {
            return Err(IdentityError::Invalid);
        }
        self.read(|state| {
            state
                .admin
                .sessions
                .iter()
                .find(|entry| {
                    entry.expires_at > now
                        && constant_hash_eq(&entry.subject, subject)
                        && constant_hash_eq(&entry.token_hash, &digest(token))
                })
                .map(|entry| AdminSession {
                    csrf_token: derive_csrf(token),
                    expires_at: entry.expires_at,
                })
                .ok_or(IdentityError::Invalid)
        })?
    }
    pub(in crate::identity) fn revoke_admin(&self, token: &str) -> Result<(), IdentityError> {
        self.update(|state| {
            state
                .admin
                .sessions
                .retain(|entry| !constant_hash_eq(&entry.token_hash, &digest(token)));
            Ok(())
        })
    }
}
