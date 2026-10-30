//! Narrow server ports. The HTTP layer depends on these interfaces instead of
//! database, OAuth, or filesystem implementations.

use wam_contracts::{
    ContributionAction, ContributionRecordResponse, ContributionReviewDecision,
    OutcomeReportRequest, PublicUsageMetrics, ScoreEventListResponse,
};
use wam_core::{
    keyword::{KeywordVocabulary, TaskVocabulary},
    leaderboard::RankingAccount,
    memory::MemoryIndex,
    profile::{CreatorProfile, PublicHandle},
    recall::RecallQuery,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRecord {
    pub index: MemoryIndex,
    pub title: String,
    pub summary: String,
    pub site: String,
    pub domain: String,
    pub verified_date: String,
    pub hint: String,
    pub detail: String,
    pub author_account_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSnapshot {
    pub profile: CreatorProfile,
    pub public_opt_in: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredMemoryVersion {
    pub memory_id: String,
    pub version_id: String,
    pub title: String,
    pub site: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct MemoryUsageAggregate {
    pub memory_id: String,
    pub version_id: String,
    pub utc_date: String,
    pub anonymous_recall_hits: u64,
    pub other_account_recall_hits: u64,
    pub anonymous_detail_views: u64,
    pub other_account_detail_views: u64,
    pub success_outcomes: u64,
    pub partial_outcomes: u64,
    pub failed_outcomes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryError {
    HandleTaken,
    AccountMissing,
    Unavailable,
    AccountDisabled,
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::HandleTaken => "profile handle is already in use",
            Self::AccountMissing => "account not found",
            Self::Unavailable => "profile storage unavailable",
            Self::AccountDisabled => "account is disabled",
        })
    }
}

impl std::error::Error for RepositoryError {}

pub trait Repository: Send + Sync + 'static {
    fn ranking_includes_ledger(&self) -> bool {
        false
    }
    fn try_memories(&self) -> Result<Vec<MemoryRecord>, RepositoryError> {
        Ok(self.memories())
    }
    /// Returns a complete candidate superset for this query. Adapters may use a
    /// database index, while the default preserves the core full-scan oracle.
    fn try_recall_candidates(
        &self,
        _query: &RecallQuery,
    ) -> Result<Vec<MemoryRecord>, RepositoryError> {
        self.try_memories()
    }
    fn try_keyword_vocabulary(&self) -> Result<KeywordVocabulary, RepositoryError> {
        Ok(self.keyword_vocabulary())
    }
    fn try_memory_by_id(&self, id: &str) -> Result<Option<MemoryRecord>, RepositoryError> {
        Ok(self
            .try_memories()?
            .into_iter()
            .find(|record| record.index.id() == id))
    }
    /// Returns a still-public immutable version. The default repository has only
    /// its current projection; durable repositories may retain superseded versions.
    fn try_memory_by_version(
        &self,
        id: &str,
        version_id: &str,
    ) -> Result<Option<MemoryRecord>, RepositoryError> {
        Ok(self
            .try_memory_by_id(id)?
            .filter(|record| record.index.version_id() == version_id))
    }
    fn try_ranking_accounts(&self) -> Result<Vec<RankingAccount>, RepositoryError> {
        Ok(self.ranking_accounts())
    }
    fn try_profile_by_handle(
        &self,
        handle: &PublicHandle,
    ) -> Result<Option<ProfileSnapshot>, RepositoryError> {
        Ok(self.profile_by_handle(handle))
    }
    fn try_profile_for_account(
        &self,
        account: &str,
    ) -> Result<Option<ProfileSnapshot>, RepositoryError> {
        Ok(self.profile_for_account(account))
    }
    fn try_authored_memory_versions(
        &self,
        account_id: &str,
    ) -> Result<Vec<AuthoredMemoryVersion>, RepositoryError> {
        Ok(self
            .try_memories()?
            .into_iter()
            .filter(|record| record.author_account_id.as_deref() == Some(account_id))
            .map(|record| AuthoredMemoryVersion {
                memory_id: record.index.id().to_owned(),
                version_id: record.index.version_id().to_owned(),
                title: record.title,
                site: record.site,
            })
            .collect())
    }
    fn memories(&self) -> Vec<MemoryRecord>;
    fn task_vocabulary(&self) -> TaskVocabulary;
    fn keyword_vocabulary(&self) -> KeywordVocabulary;
    fn ranking_accounts(&self) -> Vec<RankingAccount>;
    fn profile_by_handle(&self, handle: &PublicHandle) -> Option<ProfileSnapshot>;
    fn profile_for_account(&self, account_id: &str) -> Option<ProfileSnapshot>;
    fn ensure_account(&self, account_id: &str) -> Result<(), RepositoryError> {
        self.profile_for_account(account_id)
            .map(|_| ())
            .ok_or(RepositoryError::AccountMissing)
    }
    fn save_profile(
        &self,
        account_id: &str,
        profile: CreatorProfile,
        public_opt_in: bool,
    ) -> Result<(), RepositoryError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionStoreError(pub String);

impl std::fmt::Display for ContributionStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ContributionStoreError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedContribution {
    pub account_id: String,
    pub record: ContributionRecordResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewContribution {
    pub record: ContributionRecordResponse,
    pub review_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ReviewAuditRecord {
    pub contribution_id: String,
    pub decision: String,
    pub review_generation: u64,
    pub decided_at: u64,
    pub reviewer: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutcomeReportResult {
    Recorded(ReceiptOutcome),
    Duplicate(ReceiptOutcome),
    Conflict,
    NotFound,
    InvalidVersions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptOutcome {
    pub used_memories: Vec<ReceiptMemoryAttribution>,
    pub reported_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageStoreError(pub String);

impl std::fmt::Display for UsageStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UsageStoreError {}

/// Stores owner-scoped contribution receipts. Production adapters can replace the
/// local file adapter without changing the HTTP or application layers.
pub trait ContributionStore: Send + Sync + 'static {
    fn transactional_queue(&self) -> bool {
        false
    }
    fn create_idempotent(
        &self,
        account: &str,
        request: &wam_contracts::CreateContributionRequest,
        canonical_url: &str,
        now: u64,
    ) -> Result<ContributionRecordResponse, ContributionStoreError> {
        self.create(
            account,
            canonical_url.into(),
            request.task_category.clone(),
            request.actions.clone(),
            now,
        )
    }
    fn queue_health(
        &self,
        _now: u64,
    ) -> Result<Option<wam_contracts::ReviewQueueHealth>, ContributionStoreError> {
        Ok(None)
    }
    fn create(
        &self,
        account_id: &str,
        url_pattern: String,
        task_category: String,
        actions: Vec<ContributionAction>,
        submitted_at: u64,
    ) -> Result<ContributionRecordResponse, ContributionStoreError>;

    fn list_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<ContributionRecordResponse>, ContributionStoreError>;

    fn all_for_queue(&self) -> Result<Vec<OwnedContribution>, ContributionStoreError>;

    fn all_for_review(&self) -> Result<Vec<ReviewContribution>, ContributionStoreError>;

    fn claim_for_review(
        &self,
        id: &str,
    ) -> Result<Option<ReviewContribution>, ContributionStoreError>;

    fn decide_for_review(
        &self,
        id: &str,
        review_generation: u64,
        decision: ContributionReviewDecision,
        reviewer: &str,
        decided_at: u64,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError>;

    fn review_audit_history(&self) -> Result<Vec<ReviewAuditRecord>, ContributionStoreError>;

    fn get_for_account(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError>;

    fn withdraw_for_account(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<bool, ContributionStoreError>;
}

/// Stores aggregate usage counters and short-lived owner-bound recall receipts.
/// Implementations must never store the request URL or page content.
pub trait UsageStore: Send + Sync + 'static {
    /// Durable adapters may commit outcome, quotas, score ledger and outbox together.
    fn report_and_score(
        &self,
        _account: &str,
        _receipt: &str,
        _request: &OutcomeReportRequest,
        _now: u64,
    ) -> Option<Result<wam_contracts::OutcomeReportResponse, crate::application::ServiceError>>
    {
        None
    }
    fn record_recall(
        &self,
        account_id: Option<&str>,
        memories: &[ReceiptMemoryAttribution],
        issued_at: u64,
    ) -> Result<Option<String>, UsageStoreError>;

    fn record_detail(
        &self,
        memory: &ReceiptMemoryAttribution,
        account_id: Option<&str>,
        viewed_at: u64,
    ) -> Result<(), UsageStoreError>;

    fn memory_usage(
        &self,
        _version_ids: &[String],
        _from_date: &str,
        _to_date: &str,
    ) -> Result<Vec<MemoryUsageAggregate>, UsageStoreError> {
        Ok(Vec::new())
    }

    /// Atomically validates receipt membership and freezes the outcome and used versions.
    /// Duplicate reports return the original selection and timestamp for safe score retries.
    fn report_outcome(
        &self,
        account_id: &str,
        receipt_id: &str,
        request: &OutcomeReportRequest,
        reported_at: u64,
    ) -> Result<OutcomeReportResult, UsageStoreError>;

    fn public_metrics(&self) -> Result<PublicUsageMetrics, UsageStoreError>;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ReceiptMemoryAttribution {
    pub memory_id: String,
    pub version_id: String,
    pub author_account_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessfulUseAward {
    pub beneficiary_account_id: String,
    pub consumer_account_id: String,
    pub memory_id: String,
    pub memory_version_id: String,
    pub receipt_id: String,
    pub created_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreAwardResult {
    Recorded,
    Duplicate,
    DailyLimit,
    SelfUse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreStoreError(pub String);

impl std::fmt::Display for ScoreStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ScoreStoreError {}

/// Append-only score events. Totals are derived from event deltas.
pub trait ScoreStore: Send + Sync + 'static {
    fn award_successful_use(
        &self,
        award: SuccessfulUseAward,
    ) -> Result<ScoreAwardResult, ScoreStoreError>;

    fn total_points_for(&self, account_id: &str) -> Result<i64, ScoreStoreError>;

    /// Returns only this beneficiary's events, in reverse append order.
    /// A foreign or unknown cursor is indistinguishable and returns None.
    fn events_for_account(
        &self,
        account_id: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Option<ScoreEventListResponse>, ScoreStoreError>;
}
