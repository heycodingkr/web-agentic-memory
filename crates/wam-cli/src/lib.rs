#![forbid(unsafe_code)]

//! CLI parsing and output are kept separate from network transport.

pub mod account;
pub mod browser;
pub mod guide;
pub mod install;
pub mod local;

use std::io::Read;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::to_string_pretty;
use wam_client::{ApiError, DiagnosticOperation, WamApi};
use wam_contracts::{
    CONTRIBUTION_CONSENT_VERSION, ContributionAction, ContributionListResponse,
    ContributionRecordResponse, CreateContributionRequest, DashboardResponse, LeaderboardResponse,
    MemoryDetailResponse, MemoryOutcome, OutcomeReportRequest, OutcomeReportResponse,
    OwnerUsageResponse, RecallCacheStatus, RecallRequest, RecallResponse, VocabResponse,
};

#[derive(Clone, Debug, Parser)]
#[command(name = "wam", version, about = "Web Agentic Memory client")]
pub struct Cli {
    #[arg(
        long,
        global = true,
        env = "WAM_ENDPOINT",
        default_value = "http://127.0.0.1:3000"
    )]
    pub endpoint: String,
    /// Skip saved credentials and WAM_TOKEN for public reads.
    #[arg(long, global = true)]
    pub anonymous: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Print or explicitly update WAM recall instructions without connecting to a server.
    Guide {
        #[command(subcommand)]
        action: guide::GuideAction,
    },
    /// Register WAM through the host's own CLI.
    Install {
        #[arg(value_enum)]
        host: install::Host,
        #[arg(long)]
        local: bool,
        #[arg(long, value_enum, default_value = "user")]
        scope: install::Scope,
        #[arg(long)]
        mcp_bin: Option<std::path::PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove only WAM's host registration; credentials and cache remain shared.
    Uninstall {
        #[arg(value_enum)]
        host: install::Host,
        #[arg(long, value_enum, default_value = "user")]
        scope: install::Scope,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Authorize this CLI and local MCP bridge through the web device flow.
    Link {
        #[arg(long)]
        no_browser: bool,
        #[arg(long)]
        json: bool,
    },
    /// Revoke this device and remove its saved credential.
    Unlink {
        /// Remove the local credential while leaving server revocation to web settings.
        #[arg(long)]
        local_only: bool,
        #[arg(long)]
        json: bool,
    },
    /// Check API reachability, saved authentication, and local cache.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Vocab {
        #[arg(long)]
        json: bool,
    },
    Recall {
        #[arg(required_unless_present = "stdin")]
        url: Option<String>,
        /// Read one recall request JSON object without putting its URL in process arguments.
        #[arg(long, conflicts_with_all = ["url", "keywords", "task", "kind", "limit"])]
        stdin: bool,
        #[arg(long = "keyword")]
        keywords: Vec<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = 3)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Keep a browser MCP connection alive and add an opt-in current-tab recall tool.
    BrowserMcp {
        #[arg(long, env = "WAM_BROWSER_MCP")]
        browser_mcp: std::path::PathBuf,
        #[arg(long = "browser-arg", allow_hyphen_values = true)]
        browser_args: Vec<String>,
    },
    Expand {
        id: String,
        #[arg(long, default_value_t = 2)]
        level: u8,
        #[arg(long)]
        version_id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Leaderboard {
        #[arg(long)]
        json: bool,
    },
    Rank {
        #[arg(long)]
        json: bool,
    },
    Usage {
        #[arg(long)]
        json: bool,
    },
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    Propose {
        url: String,
        #[arg(long, value_enum)]
        task: ContributionTask,
        #[arg(long = "action", value_enum, required = true, num_args = 1..=8)]
        actions: Vec<ContributionActionArg>,
        #[arg(long, required = true, action = clap::ArgAction::SetTrue)]
        consent: bool,
        /// Reuse this id for retries of the same contribution.
        #[arg(long)]
        nonce: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Contributions {
        #[arg(long)]
        json: bool,
    },
    Withdraw {
        id: String,
        #[arg(long)]
        json: bool,
    },
    Outcome {
        receipt_id: String,
        #[arg(value_enum)]
        outcome: OutcomeArg,
        /// Version actually used from the recall receipt; repeat for up to 3 versions.
        #[arg(long = "used-version")]
        used_version_ids: Vec<String>,
        #[arg(long)]
        json: bool,
    },
}

pub const MAX_STDIN_BYTES: usize = 16 * 1024;

impl Command {
    /// Resolve stdin before constructing a network client. Errors never include the body.
    pub fn read_input(&mut self, reader: impl Read) -> Result<(), ApiError> {
        if let Self::Recall {
            url,
            stdin,
            keywords,
            task,
            kind,
            limit,
            ..
        } = self
        {
            if *stdin {
                let mut bytes = Vec::new();
                reader
                    .take((MAX_STDIN_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|_| ApiError::InvalidInput)?;
                if bytes.len() > MAX_STDIN_BYTES {
                    return Err(ApiError::InvalidInput);
                }
                let request: RecallRequest =
                    serde_json::from_slice(&bytes).map_err(|_| ApiError::InvalidInput)?;
                if !matches!(request.limit, None | Some(1..=3)) {
                    return Err(ApiError::InvalidInput);
                }
                *url = Some(request.url);
                *keywords = request.keywords;
                *task = request.task_category;
                *kind = request.kind;
                *limit = request.limit.unwrap_or(3);
                *stdin = false;
            }
        }
        Ok(())
    }

    pub fn diagnostic_operation(&self) -> DiagnosticOperation {
        match self {
            Self::Recall { .. } => DiagnosticOperation::Recall,
            Self::Expand { .. } => DiagnosticOperation::Expand,
            Self::Vocab { .. } => DiagnosticOperation::Vocab,
            Self::Leaderboard { .. } => DiagnosticOperation::Catalog,
            Self::Rank { .. } | Self::Status { .. } => DiagnosticOperation::Dashboard,
            Self::Usage { .. } => DiagnosticOperation::Usage,
            Self::Propose { .. } | Self::Contributions { .. } | Self::Withdraw { .. } => {
                DiagnosticOperation::Contribution
            }
            Self::Outcome { .. } => DiagnosticOperation::Outcome,
            Self::Link { .. } | Self::Unlink { .. } | Self::Doctor { .. } => {
                DiagnosticOperation::Device
            }
            Self::Cache { .. }
            | Self::BrowserMcp { .. }
            | Self::Guide { .. }
            | Self::Install { .. }
            | Self::Uninstall { .. } => DiagnosticOperation::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Subcommand)]
pub enum CacheAction {
    Status,
    Clear,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ContributionTask {
    #[value(name = "compose-draft")]
    ComposeDraft,
    Navigate,
    Research,
}

impl ContributionTask {
    fn code(self) -> &'static str {
        match self {
            Self::ComposeDraft => "compose-draft",
            Self::Navigate => "navigate",
            Self::Research => "research",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, ValueEnum)]
pub enum ContributionActionArg {
    Navigate,
    Edit,
    Save,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutcomeArg {
    Success,
    Partial,
    Failed,
}

impl From<OutcomeArg> for MemoryOutcome {
    fn from(value: OutcomeArg) -> Self {
        match value {
            OutcomeArg::Success => Self::Success,
            OutcomeArg::Partial => Self::Partial,
            OutcomeArg::Failed => Self::Failed,
        }
    }
}

impl From<ContributionActionArg> for ContributionAction {
    fn from(value: ContributionActionArg) -> Self {
        match value {
            ContributionActionArg::Navigate => Self::Navigate,
            ContributionActionArg::Edit => Self::Edit,
            ContributionActionArg::Save => Self::Save,
        }
    }
}

pub async fn execute(api: &dyn WamApi, command: Command) -> Result<String, ApiError> {
    match command {
        Command::BrowserMcp { .. } => Err(ApiError::InvalidInput),
        Command::Guide { .. }
        | Command::Link { .. }
        | Command::Unlink { .. }
        | Command::Doctor { .. }
        | Command::Install { .. }
        | Command::Uninstall { .. } => Err(ApiError::InvalidInput),
        Command::Status { json } => {
            let status = api.memory_status().await?;
            if json {
                render_json(&status)
            } else {
                Ok(format!(
                    "WAM API is reachable\nEndpoint: {}\nAuthentication: {:?}\nContribution consent: {:?}",
                    status.endpoint, status.auth, status.consent
                ))
            }
        }
        Command::Vocab { json } => {
            let response = api.vocab().await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_vocab(&response))
            }
        }
        Command::Recall {
            url,
            stdin,
            keywords,
            task,
            kind,
            limit,
            json,
        } => {
            if stdin || !(1..=3).contains(&limit) {
                return Err(ApiError::InvalidInput);
            }
            let response = api
                .recall(RecallRequest {
                    url: url.ok_or(ApiError::InvalidInput)?,
                    keywords,
                    task_category: task,
                    kind,
                    limit: Some(limit),
                })
                .await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_recall(&response))
            }
        }
        Command::Expand {
            id,
            level,
            version_id,
            json,
        } => {
            validate_expand_level(level)?;
            let response = api
                .expand_version(&id, level, version_id.as_deref())
                .await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_memory(&response))
            }
        }
        Command::Leaderboard { json } => {
            let response = api.leaderboard().await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_leaderboard(&response))
            }
        }
        Command::Rank { json } => {
            let response = api.dashboard().await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_dashboard(&response))
            }
        }
        Command::Usage { json } => {
            let response = api.my_usage().await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_usage(&response))
            }
        }
        Command::Cache {
            action: CacheAction::Status,
        } => {
            let status = api.cache_status().await?;
            Ok(format!(
                "Local anonymous cache\nEntries: {} / {}\nPayload bytes: {} / {}",
                status.entries, status.max_entries, status.bytes, status.max_bytes
            ))
        }
        Command::Cache {
            action: CacheAction::Clear,
        } => {
            api.clear_cache().await?;
            Ok("Local cache cleared.".to_owned())
        }
        Command::Propose {
            url,
            task,
            actions,
            consent,
            nonce,
            json,
        } => {
            if !consent {
                return Err(ApiError::InvalidInput);
            }
            let actions = contribution_actions(actions)?;
            let response = api
                .submit_contribution(CreateContributionRequest {
                    url,
                    task_category: task.code().to_owned(),
                    actions,
                    consent,
                    consent_version: CONTRIBUTION_CONSENT_VERSION.to_owned(),
                    nonce,
                })
                .await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_contribution(&response))
            }
        }
        Command::Contributions { json } => {
            let response = api.my_contributions().await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_contributions(&response))
            }
        }
        Command::Withdraw { id, json } => {
            api.withdraw_contribution(&id).await?;
            if json {
                render_json(&serde_json::json!({
                    "schema_version": 1,
                    "id": id,
                    "withdrawn": true
                }))
            } else {
                Ok(format!("Contribution {id} withdrawn"))
            }
        }
        Command::Outcome {
            receipt_id,
            outcome,
            used_version_ids,
            json,
        } => {
            validate_receipt_id(&receipt_id)?;
            let response = api
                .report_outcome(
                    &receipt_id,
                    OutcomeReportRequest {
                        outcome: outcome.into(),
                        used_version_ids,
                    },
                )
                .await?;
            if json {
                render_json(&response)
            } else {
                Ok(render_outcome(&response))
            }
        }
    }
}

