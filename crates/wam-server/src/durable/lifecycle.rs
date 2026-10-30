use super::{
    ApplicationRepository,
    adapters::append_event,
    state::{AppState, Work},
};
use crate::{
    application::ServiceError, contribution_store::state::decide_review_record, ports::Repository,
};
use serde::Serialize;
use wam_contracts::{ContributionRecordResponse, ContributionReviewDecision};
use wam_gatekeeper::{record::sha256_hex, store::Status, tools::Tools};

#[derive(Clone, Debug, Default, Serialize)]
pub struct PurgeReport {
    pub contributions: usize,
    pub receipts: usize,
}

pub(super) fn reverse_version(
    state: &mut AppState,
    memory: &str,
    version: u32,
    now: u64,
) -> Result<(), ServiceError> {
    let version_id = super::version_id(memory, version);
    let events: Vec<_> = state
        .scores
        .events
        .iter()
        .filter(|event| {
            event.memory_id == memory
                && event.memory_version_id == version_id
                && event.points_delta > 0
        })
        .cloned()
        .collect();
    for event in events {
        append_event(
            state,
            &event.beneficiary_account_id,
            (memory, version),
            "reversal",
            -event.points_delta,
            format!("reversal:{}", event.id),
            now,
        )?;
    }
    Ok(())
}

fn revoke_sources(
    state: &mut AppState,
    account: &str,
    contribution: Option<&str>,
    now: u64,
) -> Result<(), ServiceError> {
    let mut affected = std::collections::BTreeSet::new();
    for attribution in &mut state.attributions {
        if attribution.account_id.as_deref() == Some(account)
            && contribution.is_none_or(|id| attribution.contribution_id.as_deref() == Some(id))
        {
            affected.insert((attribution.memory_id.clone(), attribution.version));
            attribution.account_id = None;
            attribution.contribution_id = None;
        }
    }
    for (memory_id, version) in affected {
        let independent = state.attributions.iter().any(|entry| {
            entry.memory_id == memory_id
                && entry.version == version
                && entry
                    .account_id
                    .as_deref()
                    .is_some_and(|account| state.active(account))
                && !entry.evidence_digest.is_empty()
        });
        let support_count = state
            .attributions
            .iter()
            .filter(|entry| entry.memory_id == memory_id && entry.version == version)
            .filter_map(|entry| entry.account_id.as_deref())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let memory = state
            .catalog
            .memories
            .get_mut(&memory_id)
            .ok_or(ServiceError::Conflict)?;
        if memory
            .versions
            .last()
            .is_some_and(|stored| stored.version == version)
        {
            memory.support_count = support_count as u32;
        }
        if !independent {
            if let Some(stored) = memory
                .versions
                .iter_mut()
                .find(|stored| stored.version == version)
            {
                stored.status = Status::Retired;
            }
            if memory
                .versions
                .last()
                .is_some_and(|stored| stored.version == version)
            {
                memory.status = Status::Retired;
            }
            reverse_version(state, &memory_id, version, now)?;
        }
        let id = format!("withdrawal:{memory_id}:{version}");
        state.work.entry(id.clone()).or_insert(Work {
            id,
            contribution_id: None,
            memory_id: Some(memory_id.clone()),
            kind: "withdrawal".into(),
            state: "queued".into(),
            generation: 0,
            lease_until: None,
            holder: None,
            enqueued_at: now,
            input_digest: None,
            not_before: 0,
            attempts: 0,
        });
        state.emit("attribution_withdrawn", &memory_id, now);
    }
    Ok(())
}

fn withdraw(state: &mut AppState, account: &str, id: &str, now: u64) -> Result<bool, ServiceError> {
    let Some(record) = state
        .contributions
        .contributions
        .iter_mut()
        .find(|entry| entry.account_id == account && entry.record.id == id)
    else {
        return Ok(false);
    };
    record.record.status = "withdrawn".into();
    record.record.actions.clear();
    record.record.url_pattern.clear();
    record.record.task_category.clear();
    if let Some(consent) = state.consents.get_mut(id) {
        consent.withdrawn_at.get_or_insert(now);
        consent.purged_at.get_or_insert(now);
    }
    if let Some(work) = state.work.get_mut(&format!("contribution:{id}")) {
        work.state = "canceled".into();
        work.generation += 1;
        work.holder = None;
        work.lease_until = None;
    }
    state.proposals.remove(id);
    revoke_sources(state, account, Some(id), now)?;
    state.emit("contribution_withdrawn", id, now);
    Ok(true)
}

