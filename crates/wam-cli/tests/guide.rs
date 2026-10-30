use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).unwrap();
        let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = std::env::temp_dir().join(format!("wam-guide-test-{id}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_wam"))
            .args(args)
            .current_dir(&self.0)
            .env("WAM_ENDPOINT", "invalid endpoint")
            .env("WAM_CONFIG_DIR", self.0.join("config"))
            .env("WAM_TOKEN", "guide-secret-sentinel")
            .env("WAM_TELEMETRY", "off")
            .output()
            .unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn guide_print_is_offline_and_leaves_instruction_files_untouched() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("AGENTS.md"), "Existing instructions\n").unwrap();
    let output = workspace.run(&["guide", "print", "--host", "codex"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snippet = String::from_utf8(output.stdout).unwrap();
    for required in [
        "wam recall --stdin --json",
        "5 minutes",
        "untrusted",
        "user",
        "consent",
        "<!-- wam:begin -->",
        "<!-- wam:end -->",
    ] {
        assert!(snippet.contains(required), "missing {required}");
    }
    assert!(!snippet.contains("guide-secret-sentinel"));
    assert!(!snippet.contains("invalid endpoint"));
    assert_eq!(
        fs::read_to_string(workspace.0.join("AGENTS.md")).unwrap(),
        "Existing instructions\n"
    );
    assert_eq!(fs::read_dir(&workspace.0).unwrap().count(), 1);
}

#[test]
fn guide_write_preserves_outside_text_and_is_idempotent_for_each_host() {
    for (host, filename) in [("codex", "AGENTS.md"), ("claude-code", "CLAUDE.md")] {
        let workspace = Workspace::new();
        let path = workspace.0.join(filename);
        fs::write(&path, "# Human rules\r\nDo this first.\r\n\r\n<!-- wam:begin -->\r\nold generated text\r\n<!-- wam:end -->\r\n\r\nKeep this exact tail.").unwrap();
        let output = workspace.run(&["guide", "print", "--host", host, "--write"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.starts_with("# Human rules\r\nDo this first.\r\n\r\n"));
        assert!(updated.ends_with("\r\n\r\nKeep this exact tail."));
        assert_eq!(updated.matches("<!-- wam:begin -->").count(), 1);
        assert!(!updated.contains("old generated text"));
        assert!(!updated.replace("\r\n", "").contains('\n'));
        assert!(
            workspace
                .run(&["guide", "print", "--host", host, "--write"])
                .status
                .success()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), updated);
        assert_eq!(fs::read_dir(&workspace.0).unwrap().count(), 1);
    }
}

#[test]
fn guide_write_creates_only_the_selected_file_and_appends_without_truncation() {
    let workspace = Workspace::new();
    assert!(
        workspace
            .run(&["guide", "print", "--write"])
            .status
            .success()
    );
    assert!(workspace.0.join("AGENTS.md").is_file());
    assert!(!workspace.0.join("CLAUDE.md").exists());
    fs::write(
        workspace.0.join("CLAUDE.md"),
        "Keep without trailing newline",
    )
    .unwrap();
    assert!(
        workspace
            .run(&["guide", "print", "--host", "claude-code", "--write"])
            .status
            .success()
    );
    let written = fs::read_to_string(workspace.0.join("CLAUDE.md")).unwrap();
    assert!(written.starts_with("Keep without trailing newline\n\n<!-- wam:begin -->"));
}

#[test]
fn guide_refuses_ambiguous_blocks_and_preserves_the_original() {
    let workspace = Workspace::new();
    let path = workspace.0.join("AGENTS.md");
    for text in [
        "Rules\n<!-- wam:begin -->\nmissing end",
        "<!-- wam:end -->\n<!-- wam:begin -->",
        "<!-- wam:begin -->\n<!-- wam:end -->\n<!-- wam:begin -->\n<!-- wam:end -->",
        "Example <!-- wam:begin -->\n<!-- wam:end -->",
    ] {
        fs::write(&path, text).unwrap();
        assert!(
            !workspace
                .run(&["guide", "print", "--write"])
                .status
                .success()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        assert_eq!(fs::read_dir(&workspace.0).unwrap().count(), 1);
    }
}

#[test]
fn guide_refuses_oversized_non_utf8_and_locked_files() {
    let workspace = Workspace::new();
    let path = workspace.0.join("AGENTS.md");
    for data in [vec![b'x'; 1024 * 1024 + 1], vec![0xff]] {
        fs::write(&path, &data).unwrap();
        assert!(
            !workspace
                .run(&["guide", "print", "--write"])
                .status
                .success()
        );
        assert_eq!(fs::read(&path).unwrap(), data);
    }
    fs::write(&path, "Keep").unwrap();
    fs::write(workspace.0.join(".wam-guide.lock"), "").unwrap();
    assert!(
        !workspace
            .run(&["guide", "print", "--write"])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "Keep");
}

#[cfg(unix)]
#[test]
fn guide_refuses_symlinks_and_preserves_file_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let workspace = Workspace::new();
    let outside = workspace.0.join("human.md");
    let path = workspace.0.join("AGENTS.md");
    fs::write(&outside, "Human rules").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(
        !workspace
            .run(&["guide", "print", "--write"])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&outside).unwrap(), "Human rules");
    assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
    fs::remove_file(&path).unwrap();
    fs::write(&path, "Human rules").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        workspace
            .run(&["guide", "print", "--write"])
            .status
            .success()
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
