//! Optional browser MCP bridge. One child lives for the host session, retaining its lease proof.
//! The memory tool reads the child's owned tab locally; it never opens a private socket or CDP.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};
use wam_client::{ApiError, WamApi};
use wam_contracts::{RecallRequest, RecallResponse};
use wam_core::url_scope::{CanonicalUrl, UrlMode};

const MAX_FRAME: usize = 1024 * 1024;
const INTERNAL_ID: &str = "wam-tab-context-internal-";
pub const CURRENT_TAB_TOOL: &str = "web_memory_recall_current_tab";

async fn frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Option<Vec<u8>>, ApiError> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().await.map_err(|_| ApiError::Transport)?;
        if bytes.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(ApiError::InvalidResponse)
            };
        }
        let end = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|at| at + 1);
        let count = end.unwrap_or(bytes.len());
        if line.len() + count > MAX_FRAME {
            return Err(ApiError::InvalidResponse);
        }
        line.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(Some(line));
        }
    }
}

async fn emit(writer: &mut (impl AsyncWrite + Unpin), message: &Value) -> Result<(), ApiError> {
    let mut bytes = serde_json::to_vec(message).map_err(|_| ApiError::InvalidInput)?;
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| ApiError::Transport)?;
    writer.flush().await.map_err(|_| ApiError::Transport)
}