impl ApplicationRepository {
    pub(super) fn withdraw_contribution(
        &self,
        account: &str,
        id: &str,
        now: u64,
    ) -> Result<bool, ServiceError> {
        self.update(|state| {
            if !state.active(account) {
                return Err(ServiceError::Unauthorized);
            }
            withdraw(state, account, id, now)
        })
    }

    pub fn consent_enabled(&self, account: &str) -> Result<bool, ServiceError> {
        self.read(|state| {
            state
                .accounts
                .get(account)
                .map(|entry| entry.contribution_enabled)
                .ok_or(ServiceError::Unauthorized)
        })
    }

    pub fn set_consent(
        &self,
        account: &str,
        enabled: bool,
        version: &str,
        now: u64,
    ) -> Result<(), ServiceError> {
        if version != wam_contracts::CONTRIBUTION_CONSENT_VERSION {
            return Err(ServiceError::InvalidInput("invalid_consent_version"));
        }
        self.update(|state| {
            state
                .accounts
                .get_mut(account)
                .ok_or(ServiceError::Unauthorized)?
                .contribution_enabled = enabled;
            if !enabled {
                let ids: Vec<_> = state
                    .contributions
                    .contributions
                    .iter()
                    .filter(|entry| entry.account_id == account)
                    .map(|entry| entry.record.id.clone())
                    .collect();
                for id in ids {
                    withdraw(state, account, &id, now)?;
                }
                revoke_sources(state, account, None, now)?;
            }
            state.emit(
                if enabled {
                    "consent_enabled"
                } else {
                    "consent_withdrawn"
                },
                &sha256_hex(account.as_bytes()),
                now,
            );
            Ok(())
        })
    }

    pub fn delete_account(&self, account: &str, now: u64) -> Result<(), ServiceError> {
        self.update(|state| {
            if !state.active(account) {
                return Err(ServiceError::Unauthorized);
            }
            let ids: Vec<_> = state
                .contributions
                .contributions
                .iter()
                .filter(|entry| entry.account_id == account)
                .map(|entry| entry.record.id.clone())
                .collect();
            for id in &ids {
                withdraw(state, account, id, now)?;
            }
            revoke_sources(state, account, None, now)?;
            for entry in &mut state.contributions.contributions {
                if entry.account_id == account {
                    entry.account_id.clear();
                }
            }
            state.identity.erase_account(account, now);
            state
                .usage
                .receipts
                .retain(|receipt| receipt.owner_account != account);
            for receipt in &mut state.usage.receipts {
                for memory in &mut receipt.memory_uses {
                    if memory.author_account_id.as_deref() == Some(account) {
                        memory.author_account_id = None;
                    }
                }
            }
            // Keep the immutable financial facts; erase their identifying links.
            for event in &mut state.scores.events {
                if event.beneficiary_account_id == account {
                    event.beneficiary_account_id = "deleted".into();
                }
                if event.consumer_account_id == account {
                    event.consumer_account_id = "deleted".into();
                }
            }
            state.accounts.remove(account);
            state
                .disabled_accounts
                .insert(sha256_hex(account.as_bytes()));
            state.emit("account_deleted", &sha256_hex(account.as_bytes()), now);
            Ok(())
        })
    }

    pub fn purge(&self, now: u64) -> Result<PurgeReport, ServiceError> {
        self.update(|state| {
            let mut report = PurgeReport::default();
            let expired: Vec<_> = state
                .consents
                .values()
                .filter(|consent| consent.purge_at <= now && consent.purged_at.is_none())
                .map(|consent| consent.contribution_id.clone())
                .collect();
            for id in expired {
                if let Some(record) = state
                    .contributions
                    .contributions
                    .iter_mut()
                    .find(|entry| entry.record.id == id)
                {
                    record.record.actions.clear();
                    record.record.url_pattern.clear();
                    record.record.task_category.clear();
                    if matches!(record.record.status.as_str(), "queued" | "in_review") {
                        record.record.status = "expired".into();
                    }
                }
                if let Some(consent) = state.consents.get_mut(&id) {
                    consent.purged_at = Some(now);
                }
                if let Some(work) = state.work.get_mut(&format!("contribution:{id}")) {
                    if work.state != "done" {
                        work.state = "canceled".into();
                        work.generation += 1;
                        work.holder = None;
                        work.lease_until = None;
                    }
                }
                state.proposals.remove(&id);
                report.contributions += 1;
            }
            let prior = state.usage.receipts.len();
            state
                .usage
                .receipts
                .retain(|receipt| now.saturating_sub(receipt.issued_at) <= 30 * 24 * 60 * 60);
            report.receipts = prior - state.usage.receipts.len();
            state.identity.purge(now);
            // Withdrawal is conservative: absent independent evidence, memory
            // is already hidden. The durable review records that safe decision.
            for work in state
                .work
                .values_mut()
                .filter(|work| work.kind == "withdrawal" && work.state == "queued")
            {
                work.state = "done".into();
            }
            if report.contributions > 0 || report.receipts > 0 {
                state.emit("retention_purged", "retention-v1-7d", now);
            }
            Ok(report)
        })
    }

