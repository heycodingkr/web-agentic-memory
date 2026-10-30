use super::{
    ApplicationRepository,
    state::{Account, Consent, RETENTION_SECS, Work},
};
use crate::{
    application::ServiceError,
    contribution_store::state as contributions,
    ports::*,
    profile_wire::profile_response,
    score_store::{self, ScoreEvent},
    usage_store,
};
use std::collections::BTreeSet;
use wam_contracts::*;
use wam_core::{
    keyword::{KeywordVocabulary, TaskVocabulary},
    leaderboard::RankingAccount,
    profile::{CreatorProfile, PublicHandle},
};

pub(super) fn ensure_account(
    state: &mut super::state::AppState,
    account: &str,
) -> Result<(), ServiceError> {
    if state
        .disabled_accounts
        .contains(&wam_gatekeeper::record::sha256_hex(account.as_bytes()))
    {
        return Err(ServiceError::Unauthorized);
    }
    if state.accounts.contains_key(account) {
        return Ok(());
    }
    if !account.strip_prefix("github_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 128
            && suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }) {
        return Err(ServiceError::NotFound);
    }
    let prefix = format!(
        "member_{}",
        account
            .trim_start_matches("github_")
            .chars()
            .take(10)
            .collect::<String>()
    );
    let mut handle = prefix.clone();
    let mut number = 1;
    while state
        .accounts
        .values()
        .any(|entry| entry.profile.handle == handle)
    {
        handle = format!("{prefix}_{number}");
        number += 1;
    }
    state.accounts.insert(
        account.into(),
        Account {
            id: account.into(),
            base_points: 0,
            contribution_enabled: true,
            profile: ProfilePutRequest {
                handle,
                bio: vec![],
                service_links: vec![],
                public_opt_in: false,
            },
        },
    );
    Ok(())
}

fn repository_error(error: ServiceError) -> RepositoryError {
    match error {
        ServiceError::NotFound => RepositoryError::AccountMissing,
        ServiceError::Unauthorized => RepositoryError::AccountDisabled,
        ServiceError::Conflict => RepositoryError::HandleTaken,
        _ => RepositoryError::Unavailable,
    }
}

