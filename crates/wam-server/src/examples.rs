//! Built-in example memories for local development and demos (issue #102).
//!
//! The data is in `seed/example_memories.json` and `seed/productivity_memories.json`.
//! Each entry goes through the same validator as a published memory,
//! has "Example:" at the start of its title and no author, so it can never be mistaken for verified catalog
//! knowledge and never earns anybody points. Only the local file store loads them by default: `WAM_ENV=production`
//! and PostgreSQL stay empty unless `WAM_EXAMPLES=on` says otherwise.

use serde::Deserialize;
use std::collections::HashSet;
use wam_core::{TaskVocabulary, UrlMode};
use wam_gatekeeper::record::{RecordPayload, ValidationCtx, validate};

pub const SOURCE: &str = include_str!("../seed/example_memories.json");
pub const PRODUCTIVITY_SOURCE: &str = include_str!("../seed/productivity_memories.json");
/// 2026-10-30T00:00:00Z, when the examples were written. It is shown as their date; it is not a verification.
pub const WRITTEN_AT: u64 = 1_793_318_400;
pub const TITLE_PREFIX: &str = "Example:";
pub const ID_PREFIX: &str = "web:example-";

/// Explicit refresh results. Protected catalog entries are never rewritten.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RefreshReport {
    pub added: usize,
    pub updated: usize,
    pub protected: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub id: String,
    pub payload: RecordPayload,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ExampleError(pub String);

pub fn task_vocabulary() -> TaskVocabulary {
    TaskVocabulary::new("2026-01", ["compose-draft", "navigate", "research"])
        .expect("static task codes")
}

/// Parses the bundled files and runs every entry through the gatekeeper's record validator.
pub fn load() -> Result<Vec<Example>, ExampleError> {
    let mut raw = Vec::new();
    for (name, source) in [
        ("example_memories.json", SOURCE),
        ("productivity_memories.json", PRODUCTIVITY_SOURCE),
    ] {
        let entries: Vec<Example> = serde_json::from_str(source)
            .map_err(|error| ExampleError(format!("{name}: {error}")))?;
        raw.extend(entries);
    }
    let tasks = task_vocabulary();
    let mut seen = HashSet::new();
    let mut checked = Vec::with_capacity(raw.len());
    for example in raw {
        let fail = |reason: String| ExampleError(format!("{}: {reason}", example.id));
        if !example.id.starts_with(ID_PREFIX)
            || example.id.len() > 64
            || !seen.insert(example.id.clone())
        {
            return Err(fail(
                "id must be unique, start with web:example- and stay within 64 characters".into(),
            ));
        }
        if !example.payload.l1.title.starts_with(TITLE_PREFIX) {
            return Err(fail("title must start with Example:".into()));
        }
        let l0 = &example.payload.l0;
        let context = ValidationCtx {
            host: &l0.site_host,
            task: l0.task_category.as_deref().unwrap_or_default(),
            intent_key: l0.intent_key.as_deref().unwrap_or_default(),
            tasks: &tasks,
            mode: UrlMode::Public,
        };
        let validated = validate(example.payload.clone(), &context)
            .map_err(|error| fail(format!("{:?}: {}", error.code, error.message)))?;
        checked.push(Example {
            id: example.id.clone(),
            payload: validated.payload,
        });
    }
    Ok(checked)
}

/// `WAM_EXAMPLES` is `on` or `off`; anything else (or nothing) keeps `default_on`.
pub fn enabled(default_on: bool, setting: Option<&str>) -> bool {
    match setting
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("on" | "1" | "true" | "yes" | "refresh") => true,
        Some("off" | "0" | "false" | "no") => false,
        _ => default_on,
    }
}

/// Refresh is deliberately opt-in; ordinary startup only tops up missing examples.
pub fn refresh_requested(setting: Option<&str>) -> bool {
    setting.is_some_and(|value| value.trim().eq_ignore_ascii_case("refresh"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_overrides_the_default_in_both_directions() {
        assert!(enabled(true, None));
        assert!(!enabled(false, None));
        assert!(enabled(false, Some(" ON ")));
        assert!(!enabled(true, Some("off")));
        assert!(enabled(true, Some("maybe")));
        assert!(enabled(false, Some(" refresh ")));
        assert!(refresh_requested(Some(" REFRESH ")));
        assert!(!refresh_requested(Some("on")));
        assert!(!refresh_requested(None));
    }
}
