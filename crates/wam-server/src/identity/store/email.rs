use super::*;
use crate::identity::email::{EMAIL_CODE_TTL, EMAIL_RESEND_DELAY};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct EmailChallenge {
    subject_hash: String,
    challenge_hash: String,
    code_hash: String,
    expires_at: u64,
    attempts: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct EmailSend {
    subject_hash: String,
    sent_at: u64,
}

impl IdentityState {
    pub(super) fn purge_email(&mut self, now: u64) {
        self.email_challenges.retain(|entry| entry.expires_at > now);
        self.email_sends
            .retain(|entry| entry.sent_at.saturating_add(3600) > now);
    }

    pub(super) fn erase_email(&mut self, account_id: &str) {
        let subjects: Vec<_> = self
            .accounts
            .iter()
            .filter(|entry| {
                entry.account_id == account_id && entry.subject_hash.starts_with("email:")
            })
            .map(|entry| entry.subject_hash.clone())
            .collect();
        self.email_challenges
            .retain(|entry| !subjects.contains(&entry.subject_hash));
        self.email_sends
            .retain(|entry| !subjects.contains(&entry.subject_hash));
    }
}

impl FileIdentityStore {
    pub(in crate::identity) fn reserve_email(
        &self,
        subject_hash: &str,
        challenge: &str,
        code: &str,
        now: u64,
    ) -> Result<(), IdentityError> {
        self.update(|state| {
            state.purge_email(now);
            let sends: Vec<_> = state
                .email_sends
                .iter()
                .filter(|entry| entry.subject_hash == subject_hash)
                .collect();
            if sends.len() >= 5
                || sends
                    .iter()
                    .any(|entry| entry.sent_at.saturating_add(EMAIL_RESEND_DELAY) > now)
            {
                return Err(IdentityError::RateLimited);
            }
            state
                .email_challenges
                .retain(|entry| entry.subject_hash != subject_hash);
            state.email_challenges.push(EmailChallenge {
                subject_hash: subject_hash.into(),
                challenge_hash: digest(challenge),
                code_hash: digest(&format!("{challenge}:{code}")),
                expires_at: now.saturating_add(EMAIL_CODE_TTL),
                attempts: 0,
            });
            state.email_sends.push(EmailSend {
                subject_hash: subject_hash.into(),
                sent_at: now,
            });
            Ok(())
        })
    }

    pub(in crate::identity) fn cancel_email(&self, challenge: &str) -> Result<(), IdentityError> {
        self.update(|state| {
            state
                .email_challenges
                .retain(|entry| !constant_hash_eq(&entry.challenge_hash, &digest(challenge)));
            Ok(())
        })
    }

    pub(in crate::identity) fn consume_email(
        &self,
        challenge: &str,
        code: &str,
        now: u64,
    ) -> Result<(String, String, String, u64), IdentityError> {
        if challenge.len() != 43 {
            return Err(IdentityError::Invalid);
        }
        let token = random_secret()?;
        let csrf = derive_csrf(&token);
        let expires_at = now.saturating_add(SESSION_TTL_SECS);
        self.update(|state| {
            let index = state
                .email_challenges
                .iter()
                .position(|entry| constant_hash_eq(&entry.challenge_hash, &digest(challenge)))
                .ok_or(IdentityError::Invalid)?;
            let entry = &mut state.email_challenges[index];
            if entry.expires_at <= now {
                return Err(IdentityError::Expired);
            }
            if entry.attempts >= 5 {
                return Err(IdentityError::RateLimited);
            }
            entry.attempts += 1;
            if code.len() != 6
                || !code.bytes().all(|b| b.is_ascii_digit())
                || !constant_hash_eq(&entry.code_hash, &digest(&format!("{challenge}:{code}")))
            {
                return Err(IdentityError::Invalid);
            }
            let subject_hash = state.email_challenges.remove(index).subject_hash;
            let account_id = match state
                .accounts
                .iter()
                .find(|entry| entry.subject_hash == subject_hash)
            {
                Some(entry) => entry.account_id.clone(),
                None => {
                    let account_id = format!("email_{}", &digest(&random_secret()?)[..24]);
                    state.accounts.push(AccountBinding {
                        subject_hash,
                        account_id: account_id.clone(),
                    });
                    account_id
                }
            };
            state
                .sessions
                .retain(|entry| entry.expires_at > now && entry.revoked_at.is_none());
            state.sessions.push(SessionRecord {
                token_hash: digest(&token),
                csrf_hash: digest(&csrf),
                account_id: account_id.clone(),
                expires_at,
                revoked_at: None,
            });
            Ok((account_id, token.clone(), csrf.clone(), expires_at))
        })
    }
}