    pub(super) fn review_decision(
        &self,
        id: &str,
        generation: u64,
        decision: ContributionReviewDecision,
        reviewer: &str,
        now: u64,
    ) -> Result<Option<ContributionRecordResponse>, ServiceError> {
        let tools = Tools::load().map_err(|_| ServiceError::Unavailable)?;
        self.update(|state| {
            let Some(record) = state
                .contributions
                .contributions
                .iter()
                .find(|entry| entry.record.id == id)
            else {
                return Ok(None);
            };
            if record.record.status != "in_review" || record.review_generation != generation {
                return Ok(None);
            }
            if decision == ContributionReviewDecision::Approve {
                if let Some(proposal) = state.proposals.get(id).cloned() {
                    super::worker::validate_proposal(state, &proposal, false, now)?;
                    super::worker::commit_proposal(
                        state,
                        &proposal,
                        &tools,
                        &self.task_vocabulary(),
                        now,
                    )?;
                    state
                        .contributions
                        .review_audit
                        .push(crate::ports::ReviewAuditRecord {
                            contribution_id: id.into(),
                            decision: "approved".into(),
                            review_generation: generation,
                            decided_at: now,
                            reviewer: reviewer.into(),
                        });
                    return Ok(state
                        .contributions
                        .contributions
                        .iter()
                        .find(|entry| entry.record.id == id)
                        .map(|entry| entry.record.clone()));
                }
                // A manual approval without a replay-verified proposal only
                // authorizes another worker attempt; it cannot publish.
                let record = state
                    .contributions
                    .contributions
                    .iter_mut()
                    .find(|entry| entry.record.id == id)
                    .ok_or(ServiceError::Conflict)?;
                record.record.status = "queued".into();
                let response = record.record.clone();
                if let Some(work) = state.work.get_mut(&format!("contribution:{id}")) {
                    work.state = "queued".into();
                    work.generation += 1;
                    work.attempts = 0;
                    work.not_before = 0;
                    work.lease_until = None;
                    work.holder = None;
                }
                state
                    .contributions
                    .review_audit
                    .push(crate::ports::ReviewAuditRecord {
                        contribution_id: id.into(),
                        decision: "verification_requested".into(),
                        review_generation: generation,
                        decided_at: now,
                        reviewer: reviewer.into(),
                    });
                state.emit("verification_requested", id, now);
                return Ok(Some(response));
            }
            let response = decide_review_record(
                &mut state.contributions,
                id,
                generation,
                decision,
                reviewer,
                now,
            );
            if response.is_some() {
                state.proposals.remove(id);
                if let Some(work) = state.work.get_mut(&format!("contribution:{id}")) {
                    work.state = "canceled".into();
                    work.generation += 1;
                    work.lease_until = None;
                    work.holder = None;
                }
                state.emit("contribution_rejected", id, now);
            }
            Ok(response)
        })
    }

    pub fn retire_version(
        &self,
        memory_id: &str,
        expected_version: u32,
        now: u64,
    ) -> Result<(), ServiceError> {
        self.update(|state| {
            let memory = state
                .catalog
                .memories
                .get_mut(memory_id)
                .ok_or(ServiceError::NotFound)?;
            if memory
                .versions
                .last()
                .is_none_or(|version| version.version != expected_version)
            {
                return Err(ServiceError::Conflict);
            }
            memory.status = Status::Retired;
            if let Some(version) = memory.versions.last_mut() {
                version.status = Status::Retired;
            }
            reverse_version(state, memory_id, expected_version, now)?;
            state.emit("memory_retired", memory_id, now);
            Ok(())
        })
    }
}
