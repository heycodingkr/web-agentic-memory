//! Storage seam for curated memory. `MemStore` serves tests and local development; a Postgres adapter
//! (docs/spec/database.md) implements the same trait later.

use crate::record::{RecordPayload, build_pattern, kind_str, parse_kind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wam_core::{KeywordVocabulary, MemoryIndex, MemoryStatus, TaskVocabulary, UrlMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    NeedsReverify,
    Superseded,
    Retired,
}

impl Status {
    pub fn recallable(self) -> bool {
        matches!(self, Self::Active | Self::NeedsReverify)
    }

    fn core(self) -> MemoryStatus {
        match self {
            Self::Active => MemoryStatus::Active,
            Self::NeedsReverify => MemoryStatus::NeedsReverify,
            Self::Superseded => MemoryStatus::Superseded,
            Self::Retired => MemoryStatus::Retired,
        }
    }
}

/// `(site_host, canonical_url_pattern, kind, task_category, intent_key)` from docs/spec/dedup.md and memory.md.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ScopeKey {
    pub host: String,
    pub pattern: String,
    pub kind: String,
    pub task: String,
    pub intent: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub independent_contributors: u32,
    pub success: u32,
    #[serde(default)]
    pub partial: u32,
    #[serde(default)]
    pub failed: u32,
    #[serde(default)]
    pub ui_fingerprint: Option<String>,
}

