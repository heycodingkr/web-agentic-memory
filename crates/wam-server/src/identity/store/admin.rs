use super::*;
use crate::identity::admin::{ADMIN_LINK_TTL, ADMIN_SESSION_TTL, AdminSession};

/// Seconds an instance owns a delivery it took to send. When it runs out (the instance died while sending) another
/// instance takes the delivery over.
const DELIVERY_LEASE_SECS: u64 = 60;
/// A failed send is tried again after these many seconds; the failure after the last one gives the link up.
const DELIVERY_BACKOFF_SECS: [u64; 4] = [5, 15, 30, 60];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct AdminState {
    challenges: Vec<AdminChallenge>,
    sessions: Vec<AdminSessionRecord>,
    sends: Vec<u64>,
    /// Links waiting to be sent, or being sent (docs/spec/admin.md, "링크 전달 outbox"). Older documents have none.
    #[serde(default)]
    deliveries: Vec<AdminDelivery>,
}
/// One link to send. It holds no link, no token and no address: the token is recomputed from `nonce` and the server's
/// identity secret whenever it is sent, so this stored state alone cannot produce a link.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct AdminDelivery {
    /// SHA-256 of the token: ties the entry to its challenge and is the provider's idempotency key.
    token_hash: String,
    nonce: String,
    attempts: u32,
    next_attempt_at: u64,
    /// The entry is owned by the instance that took it until this time.
    claimed_until: u64,
    expires_at: u64,
}
impl AdminDelivery {
    fn due(&self, now: u64) -> bool {
        self.expires_at > now && self.next_attempt_at <= now && self.claimed_until <= now
    }
}
/// A delivery an instance took to send.
pub(in crate::identity) struct DeliveryClaim {
    pub nonce: String,
    pub token_hash: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::identity) enum DeliveryResult {
    Delivered,
    Failed,
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
        self.deliveries.retain(|entry| entry.expires_at > now);
        self.sessions.retain(|entry| entry.expires_at > now);
        self.sends.retain(|sent| sent.saturating_add(3600) > now);
    }
}
impl FileIdentityStore {
    /// Starts a challenge. With a `nonce` the link is also queued for delivery in the same transaction; the token must
    /// be the one `nonce` derives.
    pub(in crate::identity) fn reserve_admin(
        &self,
        subject: &str,
        binding: &str,
        token: &str,
        nonce: Option<&str>,
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
            // A new link invalidates the previous one, and so ends its delivery.
            admin.challenges.clear();
            admin.deliveries.clear();
            admin.challenges.push(AdminChallenge {
                subject: subject.into(),
                binding_hash: digest(binding),
                token_hash: digest(token),
                expires_at: now.saturating_add(ADMIN_LINK_TTL),
            });
            if let Some(nonce) = nonce {
                admin.deliveries.push(AdminDelivery {
                    token_hash: digest(token),
                    nonce: nonce.into(),
                    attempts: 0,
                    next_attempt_at: now,
                    claimed_until: 0,
                    expires_at: now.saturating_add(ADMIN_LINK_TTL),
                });
            }
            admin.sends.push(now);
            Ok(())
        })
    }
    /// Takes the next delivery that is due and not owned by another instance, and owns it for the lease. It also drops
    /// the entries whose link expired. The check comes first as a read so that an idle worker does not rewrite the
    /// state every few seconds.
    pub(in crate::identity) fn claim_admin_delivery(
        &self,
        now: u64,
    ) -> Result<Option<DeliveryClaim>, IdentityError> {
        let work = self.read(|state| {
            let entries = &state.admin.deliveries;
            entries.iter().any(|entry| entry.due(now))
                || entries.iter().any(|entry| entry.expires_at <= now)
        })?;
        if !work {
            return Ok(None);
        }
        self.update(|state| {
            state.admin.purge(now);
            Ok(state
                .admin
                .deliveries
                .iter_mut()
                .find(|entry| entry.due(now))
                .map(|entry| {
                    entry.claimed_until = now.saturating_add(DELIVERY_LEASE_SECS);
                    DeliveryClaim {
                        nonce: entry.nonce.clone(),
                        token_hash: entry.token_hash.clone(),
                    }
                }))
        })
    }
    /// Books the outcome of a send. A delivered link leaves the outbox. A failure schedules the next try, and the
    /// failure after the last try cancels the challenge, so that nothing keeps a link that cannot arrive.
    pub(in crate::identity) fn finish_admin_delivery(
        &self,
        token_hash: &str,
        result: DeliveryResult,
        now: u64,
    ) -> Result<(), IdentityError> {
        self.update(|state| {
            let admin = &mut state.admin;
            let Some(index) = admin
                .deliveries
                .iter()
                .position(|entry| constant_hash_eq(&entry.token_hash, token_hash))
            else {
                return Ok(());
            };
            if result == DeliveryResult::Delivered {
                admin.deliveries.remove(index);
                return Ok(());
            }
            let entry = &mut admin.deliveries[index];
            entry.attempts = entry.attempts.saturating_add(1);
            match DELIVERY_BACKOFF_SECS.get(entry.attempts as usize - 1) {
                Some(wait) => {
                    entry.next_attempt_at = now.saturating_add(*wait);
                    entry.claimed_until = 0;
                }
                None => {
                    admin.deliveries.remove(index);
                    admin
                        .challenges
                        .retain(|entry| !constant_hash_eq(&entry.token_hash, token_hash));
                }
            }
            Ok(())
        })
    }
    /// Is any link that has not expired still waiting to be sent?
    pub(in crate::identity) fn admin_deliveries_pending(
        &self,
        now: u64,
    ) -> Result<bool, IdentityError> {
        self.read(|state| {
            state
                .admin
                .deliveries
                .iter()
                .any(|entry| entry.expires_at > now)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_from_before_the_outbox_still_loads() {
        // What the previous release stored: no `deliveries`.
        let state: AdminState =
            serde_json::from_str(r#"{"challenges":[],"sessions":[],"sends":[1000]}"#).unwrap();
        assert!(state.deliveries.is_empty());
        assert_eq!(state.sends, vec![1000]);
        // An older release ignores the field it does not know (the document has no `deny_unknown_fields`).
        let mut value = serde_json::to_value(&state).unwrap();
        value["a_field_of_a_future_release"] = serde_json::json!(1);
        assert!(serde_json::from_value::<AdminState>(value).is_ok());
    }

    #[test]
    fn a_delivery_is_due_only_inside_its_window_and_when_nobody_owns_it() {
        let entry = AdminDelivery {
            token_hash: "hash".into(),
            nonce: "nonce".into(),
            attempts: 0,
            next_attempt_at: 100,
            claimed_until: 0,
            expires_at: 700,
        };
        assert!(!entry.due(99), "not before its time");
        assert!(entry.due(100) && entry.due(699));
        assert!(!entry.due(700), "not once the link expired");
        let owned = AdminDelivery {
            claimed_until: 160,
            ..entry.clone()
        };
        assert!(!owned.due(159), "not while another instance owns it");
        assert!(owned.due(160), "when the lease ran out it is taken over");
    }
}
