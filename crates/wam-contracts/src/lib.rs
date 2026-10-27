#![forbid(unsafe_code)]

//! Small versioned transport types. Core domain constructors remain the authority
//! for validation; server adapters convert these untrusted DTOs before use.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u8 = 1;
pub const CONTRIBUTION_CONSENT_VERSION: &str = "local-development-v1";

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallRequest {
    pub url: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub task_category: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct VocabResponse {
    pub schema_version: u8,
    pub version: String,
    pub task_categories: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct MatchDetails {
    pub route: String,
    pub task: bool,
    pub keyword_matches: u8,
    #[serde(default)]
    pub text_matches: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct RecallHit {
    pub memory_id: String,
    pub version_id: String,
    pub task_category: String,
    pub kind: String,
    pub hint: String,
    pub score: u16,
    pub matched_on: MatchDetails,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct RecallResponse {
    pub schema_version: u8,
    pub reason: String,
    pub hits: Vec<RecallHit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct MemoryDetailResponse {
    pub schema_version: u8,
    pub memory_id: String,
    pub version_id: String,
    pub level: u8,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct MemoryCatalogEntry {
    pub memory_id: String,
    pub version_id: String,
    pub title: String,
    pub summary: String,
    pub site: String,
    pub domain: String,
    pub task_category: String,
    pub kind: String,
    pub status: String,
    pub verified_date: String,
    pub canonical_url_patterns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct MemoryCatalogResponse {
    pub schema_version: u8,
    pub memories: Vec<MemoryCatalogEntry>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContributionAction {
    Navigate,
    Edit,
    Save,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateContributionRequest {
    pub url: String,
    pub task_category: String,
    pub actions: Vec<ContributionAction>,
    pub consent: bool,
    pub consent_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContributionRecordResponse {
    pub id: String,
    pub url_pattern: String,
    pub task_category: String,
    pub actions: Vec<ContributionAction>,
    pub status: String,
    pub submitted_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContributionListResponse {
    pub schema_version: u8,
    pub contributions: Vec<ContributionRecordResponse>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContributionReviewDecision {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContributionReviewDecisionRequest {
    pub review_generation: u64,
    pub decision: ContributionReviewDecision,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReviewQueueItem {
    pub record: ContributionRecordResponse,
    pub review_generation: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReviewQueueCounts {
    pub queued: usize,
    pub in_review: usize,
    pub approved: usize,
    pub rejected: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReviewQueueHealth {
    pub active_jobs: usize,
    pub oldest_age_secs: u64,
    pub alert: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReviewDecisionEvent {
    pub contribution_id: String,
    pub decision: String,
    pub review_generation: u64,
    pub decided_at: u64,
    pub reviewer: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReviewQueueResponse {
    pub schema_version: u8,
    pub counts: ReviewQueueCounts,
    pub queue_health: ReviewQueueHealth,
    pub contributions: Vec<ReviewQueueItem>,
    pub decision_history: Vec<ReviewDecisionEvent>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOutcome {
    Success,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutcomeReportRequest {
    pub outcome: MemoryOutcome,
    /// Versions actually used, selected from the owner's recall receipt.
    /// An omitted or empty list records the outcome without awarding points.
    #[serde(default)]
    pub used_version_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct OutcomeReportResponse {
    pub receipt_id: String,
    pub outcome: MemoryOutcome,
    pub recorded: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct PublicUsageMetrics {
    pub schema_version: u8,
    pub recall_requests: u64,
    pub authenticated_recall_requests: u64,
    pub anonymous_recall_requests: u64,
    pub matched_memory_hits: u64,
    pub detail_requests: u64,
    pub success_outcomes: u64,
    pub partial_outcomes: u64,
    pub failed_outcomes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum BioInline {
    Text { text: String },
    Strong { text: String },
    Emphasis { text: String },
    Link { text: String, url: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum BioBlock {
    Paragraph { content: Vec<BioInline> },
    Heading { level: u8, content: Vec<BioInline> },
    BulletList { items: Vec<Vec<BioInline>> },
    OrderedList { items: Vec<Vec<BioInline>> },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServiceLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfilePutRequest {
    pub handle: String,
    #[serde(default)]
    pub bio: Vec<BioBlock>,
    #[serde(default)]
    pub service_links: Vec<ServiceLink>,
    pub public_opt_in: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProfileResponse {
    pub schema_version: u8,
    pub handle: String,
    pub bio: Vec<BioBlock>,
    pub service_links: Vec<ServiceLink>,
    pub public_opt_in: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct LeaderboardEntry {
    pub rank: usize,
    pub handle: String,
    pub total_points: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct LeaderboardResponse {
    pub schema_version: u8,
    pub entries: Vec<LeaderboardEntry>,
}

/// Owner-facing projection: consumer identity and receipt IDs are never exposed.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ScoreEventSummary {
    pub id: String,
    pub event_type: String,
    pub points_delta: i64,
    pub memory_id: String,
    pub memory_version_id: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ScoreEventListResponse {
    pub schema_version: u8,
    pub events: Vec<ScoreEventSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct DashboardResponse {
    pub schema_version: u8,
    pub total_points: i64,
    pub overall_rank: Option<usize>,
    pub public_rank: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::{MatchDetails, OutcomeReportRequest, RecallRequest, RecallResponse};

    #[test]
    fn recall_request_defaults_optional_fields_and_rejects_unknown_fields() {
        let request: RecallRequest = serde_json::from_str(r#"{"url":"https://example.com/"}"#)
            .expect("minimal recall request");
        assert!(request.keywords.is_empty());
        assert_eq!(request.task_category, None);
        assert_eq!(request.kind, None);
        assert_eq!(request.limit, None);

        assert!(
            serde_json::from_str::<RecallRequest>(
                r#"{"url":"https://example.com/","free_text":"private"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_outcome_reports_do_not_imply_any_used_versions() {
        let request: OutcomeReportRequest =
            serde_json::from_str(r#"{"outcome":"success"}"#).unwrap();
        assert!(request.used_version_ids.is_empty());
        assert!(
            serde_json::from_str::<OutcomeReportRequest>(
                r#"{"outcome":"success","author_account_id":"someone"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn match_details_reads_older_payloads_without_text_matches() {
        let details: MatchDetails =
            serde_json::from_str(r#"{"route":"exact","task":true,"keyword_matches":1}"#)
                .expect("legacy match details");
        assert_eq!(details.text_matches, 0);
    }

    #[test]
    fn recall_response_omits_an_absent_receipt() {
        let response = RecallResponse {
            schema_version: 1,
            reason: "no_match".to_owned(),
            hits: Vec::new(),
            receipt_id: None,
        };
        let json = serde_json::to_value(response).expect("serialize response");
        assert!(json.get("receipt_id").is_none());
    }
}
