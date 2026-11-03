//! Built-in example memories for local development and demos (issue #102).
//!
//! Data is bundled in the example, productivity, design and SEO seed JSON files.
//! Each entry goes through the same validator as a published memory,
//! has "Example:" at the start of its title, and credits the official WAM publisher.
//! Publication credit does not imply independent verification. Only the local file store loads them by default: `WAM_ENV=production`
//! and PostgreSQL stay empty unless `WAM_EXAMPLES=on` says otherwise.

use serde::Deserialize;
use std::collections::HashSet;
use wam_core::{TaskVocabulary, UrlMode};
use wam_gatekeeper::record::{RecordPayload, ValidationCtx, validate};

pub const SOURCE: &str = include_str!("../seed/example_memories.json");
pub const PRODUCTIVITY_SOURCE: &str = include_str!("../seed/productivity_memories.json");
pub const DESIGN_SOURCE: &str = include_str!("../seed/design_memories.json");
pub const SEO_SOURCE: &str = include_str!("../seed/seo_memories.json");
pub const TRAVEL_SOURCE: &str = include_str!("../seed/travel_memories.json");
/// 2026-11-01T00:00:00Z, the bundled revision date. This is not an independent verification.
pub const WRITTEN_AT: u64 = 1_793_491_200;
pub const TITLE_PREFIX: &str = "Example:";
pub const ID_PREFIX: &str = "web:example-";
pub const AUTHOR_ACCOUNT_ID: &str = "builtin:wam-official";
pub const AUTHOR_HANDLE: &str = "wam_official";

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
        ("design_memories.json", DESIGN_SOURCE),
        ("seo_memories.json", SEO_SOURCE),
        ("travel_memories.json", TRAVEL_SOURCE),
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
    fn refresh_is_persisted_under_the_repository_lock() {
        let directory = std::env::temp_dir().join(format!(
            "wam-example-refresh-{}",
            crate::diagnostics::random_id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("application.json");
        let repository = crate::ApplicationRepository::open(&path).unwrap();
        let current = load().unwrap().remove(0);
        let mut older = current.clone();
        older.payload.l1.summary = "An older bundled workflow.".into();
        repository
            .update(|state| {
                state.add_examples(&[older.clone()]);
                Ok(())
            })
            .unwrap();
        assert!(
            crate::ApplicationRepository::open(&path).is_err(),
            "a live store must stay exclusively locked"
        );
        assert_eq!(
            repository.ensure_examples().unwrap(),
            load().unwrap().len() - 1
        );
        assert_eq!(repository.refresh_examples().unwrap().updated, 1);
        drop(repository);
        let reopened = crate::ApplicationRepository::open(&path).unwrap();
        reopened
            .read(|state| {
                let versions = &state.catalog.memories[&current.id].versions;
                assert_eq!(versions.len(), 2);
                assert_eq!(versions[0].payload, older.payload);
                assert_eq!(versions[1].payload, current.payload);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            reopened.refresh_examples().unwrap(),
            RefreshReport::default()
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

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
