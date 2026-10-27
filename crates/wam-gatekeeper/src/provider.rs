//! The model seam. The curator loop talks to [`LlmProvider`] and nothing else.
//!
//! * `ScriptedProvider`: deterministic fake for tests and for the scripted scenario runs.
//! * `CliProvider`: the locally installed Claude Code or Codex CLI, used as a plain text completer with no tools,
//!   for local development (it reuses the user's own login, so no API key is needed). Tool calls travel as a JSON
//!   turn protocol (`{"tool": ..., "args": ...}`), the fallback docs/spec/curator.md describes for models without
//!   native tool calling.
//! * `openai-compat` (DeepSeek, Meta Muse): attached last, behind this same trait, selected by configuration.

use crate::tools::ToolSpec;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fmt;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Job(String),
    Call { tool: String, args: Value },
    Result { tool: String, value: Value },
}

pub struct TurnRequest<'a> {
    pub system: &'a str,
    pub tools: &'a [ToolSpec],
    pub transcript: &'a [Message],
}

#[derive(Clone, Debug)]
pub struct ModelTurn {
    pub tool: String,
    pub args: Value,
    pub raw: String,
    /// Provider-specific accounting (cost, tokens) when known.
    pub meta: Value,
}

#[derive(Debug)]
pub enum ProviderError {
    Spawn(String),
    Timeout(u64),
    Failed(String),
    Malformed {
        raw: String,
        why: String,
    },
    NotBuilt(String),
    Exhausted,
    /// The account's usage limit or credit is spent. Trying again at once cannot help, so the queue parks the job
    /// (`waiting_budget`) and the worker pauses the budget instead of charging an attempt. `retry_after_secs` is
    /// what the provider said about when the quota returns, if it said anything.
    QuotaExhausted {
        why: String,
        retry_after_secs: Option<u64>,
    },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(why) => write!(f, "could not start the model process: {why}"),
            Self::Timeout(seconds) => write!(f, "the model call timed out after {seconds}s"),
            Self::Failed(why) => write!(f, "the model call failed: {why}"),
            Self::Malformed { why, .. } => write!(f, "the model reply was not a tool call: {why}"),
            Self::NotBuilt(why) => write!(f, "{why}"),
            Self::Exhausted => write!(f, "the scripted provider has no more turns"),
            Self::QuotaExhausted { why, .. } => write!(f, "the model quota is exhausted: {why}"),
        }
    }
}

impl std::error::Error for ProviderError {}

pub trait LlmProvider {
    fn name(&self) -> String;
    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError>;
}

/// One step of a scripted run.
#[derive(Clone, Debug)]
pub enum ScriptStep {
    Call(String, Value),
    /// A reply that is not a tool call (exercises the loop's handling of malformed output).
    Raw(String),
}

pub struct ScriptedProvider {
    steps: VecDeque<ScriptStep>,
}

impl ScriptedProvider {
    pub fn new(steps: impl IntoIterator<Item = ScriptStep>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
        }
    }

    pub fn calls(calls: impl IntoIterator<Item = (String, Value)>) -> Self {
        Self::new(
            calls
                .into_iter()
                .map(|(tool, args)| ScriptStep::Call(tool, args)),
        )
    }
}

impl LlmProvider for ScriptedProvider {
    fn name(&self) -> String {
        "fake:scripted".into()
    }