fn contribution_actions(
    actions: Vec<ContributionActionArg>,
) -> Result<Vec<ContributionAction>, ApiError> {
    if actions.is_empty()
        || actions.len() > 8
        || actions
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != actions.len()
    {
        return Err(ApiError::InvalidInput);
    }
    Ok(actions.into_iter().map(Into::into).collect())
}

fn render_contribution(value: &ContributionRecordResponse) -> String {
    format!(
        "{} | {} | {} / {}",
        value.id, value.status, value.url_pattern, value.task_category
    )
}

fn render_contributions(value: &ContributionListResponse) -> String {
    if value.contributions.is_empty() {
        return "No contributions yet".to_owned();
    }
    value
        .contributions
        .iter()
        .map(render_contribution)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_outcome(value: &OutcomeReportResponse) -> String {
    if value.recorded {
        format!(
            "Outcome {} recorded for receipt {}",
            outcome_name(value.outcome),
            value.receipt_id
        )
    } else {
        format!(
            "Outcome {} was already recorded for receipt {}",
            outcome_name(value.outcome),
            value.receipt_id
        )
    }
}

fn outcome_name(outcome: MemoryOutcome) -> &'static str {
    match outcome {
        MemoryOutcome::Success => "success",
        MemoryOutcome::Partial => "partial",
        MemoryOutcome::Failed => "failed",
    }
}