impl Repository for ApplicationRepository {
    fn ranking_includes_ledger(&self) -> bool {
        true
    }
    fn memories(&self) -> Vec<MemoryRecord> {
        self.try_memories().unwrap_or_default()
    }
    fn try_memories(&self) -> Result<Vec<MemoryRecord>, RepositoryError> {
        self.read(|state| state.records(&self.task_vocabulary()))
            .map_err(repository_error)
    }
    fn try_recall_candidates(
        &self,
        query: &wam_core::recall::RecallQuery,
    ) -> Result<Vec<MemoryRecord>, RepositoryError> {
        if let Some(records) = self
            .indexed_recall_candidates(query, &self.task_vocabulary())
            .map_err(repository_error)?
        {
            return Ok(records);
        }
        self.try_memories()
    }
    fn task_vocabulary(&self) -> TaskVocabulary {
        TaskVocabulary::new("2026-01", ["compose-draft", "navigate", "research"])
            .expect("static task codes")
    }
    fn keyword_vocabulary(&self) -> KeywordVocabulary {
        let mut values: BTreeSet<String> = ["blog", "draft", "publish", "search", "write"]
            .map(str::to_owned)
            .into();
        if let Ok(words) = self.read(|state| {
            Ok(state
                .catalog
                .memories
                .values()
                .filter_map(|memory| memory.active())
                .flat_map(|version| version.payload.l0.keywords.clone())
                .collect::<Vec<_>>())
        }) {
            values.extend(words);
        }
        let digest = wam_gatekeeper::record::sha256_hex(
            values
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("|")
                .as_bytes(),
        );
        KeywordVocabulary::new(format!("catalog-{}", &digest[..12]), values)
            .expect("validated stored keyword vocabulary")
    }
    fn try_keyword_vocabulary(&self) -> Result<KeywordVocabulary, RepositoryError> {
        match self
            .indexed_keyword_vocabulary()
            .map_err(repository_error)?
        {
            Some(vocabulary) => Ok(vocabulary),
            None => Ok(self.keyword_vocabulary()),
        }
    }
    fn try_memory_by_id(&self, id: &str) -> Result<Option<MemoryRecord>, RepositoryError> {
        if let Some(record) = self
            .indexed_memory_by_id(id, &self.task_vocabulary())
            .map_err(repository_error)?
        {
            return Ok(Some(record));
        }
        self.read(|state| {
            Ok(state
                .records(&self.task_vocabulary())?
                .into_iter()
                .find(|record| record.index.id() == id))
        })
        .map_err(repository_error)
    }
    fn try_memory_by_version(
        &self,
        id: &str,
        version_id: &str,
    ) -> Result<Option<MemoryRecord>, RepositoryError> {
        if let Some(record) = self
            .indexed_memory_by_version(id, version_id, &self.task_vocabulary())
            .map_err(repository_error)?
        {
            return Ok(Some(record));
        }
        self.read(|state| state.memory_by_version(id, version_id, &self.task_vocabulary()))
            .map_err(repository_error)
    }
    fn try_authored_memory_versions(
        &self,
        account_id: &str,
    ) -> Result<Vec<AuthoredMemoryVersion>, RepositoryError> {
        if let Some(result) = self.indexed_authored_memory_versions(account_id) {
            return result.map_err(repository_error);
        }
        self.read(|state| {
            if !state.active(account_id) {
                return Err(ServiceError::Unauthorized);
            }
            let mut result = Vec::new();
            for attribution in state.attributions.iter().filter(|entry| {
                entry.role == "author" && entry.account_id.as_deref() == Some(account_id)
            }) {
                let Some(memory) = state.catalog.memories.get(&attribution.memory_id) else {
                    continue;
                };
                let Some(version) = memory
                    .versions
                    .iter()
                    .find(|version| version.version == attribution.version)
                else {
                    continue;
                };
                result.push(AuthoredMemoryVersion {
                    memory_id: memory.id.clone(),
                    version_id: super::version_id(&memory.id, version.version),
                    title: version.payload.l1.title.clone(),
                    site: version.payload.l0.site_host.clone(),
                });
            }
            result.sort_by(|left, right| {
                left.memory_id
                    .cmp(&right.memory_id)
                    .then(left.version_id.cmp(&right.version_id))
            });
            result.dedup_by(|left, right| left.version_id == right.version_id);
            Ok(result)
        })
        .map_err(repository_error)
    }
    fn ranking_accounts(&self) -> Vec<RankingAccount> {
        self.try_ranking_accounts().unwrap_or_default()
    }
    fn try_ranking_accounts(&self) -> Result<Vec<RankingAccount>, RepositoryError> {
        self.read(|state| {
            state
                .accounts
                .values()
                .map(|account| {
                    let snapshot = account.snapshot()?;
                    Ok(RankingAccount::new(
                        account.id.clone(),
                        account
                            .base_points
                            .saturating_add(score_store::total_points_for(
                                &state.scores,
                                &account.id,
                            )),
                        Some(snapshot.profile.handle().clone()),
                        snapshot.public_opt_in,
                    ))
                })
                .collect()
        })
        .map_err(repository_error)
    }
    fn profile_by_handle(&self, handle: &PublicHandle) -> Option<ProfileSnapshot> {
        self.try_profile_by_handle(handle).ok().flatten()
    }
    fn try_profile_by_handle(
        &self,
        handle: &PublicHandle,
    ) -> Result<Option<ProfileSnapshot>, RepositoryError> {
        self.read(|state| {
            state
                .accounts
                .values()
                .find(|entry| entry.profile.handle == handle.as_str())
                .map(Account::snapshot)
                .transpose()
        })
        .map_err(repository_error)
    }
    fn profile_for_account(&self, account: &str) -> Option<ProfileSnapshot> {
        self.try_profile_for_account(account).ok().flatten()
    }
    fn try_profile_for_account(
        &self,
        account: &str,
    ) -> Result<Option<ProfileSnapshot>, RepositoryError> {
        self.read(|state| {
            state
                .accounts
                .get(account)
                .map(Account::snapshot)
                .transpose()
        })
        .map_err(repository_error)
    }
    fn ensure_account(&self, account: &str) -> Result<(), RepositoryError> {
        let exists = self
            .read(|state| {
                if state
                    .disabled_accounts
                    .contains(&wam_gatekeeper::record::sha256_hex(account.as_bytes()))
                {
                    return Err(ServiceError::Unauthorized);
                }
                Ok(state.accounts.contains_key(account))
            })
            .map_err(repository_error)?;
        if exists {
            return Ok(());
        }
        self.update(|state| ensure_account(state, account))
            .map_err(repository_error)
    }
    fn save_profile(
        &self,
        account: &str,
        profile: CreatorProfile,
        public_opt_in: bool,
    ) -> Result<(), RepositoryError> {
        self.update(|state| {
            if !state.active(account) {
                return Err(ServiceError::Unauthorized);
            }
            let response = profile_response(&profile, public_opt_in);
            if state
                .accounts
                .values()
                .any(|entry| entry.id != account && entry.profile.handle == response.handle)
            {
                return Err(ServiceError::Conflict);
            }
            let stored = state
                .accounts
                .get_mut(account)
                .ok_or(ServiceError::NotFound)?;
            stored.profile = ProfilePutRequest {
                handle: response.handle,
                bio: response.bio,
                service_links: response.service_links,
                public_opt_in,
            };
            state.emit(
                "profile_changed",
                &wam_gatekeeper::record::sha256_hex(account.as_bytes()),
                crate::application::now_unix(),
            );
            Ok(())
        })
        .map_err(repository_error)
    }
}

