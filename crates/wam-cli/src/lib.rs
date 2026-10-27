#![forbid(unsafe_code)]

//! CLI parsing and output are kept separate from network transport.

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::to_string_pretty;
use wam_client::{ApiError, WamApi};
use wam_contracts::{
    CONTRIBUTION_CONSENT_VERSION, ContributionAction, ContributionListResponse,
    ContributionRecordResponse, CreateContributionRequest, DashboardResponse, LeaderboardResponse,
    MemoryDetailResponse, MemoryOutcome, OutcomeReportRequest, OutcomeReportResponse,
    RecallRequest, RecallResponse, VocabResponse,
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
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    Status {
        #[arg(long)]
        json: bool,
    },
    Vocab {
        #[arg(long)]
        json: bool,
    },
    Recall {
        url: String,
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
    Expand {
        id: String,
        #[arg(long, default_value_t = 2)]
        level: u8,
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
    Propose {
        url: String,
        #[arg(long, value_enum)]
        task: ContributionTask,
        #[arg(long = "action", value_enum, required = true, num_args = 1..=8)]
        actions: Vec<ContributionActionArg>,
        #[arg(long, required = true, action = clap::ArgAction::SetTrue)]
        consent: bool,
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
        Command::Status { json } => {
            api.status().await?;
            if json {
                Ok(r#"{"schema_version":1,"status":"ok"}"#.to_owned())
            } else {
                Ok("WAM API is reachable".to_owned())
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
            keywords,
            task,
            kind,
            limit,
            json,
        } => {
            let response = api
                .recall(RecallRequest {
                    url,
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
        Command::Expand { id, level, json } => {
            validate_expand_level(level)?;
            let response = api.expand(&id, level).await?;
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
        Command::Propose {
            url,
            task,
            actions,
            consent,
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
        return format!("No memories found ({})", value.reason);
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
    lines.join("\n")
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
                assert_eq!(url, "https://example.com/guide");
                assert_eq!(keywords, vec!["blog"]);
                assert_eq!(task.as_deref(), Some("compose-draft"));
                assert_eq!(limit, 3);
            }
            _ => panic!("expected recall subcommand"),
        }
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
