//! Use cases compose pure core rules with injected repository ports.

mod catalog_recall;
mod contributions;
mod profiles;

use crate::{
    ports::{
        ContributionStore, ContributionStoreError, OutcomeReportResult, ReceiptMemoryAttribution,
        Repository, RepositoryError, ReviewContribution as StoredReviewContribution, ScoreStore,
        SuccessfulUseAward, UsageStore,
    },
    profile_wire::{profile_response, validate_profile},
};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use wam_contracts::{
    CONTRIBUTION_CONSENT_VERSION, ContributionListResponse, ContributionRecordResponse,
    ContributionReviewDecision, CreateContributionRequest, DashboardResponse, LeaderboardEntry,
    LeaderboardResponse, MatchDetails, MemoryCatalogEntry, MemoryCatalogResponse,
    MemoryDetailResponse, OutcomeReportRequest, OutcomeReportResponse, ProfilePutRequest,
    ProfileResponse, PublicUsageMetrics, RecallHit, RecallRequest, RecallResponse,
    ReviewDecisionEvent, ReviewQueueCounts, ReviewQueueHealth, ReviewQueueItem,
    ReviewQueueResponse, SCHEMA_VERSION, ScoreEventListResponse, VocabResponse,
};
use wam_core::{
    leaderboard::{personal_public_rank, personal_rank, public_leaderboard},
    memory::MemoryStatus,
    profile::PublicHandle,
    recall::{RecallDocument, RecallQuery, recall_documents},
    url_scope::{CanonicalUrl, RouteCoverage, RouteMatchLevel, UrlMode, UrlPattern},
};
use wam_gatekeeper::queue::{AccountClass, JobKind, JobQueue, NewJob, Priority, QueueConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceError {
    InvalidInput(&'static str),
    NotFound,
    Conflict,
    Unauthorized,
    Unavailable,
    Internal,
}

pub struct WamService<R> {
    repository: R,
    contributions: Arc<dyn ContributionStore>,
    usage: Arc<dyn UsageStore>,
    scores: Arc<dyn ScoreStore>,
    queue: Mutex<JobQueue>,
    lifecycle: Option<crate::durable::ApplicationRepository>,
}

impl<R: Repository> WamService<R> {
    pub fn new(repository: R) -> Self {
        Self::with_contribution_store(
            repository,
            Arc::new(crate::contribution_store::InMemoryContributionStore::default()),
        )
        .expect("an empty in-memory contribution store restores an empty queue")
    }

    pub fn with_contribution_store(
        repository: R,
        contributions: Arc<dyn ContributionStore>,
    ) -> Result<Self, ContributionStoreError> {
        Self::with_stores(
            repository,
            contributions,
            Arc::new(crate::usage_store::InMemoryUsageStore::default()),
        )
    }

    pub fn with_stores(
        repository: R,
        contributions: Arc<dyn ContributionStore>,
        usage: Arc<dyn UsageStore>,
    ) -> Result<Self, ContributionStoreError> {
        Self::with_all_stores(
            repository,
            contributions,
            usage,
            Arc::new(crate::score_store::InMemoryScoreStore::default()),
        )
    }

    pub fn with_all_stores(
        repository: R,
        contributions: Arc<dyn ContributionStore>,
        usage: Arc<dyn UsageStore>,
        scores: Arc<dyn ScoreStore>,
    ) -> Result<Self, ContributionStoreError> {
        let mut queue = JobQueue::new(QueueConfig::default());
        for owned in if contributions.transactional_queue() { Vec::new() } else { contributions.all_for_queue()? } {
            queue
                .enqueue(
                    queue_job(&owned.account_id, &owned.record)?,
                    owned.record.submitted_at,
                )
                .map_err(|error| {
                    ContributionStoreError(format!(
                        "could not restore contribution queue: {error:?}"
                    ))
                })?;
        }
        Ok(Self {
            repository,
            contributions,
            usage,
            scores,
            queue: Mutex::new(queue),
            lifecycle: None,
        })
    }
}

fn review_queue_item(stored: StoredReviewContribution) -> ReviewQueueItem {
    ReviewQueueItem {
        record: stored.record,
        review_generation: stored.review_generation,
    }
}

fn queue_job(
    account_id: &str,
    record: &ContributionRecordResponse,
) -> Result<NewJob, ContributionStoreError> {
    let authority_and_path = record.url_pattern.strip_prefix("https://").ok_or_else(|| {
        ContributionStoreError("stored URL pattern is not canonical HTTPS".into())
    })?;
    let host = authority_and_path
        .split('/')
        .next()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| ContributionStoreError("stored URL pattern has no host".into()))?;
    let scope_key = format!("{host}|{}|{}", record.task_category, record.task_category);
    Ok(NewJob {
        kind: JobKind::Contribution,
        scope_key,
        contributor: account_id.to_owned(),
        device: format!("local-web:{account_id}"),
        class: AccountClass::Established,
        nonce: record.id.clone(),
        contribution_id: record.id.clone(),
        priority: Priority::P1,
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn parse_kind(raw: Option<&str>) -> Result<Option<wam_core::MemoryKind>, ServiceError> {
    match raw {
        None => Ok(None),
        Some("workflow") => Ok(Some(wam_core::MemoryKind::Workflow)),
        Some("site-note") => Ok(Some(wam_core::MemoryKind::SiteNote)),
        Some("gotcha") => Ok(Some(wam_core::MemoryKind::Gotcha)),
        Some(_) => Err(ServiceError::InvalidInput("invalid_kind")),
    }
}

fn memory_kind_name(kind: wam_core::MemoryKind) -> &'static str {
    match kind {
        wam_core::MemoryKind::Workflow => "workflow",
        wam_core::MemoryKind::SiteNote => "site-note",
        wam_core::MemoryKind::Gotcha => "gotcha",
    }
}

fn memory_status_name(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Active => "active",
        MemoryStatus::NeedsReverify => "needs-reverify",
        MemoryStatus::Superseded => "superseded",
        MemoryStatus::Retired => "retired",
    }
}

fn format_url_pattern(pattern: &UrlPattern) -> String {
    match pattern.coverage() {
        RouteCoverage::Exact => format!("https://{}{}", pattern.host(), pattern.path()),
        RouteCoverage::Host => format!("https://{}/**", pattern.host()),
        RouteCoverage::Subtree if pattern.path() == "/" => {
            format!("https://{}/**", pattern.host())
        }
        RouteCoverage::Subtree => format!("https://{}{}/**", pattern.host(), pattern.path()),
    }
}

fn route_name(route: RouteMatchLevel) -> &'static str {
    match route {
        RouteMatchLevel::Host => "host",
        RouteMatchLevel::Parent => "parent",
        RouteMatchLevel::Exact => "exact",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contribution_store::InMemoryContributionStore, memory_repository::InMemoryRepository,
    };
    use wam_contracts::{ContributionAction, CreateContributionRequest};

    fn request() -> CreateContributionRequest {
        CreateContributionRequest {
            url: "https://example.com/guide?private-sentinel=secret#top".into(),
            task_category: "compose-draft".into(),
            actions: vec![ContributionAction::Navigate, ContributionAction::Save],
            consent: true,
            consent_version: CONTRIBUTION_CONSENT_VERSION.into(),
        }
    }

    #[test]
    fn contributions_are_queued_rehydrated_and_canceled_on_withdrawal() {
        let store = Arc::new(InMemoryContributionStore::default());
        let service =
            WamService::with_contribution_store(InMemoryRepository::seeded(), store.clone())
                .expect("initialize queue");
        let record = service
            .create_contribution("demo-user", request())
            .expect("accept contribution");
        assert_eq!(service.pending_gatekeeper_jobs().unwrap(), 1);

        let restarted = WamService::with_contribution_store(InMemoryRepository::seeded(), store)
            .expect("rebuild queue from stored receipts");
        assert_eq!(restarted.pending_gatekeeper_jobs().unwrap(), 1);
        restarted
            .withdraw_contribution("demo-user", &record.id)
            .expect("withdraw contribution");
        assert_eq!(restarted.pending_gatekeeper_jobs().unwrap(), 0);
    }
}