fn render_json<T: serde::Serialize>(value: &T) -> Result<String, ApiError> {
    to_string_pretty(value).map_err(|_| ApiError::InvalidResponse)
}

fn render_vocab(value: &VocabResponse) -> String {
    format!(
        "Vocabulary {}\nTasks: {}\nKeywords: {}",
        value.version,
        list_or_none(&value.task_categories),
        list_or_none(&value.keywords),
    )
}

fn render_recall(value: &RecallResponse) -> String {
    if value.hits.is_empty() {
        let mut result = format!("No memories found ({})", value.reason);
        if let Some(status) = &value.cache_status {
            result.push_str(&format!("\nCache: {}", cache_status_label(status)));
        }
        return result;
    }
    let mut lines: Vec<_> = value
        .hits
        .iter()
        .map(|hit| {
            format!(
                "{} @ {} | {} / {} | score {} | {}",
                hit.memory_id, hit.version_id, hit.task_category, hit.kind, hit.score, hit.hint
            )
        })
        .collect();
    if let Some(receipt_id) = &value.receipt_id {
        lines.push(format!("Receipt: {receipt_id}"));
    }
    if let Some(status) = &value.cache_status {
        lines.push(format!("Cache: {}", cache_status_label(status)));
    }
    lines.join("\n")
}