impl Evidence {
    pub fn success_rate(&self) -> f64 {
        let total = self.success + self.partial + self.failed;
        if total == 0 {
            0.0
        } else {
            f64::from(self.success) / f64::from(total)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionRow {
    pub version: u32,
    pub payload: RecordPayload,
    pub fingerprint: String,
    pub verified_at: u64,
    pub status: Status,
    pub evidence: Evidence,
    pub job_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: String,
    pub scope: ScopeKey,
    pub versions: Vec<VersionRow>,
    pub support_count: u32,
    pub status: Status,
    pub confidence_band: u8,
}

impl MemoryRow {
    /// The version served to agents: the newest one, if the memory is still recallable.
    pub fn active(&self) -> Option<&VersionRow> {
        if self.status.recallable() {
            self.versions.last()
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub job_id: String,
    pub provider: String,
    pub instructions_hash: String,
    pub tools_hash: String,
    pub job_digest: String,
    pub calls: Vec<serde_json::Value>,
    pub decision: Option<serde_json::Value>,
    pub outcome: String,
    pub at: u64,
}

pub trait Store {
    /// Recallable memories (active or needs-reverify) of one host.
    fn memories_in_host(&self, host: &str) -> Vec<&MemoryRow>;
    fn get(&self, id: &str) -> Option<&MemoryRow>;
    fn find_by_scope(&self, scope: &ScopeKey) -> Option<&MemoryRow>;
    fn create(&mut self, scope: ScopeKey, first: VersionRow, confidence_band: u8) -> String;
    /// Adds a new active version and marks the previous one superseded. Returns the new version number.
    fn supersede(&mut self, id: &str, next: VersionRow, confidence_band: u8) -> u32;
    fn add_support(&mut self, id: &str, count: u32);
    fn retire(&mut self, id: &str);
    fn log_audit(&mut self, audit: AuditRecord);
    /// Was this exact job (same id, same content digest) already decided? A retried queue batch must not apply
    /// the same decision, or the same evidence, twice.
    fn decided(&self, job_id: &str, job_digest: &str) -> bool;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemStore {
    pub memories: BTreeMap<String, MemoryRow>,
    pub audits: Vec<AuditRecord>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn new_id(&self, scope: &ScopeKey) -> String {
        let host = scope
            .host
            .split(':')
            .next()
            .unwrap_or(&scope.host)
            .replace('.', "-");
        let base = format!("web:{host}-{}", scope.intent);
        if !self.memories.contains_key(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|candidate| !self.memories.contains_key(candidate))
            .expect("an unused suffix exists")
    }

    /// The wam-core L0 projection of the recallable memories, for `wam_core::recall`.
    pub fn indexes(&self, tasks: &TaskVocabulary, mode: UrlMode) -> Vec<MemoryIndex> {
        let mut out = Vec::new();
        for row in self.memories.values() {
            let Some(version) = row.active() else {
                continue;
            };
            let l0 = &version.payload.l0;
            let (Some(kind), Some(task)) = (parse_kind(&l0.kind), l0.task_category.as_deref())
            else {
                continue;
            };
            let Ok(task) = tasks.resolve(task) else {
                continue;
            };
            let patterns: Vec<_> = l0
                .url_patterns
                .iter()
                .filter_map(|route| build_pattern(&l0.site_host, route, mode).ok())
                .collect();
            let keywords = if l0.keywords.is_empty() {
                Vec::new()
            } else {
                match KeywordVocabulary::new("index", &l0.keywords) {
                    Ok(vocabulary) => l0
                        .keywords
                        .iter()
                        .filter_map(|k| vocabulary.resolve(k))
                        .collect(),
                    Err(_) => continue,
                }
            };
            let version_id = format!("{}.v{}", row.id.replace(':', "-"), version.version);
            if let Ok(index) = MemoryIndex::new(
                row.id.clone(),
                version_id,
                patterns,
                task,
                kind,
                l0.intent_key.clone().unwrap_or_default(),
                keywords,
                row.confidence_band,
                row.status.core(),
                version.verified_at,
            ) {
                out.push(index);
            }
        }
        out
    }
}

impl Store for MemStore {
    fn memories_in_host(&self, host: &str) -> Vec<&MemoryRow> {
        self.memories
            .values()
            .filter(|row| row.scope.host == host && row.status.recallable())
            .collect()
    }

    fn get(&self, id: &str) -> Option<&MemoryRow> {
        self.memories.get(id)
    }

    fn find_by_scope(&self, scope: &ScopeKey) -> Option<&MemoryRow> {
        self.memories
            .values()
            .find(|row| row.status.recallable() && &row.scope == scope)
    }

    fn create(&mut self, scope: ScopeKey, first: VersionRow, confidence_band: u8) -> String {
        let id = self.new_id(&scope);
        debug_assert_eq!(
            kind_str(parse_kind(&scope.kind).expect("valid kind")),
            scope.kind
        );
        let row = MemoryRow {
            id: id.clone(),
            scope,
            status: first.status,
            versions: vec![first],
            support_count: 0,
            confidence_band,
        };
        self.memories.insert(id.clone(), row);
        id
    }

    fn supersede(&mut self, id: &str, mut next: VersionRow, confidence_band: u8) -> u32 {
        let row = self.memories.get_mut(id).expect("memory exists");
        if let Some(previous) = row.versions.last_mut() {
            previous.status = Status::Superseded;
        }
        next.version = row.versions.last().map_or(1, |v| v.version + 1);
        next.status = Status::Active;
        let number = next.version;
        row.versions.push(next);
        row.status = Status::Active;
        row.confidence_band = confidence_band;
        number
    }

    fn add_support(&mut self, id: &str, count: u32) {
        if let Some(row) = self.memories.get_mut(id) {
            row.support_count += count;
        }
    }

    fn retire(&mut self, id: &str) {
        if let Some(row) = self.memories.get_mut(id) {
            row.status = Status::Retired;
            if let Some(last) = row.versions.last_mut() {
                last.status = Status::Retired;
            }
        }
    }

    fn log_audit(&mut self, audit: AuditRecord) {
        self.audits.push(audit);
    }

    fn decided(&self, job_id: &str, job_digest: &str) -> bool {
        self.audits.iter().any(|audit| {
            audit.job_id == job_id && audit.job_digest == job_digest && audit.outcome == "decided"
        })
    }
}
