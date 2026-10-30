//! Host registration uses the host CLI and shared templates; it never edits host config files.
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use wam_client::{ApiError, credentials::config_dir, normalize_endpoint};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Host {
    Codex,
    ClaudeCode,
}
impl Host {
    fn key(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
}
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Scope {
    User,
    Local,
    Project,
}
impl Scope {
    fn value(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Local => "local",
            Self::Project => "project",
        }
    }
}

#[derive(Deserialize)]
struct Templates {
    hosts: HashMap<String, Template>,
}
#[derive(Deserialize)]
struct Template {
    program: String,
    remote: Vec<String>,
    local: Vec<String>,
    get: Vec<String>,
    remove: Vec<String>,
}

#[derive(Serialize)]
pub struct InstallPlan {
    pub schema_version: u8,
    pub host: &'static str,
    pub program: String,
    pub args: Vec<String>,
    pub transport: &'static str,
    #[serde(skip)]
    host_kind: Host,
    #[serde(skip)]
    scope: Scope,
    #[serde(skip)]
    local: bool,
    #[serde(skip)]
    endpoint: String,
    #[serde(skip)]
    binary: String,
    #[serde(skip)]
    config_dir: String,
    #[serde(skip)]
    get: Vec<String>,
    #[serde(skip)]
    remove: bool,
}

pub fn plan(
    host: Host,
    scope: Scope,
    endpoint: &str,
    local: bool,
    binary: Option<&Path>,
    remove: bool,
) -> Result<InstallPlan, ApiError> {
    if matches!(host, Host::Codex) && !matches!(scope, Scope::User) {
        return Err(ApiError::InvalidInput); // This Codex CLI exposes user registration only.
    }
    let endpoint = normalize_endpoint(endpoint)?;
    let config = config_dir();
    let config = if config.is_absolute() {
        config
    } else {
        std::env::current_dir()
            .map_err(|_| ApiError::InvalidInput)?
            .join(config)
    };
    let config = config.to_str().ok_or(ApiError::InvalidInput)?.to_owned();
    let binary = match binary {
        Some(path) => std::path::absolute(path)
            .map_err(|_| ApiError::InvalidInput)?
            .to_str()
            .ok_or(ApiError::InvalidInput)?
            .to_owned(),
        None => default_mcp_binary().to_string_lossy().into_owned(),
    };
    let templates: Templates =
        serde_json::from_str(include_str!("../../../guides/install-templates.json"))
            .map_err(|_| ApiError::InvalidResponse)?;
    let template = templates
        .hosts
        .get(host.key())
        .ok_or(ApiError::InvalidResponse)?;
    let expand = |args: &[String]| {
        args.iter()
            .map(|arg| {
                arg.replace("{mcp_endpoint}", &format!("{endpoint}/mcp"))
                    .replace("{endpoint}", &endpoint)
                    .replace("{scope}", scope.value())
                    .replace("{config_dir}", &config)
                    .replace("{mcp_binary}", &binary)
            })
            .collect::<Vec<_>>()
    };
    let args = if remove {
        &template.remove
    } else if local {
        &template.local
    } else {
        &template.remote
    };
    let args = expand(args);
    let get = expand(&template.get);
    Ok(InstallPlan {
        schema_version: 1,
        host: host.key(),
        program: template.program.clone(),
        args,
        transport: if local { "stdio" } else { "http" },
        host_kind: host,
        scope,
        local,
        endpoint,
        binary,
        config_dir: config,
        get,
        remove,
    })
}

