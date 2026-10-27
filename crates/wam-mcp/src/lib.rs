#![forbid(unsafe_code)]

//! Minimal JSON-RPC MCP stdio adapter. The protocol dispatcher is separate from
//! stdin/stdout so tools can be contract-tested with a fake WamApi.

use serde_json::{Value, json};
use wam_client::WamApi;
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
            "description": "Check whether the Web Agentic Memory API is reachable.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_recall",
            "description": "Find approved memory hints for a public URL. Allowlisted keywords rank matching metadata with BM25 and reciprocal-rank fusion; free-form task text is not sent.",
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
            "description": "Read an approved memory hint or its detailed workflow.",
            "inputSchema": {
                "type":"object",
                "properties":{"memory_id":{"type":"string"},"level":{"type":"integer","enum":[1,2]}},
                "required":["memory_id"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_leaderboard",
            "description": "Read public opt-in leaderboard handles, ranks, and points.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_my_rank",
            "description": "Read the authenticated caller's private overall rank and optional public rank.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_propose",
            "description": "After the user explicitly consents, submit only a canonical public URL, closed task category, and selected action codes. Page content, prompts, and browser inputs are not sent.",
            "inputSchema": {
                "type":"object",
                "properties": {
                    "url":{"type":"string","maxLength":2048},
                    "task_category":{"type":"string","enum":["compose-draft","navigate","research"]},
                    "actions":{"type":"array","items":{"type":"string","enum":["navigate","edit","save"]},"minItems":1,"maxItems":8,"uniqueItems":true},
                    "consent":{"type":"boolean","const":true},
                    "consent_version":{"type":"string","const":CONTRIBUTION_CONSENT_VERSION}
                },
                "required":["url","task_category","actions","consent","consent_version"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_my_contributions",
            "description": "List the authenticated caller's local contribution receipts.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "web_memory_withdraw",
            "description": "Withdraw one of the authenticated caller's unposted local contribution receipts.",
            "inputSchema": {
                "type":"object",
                "properties":{"contribution_id":{"type":"string","minLength":1,"maxLength":128}},
                "required":["contribution_id"],
                "additionalProperties":false
            }
        },
        {
            "name": "web_memory_outcome",
            "description": "After using a memory for the reported receipt, tell WAM whether the task succeeded, partly succeeded, or failed. Requires the authenticated receipt owner and report:outcome permission. List only used_version_ids actually used from that receipt; omitted or empty means no score. The outcome and version selection cannot be changed after reporting.",
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

pub async fn handle_message<C: WamApi + ?Sized>(client: &C, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return id.map(|id| rpc_error(id, -32600, "invalid request"));
    };

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
            let params = message.get("params").and_then(Value::as_object);
            let Some(name) = params.and_then(|p| p.get("name")).and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "tool name is required"));
            };
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = match name {
                "web_memory_status" => match no_args(&args) {
                    Ok(()) => match client.status().await {
                        Ok(()) => Ok(json!({"status":"ok"})),
                        Err(error) => Err(error.to_string()),
                    },
                    Err(error) => Err(error.to_owned()),
                },
                "web_memory_recall" => match serde_json::from_value::<RecallRequest>(args) {
                    Ok(request) => client
                        .recall(request)
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(|e| e.to_string()),
                    Err(_) => Err("invalid recall arguments".to_owned()),
                },
                "web_memory_expand" => match parse_expand_args(&args) {
                    Ok((memory_id, level)) => client
                        .expand(memory_id, level)
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_owned()),
                },
                "web_memory_leaderboard" => match no_args(&args) {
                    Ok(()) => client
                        .leaderboard()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_owned()),
                },
                "web_memory_my_rank" => match no_args(&args) {
                    Ok(()) => client
                        .dashboard()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_owned()),
                },
                "web_memory_propose" => {
                    match serde_json::from_value::<CreateContributionRequest>(args) {
                        Ok(request) => client
                            .submit_contribution(request)
                            .await
                            .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                            .map_err(|error| error.to_string()),
                        Err(_) => Err("invalid contribution arguments".to_owned()),
                    }
                }
                "web_memory_my_contributions" => match no_args(&args) {
                    Ok(()) => client
                        .my_contributions()
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_owned()),
                },
                "web_memory_withdraw" => match parse_withdraw_id(&args) {
                    Ok(contribution_id) => client
                        .withdraw_contribution(contribution_id)
                        .await
                        .map(|()| json!({"id":contribution_id,"withdrawn":true}))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_owned()),
                },
                "web_memory_outcome" => match parse_outcome_args(&args) {
                    Ok((receipt_id, request)) => client
                        .report_outcome(receipt_id, request)
                        .await
                        .map(|result| serde_json::to_value(result).unwrap_or(Value::Null))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_owned()),
                },
                _ => return Some(rpc_error(id, -32602, "unknown tool")),
            };
            Some(match result {
                Ok(value) => json!({
                    "jsonrpc":"2.0","id":id,"result":{
                        "content":[{"type":"text","text":value.to_string()}],
                        "isError":false
                    }
                }),
                Err(message) => json!({
                    "jsonrpc":"2.0","id":id,"result":{
                        "content":[{"type":"text","text":message}],
                        "isError":true
                    }
                }),
            })
        }
        _ => id.map(|id| rpc_error(id, -32601, "method not found")),
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

fn parse_expand_args(args: &Value) -> Result<(&str, u8), &'static str> {
    let object = args
        .as_object()
        .ok_or("expand arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "memory_id" | "level"))
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
    Ok((memory_id, level))
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use wam_client::ApiError;
    use wam_contracts::{
        ContributionListResponse, ContributionRecordResponse, CreateContributionRequest,
        DashboardResponse, LeaderboardEntry, LeaderboardResponse, MemoryDetailResponse,
        OutcomeReportRequest, OutcomeReportResponse, ProfilePutRequest, ProfileResponse,
        RecallResponse, VocabResponse,
    };

    struct StubApi;

    #[async_trait]
    impl WamApi for StubApi {
        async fn status(&self) -> Result<(), ApiError> {
            Ok(())
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
            Ok(("mem-1", 2))
        );
        assert!(parse_expand_args(&json!({"memory_id":"mem-1","level":257})).is_err());
        assert!(parse_expand_args(&json!({"memory_id":"mem-1","unexpected":true})).is_err());
    }

    #[test]
    fn catalog_exposes_implemented_read_and_contribution_tools() {
        let catalog = tool_catalog();
        let tools = catalog.as_array().unwrap();
        assert_eq!(tools.len(), 9);
        assert_eq!(tools[0]["name"], "web_memory_status");
        assert_eq!(tools[1]["name"], "web_memory_recall");
        assert_eq!(tools[2]["name"], "web_memory_expand");
        assert_eq!(tools[3]["name"], "web_memory_leaderboard");
        assert_eq!(tools[4]["name"], "web_memory_my_rank");
        assert_eq!(tools[5]["name"], "web_memory_propose");
        assert_eq!(tools[6]["name"], "web_memory_my_contributions");
        assert_eq!(tools[7]["name"], "web_memory_withdraw");
        assert_eq!(tools[8]["name"], "web_memory_outcome");
        assert_eq!(tools[1]["inputSchema"]["additionalProperties"], false);
        assert_eq!(tools[4]["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            tools[5]["inputSchema"]["properties"]["consent"]["const"],
            true
        );
        assert_eq!(
            tools[5]["inputSchema"]["properties"]["consent_version"]["const"],
            CONTRIBUTION_CONSENT_VERSION
        );
        assert_eq!(tools[8]["inputSchema"]["additionalProperties"], false);
    }
}