    fn next_turn(&mut self, _request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        match self.steps.pop_front().ok_or(ProviderError::Exhausted)? {
            ScriptStep::Call(tool, args) => Ok(ModelTurn {
                raw: json!({"tool": tool, "args": args}).to_string(),
                tool,
                args,
                meta: Value::Null,
            }),
            ScriptStep::Raw(raw) => Err(ProviderError::Malformed {
                why: "scripted malformed reply".into(),
                raw,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliKind {
    Claude,
    Codex,
}

pub struct CliProvider {
    kind: CliKind,
    model: Option<String>,
    timeout: Duration,
    workdir: PathBuf,
    calls: u32,
}

impl CliProvider {
    /// The CLI runs in an empty temporary directory, with no tools, no session, and (Codex) a read-only sandbox.
    pub fn new(
        kind: CliKind,
        model: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let workdir =
            std::env::temp_dir().join(format!("wam-gatekeeper-cli-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&workdir)
            .map_err(|error| ProviderError::Spawn(error.to_string()))?;
        Ok(Self {
            kind,
            model,
            timeout,
            workdir,
            calls: 0,
        })
    }
}

impl Drop for CliProvider {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.workdir);
    }
}

/// The protocol text appended to the instructions for models without native tool calling.
pub fn protocol_text(tools: &[ToolSpec]) -> String {
    let mut out = String::from(
        "TOOL PROTOCOL\nThis session has no native tool calling. On every turn reply with exactly ONE JSON object and \
         nothing else (no markdown fence, no commentary): {\"tool\": \"<tool name>\", \"args\": {...}}\nThe server runs \
         the tool and shows you the result on the next turn. Tools, each with its argument JSON Schema:\n",
    );
    for tool in tools {
        let line = json!({"name": tool.name, "kind": match tool.kind {
            crate::tools::ToolKind::Read => "read",
            crate::tools::ToolKind::Decision => "decision",
        }, "description": tool.description, "input_schema": tool.input_schema});
        out.push_str(&line.to_string());
        out.push('\n');
    }
    out
}

pub fn render_transcript(messages: &[Message]) -> String {
    let mut out = String::from("TRANSCRIPT\n");
    for message in messages {
        match message {
            Message::Job(text) => out.push_str(&format!("[JOB] {text}\n")),
            Message::Call { tool, args } => {
                out.push_str(&format!("[YOU] {}\n", json!({"tool": tool, "args": args})))
            }
            Message::Result { tool, value } => {
                out.push_str(&format!("[TOOL RESULT {tool}] {value}\n"))
            }
        }
    }
    out.push_str("Reply now with your next tool call as one JSON object.");
    out
}

/// Pulls the first balanced JSON object out of a model reply and checks that it is a tool call.
pub fn extract_turn(text: &str) -> Result<(String, Value), ProviderError> {
    let malformed = |why: &str| ProviderError::Malformed {
        raw: text.chars().take(400).collect(),
        why: why.to_owned(),
    };
    let start = text
        .find('{')
        .ok_or_else(|| malformed("no JSON object found"))?;
    let (mut depth, mut in_string, mut escaped, mut end) = (0i32, false, false, None);
    for (offset, character) in text[start..].char_indices() {
        if in_string {
            match (escaped, character) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + offset + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| malformed("the JSON object is not closed"))?;
    let value: Value = serde_json::from_str(&text[start..end])
        .map_err(|_| malformed("the JSON object does not parse"))?;
    let tool = value["tool"]
        .as_str()
        .ok_or_else(|| malformed("missing string field \"tool\""))?
        .to_owned();
    let args = match value.get("args") {
        None | Some(Value::Null) => json!({}),
        Some(args) if args.is_object() => args.clone(),
        Some(_) => return Err(malformed("\"args\" must be an object")),
    };
    Ok((tool, args))
}

fn run(mut command: Command, timeout: Duration) -> Result<(String, String, bool), ProviderError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1");
    let mut child = command
        .spawn()
        .map_err(|error| ProviderError::Spawn(error.to_string()))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    });
    let err_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = out_thread.join().unwrap_or_default();
                let err = err_thread.join().unwrap_or_default();
                return Ok((out, err, status.success()));
            }
            Ok(None) => {
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProviderError::Timeout(timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(ProviderError::Failed(error.to_string())),
        }
    }
}

fn clip(text: &str) -> String {
    text.chars().take(300).collect()
}

/// Does an error text from a CLI or an API say that the usage limit or the credit is spent, as opposed to a passing
/// failure? The phrases are the ones the CLIs and common APIs use. A wording that is not listed is a plain failure,
/// which the queue retries with backoff and finally dead-letters: nothing is lost, it only takes longer to notice.
pub fn looks_like_quota_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "usage limit",
        "hit your limit",
        "insufficient_quota",
        "insufficient quota",
        "exceeded your current quota",
        "quota exceeded",
        "out of credits",
        "insufficient credit",
        "credit balance",
        "payment required",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

/// A failed model call: spent quota is told apart from everything else.
fn failure(text: &str) -> ProviderError {
    if looks_like_quota_error(text) {
        ProviderError::QuotaExhausted {
            why: clip(text),
            retry_after_secs: None,
        }
    } else {
        ProviderError::Failed(clip(text))
    }
}

impl CliProvider {
    fn claude_turn(&self, system: &str, transcript: &str) -> Result<ModelTurn, ProviderError> {
        let mut command = Command::new("claude");
        command.current_dir(&self.workdir).args([
            "-p",
            transcript,
            "--system-prompt",
            system,
            "--tools",
            "",
            "--no-session-persistence",
            "--output-format",
            "json",
            "--disable-slash-commands",
            "--strict-mcp-config",
        ]);
        if let Some(model) = &self.model {
            command.args(["--model", model]);
        }
        let (out, err, ok) = run(command, self.timeout)?;
        let value: Value = match serde_json::from_str(out.trim()) {
            Ok(value) => value,
            Err(_) if looks_like_quota_error(&format!("{out}\n{err}")) => {
                return Err(failure(&format!("{out}\n{err}")));
            }
            Err(error) => {
                return Err(ProviderError::Failed(format!(
                    "claude did not return JSON ({error}); stderr: {}",
                    clip(&err)
                )));
            }
        };
        if !ok || value["is_error"].as_bool().unwrap_or(false) {
            return Err(failure(value["result"].as_str().unwrap_or(&err)));
        }
        let text = value["result"].as_str().unwrap_or_default().to_owned();
        let (tool, args) = extract_turn(&text)?;
        Ok(ModelTurn {
            tool,
            args,
            raw: text,
            meta: json!({"cost_usd": value["total_cost_usd"], "usage": value["usage"]}),
        })
    }

    fn codex_turn(&self, system: &str, transcript: &str) -> Result<ModelTurn, ProviderError> {
        let last = self.workdir.join(format!("last-{}.txt", self.calls));
        let mut command = Command::new("codex");
        command
            .current_dir(&self.workdir)
            .args([
                "exec",
                "-s",
                "read-only",
                "--skip-git-repo-check",
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules",
                "-C",
            ])
            .arg(&self.workdir)
            .arg("-o")
            .arg(&last);
        if let Some(model) = &self.model {
            command.args(["-m", model]);
        }
        command.arg(format!("{system}\n\n{transcript}"));
        let (_, err, ok) = run(command, self.timeout)?;
        if !ok {
            return Err(failure(&err));
        }
        let text = std::fs::read_to_string(&last)
            .map_err(|error| ProviderError::Failed(format!("no reply file: {error}")))?;
        let (tool, args) = extract_turn(&text)?;
        Ok(ModelTurn {
            tool,
            args,
            raw: text,
            meta: Value::Null,
        })
    }
}

impl LlmProvider for CliProvider {
    fn name(&self) -> String {
        let kind = match self.kind {
            CliKind::Claude => "claude-cli",
            CliKind::Codex => "codex-cli",
        };
        match &self.model {
            Some(model) => format!("{kind}:{model}"),
            None => kind.to_owned(),
        }
    }

    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        self.calls += 1;
        let system = format!(
            "{}\n\n---\n{}",
            request.system,
            protocol_text(request.tools)
        );
        let transcript = render_transcript(request.transcript);
        match self.kind {
            CliKind::Claude => self.claude_turn(&system, &transcript),
            CliKind::Codex => self.codex_turn(&system, &transcript),
        }
    }
}

/// Provider selection by configuration: `--provider` / `WAM_LLM_PROVIDER` and `--model` / `WAM_LLM_MODEL`.
///
/// `claude-cli` and `codex-cli` are for local development. `openai-compat` (DeepSeek, Meta Muse) is the
/// deployment provider: it will read `WAM_LLM_BASE_URL` and `MODEL_API_KEY`. It is attached last and is not built yet.
pub fn build_provider(
    name: &str,
    model: Option<&str>,
) -> Result<Box<dyn LlmProvider>, ProviderError> {
    let timeout = Duration::from_secs(180);
    match name {
        "claude-cli" => Ok(Box::new(CliProvider::new(CliKind::Claude, model.map(str::to_owned), timeout)?)),
        "codex-cli" => Ok(Box::new(CliProvider::new(CliKind::Codex, model.map(str::to_owned), timeout)?)),
        "openai-compat" | "deepseek" | "muse" => Err(ProviderError::NotBuilt(
            "the OpenAI-compatible HTTP provider (DeepSeek, Meta Muse) is attached last and is not built yet; \
             use claude-cli or codex-cli for local development"
                .into(),
        )),
        "fake" => Err(ProviderError::NotBuilt("the fake provider needs a script; use a scenario's script".into())),
        other => Err(ProviderError::NotBuilt(format!("unknown provider {other:?}; expected claude-cli, codex-cli, or openai-compat"))),
    }
}
