use super::protocol::{clip, failure};
use super::{
    LlmProvider, ModelTurn, ProviderError, TurnRequest, extract_turn, looks_like_quota_error,
    protocol_text, render_transcript,
};
use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_names_are_stable_and_private_workdirs_are_removed() {
        for (kind, expected) in [
            (CliKind::Claude, "claude-cli:fixture-model"),
            (CliKind::Codex, "codex-cli:fixture-model"),
        ] {
            let provider =
                CliProvider::new(kind, Some("fixture-model".into()), Duration::from_secs(1))
                    .unwrap();
            let workdir = provider.workdir.clone();
            assert_eq!(provider.name(), expected);
            assert!(workdir.is_dir());
            assert_eq!(std::fs::read_dir(&workdir).unwrap().count(), 0);
            drop(provider);
            assert!(!workdir.exists());
        }

        let provider = CliProvider::new(CliKind::Claude, None, Duration::from_secs(1)).unwrap();
        assert_eq!(provider.name(), "claude-cli");
    }

    #[cfg(unix)]
    #[test]
    fn command_runner_captures_both_streams_and_reports_exit_status() {
        let mut successful = Command::new("/bin/sh");
        successful.args(["-c", "printf stdout; printf stderr >&2"]);
        assert_eq!(
            run(successful, Duration::from_secs(1)).unwrap(),
            ("stdout".into(), "stderr".into(), true)
        );

        let mut failed = Command::new("/bin/sh");
        failed.args(["-c", "printf rejected; exit 7"]);
        assert_eq!(
            run(failed, Duration::from_secs(1)).unwrap(),
            ("rejected".into(), String::new(), false)
        );
    }

    #[cfg(unix)]
    #[test]
    fn command_runner_kills_a_process_that_exceeds_its_deadline() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 5"]);
        assert!(matches!(
            run(command, Duration::from_millis(30)),
            Err(ProviderError::Timeout(0))
        ));
    }
}