fn default_mcp_binary() -> PathBuf {
    let name = if cfg!(windows) {
        "wam-mcp.exe"
    } else {
        "wam-mcp"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

fn matching(plan: &InstallPlan, output: &[u8]) -> bool {
    if matches!(plan.host_kind, Host::Codex) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(output) else {
            return false;
        };
        let transport = &value["transport"];
        if !plan.local {
            return transport["url"].as_str() == Some(format!("{}/mcp", plan.endpoint).as_str());
        }
        transport["command"].as_str() == Some(plan.binary.as_str())
            && transport["args"]
                .as_array()
                .is_some_and(|args| args.is_empty())
            && transport["env"]["WAM_ENDPOINT"].as_str() == Some(plan.endpoint.as_str())
            && transport["env"]["WAM_CONFIG_DIR"].as_str() == Some(plan.config_dir.as_str())
    } else {
        let text = String::from_utf8_lossy(output);
        let lines: Vec<_> = text.lines().map(str::trim).collect();
        let expected_scope = match plan.scope {
            Scope::User => "Scope: User config",
            Scope::Local => "Scope: Local config",
            Scope::Project => "Scope: Project config",
        };
        if !lines.iter().any(|line| line.starts_with(expected_scope)) {
            return false;
        }
        if !plan.local {
            lines.contains(&format!("URL: {}/mcp", plan.endpoint).as_str())
                && lines.contains(&"Type: http")
        } else {
            lines.contains(&"Type: stdio")
                && lines.contains(&format!("Command: {}", plan.binary).as_str())
                && lines.contains(&"Args:")
                && lines.contains(&format!("WAM_ENDPOINT={}", plan.endpoint).as_str())
                && lines.contains(&format!("WAM_CONFIG_DIR={}", plan.config_dir).as_str())
        }
    }
}

async fn run(program: &str, args: &[String]) -> Result<std::process::Output, &'static str> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .env_remove("WAM_TOKEN")
        .env_remove("MCP_CLIENT_SECRET");
    match tokio::time::timeout(Duration::from_secs(15), command.output()).await {
        Err(_) => Err("host command timed out"),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err("host CLI is not installed or not on PATH")
        }
        Ok(Err(_)) => Err("host CLI could not be started"),
        Ok(Ok(output)) => Ok(output),
    }
}

fn absent(output: &std::process::Output) -> bool {
    let error = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    let out = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    [error, out]
        .iter()
        .any(|text| text.contains("no mcp server") && text.contains("web-memory"))
}

pub async fn execute(
    plan: &InstallPlan,
    dry_run: bool,
    json: bool,
) -> Result<(String, i32), ApiError> {
    if dry_run {
        return Ok((
            if json {
                crate::render_json(plan)?
            } else {
                display_command(plan)
            },
            0,
        ));
    }
    let existing = match run(&plan.program, &plan.get).await {
        Ok(output) => output,
        Err(message) => {
            return failure(
                plan,
                json,
                1,
                &format!(
                    "{message}. Run this command after installing the host CLI:\n{}",
                    display_command(plan)
                ),
            );
        }
    };
    let exists = existing.status.success();
    if !exists && !absent(&existing) {
        return failure(
            plan,
            json,
            1,
            "Host registration could not be inspected; check the host CLI configuration.",
        );
    }
    let status;
    if plan.remove && !exists {
        status = "already_absent";
    } else if !plan.remove && exists {
        if !matching(plan, &existing.stdout) {
            return failure(
                plan,
                json,
                4,
                "A different web-memory registration already exists; inspect it with the host CLI before replacing it.",
            );
        }
        status = "already_registered";
    } else {
        if plan.local && !plan.remove && !binary_available(&plan.binary) {
            return failure(
                plan,
                json,
                1,
                "wam-mcp was not found; install it or pass --mcp-bin with its absolute path.",
            );
        }
        match run(&plan.program, &plan.args).await {
            Ok(output) if output.status.success() => {}
            _ => {
                return failure(
                    plan,
                    json,
                    1,
                    "Host registration command failed; inspect the host CLI configuration.",
                );
            }
        }
        status = if plan.remove { "removed" } else { "registered" };
    }
    let output = if json {
        crate::render_json(&serde_json::json!({
            "schema_version": 1, "host": plan.host, "status": status,
            "connection_verified": false,
        }))?
    } else {
        format!(
            "Host registration: {status}.\nRun {} mcp get web-memory to inspect it.\nDevice credentials and cache are shared; use wam unlink and wam cache clear to remove them.",
            plan.program
        )
    };
    Ok((output, 0))
}

fn failure(
    plan: &InstallPlan,
    json: bool,
    code: i32,
    message: &str,
) -> Result<(String, i32), ApiError> {
    let output = if json {
        crate::render_json(
            &serde_json::json!({ "schema_version": 1, "status": "error", "error": message, "plan": plan }),
        )?
    } else {
        message.to_owned()
    };
    Ok((output, code))
}

fn binary_available(binary: &str) -> bool {
    let path = Path::new(binary);
    if path.is_absolute() || path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(binary).is_file()))
}

pub fn display_command(plan: &InstallPlan) -> String {
    std::iter::once(plan.program.as_str())
        .chain(plan.args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}
fn shell_quote(value: &str) -> String {
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_./:=[]".contains(&b))
        && !value.is_empty()
    {
        value.to_owned()
    } else if cfg!(windows) {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