fn cache_status_label(status: &RecallCacheStatus) -> &'static str {
    match status {
        RecallCacheStatus::Network => "network",
        RecallCacheStatus::Fresh => "local fresh hit",
        RecallCacheStatus::Stale => "local stale fallback",
    }
}

fn render_memory(value: &MemoryDetailResponse) -> String {
    format!(
        "{} @ {} (level {})\n{}",
        value.memory_id, value.version_id, value.level, value.content
    )
}

fn render_leaderboard(value: &LeaderboardResponse) -> String {
    if value.entries.is_empty() {
        return "Leaderboard is empty".to_owned();
    }
    value
        .entries
        .iter()
        .map(|entry| {
            format!(
                "{}. @{} — {} points",
                entry.rank, entry.handle, entry.total_points
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_dashboard(value: &DashboardResponse) -> String {
    let overall_rank = value
        .overall_rank
        .map(|rank| format!("#{rank}"))
        .unwrap_or_else(|| "—".to_owned());
    let public_rank = value
        .public_rank
        .map(|rank| format!("#{rank}"))
        .unwrap_or_else(|| "Private".to_owned());
    format!(
        "Points: {}\nOverall rank: {}\nPublic rank: {}",
        value.total_points, overall_rank, public_rank
    )
}

fn render_usage(value: &OwnerUsageResponse) -> String {
    let requests: u64 = value.daily.iter().map(|day| day.requests).sum();
    let units: u64 = value.daily.iter().map(|day| day.units).sum();
    let denied_requests: u64 = value.daily.iter().map(|day| day.denied_requests).sum();
    format!(
        "API quota: {} weighted units/minute\nBurst remaining: {} / {}\nRefills in: {} seconds\nLast {} days: {} requests, {} units, {} rate limited requests",
        value.limit_per_minute,
        value.remaining_units,
        value.burst_capacity,
        value.resets_in_seconds,
        value.period_days,
        requests,
        units,
        denied_requests,
    )
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_owned()
    } else {
        values.join(", ")
    }
}

fn validate_expand_level(level: u8) -> Result<(), ApiError> {
    if matches!(level, 1 | 2) {
        Ok(())
    } else {
        Err(ApiError::InvalidInput)
    }
}

fn validate_receipt_id(receipt_id: &str) -> Result<(), ApiError> {
    if receipt_id.is_empty() || receipt_id.chars().count() > 128 {
        Err(ApiError::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_requires_safe_explicit_url_and_parses_filters() {
        let cli = Cli::try_parse_from([
            "wam",
            "recall",
            "https://example.com/guide",
            "--keyword",
            "blog",
            "--task",
            "compose-draft",
        ])
        .unwrap();
        match cli.command {
            Command::Recall {
                url,
                keywords,
                task,
                limit,
                ..
            } => {
                assert_eq!(url.as_deref(), Some("https://example.com/guide"));
                assert_eq!(keywords, vec!["blog"]);
                assert_eq!(task.as_deref(), Some("compose-draft"));
                assert_eq!(limit, 3);
            }
            _ => panic!("expected recall subcommand"),
        }
    }

    #[test]
    fn usage_text_includes_rate_limited_request_count() {
        let response = OwnerUsageResponse {
            schema_version: 1,
            period_days: 30,
            limit_per_minute: 300,
            burst_capacity: 60,
            remaining_units: 42,
            resets_in_seconds: 4,
            daily: vec![wam_contracts::UsageDailyPoint {
                utc_date: "2026-10-27".to_owned(),
                requests: 3,
                units: 5,
                recall_requests: 1,
                detail_requests: 1,
                other_requests: 1,
                denied_requests: 2,
            }],
        };

        let rendered = render_usage(&response);

        assert!(rendered.contains("2 rate limited requests"));
    }

    #[test]
    fn propose_requires_explicit_consent_and_closed_task_and_action_values() {
        assert!(
            Cli::try_parse_from([
                "wam",
                "propose",
                "https://example.com/guide",
                "--task",
                "compose-draft",
                "--action",
                "navigate"
            ])
            .is_err()
        );

        let parsed = Cli::try_parse_from([
            "wam",
            "propose",
            "https://example.com/guide",
            "--task",
            "compose-draft",
            "--action",
            "navigate",
            "--action",
            "edit",
            "--consent",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Command::Propose { consent: true, .. }
        ));
        assert!(
            Cli::try_parse_from([
                "wam",
                "propose",
                "https://example.com/guide",
                "--task",
                "unlisted",
                "--action",
                "navigate",
                "--consent"
            ])
            .is_err()
        );
        assert_eq!(
            contribution_actions(vec![
                ContributionActionArg::Navigate,
                ContributionActionArg::Navigate
            ]),
            Err(ApiError::InvalidInput)
        );
    }

    #[test]
    fn contribution_human_renderers_show_only_canonical_receipt_fields() {
        let record = ContributionRecordResponse {
            id: "local-000001".to_owned(),
            url_pattern: "https://example.com/guide".to_owned(),
            task_category: "compose-draft".to_owned(),
            actions: vec![ContributionAction::Navigate, ContributionAction::Save],
            status: "received".to_owned(),
            submitted_at: 1,
            progress: None,
            reflection: None,
        };
        assert_eq!(
            render_contribution(&record),
            "local-000001 | received | https://example.com/guide / compose-draft"
        );
        assert_eq!(
            render_contributions(&ContributionListResponse {
                schema_version: 1,
                contributions: vec![record],
            }),
            "local-000001 | received | https://example.com/guide / compose-draft"
        );
    }

    #[test]
    fn cli_rejects_missing_subcommand_and_invalid_level_at_execution_boundary() {
        assert!(Cli::try_parse_from(["wam"]).is_err());
        assert_eq!(validate_expand_level(3), Err(ApiError::InvalidInput));
        assert!(validate_expand_level(1).is_ok());
    }

    #[test]
    fn outcome_command_has_a_closed_enum_and_clear_idempotent_output() {
        let cli = Cli::try_parse_from([
            "wam",
            "outcome",
            "receipt-1",
            "success",
            "--used-version",
            "draft-v1",
            "--used-version",
            "note-v2",
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Outcome {
                ref receipt_id,
                outcome: OutcomeArg::Success,
                ref used_version_ids,
                json: true,
            } if receipt_id == "receipt-1" && used_version_ids == &["draft-v1", "note-v2"]
        ));
        assert!(Cli::try_parse_from(["wam", "outcome", "receipt-1", "succeeded"]).is_err());
        assert!(validate_receipt_id("receipt-1").is_ok());
        assert_eq!(validate_receipt_id(""), Err(ApiError::InvalidInput));
        assert_eq!(
            validate_receipt_id(&"r".repeat(129)),
            Err(ApiError::InvalidInput)
        );
        assert_eq!(
            render_outcome(&OutcomeReportResponse {
                receipt_id: "receipt-1".into(),
                outcome: MemoryOutcome::Success,
                recorded: true,
            }),
            "Outcome success recorded for receipt receipt-1"
        );
        assert_eq!(
            render_outcome(&OutcomeReportResponse {
                receipt_id: "receipt-1".into(),
                outcome: MemoryOutcome::Success,
                recorded: false,
            }),
            "Outcome success was already recorded for receipt receipt-1"
        );
    }

    #[test]
    fn recall_text_exposes_receipt_and_version_for_reporting_usage() {
        let mut response = RecallResponse {
            schema_version: 1,
            reason: "ok".to_owned(),
            hits: vec![wam_contracts::RecallHit {
                memory_id: "web:example-draft".to_owned(),
                version_id: "draft-v1".to_owned(),
                task_category: "compose-draft".to_owned(),
                kind: "workflow".to_owned(),
                hint: "Save the draft.".to_owned(),
                score: 10,
                matched_on: wam_contracts::MatchDetails {
                    route: "exact".to_owned(),
                    task: true,
                    keyword_matches: 1,
                    text_matches: 0,
                },
            }],
            receipt_id: Some("local-use-000001".to_owned()),
            cache_status: None,
        };
        let text = render_recall(&response);
        assert!(text.contains("web:example-draft @ draft-v1"));
        assert!(text.contains("Receipt: local-use-000001"));
        response.receipt_id = None;
        assert!(!render_recall(&response).contains("Receipt:"));
    }

    #[test]
    fn human_renderers_are_readable_and_json_renderer_stays_machine_parseable() {
        let cli = Cli::try_parse_from(["wam", "rank"]).unwrap();
        assert!(matches!(cli.command, Command::Rank { json: false }));
        let usage = Cli::try_parse_from(["wam", "usage"]).unwrap();
        assert!(matches!(usage.command, Command::Usage { json: false }));
        let cache = Cli::try_parse_from(["wam", "cache", "status"]).unwrap();
        assert!(matches!(
            cache.command,
            Command::Cache {
                action: CacheAction::Status
            }
        ));

        let dashboard = DashboardResponse {
            schema_version: 1,
            total_points: 120,
            overall_rank: Some(7),
            public_rank: Some(4),
        };
        assert_eq!(
            render_dashboard(&dashboard),
            "Points: 120\nOverall rank: #7\nPublic rank: #4"
        );

        let vocab = VocabResponse {
            schema_version: 1,
            version: "2026-01".to_owned(),
            task_categories: vec!["compose-draft".to_owned()],
            keywords: vec!["blog".to_owned(), "research".to_owned()],
        };
        assert_eq!(
            render_vocab(&vocab),
            "Vocabulary 2026-01\nTasks: compose-draft\nKeywords: blog, research"
        );
        assert!(serde_json::from_str::<VocabResponse>(&render_json(&vocab).unwrap()).is_ok());

        let recall = RecallResponse {
            schema_version: 1,
            reason: "no_match".to_owned(),
            hits: Vec::new(),
            receipt_id: None,
            cache_status: None,
        };
        assert_eq!(render_recall(&recall), "No memories found (no_match)");

        let memory = MemoryDetailResponse {
            schema_version: 1,
            memory_id: "mem-1".to_owned(),
            version_id: "ver-1".to_owned(),
            level: 2,
            content: "Use typed boundaries.".to_owned(),
        };
        assert_eq!(
            render_memory(&memory),
            "mem-1 @ ver-1 (level 2)\nUse typed boundaries."
        );

        let leaderboard = LeaderboardResponse {
            schema_version: 1,
            entries: vec![wam_contracts::LeaderboardEntry {
                rank: 2,
                handle: "alice".to_owned(),
                total_points: 45,
            }],
        };
        assert_eq!(render_leaderboard(&leaderboard), "2. @alice — 45 points");
    }
}