struct BrowserMcp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl BrowserMcp {
    fn start(program: &Path, args: &[String]) -> Result<Self, ApiError> {
        if !program.is_absolute() || !program.is_file() {
            return Err(ApiError::InvalidInput);
        }
        let mut command = Command::new(program);
        command
            .args(args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for key in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "SYSTEMROOT",
            "XDG_CONFIG_HOME",
            "XDG_CACHE_HOME",
            "XDG_DATA_HOME",
            "TMPDIR",
            "TEMP",
            "TMP",
            "LANG",
            "LC_ALL",
            "AGENT_BROWSER_NO_KEYCHAIN",
            "AGENT_BROWSER_SESSIONS_DIR",
            "AGENT_BROWSER_UI_STATE",
            "AGENT_BROWSER_PROFILES_DIR",
            "ARISE_NO_UPDATE_CHECK",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|_| ApiError::Transport)?;
        let input = child.stdin.take().ok_or(ApiError::Transport)?;
        let output = BufReader::new(child.stdout.take().ok_or(ApiError::Transport)?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    async fn request(
        &mut self,
        message: &Value,
        output: &mut (impl AsyncWrite + Unpin),
    ) -> Result<Value, ApiError> {
        emit(&mut self.input, message).await?;
        for _ in 0..64 {
            let bytes = frame(&mut self.output).await?.ok_or(ApiError::Transport)?;
            let response: Value =
                serde_json::from_slice(&bytes).map_err(|_| ApiError::InvalidResponse)?;
            if response["jsonrpc"] != "2.0" {
                return Err(ApiError::InvalidResponse);
            }
            if response.get("id").is_none() && response["method"].is_string() {
                emit(output, &response).await?;
            } else if response["id"] == message["id"] && response.get("method").is_none() {
                return Ok(response);
            } else {
                return Err(ApiError::InvalidResponse);
            }
        }
        Err(ApiError::InvalidResponse)
    }

    async fn tabs(
        &mut self,
        counter: u64,
        session: Option<&str>,
        output: &mut (impl AsyncWrite + Unpin),
    ) -> Result<Value, ApiError> {
        let args = session
            .map(|session| json!({"session_id":session}))
            .unwrap_or_else(|| json!({}));
        let response = self.request(&json!({"jsonrpc":"2.0","id":format!("{INTERNAL_ID}{counter}"),"method":"tools/call","params":{"name":"browser_tabs","arguments":args}}), output).await?;
        let result = &response["result"];
        if response.get("error").is_some() || result["isError"] == true {
            return Err(ApiError::Transport);
        }
        let context = if let Some(value) = result.get("structuredContent") {
            value.clone()
        } else {
            let text = result["content"]
                .as_array()
                .and_then(|items| {
                    items.iter().find_map(|item| {
                        (item["type"] == "text")
                            .then(|| item["text"].as_str())
                            .flatten()
                    })
                })
                .ok_or(ApiError::InvalidResponse)?;
            serde_json::from_str(text).map_err(|_| ApiError::InvalidResponse)?
        };
        Ok(context.get("reply").cloned().unwrap_or(context))
    }
}

// No Debug/Serialize: raw tab URLs never enter error text or WAM telemetry.
#[derive(PartialEq)]
struct SelectedTab {
    id: Value,
    window: Value,
    raw_url: String,
}

fn selected_tab(
    context: &Value,
    session: &str,
    client: &str,
    now_ms: u64,
) -> Result<SelectedTab, ApiError> {
    if context["ok"] != true || context["session_id"] != session {
        return Err(ApiError::Forbidden);
    }
    let target = context
        .get("automation_target_tab")
        .filter(|value| value.is_u64())
        .ok_or(ApiError::Forbidden)?;
    let tabs = context["tabs"]
        .as_array()
        .ok_or(ApiError::InvalidResponse)?;
    let mut matches = tabs
        .iter()
        .filter(|tab| tab.get("tab_id").or_else(|| tab.get("id")) == Some(target));
    let tab = matches.next().ok_or(ApiError::NotFound)?;
    if matches.next().is_some()
        || tab["is_mine"] != true
        || tab["is_automation_target"] != true
        || tab["ready"] != true
        || !tab["window_id"].is_u64()
    {
        return Err(ApiError::Forbidden);
    }
    let owner = &tab["owner"];
    if owner["is_mine"] != true
        || owner["session_id"] != session
        || owner["client_name"] != client
        || owner["tab_id"] != *target
        || owner["window_id"] != tab["window_id"]
        || !owner["expires_at_ms"]
            .as_u64()
            .is_some_and(|expiry| expiry > now_ms)
    {
        return Err(ApiError::Forbidden);
    }
    let raw_url = tab["url"]
        .as_str()
        .filter(|url| url.len() <= 2048)
        .ok_or(ApiError::InvalidInput)?
        .to_owned();
    Ok(SelectedTab {
        id: target.clone(),
        window: tab["window_id"].clone(),
        raw_url,
    })
}
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(u64::MAX)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TabRecall {
    session_id: Option<String>,
    task_category: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    limit: Option<usize>,
}

pub fn tool_schema() -> Value {
    json!({"name":CURRENT_TAB_TOOL,"description":"Read WAM hints for this browser MCP session's current leased tab. The bridge normalizes the URL locally; do not supply a URL or page text. Call only when the user's instructions allow memory. Never acquires a different tab, submits contributions, or reports outcomes.","inputSchema":{"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"task_category":{"type":"string","enum":["compose-draft","navigate","research"]},"keywords":{"type":"array","items":{"type":"string","maxLength":32},"maxItems":8},"limit":{"type":"integer","minimum":1,"maximum":3}},"additionalProperties":false}})
}

async fn recall_tab(
    api: &dyn WamApi,
    browser: &mut BrowserMcp,
    args: Value,
    client: &str,
    counter: &mut u64,
    output: &mut (impl AsyncWrite + Unpin),
) -> Result<RecallResponse, ApiError> {
    let args: TabRecall = serde_json::from_value(args).map_err(|_| ApiError::InvalidInput)?;
    let limit = args.limit.unwrap_or(3);
    if !(1..=3).contains(&limit)
        || args.keywords.len() > 8
        || args.keywords.iter().any(|keyword| keyword.len() > 32)
        || args.session_id.as_ref().is_some_and(|session| {
            session.is_empty() || session.len() > 128 || session.chars().any(char::is_control)
        })
    {
        return Err(ApiError::InvalidInput);
    }
    *counter = counter.checked_add(1).ok_or(ApiError::InvalidInput)?;
    let context = browser
        .tabs(*counter, args.session_id.as_deref(), output)
        .await?;
    let session = args
        .session_id
        .as_deref()
        .or_else(|| context["session_id"].as_str())
        .ok_or(ApiError::Forbidden)?;
    let first = selected_tab(&context, session, client, now_millis())?;
    let parsed = url::Url::parse(&first.raw_url).map_err(|_| ApiError::InvalidInput)?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::InvalidInput);
    }
    let canonical =
        CanonicalUrl::parse(&first.raw_url, UrlMode::Public).map_err(|_| ApiError::InvalidInput)?;
    let url = format!("https://{}{}", canonical.host(), canonical.path());
    if !args.keywords.is_empty() || args.task_category.is_some() {
        let vocabulary = api.vocab().await?;
        if args
            .keywords
            .iter()
            .any(|keyword| !vocabulary.keywords.contains(keyword))
            || args
                .task_category
                .as_ref()
                .is_some_and(|task| !vocabulary.task_categories.contains(task))
        {
            return Err(ApiError::InvalidInput);
        }
    }
    *counter = counter.checked_add(1).ok_or(ApiError::InvalidInput)?;
    let second = selected_tab(
        &browser.tabs(*counter, Some(session), output).await?,
        session,
        client,
        now_millis(),
    )?;
    if first != second {
        return Err(ApiError::Conflict);
    }
    api.recall(RecallRequest {
        url,
        keywords: args.keywords,
        task_category: args.task_category,
        limit: Some(limit),
        kind: None,
    })
    .await
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub async fn serve(api: &dyn WamApi, program: &Path, args: &[String]) -> Result<(), ApiError> {
    let mut browser = BrowserMcp::start(program, args)?;
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    let mut client = None;
    let mut counter = 0;
    let result = async {
        while let Some(bytes) = frame(&mut input).await? {
            let message: Value = match serde_json::from_slice(&bytes) {
                Ok(message) => message,
                Err(_) => { emit(&mut output, &error(Value::Null,-32700,"parse error")).await?; continue; }
            };
            let id = message.get("id").filter(|id| id.is_string() || id.is_i64() || id.is_u64()).cloned();
            if message["jsonrpc"] != "2.0" || !message["method"].is_string() || message.get("params").is_some_and(|params| !params.is_object()) || message.get("id").is_some() && id.is_none() || id.as_ref().and_then(Value::as_str).is_some_and(|id| id.starts_with(INTERNAL_ID)) {
                emit(&mut output,&error(Value::Null,-32600,"invalid request")).await?;
                continue;
            }
            let Some(id) = id else { emit(&mut browser.input,&message).await?; continue; };
            let method = message["method"].as_str().unwrap_or_default();
            if method == "initialize" {
                let name = message.pointer("/params/clientInfo/name").and_then(Value::as_str).filter(|name| !name.is_empty() && name.len() <= 128).ok_or(ApiError::InvalidInput)?;
                client = Some(name.to_owned());
            }
            if method == "tools/call" && message.pointer("/params/name").and_then(Value::as_str) == Some(CURRENT_TAB_TOOL) {
                let outcome = if let Some(client) = client.as_deref() {
                    tokio::time::timeout(Duration::from_secs(12), recall_tab(api,&mut browser,message.pointer("/params/arguments").cloned().unwrap_or_else(||json!({})),client,&mut counter,&mut output)).await.map_err(|_|ApiError::Transport).and_then(|result|result)
                } else { Err(ApiError::InvalidInput) };
                let result = match outcome {
                    Ok(value) => json!({"content":[{"type":"text","text":serde_json::to_string(&value).map_err(|_|ApiError::InvalidResponse)?}],"isError":false}),
                    Err(cause) => json!({"content":[{"type":"text","text":cause.to_string()}],"isError":true}),
                };
                emit(&mut output,&json!({"jsonrpc":"2.0","id":id,"result":result})).await?;
            } else {
                let mut response = browser.request(&message,&mut output).await?;
                if method == "tools/list" {
                    if let Some(tools) = response.pointer_mut("/result/tools").and_then(Value::as_array_mut) {
                        if tools.iter().any(|tool| tool["name"] == CURRENT_TAB_TOOL) { return Err(ApiError::Conflict); }
                        tools.push(tool_schema());
                    }
                }
                emit(&mut output,&response).await?;
            }
        }
        Ok(())
    }.await;
    let _ = browser.child.kill().await;
    let _ = browser.child.wait().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Value {
        json!({"ok":true,"session_id":"owned-session","automation_target_tab":7,"tabs":[{"tab_id":7,"window_id":2,"is_mine":true,"is_automation_target":true,"ready":true,"owner":{"is_mine":true,"session_id":"owned-session","client_name":"test-client","tab_id":7,"window_id":2,"expires_at_ms":200},"url":"https://example.com/guide?token=private#fragment"}]})
    }
    #[test]
    fn tab_selection_requires_one_ready_owned_lease_and_never_falls_back_to_active_tab() {
        let base = context();
        assert!(selected_tab(&base, "owned-session", "test-client", 100).is_ok());
        for (session, client, time) in [
            ("other-session", "test-client", 100),
            ("owned-session", "foreign-client", 100),
            ("owned-session", "test-client", 200),
        ] {
            assert!(selected_tab(&base, session, client, time).is_err());
        }
        for field in ["is_mine", "is_automation_target", "ready"] {
            let mut changed = base.clone();
            changed["tabs"][0][field] = json!(false);
            assert!(selected_tab(&changed, "owned-session", "test-client", 100).is_err());
        }
        let mut missing = base.clone();
        missing["automation_target_tab"] = Value::Null;
        missing["tabs"][0]["active"] = json!(true);
        assert!(selected_tab(&missing, "owned-session", "test-client", 100).is_err());
        let mut duplicate = base.clone();
        duplicate["tabs"]
            .as_array_mut()
            .unwrap()
            .push(base["tabs"][0].clone());
        assert!(selected_tab(&duplicate, "owned-session", "test-client", 100).is_err());
    }
}