impl ContributionStore for ApplicationRepository {
    fn transactional_queue(&self) -> bool {
        true
    }
    fn queue_health(&self, now: u64) -> Result<Option<ReviewQueueHealth>, ContributionStoreError> {
        self.read(|state| {
            let pending: Vec<_> = state
                .work
                .values()
                .filter(|job| {
                    matches!(
                        job.state.as_str(),
                        "queued" | "leased" | "retry" | "waiting_review"
                    )
                })
                .collect();
            let oldest_age_secs = pending
                .iter()
                .map(|job| now.saturating_sub(job.enqueued_at))
                .max()
                .unwrap_or_default();
            Ok(Some(ReviewQueueHealth {
                active_jobs: pending.len(),
                oldest_age_secs,
                alert: pending.len() >= 100 || oldest_age_secs > 3600,
            }))
        })
        .map_err(|_| ContributionStoreError("application storage unavailable".into()))
    }
    fn create(
        &self,
        account: &str,
        url: String,
        task: String,
        actions: Vec<ContributionAction>,
        now: u64,
    ) -> Result<ContributionRecordResponse, ContributionStoreError> {
        self.create_idempotent(
            account,
            &CreateContributionRequest {
                url: url.clone(),
                task_category: task,
                actions,
                consent: true,
                consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
                nonce: None,
            },
            &url,
            now,
        )
    }
    fn create_idempotent(
        &self,
        account: &str,
        request: &CreateContributionRequest,
        canonical_url: &str,
        now: u64,
    ) -> Result<ContributionRecordResponse, ContributionStoreError> {
        self.update(|state| {
            let owner = state
                .accounts
                .get(account)
                .ok_or(ServiceError::Unauthorized)?;
            if !owner.contribution_enabled {
                return Err(ServiceError::InvalidInput("consent_withdrawn"));
            }
            if !request.consent || request.consent_version != CONTRIBUTION_CONSENT_VERSION {
                return Err(ServiceError::InvalidInput("consent_required"));
            }
            let nonce = request
                .nonce
                .as_ref()
                .map(|nonce| wam_gatekeeper::record::sha256_hex(nonce.as_bytes()));
            if let Some(nonce) = &nonce {
                let old = state
                    .consents
                    .values()
                    .find(|consent| {
                        &consent.nonce == nonce
                            && state.contributions.contributions.iter().any(|entry| {
                                entry.record.id == consent.contribution_id
                                    && entry.account_id == account
                            })
                    })
                    .cloned();
                if let Some(consent) = old {
                    if consent.withdrawn_at.is_some() {
                        return Err(ServiceError::Conflict);
                    }
                    if consent.purge_at > now {
                        let old = state
                            .contributions
                            .contributions
                            .iter()
                            .find(|entry| entry.record.id == consent.contribution_id)
                            .ok_or(ServiceError::Conflict)?;
                        if old.record.url_pattern != canonical_url
                            || old.record.task_category != request.task_category
                            || old.record.actions != request.actions
                        {
                            return Err(ServiceError::Conflict);
                        }
                        return Ok(old.record.clone());
                    }
                    state
                        .consents
                        .get_mut(&consent.contribution_id)
                        .ok_or(ServiceError::Conflict)?
                        .nonce = format!("expired:{}", consent.contribution_id);
                }
            }
            if state
                .contributions
                .contributions
                .iter()
                .filter(|entry| {
                    entry.account_id == account
                        && matches!(entry.record.status.as_str(), "queued" | "in_review")
                })
                .count()
                >= 100
            {
                return Err(ServiceError::Unavailable);
            }
            let record = contributions::create_record(
                &mut state.contributions,
                account,
                canonical_url.into(),
                request.task_category.clone(),
                request.actions.clone(),
                now,
            )
            .map_err(|_| ServiceError::Unavailable)?;
            state.consents.insert(
                record.id.clone(),
                Consent {
                    contribution_id: record.id.clone(),
                    version: CONTRIBUTION_CONSENT_VERSION.into(),
                    nonce: nonce.unwrap_or_else(|| record.id.clone()),
                    received_at: now,
                    purge_at: now.saturating_add(RETENTION_SECS),
                    withdrawn_at: None,
                    purged_at: None,
                },
            );
            let id = format!("contribution:{}", record.id);
            state.work.insert(
                id.clone(),
                Work {
                    id,
                    contribution_id: Some(record.id.clone()),
                    memory_id: None,
                    state: "queued".into(),
                    generation: 0,
                    lease_until: None,
                    holder: None,
                    enqueued_at: now,
                    kind: "contribution".into(),
                    input_digest: None,
                    not_before: 0,
                    attempts: 0,
                },
            );
            state.emit("contribution_queued", &record.id, now);
            Ok(record)
        })
        .map_err(|error| {
            ContributionStoreError(
                match error {
                    ServiceError::InvalidInput("consent_withdrawn") => "consent_withdrawn",
                    ServiceError::Conflict => "nonce_conflict",
                    ServiceError::Unauthorized => "account_disabled",
                    ServiceError::Unavailable => "queue_unavailable",
                    _ => "contribution unavailable",
                }
                .into(),
            )
        })
    }
    fn list_for_account(
        &self,
        account: &str,
    ) -> Result<Vec<ContributionRecordResponse>, ContributionStoreError> {
        self.read(|state| Ok(contributions::list_records(&state.contributions, account)))
            .map_err(storage_error)
    }
    fn all_for_queue(&self) -> Result<Vec<OwnedContribution>, ContributionStoreError> {
        self.read(|state| Ok(contributions::all_queue_records(&state.contributions)))
            .map_err(storage_error)
    }
    fn all_for_review(&self) -> Result<Vec<ReviewContribution>, ContributionStoreError> {
        self.read(|state| Ok(contributions::all_review_records(&state.contributions)))
            .map_err(storage_error)
    }
    fn claim_for_review(
        &self,
        id: &str,
    ) -> Result<Option<ReviewContribution>, ContributionStoreError> {
        self.update(|state| {
            let record = contributions::claim_review_record(&mut state.contributions, id)
                .map_err(|_| ServiceError::Unavailable)?;
            if record.is_some() {
                if let Some(work) = state.work.get_mut(&format!("contribution:{id}")) {
                    work.generation += 1;
                    work.state = "waiting_review".into();
                    work.lease_until = None;
                    work.holder = None;
                }
            }
            Ok(record)
        })
        .map_err(storage_error)
    }
    fn decide_for_review(
        &self,
        id: &str,
        generation: u64,
        decision: ContributionReviewDecision,
        reviewer: &str,
        now: u64,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError> {
        self.review_decision(id, generation, decision, reviewer, now)
            .map_err(storage_error)
    }
    fn review_audit_history(&self) -> Result<Vec<ReviewAuditRecord>, ContributionStoreError> {
        self.read(|state| Ok(contributions::all_audit_records(&state.contributions)))
            .map_err(storage_error)
    }
    fn get_for_account(
        &self,
        account: &str,
        id: &str,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError> {
        self.read(|state| Ok(contributions::get_record(&state.contributions, account, id)))
            .map_err(storage_error)
    }
    fn withdraw_for_account(
        &self,
        account: &str,
        id: &str,
    ) -> Result<bool, ContributionStoreError> {
        self.withdraw_contribution(account, id, crate::application::now_unix())
            .map_err(storage_error)
    }
}
fn storage_error(_: ServiceError) -> ContributionStoreError {
    ContributionStoreError("application storage unavailable".into())
}

impl UsageStore for ApplicationRepository {
    fn record_recall(
        &self,
        account: Option<&str>,
        memories: &[ReceiptMemoryAttribution],
        now: u64,
    ) -> Result<Option<String>, UsageStoreError> {
        if let Some(result) = self.direct_record_recall(account, memories, now) {
            return result;
        }
        self.update(|state| {
            if account.is_some_and(|account| !state.active(account)) {
                return Err(ServiceError::Unauthorized);
            }
            let authoritative: Vec<_> = memories
                .iter()
                .map(|memory| {
                    let (row, version) = state
                        .catalog
                        .memories
                        .values()
                        .find_map(|row| {
                            row.versions
                                .iter()
                                .find(|version| {
                                    super::version_id(&row.id, version.version) == memory.version_id
                                        && row.id == memory.memory_id
                                })
                                .map(|version| (row, version))
                        })
                        .ok_or(ServiceError::Conflict)?;
                    let author_account_id = state
                        .attributions
                        .iter()
                        .find(|attribution| {
                            attribution.memory_id == row.id
                                && attribution.version == version.version
                                && attribution.role == "author"
                        })
                        .and_then(|attribution| attribution.account_id.as_ref())
                        .filter(|account| state.active(account))
                        .cloned();
                    Ok(ReceiptMemoryAttribution {
                        memory_id: row.id.clone(),
                        version_id: memory.version_id.clone(),
                        author_account_id,
                    })
                })
                .collect::<Result<_, ServiceError>>()?;
            let receipt =
                usage_store::record_recall(&mut state.usage, account, &authoritative, now)
                    .map_err(|_| ServiceError::Unavailable)?;
            if let Some(old) = receipt {
                let opaque = format!(
                    "use-{}",
                    crate::identity::random_secret().map_err(|_| ServiceError::Unavailable)?
                );
                state
                    .usage
                    .receipts
                    .iter_mut()
                    .find(|receipt| receipt.id == old)
                    .ok_or(ServiceError::Conflict)?
                    .id = opaque.clone();
                Ok(Some(opaque))
            } else {
                Ok(None)
            }
        })
        .map_err(|_| UsageStoreError("usage storage unavailable".into()))
    }
    fn record_detail(
        &self,
        memory: &ReceiptMemoryAttribution,
        account_id: Option<&str>,
        viewed_at: u64,
    ) -> Result<(), UsageStoreError> {
        if let Some(result) = self.direct_record_memory_detail(memory, account_id, viewed_at) {
            return result;
        }
        self.update(|state| {
            usage_store::record_detail(&mut state.usage, memory, account_id, viewed_at);
            Ok(())
        })
        .map_err(|_| UsageStoreError("usage storage unavailable".into()))
    }
    fn memory_usage(
        &self,
        version_ids: &[String],
        from_date: &str,
        to_date: &str,
    ) -> Result<Vec<MemoryUsageAggregate>, UsageStoreError> {
        if let Some(result) = self.direct_memory_usage(version_ids, from_date, to_date) {
            return result;
        }
        self.read(|state| {
            Ok(usage_store::memory_usage(
                &state.usage,
                version_ids,
                from_date,
                to_date,
            ))
        })
        .map_err(|_| UsageStoreError("usage storage unavailable".into()))
    }
    fn report_outcome(
        &self,
        account: &str,
        receipt: &str,
        request: &OutcomeReportRequest,
        now: u64,
    ) -> Result<OutcomeReportResult, UsageStoreError> {
        self.update(|state| {
            if !state.active(account) {
                return Err(ServiceError::Unauthorized);
            }
            Ok(usage_store::report_outcome(
                &mut state.usage,
                account,
                receipt,
                request,
                now,
            ))
        })
        .map_err(|_| UsageStoreError("usage storage unavailable".into()))
    }
    fn report_and_score(
        &self,
        account: &str,
        receipt: &str,
        request: &OutcomeReportRequest,
        now: u64,
    ) -> Option<Result<OutcomeReportResponse, ServiceError>> {
        Some(self.update(|state| {
            if !state.active(account) {
                return Err(ServiceError::Unauthorized);
            }
            let (recorded, outcome) =
                match usage_store::report_outcome(&mut state.usage, account, receipt, request, now)
                {
                    OutcomeReportResult::Recorded(outcome) => (true, outcome),
                    OutcomeReportResult::Duplicate(outcome) => (false, outcome),
                    OutcomeReportResult::InvalidVersions => {
                        return Err(ServiceError::InvalidInput("invalid_used_version_ids"));
                    }
                    OutcomeReportResult::NotFound => return Err(ServiceError::NotFound),
                    OutcomeReportResult::Conflict => return Err(ServiceError::Conflict),
                };
            let prior_events = state.scores.events.len();
            if request.outcome == MemoryOutcome::Success {
                for memory in outcome.used_memories {
                    let Some(author) = memory
                        .author_account_id
                        .filter(|author| state.active(author))
                    else {
                        continue;
                    };
                    let eligible =
                        state
                            .catalog
                            .memories
                            .get(&memory.memory_id)
                            .is_some_and(|row| {
                                row.versions.iter().any(|version| {
                                    super::version_id(&row.id, version.version) == memory.version_id
                                        && version.status != wam_gatekeeper::store::Status::Retired
                                })
                            });
                    if !eligible {
                        continue;
                    }
                    score_store::record_successful_use(
                        &mut state.scores,
                        SuccessfulUseAward {
                            beneficiary_account_id: author,
                            consumer_account_id: account.into(),
                            memory_id: memory.memory_id,
                            memory_version_id: memory.version_id,
                            receipt_id: receipt.into(),
                            created_at: outcome.reported_at,
                        },
                    )
                    .map_err(|_| ServiceError::Unavailable)?;
                }
            }
            let new_ids: Vec<_> = state.scores.events[prior_events..]
                .iter()
                .map(|event| event.id.clone())
                .collect();
            for id in new_ids {
                state.emit("score_awarded", &id, now);
            }
            if recorded {
                state.emit("outcome_recorded", receipt, now);
            }
            Ok(OutcomeReportResponse {
                receipt_id: receipt.into(),
                outcome: request.outcome,
                recorded,
            })
        }))
    }
    fn public_metrics(&self) -> Result<PublicUsageMetrics, UsageStoreError> {
        if let Some(result) = self.direct_public_metrics() {
            return result;
        }
        self.read(|state| Ok(usage_store::public_metrics(&state.usage)))
            .map_err(|_| UsageStoreError("usage storage unavailable".into()))
    }
}

impl ScoreStore for ApplicationRepository {
    fn award_successful_use(
        &self,
        award: SuccessfulUseAward,
    ) -> Result<ScoreAwardResult, ScoreStoreError> {
        self.update(|state| {
            if !state.active(&award.beneficiary_account_id)
                || !state.active(&award.consumer_account_id)
            {
                return Err(ServiceError::Unauthorized);
            }
            let backed = state.usage.receipts.iter().any(|receipt| {
                receipt.id == award.receipt_id
                    && receipt.owner_account == award.consumer_account_id
                    && receipt.outcome == Some(MemoryOutcome::Success)
                    && receipt.reported_at == Some(award.created_at)
                    && receipt.used_version_ids.contains(&award.memory_version_id)
                    && receipt.memory_uses.iter().any(|memory| {
                        memory.memory_id == award.memory_id
                            && memory.version_id == award.memory_version_id
                            && memory.author_account_id.as_deref()
                                == Some(award.beneficiary_account_id.as_str())
                    })
            });
            if !backed {
                return Err(ServiceError::InvalidInput("invalid_score_source"));
            }
            score_store::record_successful_use(&mut state.scores, award)
                .map_err(|_| ServiceError::Unavailable)
        })
        .map_err(|_| ScoreStoreError("score storage unavailable".into()))
    }
    fn total_points_for(&self, account: &str) -> Result<i64, ScoreStoreError> {
        self.read(|state| Ok(score_store::total_points_for(&state.scores, account)))
            .map_err(|_| ScoreStoreError("score storage unavailable".into()))
    }
    fn events_for_account(
        &self,
        account: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Option<ScoreEventListResponse>, ScoreStoreError> {
        self.read(|state| {
            score_store::events_for_account(&state.scores, account, before, limit)
                .map_err(|_| ServiceError::Unavailable)
        })
        .map_err(|_| ScoreStoreError("score storage unavailable".into()))
    }
}

pub(super) fn append_event(
    state: &mut super::state::AppState,
    beneficiary: &str,
    target: (&str, u32),
    event_type: &str,
    points: i64,
    key: String,
    now: u64,
) -> Result<(), ServiceError> {
    let (memory, version) = target;
    if state
        .scores
        .events
        .iter()
        .any(|event| event.idempotency_key == key)
    {
        return Ok(());
    }
    let id = format!("score-{:08}", state.scores.next_event_id);
    state.scores.next_event_id = state
        .scores
        .next_event_id
        .checked_add(1)
        .ok_or(ServiceError::Unavailable)?;
    state.scores.events.push(ScoreEvent {
        id: id.clone(),
        beneficiary_account_id: beneficiary.into(),
        consumer_account_id: String::new(),
        memory_id: memory.into(),
        memory_version_id: super::version_id(memory, version),
        source_receipt_id: String::new(),
        event_type: event_type.into(),
        points_delta: points,
        idempotency_key: key,
        created_at: now,
    });
    state.emit("score_changed", &id, now);
    Ok(())
}
