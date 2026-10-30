//! Offline instruction snippets. Only --write changes the current project's instruction file.
use crate::install::Host;
use clap::Subcommand;
use std::{
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const BEGIN: &str = "<!-- wam:begin -->";
const END: &str = "<!-- wam:end -->";
const INSTRUCTIONS: &str = include_str!("../../../guides/recall-instructions.md");
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Subcommand)]
pub enum GuideAction {
    /// Print recall instructions, or explicitly update the host instruction file in this directory.
    Print {
        #[arg(long, value_enum, default_value = "codex")]
        host: Host,
        #[arg(long)]
        write: bool,
    },
}

#[derive(Debug)]
pub enum GuideError {
    FileAccess,
    InvalidFile,
    AmbiguousMarkers,
    Busy,
    Changed,
}
impl fmt::Display for GuideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FileAccess => "could not read or update the instruction file",
            Self::InvalidFile => "instruction file must be a regular, writable UTF-8 file of at most 1 MiB; symlinks are not supported",
            Self::AmbiguousMarkers => "instruction file has incomplete, duplicated, or inline WAM markers; repair the markers before retrying",
            Self::Busy => "another guide update holds .wam-guide.lock; retry after it finishes (remove a stale lock only after checking no writer is active)",
            Self::Changed => "instruction file changed during this update; no changes were committed; retry",
        })
    }
}
impl std::error::Error for GuideError {}

pub fn render() -> String {
    format!("{BEGIN}\n{}\n{END}\n", INSTRUCTIONS.trim_end())
}

fn merge(original: &str) -> Result<String, GuideError> {
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let block = render().trim_end().replace('\n', newline);
    let starts: Vec<_> = original.match_indices(BEGIN).map(|(i, _)| i).collect();
    let ends: Vec<_> = original.match_indices(END).map(|(i, _)| i).collect();
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => {
            let separator =
                if original.is_empty() || original.ends_with(&format!("{newline}{newline}")) {
                    String::new()
                } else if original.ends_with('\n') {
                    newline.to_owned()
                } else {
                    format!("{newline}{newline}")
                };
            Ok(format!("{original}{separator}{block}{newline}"))
        }
        ([start], [end])
            if start < end
                && marker_is_line(original, *start, BEGIN)
                && marker_is_line(original, *end, END) =>
        {
            Ok(format!(
                "{}{}{}",
                &original[..*start],
                block,
                &original[end + END.len()..]
            ))
        }
        _ => Err(GuideError::AmbiguousMarkers),
    }
}

fn marker_is_line(text: &str, index: usize, marker: &str) -> bool {
    let after = &text[index + marker.len()..];
    (index == 0 || text.as_bytes()[index - 1] == b'\n')
        && (after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n"))
}

struct GuideLock {
    path: PathBuf,
    file: Option<fs::File>,
}
impl Drop for GuideLock {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read_file(path: &Path) -> Result<Option<(Vec<u8>, fs::Permissions)>, GuideError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(GuideError::FileAccess),
    };
    if !metadata.is_file()
        || metadata.is_symlink()
        || metadata.permissions().readonly()
        || metadata.len() > MAX_FILE_BYTES
    {
        return Err(GuideError::InvalidFile);
    }
    let mut contents = Vec::new();
    fs::File::open(path)
        .map_err(|_| GuideError::FileAccess)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(|_| GuideError::FileAccess)?;
    if contents.len() as u64 > MAX_FILE_BYTES {
        return Err(GuideError::InvalidFile);
    }
    Ok(Some((contents, metadata.permissions())))
}

pub fn write_in_directory(directory: &Path, host: Host) -> Result<&'static str, GuideError> {
    let filename = match host {
        Host::Codex => "AGENTS.md",
        Host::ClaudeCode => "CLAUDE.md",
    };
    let target = directory.join(filename);
    let lock = directory.join(".wam-guide.lock");
    let lock_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                GuideError::Busy
            } else {
                GuideError::FileAccess
            }
        })?;
    let _lock_guard = GuideLock {
        path: lock,
        file: Some(lock_file),
    };
    let snapshot = read_file(&target)?;
    let original = snapshot
        .as_ref()
        .map(|(text, _)| text.as_slice())
        .unwrap_or_default();
    let original_text = std::str::from_utf8(original).map_err(|_| GuideError::InvalidFile)?;
    let updated = merge(original_text)?;
    if updated.as_bytes() == original {
        return Ok(filename);
    }
    if updated.len() as u64 > MAX_FILE_BYTES {
        return Err(GuideError::InvalidFile);
    }

    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| GuideError::FileAccess)?;
    let id: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = directory.join(format!(".wam-guide-{id}.tmp"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| GuideError::FileAccess)?;
    let _temp_guard = RemoveOnDrop(temporary.clone());
    if let Some((_, permissions)) = &snapshot {
        file.set_permissions(permissions.clone())
            .map_err(|_| GuideError::FileAccess)?;
    }
    file.write_all(updated.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| GuideError::FileAccess)?;
    drop(file);

    // Detect other cooperative writers and common editor changes before the atomic replacement.
    let current = read_file(&target)?;
    if current.as_ref().map(|(bytes, _)| bytes) != snapshot.as_ref().map(|(bytes, _)| bytes) {
        return Err(GuideError::Changed);
    }
    fs::rename(&temporary, &target).map_err(|_| GuideError::FileAccess)?;
    Ok(filename)
}

pub fn execute(action: &GuideAction) -> Result<String, GuideError> {
    match action {
        GuideAction::Print { write: false, .. } => Ok(render()),
        GuideAction::Print { host, write: true } => {
            let directory = std::env::current_dir().map_err(|_| GuideError::FileAccess)?;
            let filename = write_in_directory(&directory, *host)?;
            Ok(format!("Updated the WAM block in {filename}"))
        }
    }
}
