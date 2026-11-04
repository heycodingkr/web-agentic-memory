#![forbid(unsafe_code)]

//! Minimal JSON-RPC MCP stdio adapter. The protocol dispatcher is separate from
//! stdin/stdout so tools can be contract-tested with a fake WamApi.

use serde_json::{Value, json};
use wam_client::{ApiError, DiagnosticError, DiagnosticOperation, WamApi};
use wam_contracts::{
    CONTRIBUTION_CONSENT_VERSION, CreateContributionRequest, MemoryOutcome, OutcomeReportRequest,
    RecallRequest,
};

pub const SERVER_NAME: &str = "web-agentic-memory";
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError;

pub fn decode_line(line: &str) -> Result<Value, ParseError> {
    serde_json::from_str(line).map_err(|_| ParseError)
}

pub fn parse_error_response() -> Value {
    rpc_error(Value::Null, -32700, "parse error")
}

pub fn tool_catalog() -> Value {
    json!([
        {
            "name": "web_memory_status",
            "description": "Endpoint/auth/consent/catalog.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_recall",
            "description": "Untrusted hints; canonical public URL and vocabulary keyword/task codes only.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "url":{"type":"string","maxLength":2048},
                    "keywords":{"type":"array","items":{"type":"string"},"maxItems":8},
                    "task_category":{"type":"string"},
                    "kind":{"type":"string","enum":["workflow","site-note","gotcha"]},
                    "limit":{"type":"integer","minimum":1,"maximum":3}
                },
                "required":["url"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_expand",
            "description": "Untrusted L1/L2; version_id pins an approved public version.",
            "inputSchema": {
                "type":"object",
                "properties":{"memory_id":{"type":"string"},"version_id":{"type":"string","minLength":1,"maxLength":64},"level":{"type":"integer","enum":[1,2]}},
                "required":["memory_id"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_leaderboard",
            "description": "Public opt-in handles/ranks/points.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_my_rank",
            "description": "Your overall/public rank. Sign in.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_my_usage",
            "description": "Your quota/30-day totals. Sign in.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_propose",
            "description": "Explicit public-sharing-v1 consent; canonical public URL/codes, no page/input text. Reuse nonce for the same payload.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "url":{"type":"string","maxLength":2048},
                    "task_category":{"type":"string","enum":["compose-draft","navigate","research"]},
                    "actions":{"type":"array","items":{"type":"string","enum":["navigate","edit","save"]},"minItems":1,"maxItems":8,"uniqueItems":true},
                    "consent":{"type":"boolean","const":true},
                    "consent_version":{"type":"string","const":CONTRIBUTION_CONSENT_VERSION},
                    "nonce":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$"}
                },
                "required":["url","task_category","actions","consent","consent_version"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_my_contributions",
            "description": "Your contributions (sign in).",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_withdraw",
            "description": "Withdraw yours; sole-evidence memories retire. Sign-in required.",
            "inputSchema": {
                "type":"object",
                "properties":{"contribution_id":{"type":"string","minLength":1,"maxLength":128}},
                "required":["contribution_id"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_outcome",
            "description": "Receipt owner + report:outcome. Only actually used_version_ids; empty scores nothing. Outcome/version set immutable.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "receipt_id":{"type":"string","minLength":1,"maxLength":128},
                    "outcome":{"type":"string","enum":["success","partial","failed"]},
                    "used_version_ids":{"type":"array","items":{"type":"string","minLength":1,"maxLength":64},"maxItems":3,"uniqueItems":true}
                },
                "required":["receipt_id","outcome"],
                "additionalProperties":false
            }
        }
    ])
}

/// Shared envelope validation for stdio and HTTP. MCP request IDs are strings or integers;
/// notifications have no ID and params, when present, must be an object.
pub fn valid_request_shape(message: &Value) -> bool {
    let Some(object) = message.as_object() else {
        return false;
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return false;
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || object
            .get("params")
            .is_some_and(|params| !params.is_object())
        || method.starts_with("notifications/") == object.contains_key("id")
    {
        return false;
    }
    object.get("id").is_none_or(valid_request_id)
}

fn valid_request_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

pub async fn handle_message<C: WamApi + ?Sized>(client: &C, message: Value) -> Option<Value> {
    let id = message.get("id").filter(|id| valid_request_id(id)).cloned();
    if !valid_request_shape(&message) {
        let trace = client
            .report_diagnostic(DiagnosticOperation::Unknown, DiagnosticError::InvalidInput)
            .await;
        return Some(rpc_error_with_trace(
            id.unwrap_or(Value::Null),
            -32600,
            "invalid request",
            trace,
        ));
    }
    let method = message.get("method").and_then(Value::as_str)?;

    match method {
        "notifications/initialized" | "notifications/cancelled" => None,
        "ping" => id.map(|id| json!({"jsonrpc":"2.0","id":id,"result":{}})),
        "initialize" => id.map(|id| {
            let requested_version = message
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(Value::as_str);
            let protocol_version = requested_version
                .filter(|version| SUPPORTED_PROTOCOL_VERSIONS.contains(version))
                .unwrap_or(LATEST_PROTOCOL_VERSION);
            json!({
                "jsonrpc":"2.0",
                "id":id,
                "result":{
                    "protocolVersion":protocol_version,
                    "capabilities":{"tools":{}},
                    "serverInfo":{"name":SERVER_NAME,"version":env!("CARGO_PKG_VERSION")}
                }
            })
        }),
        "tools/list" => id.map(|id| {
            json!({
                "jsonrpc":"2.0","id":id,"result":{"tools":tool_catalog()}
            })
        }),
        "tools/call" => {
            let id = id?;
            let previous_trace = client.last_trace_id();
            let params = message.get("params").and_then(Value::as_object);
            let Some(name) = params.and_then(|p| p.get("name")).and_then(Value::as_str) else {
                let trace = client
                    .report_diagnostic(DiagnosticOperation::Unknown, DiagnosticError::InvalidInput)
                    .await;
                return Some(rpc_error_with_trace(
                    id,
                    -32602,
                    "tool name is required",
                    trace,
                ));
            };
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = match name {
                "web_memory_status" => match no_args(&args) {
                    Ok(()) => match client.memory_status().await {
                        Ok(metadata) => {
                            let mut status = serde_json::to_value(metadata).unwrap_or(Value::Null);
                            if let Ok(cache) = client.cache_status().await {
                                status["cache"] =
                                    serde_json::to_value(cache).unwrap_or(Value::Null);
                            }
                            Ok(status)
                        }
                        Err(error) => Err(api_failure(error)),
                    },
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_recall" => match serde_json::from_value::<RecallRequest>(args) {
                    Ok(request) => client
                        .recall(request)
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(_) => Err(input_failure("invalid recall arguments")),
                },
                "web_memory_expand" => match parse_expand_args(&args) {
                    Ok((memory_id, level, version_id)) => client
                        .expand_version(memory_id, level, version_id)
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_leaderboard" => match no_args(&args) {
                    Ok(()) => client
                        .leaderboard()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_my_rank" => match no_args(&args) {
                    Ok(()) => client
                        .dashboard()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_my_usage" => match no_args(&args) {
                    Ok(()) => client
                        .my_usage()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_propose" => {
                    match serde_json::from_value::<CreateContributionRequest>(args) {
                        Ok(request) => client
                            .submit_contribution(request)
                            .await
                            .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                            .map_err(api_failure),
                        Err(_) => Err(input_failure("invalid contribution arguments")),
                    }
                }
                "web_memory_my_contributions" => match no_args(&args) {
                    Ok(()) => client
                        .my_contributions()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_withdraw" => match parse_withdraw_id(&args) {
                    Ok(contribution_id) => client
                        .withdraw_contribution(contribution_id)
                        .await
                        .map(|()| json!({"id":contribution_id,"withdrawn":true}))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                "web_memory_outcome" => match parse_outcome_args(&args) {
                    Ok((receipt_id, request)) => client
                        .report_outcome(receipt_id, request)
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(api_failure),
                    Err(error) => Err(input_failure(error)),
                },
                _ => {
                    let trace = client
                        .report_diagnostic(
                            DiagnosticOperation::Unknown,
                            DiagnosticError::InvalidInput,
                        )
                        .await;
                    return Some(rpc_error_with_trace(id, -32602, "unknown tool", trace));
                }
            };
            Some(match result {
                Ok(value) => json!({
                    "jsonrpc":"2.0","id":id,"result":{
                        "content":[{"type":"text","text":value.to_string()}],
                        "isError":false
                    }
                }),
                Err(failure) => {
                    let current_trace = client.last_trace_id();
                    let trace = if current_trace.is_some() && current_trace != previous_trace {
                        current_trace
                    } else {
                        client
                            .report_diagnostic(tool_operation(name), failure.diagnostic)
                            .await
                    };
                    let mut response = json!({
                        "jsonrpc":"2.0","id":id,"result":{
                            "content":[{"type":"text","text":failure.message}],
                            "isError":true
                        }
                    });
                    if let Some(trace) = trace {
                        response["result"]["_meta"] = json!({"trace_id":trace});
                    }
                    response
                }
            })
        }
        _ => {
            let trace = client
                .report_diagnostic(DiagnosticOperation::Unknown, DiagnosticError::InvalidInput)
                .await;
            id.map(|id| rpc_error_with_trace(id, -32601, "method not found", trace))
        }
    }
}

struct ToolFailure {
    message: String,
    diagnostic: DiagnosticError,
}

fn input_failure(message: &str) -> ToolFailure {
    ToolFailure {
        message: message.to_owned(),
        diagnostic: DiagnosticError::InvalidInput,
    }
}

fn api_failure(error: ApiError) -> ToolFailure {
    let diagnostic = match error {
        ApiError::Transport => DiagnosticError::NetworkError,
        ApiError::InvalidResponse => DiagnosticError::InvalidResponse,
        ApiError::Server | ApiError::CacheUnavailable => DiagnosticError::InternalError,
        _ => DiagnosticError::InvalidInput,
    };
    ToolFailure {
        message: error.to_string(),
        diagnostic,
    }
}

/// Closed operation category; never retain or emit the supplied tool name.
pub fn tool_operation(name: &str) -> DiagnosticOperation {
    match name {
        "web_memory_recall" => DiagnosticOperation::Recall,
        "web_memory_expand" => DiagnosticOperation::Expand,
        "web_memory_leaderboard" => DiagnosticOperation::Catalog,
        "web_memory_my_rank" | "web_memory_status" => DiagnosticOperation::Dashboard,
        "web_memory_my_usage" => DiagnosticOperation::Usage,
        "web_memory_propose" | "web_memory_my_contributions" | "web_memory_withdraw" => {
            DiagnosticOperation::Contribution
        }
        "web_memory_outcome" => DiagnosticOperation::Outcome,
        _ => DiagnosticOperation::Unknown,
    }
}

fn no_args(args: &Value) -> Result<(), &'static str> {
    if args.as_object().is_some_and(|object| object.is_empty()) {
        Ok(())
    } else {
        Err("this tool accepts no arguments")
    }
}

fn parse_withdraw_id(args: &Value) -> Result<&str, &'static str> {
    let object = args
        .as_object()
        .ok_or("withdraw arguments must be an object")?;
    if object.keys().any(|key| key != "contribution_id") {
        return Err("unknown withdraw argument");
    }
    object
        .get("contribution_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .ok_or("contribution_id is required")
}

fn parse_outcome_args(args: &Value) -> Result<(&str, OutcomeReportRequest), &'static str> {
    let object = args
        .as_object()
        .ok_or("outcome arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "receipt_id" | "outcome" | "used_version_ids"))
    {
        return Err("unknown outcome argument");
    }
    let receipt_id = object
        .get("receipt_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.chars().count() <= 128)
        .ok_or("receipt_id is required")?;
    let outcome = object
        .get("outcome")
        .cloned()
        .and_then(|value| serde_json::from_value::<MemoryOutcome>(value).ok())
        .ok_or("outcome must be success, partial, or failed")?;
    let used_version_ids: Vec<String> = match object.get("used_version_ids") {
        None => Vec::new(),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| "used_version_ids must be an array of version IDs")?,
    };
    if used_version_ids.len() > 3
        || used_version_ids.iter().enumerate().any(|(index, version)| {
            version.is_empty() || version.len() > 64 || used_version_ids[..index].contains(version)
        })
    {
        return Err("used_version_ids must contain up to 3 distinct version IDs");
    }
    Ok((
        receipt_id,
        OutcomeReportRequest {
            outcome,
            used_version_ids,
        },
    ))
}

fn parse_expand_args(args: &Value) -> Result<(&str, u8, Option<&str>), &'static str> {
    let object = args
        .as_object()
        .ok_or("expand arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "memory_id" | "level" | "version_id"))
    {
        return Err("unknown expand argument");
    }
    let memory_id = object
        .get("memory_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or("memory_id is required")?;
    let level = match object.get("level") {
        None => 2,
        Some(value) => match value.as_u64() {
            Some(1) => 1,
            Some(2) => 2,
            _ => return Err("level must be 1 or 2"),
        },
    };
    let version_id = match object.get("version_id") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .filter(|version| !version.is_empty() && version.len() <= 64)
                .ok_or("version_id must be a nonempty string of at most 64 bytes")?,
        ),
    };
    Ok((memory_id, level, version_id))
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn rpc_error_with_trace(id: Value, code: i32, message: &str, trace: Option<String>) -> Value {
    let mut response = rpc_error(id, code, message);
    if let Some(trace) = trace {
        response["error"]["data"] = json!({"trace_id":trace});
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use wam_client::ApiError;
    use wam_client::LocalCacheStatus;
    use wam_contracts::{
        ContributionListResponse, ContributionRecordResponse, CreateContributionRequest,
        DashboardResponse, LeaderboardEntry, LeaderboardResponse, MemoryDetailResponse,
        OutcomeReportRequest, OutcomeReportResponse, OwnerUsageResponse, ProfilePutRequest,
        ProfileResponse, RecallResponse, UsageDailyPoint, VocabResponse,
    };

    struct StubApi;

    #[tokio::test]
    async fn catalog_and_instructions_stay_within_the_declared_estimated_budget() {
        let initialized = handle_message(
            &StubApi,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .await
        .unwrap();
        let instructions = initialized["result"]["instructions"].as_str().unwrap_or("");
        let bytes = serde_json::to_vec(&tool_catalog()).unwrap().len() + instructions.len();
        // Fixed UTF-8/4 proxy from metrics.md; not an actual host tokenizer.
        assert!(
            bytes <= 800 * 4,
            "startup catalog/instructions use {bytes} bytes"
        );
    }

    #[tokio::test]
    async fn dispatcher_rejects_invalid_json_rpc_envelopes() {
        for message in [
            json!({"jsonrpc":"1.0","id":7,"method":"ping"}),
            json!({"id":"missing-version","method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":true,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":1.5,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":8,"method":"tools/list","params":[]}),
            json!({"jsonrpc":"2.0","id":9}),
            json!({"jsonrpc":"2.0","id":10,"method":"notifications/initialized"}),
            json!([]),
            json!(null),
        ] {
            let expected_id = message
                .get("id")
                .filter(|id| id.is_string() || id.is_i64() || id.is_u64())
                .cloned()
                .unwrap_or(Value::Null);
            let response = handle_message(&StubApi, message.clone())
                .await
                .expect("malformed requests must receive an invalid-request error");
            assert_eq!(response["error"]["code"], -32600, "{message}");
            assert_eq!(response["id"], expected_id);
            assert!(response.get("result").is_none());
        }
        for notification in [
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7}}),
        ] {
            assert!(handle_message(&StubApi, notification).await.is_none());
        }
    }

    #[async_trait]
    impl WamApi for StubApi {
        async fn report_diagnostic(
            &self,
            _operation: DiagnosticOperation,
            error: DiagnosticError,
        ) -> Option<String> {
            Some(
                if error == DiagnosticError::InternalError {
                    "b"
                } else {
                    "a"
                }
                .repeat(32),
            )
        }

        async fn status(&self) -> Result<(), ApiError> {
            Ok(())
        }

        async fn cache_status(&self) -> Result<LocalCacheStatus, ApiError> {
            Ok(LocalCacheStatus {
                entries: 2,
                bytes: 128,
                max_entries: 2_000,
                max_bytes: 50 * 1024 * 1024,
            })
        }

        async fn vocab(&self) -> Result<VocabResponse, ApiError> {
            Err(ApiError::Server)
        }

        async fn recall(&self, _request: RecallRequest) -> Result<RecallResponse, ApiError> {
            Err(ApiError::Server)
        }

        async fn expand(&self, _id: &str, _level: u8) -> Result<MemoryDetailResponse, ApiError> {
            Err(ApiError::Server)
        }

        async fn leaderboard(&self) -> Result<LeaderboardResponse, ApiError> {
            Ok(LeaderboardResponse {
                schema_version: 1,
                entries: vec![LeaderboardEntry {
                    rank: 1,
                    handle: "alice".to_owned(),
                    total_points: 90,
                }],
            })
        }

        async fn dashboard(&self) -> Result<DashboardResponse, ApiError> {
            Ok(DashboardResponse {
                schema_version: 1,
                total_points: 90,
                overall_rank: Some(1),
                public_rank: Some(1),
            })
        }

        async fn my_usage(&self) -> Result<OwnerUsageResponse, ApiError> {
            Ok(OwnerUsageResponse {
                schema_version: 1,
                period_days: 30,
                limit_per_minute: 300,
                burst_capacity: 60,
                remaining_units: 42,
                resets_in_seconds: 4,
                daily: vec![UsageDailyPoint {
                    utc_date: "2026-10-27".to_owned(),
                    requests: 3,
                    denied_requests: 2,
                    units: 5,
                    recall_requests: 1,
                    detail_requests: 1,
                    other_requests: 1,
                }],
            })
        }

        async fn public_profile(&self, _handle: &str) -> Result<ProfileResponse, ApiError> {
            Err(ApiError::Server)
        }

        async fn my_profile(&self) -> Result<ProfileResponse, ApiError> {
            Err(ApiError::Server)
        }

        async fn save_profile(
            &self,
            _profile: ProfilePutRequest,
        ) -> Result<ProfileResponse, ApiError> {
            Err(ApiError::Server)
        }

        async fn submit_contribution(
            &self,
            request: CreateContributionRequest,
        ) -> Result<ContributionRecordResponse, ApiError> {
            Ok(ContributionRecordResponse {
                id: "local-000001".to_owned(),
                url_pattern: request.url,
                task_category: request.task_category,
                actions: request.actions,
                status: "received".to_owned(),
                submitted_at: 1,
                progress: None,
                reflection: None,
            })
        }

        async fn my_contributions(&self) -> Result<ContributionListResponse, ApiError> {
            Ok(ContributionListResponse {
                schema_version: 1,
                contributions: Vec::new(),
            })
        }

        async fn withdraw_contribution(&self, _id: &str) -> Result<(), ApiError> {
            Ok(())
        }

        async fn report_outcome(
            &self,
            receipt_id: &str,
            request: OutcomeReportRequest,
        ) -> Result<OutcomeReportResponse, ApiError> {
            assert_eq!(request.used_version_ids, ["draft-v1"]);
            Ok(OutcomeReportResponse {
                receipt_id: receipt_id.to_owned(),
                outcome: request.outcome,
                recorded: true,
            })
        }
    }

    #[tokio::test]
    async fn invalid_tool_input_stays_framed_and_only_exposes_trace_reference() {
        let response = handle_message(&StubApi, json!({
            "jsonrpc":"2.0", "id":4, "method":"tools/call",
            "params":{"name":"web_memory_expand", "arguments":{"memory_id":"private-sentinel", "level":999}}
        })).await.unwrap();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 4);
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(response["result"]["_meta"]["trace_id"], "a".repeat(32));
        assert!(!response.to_string().contains("private-sentinel"));
        assert!(serde_json::from_str::<Value>(&response.to_string()).is_ok());
    }

    #[tokio::test]
    async fn api_failure_keeps_its_diagnostic_category_instead_of_becoming_invalid_input() {
        let response = handle_message(&StubApi, json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"web_memory_recall","arguments":{"url":"https://example.org","keywords":[]}}
        })).await.unwrap();
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(response["result"]["_meta"]["trace_id"], "b".repeat(32));
    }

    #[test]
    fn invalid_json_line_maps_to_json_rpc_parse_error() {
        assert_eq!(decode_line("{"), Err(ParseError));
        assert_eq!(
            parse_error_response(),
            json!({
                "jsonrpc":"2.0",
                "id":null,
                "error":{"code":-32700,"message":"parse error"}
            })
        );
    }

    #[tokio::test]
    async fn rank_tools_return_data_from_the_shared_api_client() {
        let api = StubApi;
        let leaderboard = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0",
                "id":7,
                "method":"tools/call",
                "params":{"name":"web_memory_leaderboard","arguments":{}}
            }),
        )
        .await
        .unwrap();
        let leaderboard_body: Value = serde_json::from_str(
            leaderboard["result"]["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(leaderboard_body["entries"][0]["handle"], "alice");

        let rank = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0",
                "id":8,
                "method":"tools/call",
                "params":{"name":"web_memory_my_rank","arguments":{}}
            }),
        )
        .await
        .unwrap();
        let rank_body: Value =
            serde_json::from_str(rank["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(rank_body["overall_rank"], 1);
        assert_eq!(rank_body["total_points"], 90);
    }

    #[tokio::test]
    async fn local_status_and_usage_tools_show_only_the_callers_cache_and_usage() {
        let api = StubApi;
        let status = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0","id":19,"method":"tools/call",
                "params":{"name":"web_memory_status","arguments":{}}
            }),
        )
        .await
        .unwrap();
        let status_body: Value =
            serde_json::from_str(status["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(status_body["status"], "ok");
        assert_eq!(status_body["cache"]["entries"], 2);
        assert_eq!(status_body["cache"]["bytes"], 128);

        let usage = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0","id":20,"method":"tools/call",
                "params":{"name":"web_memory_my_usage","arguments":{}}
            }),
        )
        .await
        .unwrap();
        let usage_body: Value =
            serde_json::from_str(usage["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(usage_body["remaining_units"], 42);
        assert_eq!(usage_body["daily"][0]["requests"], 3);
        assert_eq!(usage_body["daily"][0]["denied_requests"], 2);
    }

    #[tokio::test]
    async fn outcome_tool_reports_the_receipt_and_closed_result() {
        let api = StubApi;
        let response = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0",
                "id":11,
                "method":"tools/call",
                "params":{
                    "name":"web_memory_outcome",
                    "arguments":{"receipt_id":"receipt-1","outcome":"success","used_version_ids":["draft-v1"]}
                }
            }),
        )
        .await
        .unwrap();
        let body: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(body["receipt_id"], "receipt-1");
        assert_eq!(body["outcome"], "success");
        assert_eq!(body["recorded"], true);
        assert_eq!(response["result"]["isError"], false);
    }

    #[test]
    fn outcome_arguments_reject_unknown_fields_and_unlisted_values() {
        assert_eq!(
            parse_outcome_args(&json!({"receipt_id":"r1","outcome":"partial"})),
            Ok((
                "r1",
                OutcomeReportRequest {
                    outcome: MemoryOutcome::Partial,
                    used_version_ids: Vec::new(),
                }
            ))
        );
        assert!(parse_outcome_args(&json!({"receipt_id":"r1"})).is_err());
        assert!(parse_outcome_args(&json!({"receipt_id":"r1","outcome":"unknown"})).is_err());
        assert!(
            parse_outcome_args(&json!({"receipt_id":"r1","outcome":"success","points":5})).is_err()
        );
        assert!(parse_outcome_args(&json!({"receipt_id":"","outcome":"failed"})).is_err());
        for versions in [
            json!(null),
            json!("draft-v1"),
            json!([1]),
            json!([""]),
            json!(["draft-v1", "draft-v1"]),
            json!(["a", "b", "c", "d"]),
        ] {
            assert!(
                parse_outcome_args(
                    &json!({"receipt_id":"r1","outcome":"success","used_version_ids":versions})
                )
                .is_err()
            );
        }
    }

    #[tokio::test]
    async fn contribution_tools_use_the_shared_client_and_require_closed_consent_payloads() {
        let api = StubApi;
        let response = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0",
                "id":9,
                "method":"tools/call",
                "params":{
                    "name":"web_memory_propose",
                    "arguments":{
                        "url":"https://example.com/guide",
                        "task_category":"compose-draft",
                        "actions":["navigate","save"],
                        "consent":true,
                        "consent_version":CONTRIBUTION_CONSENT_VERSION
                    }
                }
            }),
        )
        .await
        .unwrap();
        let record: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(record["id"], "local-000001");
        assert_eq!(record["status"], "received");

        let invalid = handle_message(
            &api,
            json!({
                "jsonrpc":"2.0",
                "id":10,
                "method":"tools/call",
                "params":{
                    "name":"web_memory_propose",
                    "arguments":{
                        "url":"https://example.com/guide",
                        "task_category":"compose-draft",
                        "actions":["navigate"],
                        "consent":false,
                        "consent_version":CONTRIBUTION_CONSENT_VERSION,
                        "prompt":"must not be accepted"
                    }
                }
            }),
        )
        .await
        .unwrap();
        assert_eq!(invalid["result"]["isError"], true);

        assert!(parse_withdraw_id(&json!({"contribution_id":"local-000001"})).is_ok());
        assert!(
            parse_withdraw_id(&json!({"contribution_id":"local-000001","account":"x"})).is_err()
        );
    }

    #[test]
    fn tool_arguments_are_closed_and_expand_level_never_wraps() {
        assert!(no_args(&json!({})).is_ok());
        assert!(no_args(&json!({"extra":true})).is_err());
        assert_eq!(
            parse_expand_args(&json!({"memory_id":"mem-1","level":2})),
            Ok(("mem-1", 2, None))
        );
        assert!(parse_expand_args(&json!({"memory_id":"mem-1","level":257})).is_err());
        assert!(parse_expand_args(&json!({"memory_id":"mem-1","unexpected":true})).is_err());
    }

    #[test]
    fn catalog_exposes_implemented_read_and_contribution_tools() {
        let catalog = tool_catalog();
        let tools = catalog.as_array().unwrap();
        assert_eq!(tools.len(), 10);
        assert_eq!(tools[0]["name"], "web_memory_status");
        assert_eq!(tools[1]["name"], "web_memory_recall");
        assert_eq!(tools[2]["name"], "web_memory_expand");
        assert_eq!(tools[3]["name"], "web_memory_leaderboard");
        assert_eq!(tools[4]["name"], "web_memory_my_rank");
        assert_eq!(tools[5]["name"], "web_memory_my_usage");
        assert_eq!(tools[6]["name"], "web_memory_propose");
        assert_eq!(tools[7]["name"], "web_memory_my_contributions");
        assert_eq!(tools[8]["name"], "web_memory_withdraw");
        assert_eq!(tools[9]["name"], "web_memory_outcome");
        assert_eq!(tools[1]["inputSchema"]["additionalProperties"], false);
        assert_eq!(tools[4]["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            tools[6]["inputSchema"]["properties"]["consent"]["const"],
            true
        );
        assert_eq!(
            tools[6]["inputSchema"]["properties"]["consent_version"]["const"],
            CONTRIBUTION_CONSENT_VERSION
        );
        assert_eq!(tools[9]["inputSchema"]["additionalProperties"], false);
    }
}
