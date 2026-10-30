use std::process::Command;
use wam_cli::install::{Host, Scope, plan};
use wam_client::ApiError;

#[test]
fn registration_plans_use_safe_shared_templates_and_do_not_embed_tokens() {
    let codex = plan(
        Host::Codex,
        Scope::User,
        "https://wam.example",
        false,
        None,
        false,
    )
    .unwrap();
    assert_eq!(
        codex.args,
        [
            "mcp",
            "add",
            "web-memory",
            "--url",
            "https://wam.example/mcp"
        ]
    );
    let claude = plan(
        Host::ClaudeCode,
        Scope::Project,
        "https://wam.example",
        false,
        None,
        false,
    )
    .unwrap();
    assert_eq!(
        claude.args,
        [
            "mcp",
            "add",
            "--transport",
            "http",
            "--scope",
            "project",
            "web-memory",
            "https://wam.example/mcp"
        ]
    );
    assert!(matches!(
        plan(
            Host::Codex,
            Scope::Project,
            "https://wam.example",
            false,
            None,
            false
        ),
        Err(ApiError::InvalidInput)
    ));
    assert!(
        plan(
            Host::Codex,
            Scope::User,
            "https://user:secret@wam.example",
            false,
            None,
            false
        )
        .is_err()
    );
}

#[test]
fn local_registration_uses_an_absolute_binary_path() {
    let local = plan(
        Host::Codex,
        Scope::User,
        "https://wam.example",
        true,
        Some(std::path::Path::new("./target/debug/wam-mcp")),
        false,
    )
    .unwrap();
    assert!(std::path::Path::new(local.args.last().unwrap()).is_absolute());
}

#[test]
fn dry_run_is_json_and_does_not_require_the_host_or_a_network() {
    let output = Command::new(env!("CARGO_BIN_EXE_wam"))
        .args([
            "--endpoint",
            "https://wam.example",
            "install",
            "codex",
            "--dry-run",
            "--json",
        ])
        .env("WAM_TOKEN", "sentinel-secret")
        .env("PATH", "/missing-wam-test")
        .output()
        .unwrap();
    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["program"], "codex");
    assert_eq!(plan["transport"], "http");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("sentinel-secret"));
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn host_registration_is_idempotent_conflict_safe_and_removal_preserves_other_servers() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "wam-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let executable = directory.join("codex");
    std::fs::write(
        &executable,
        r#"#!/bin/sh
case "$1:$2" in
mcp:get)
  if [ -f "$WAM_TEST_STATE" ]; then /bin/cat "$WAM_TEST_STATE"; else
    echo "No MCP server named web-memory found." >&2
    exit 1
  fi ;;
mcp:add)
  echo '{"transport":{"url":"https://wam.example/mcp"}}' > "$WAM_TEST_STATE" ;;
mcp:remove)
  /bin/rm "$WAM_TEST_STATE" ;;
*) exit 2 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = directory.join("web-memory.json");
    let other = directory.join("other-server.json");
    std::fs::write(&other, b"preserve this config").unwrap();
    let run = |endpoint: &str, action: &str| {
        Command::new(env!("CARGO_BIN_EXE_wam"))
            .args(["--endpoint", endpoint, action, "codex", "--json"])
            .env("PATH", &directory)
            .env("WAM_TEST_STATE", &state)
            .output()
            .unwrap()
    };
    for (action, status) in [("install", "registered"), ("install", "already_registered")] {
        let output = run("https://wam.example", action);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["status"], status);
        assert_eq!(value["connection_verified"], false);
    }
    let before = std::fs::read(&state).unwrap();
    let conflict = run("https://different.example", "install");
    assert_eq!(conflict.status.code(), Some(4));
    let _: serde_json::Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(std::fs::read(&state).unwrap(), before);
    for status in ["removed", "already_absent"] {
        let output = run("https://wam.example", "uninstall");
        assert!(output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["status"], status);
    }
    assert_eq!(std::fs::read(&other).unwrap(), b"preserve this config");
    std::fs::remove_dir_all(directory).unwrap();
}
