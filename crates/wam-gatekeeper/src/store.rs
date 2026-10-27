//! Storage seam for curated memory. `MemStore` serves tests and local development; durable adapters implement the
//! same fallible reads and atomic decision commit contract.

use crate::queue::QueueLeaseFence;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    Unavailable,
    Conflict,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryMutation {
    None,
    AddSupport {
        memory_id: String,
        count: u32,
    },
    Create {
        scope: ScopeKey,
        version: VersionRow,
        confidence_band: u8,
    },
    Supersede {
        memory_id: String,
        expected_version: u32,
        next: VersionRow,
        confidence_band: u8,
    },
    Retire {
        memory_id: String,
        expected_version: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecisionCommit {
    pub job_id: String,
    pub job_digest: String,
    pub kind: String,
    pub reason_codes: Vec<String>,
    pub confidence_band: Option<u8>,
    pub mutation: MemoryMutation,
    /// Present for worker decisions. PostgreSQL locks and validates these lease generations in the same transaction
    /// as the memory mutation and decision journal insert.
    #[serde(default)]
    pub lease_fence: Option<QueueLeaseFence>,
    pub at: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitResult {
    pub memory_id: Option<String>,
    pub version: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredDecision {
    pub job_id: String,
    pub job_digest: String,
    pub kind: String,
    pub reason_codes: Vec<String>,
    pub confidence_band: Option<u8>,
    pub result: CommitResult,
    pub at: u64,
}

pub trait Store {
    /// Recallable memories (active or needs-reverify) of one host.
    fn memories_in_host(&self, host: &str) -> Result<Vec<MemoryRow>, StoreError>;
    fn get(&self, id: &str) -> Result<Option<MemoryRow>, StoreError>;
    fn find_by_scope(&self, scope: &ScopeKey) -> Result<Option<MemoryRow>, StoreError>;
    /// Applies a validated decision, its memory change, and its idempotency record atomically.
    fn apply_decision(&mut self, commit: DecisionCommit) -> Result<CommitResult, StoreError>;
    fn log_audit(&mut self, audit: AuditRecord) -> Result<(), StoreError>;
    /// Was this exact job (same id, same content digest) already decided? A retried queue batch must not apply
    /// the same decision, or the same evidence, twice.
    fn decided(&self, job_id: &str, job_digest: &str) -> Result<bool, StoreError>;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemStore {
    pub memories: BTreeMap<String, MemoryRow>,
    pub audits: Vec<AuditRecord>,
    #[serde(default)]
    pub decisions: BTreeMap<(String, String), StoredDecision>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_seed(
        &mut self,
        scope: ScopeKey,
        version: VersionRow,
        confidence_band: u8,
    ) -> Result<String, StoreError> {
        self.apply_memory_mutation(MemoryMutation::Create {
            scope,
            version,
            confidence_band,
        })?
        .memory_id
        .ok_or(StoreError::Unavailable)
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
    fn memories_in_host(&self, host: &str) -> Result<Vec<MemoryRow>, StoreError> {
        Ok(self
            .memories
            .values()
            .filter(|row| row.scope.host == host && row.status.recallable())
            .cloned()
            .collect())
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRow>, StoreError> {
        Ok(self.memories.get(id).cloned())
    }

    fn find_by_scope(&self, scope: &ScopeKey) -> Result<Option<MemoryRow>, StoreError> {
        Ok(self
            .memories
            .values()
            .find(|row| row.status.recallable() && &row.scope == scope)
            .cloned())
    }

    fn apply_decision(&mut self, commit: DecisionCommit) -> Result<CommitResult, StoreError> {
        let key = (commit.job_id.clone(), commit.job_digest.clone());
        if let Some(decision) = self.decisions.get(&key) {
            return Ok(decision.result.clone());
        }
        let mut staged = self.clone();
        let result = staged.apply_memory_mutation(commit.mutation)?;
        staged.decisions.insert(
            key,
            StoredDecision {
                job_id: commit.job_id,
                job_digest: commit.job_digest,
                kind: commit.kind,
                reason_codes: commit.reason_codes,
                confidence_band: commit.confidence_band,
                result: result.clone(),
                at: commit.at,
            },
        );
        *self = staged;
        Ok(result)
    }

    fn log_audit(&mut self, audit: AuditRecord) -> Result<(), StoreError> {
        self.audits.push(audit);
        Ok(())
    }

    fn decided(&self, job_id: &str, job_digest: &str) -> Result<bool, StoreError> {
        Ok(self
            .decisions
            .contains_key(&(job_id.to_owned(), job_digest.to_owned()))
            || self.audits.iter().any(|audit| {
                audit.job_id == job_id
                    && audit.job_digest == job_digest
                    && audit.outcome == "decided"
            }))
    }
}

impl MemStore {
    fn apply_memory_mutation(
        &mut self,
        mutation: MemoryMutation,
    ) -> Result<CommitResult, StoreError> {
        match mutation {
            MemoryMutation::None => Ok(CommitResult::default()),
            MemoryMutation::AddSupport { memory_id, count } => {
                let row = self
                    .memories
                    .get_mut(&memory_id)
                    .ok_or(StoreError::Missing)?;
                if !row.status.recallable() {
                    return Err(StoreError::Conflict);
                }
                row.support_count = row.support_count.saturating_add(count);
                Ok(CommitResult {
                    memory_id: Some(memory_id),
                    version: None,
                })
            }
            MemoryMutation::Create {
                scope,
                version,
                confidence_band,
            } => {
                if self
                    .memories
                    .values()
                    .any(|row| row.status.recallable() && row.scope == scope)
                {
                    return Err(StoreError::Conflict);
                }
                debug_assert_eq!(
                    kind_str(parse_kind(&scope.kind).expect("valid kind")),
                    scope.kind
                );
                let id = self.new_id(&scope);
                let version_number = version.version;
                let status = version.status;
                let row = MemoryRow {
                    id: id.clone(),
                    scope,
                    versions: vec![version],
                    support_count: 0,
                    status,
                    confidence_band,
                };
                self.memories.insert(id.clone(), row);
                Ok(CommitResult {
                    memory_id: Some(id),
                    version: Some(version_number),
                })
            }
            MemoryMutation::Supersede {
                memory_id,
                expected_version,
                mut next,
                confidence_band,
            } => {
                let row = self
                    .memories
                    .get_mut(&memory_id)
                    .ok_or(StoreError::Missing)?;
                let active = row.versions.last().ok_or(StoreError::Missing)?;
                if !row.status.recallable() || active.version != expected_version {
                    return Err(StoreError::Conflict);
                }
                let version = expected_version
                    .checked_add(1)
                    .ok_or(StoreError::Conflict)?;
                if let Some(previous) = row.versions.last_mut() {
                    previous.status = Status::Superseded;
                }
                next.version = version;
                next.status = Status::Active;
                row.versions.push(next);
                row.status = Status::Active;
                row.confidence_band = confidence_band;
                Ok(CommitResult {
                    memory_id: Some(memory_id),
                    version: Some(version),
                })
            }
            MemoryMutation::Retire {
                memory_id,
                expected_version,
            } => {
                let row = self
                    .memories
                    .get_mut(&memory_id)
                    .ok_or(StoreError::Missing)?;
                let active = row.versions.last().ok_or(StoreError::Missing)?;
                if !row.status.recallable() || active.version != expected_version {
                    return Err(StoreError::Conflict);
                }
                row.status = Status::Retired;
                if let Some(last) = row.versions.last_mut() {
                    last.status = Status::Retired;
                }
                Ok(CommitResult {
                    memory_id: Some(memory_id),
                    version: Some(expected_version),
                })
            }
        }
    }
}
