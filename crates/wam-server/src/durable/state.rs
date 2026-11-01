use crate::{
    contribution_store::state::ContributionState,
    identity::store::IdentityState,
    ports::{MemoryRecord, ProfileSnapshot, Repository},
    profile_wire::{profile_response, validate_profile},
    score_store::ScoreState,
    usage_store::UsageState,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use wam_contracts::ProfilePutRequest;
use wam_core::{MemoryIndex, MemoryStatus, TaskVocabulary, UrlMode};
use wam_gatekeeper::{
    record::{L0, L1, L2, RecordPayload, Step, build_pattern},
    store::{
        AuditRecord, Evidence, MemStore, MemoryRow, ScopeKey, Status, StoredDecision, VersionRow,
    },
};

pub(crate) const RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Account {
    pub id: String,
    pub profile: ProfilePutRequest,
    pub base_points: i64,
    #[serde(default = "consent_enabled")]
    pub contribution_enabled: bool,
}

fn consent_enabled() -> bool {
    true
}

impl Account {
    pub fn snapshot(&self) -> Result<ProfileSnapshot, crate::application::ServiceError> {
        let (profile, public_opt_in) = validate_profile(self.profile.clone())
            .map_err(|_| crate::application::ServiceError::Unavailable)?;
        Ok(ProfileSnapshot {
            profile,
            public_opt_in,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Catalog {
    pub memories: BTreeMap<String, MemoryRow>,
    pub decisions: Vec<StoredDecision>,
    pub audits: Vec<AuditRecord>,
}

impl Catalog {
    pub fn store(&self) -> MemStore {
        MemStore {
            memories: self.memories.clone(),
            decisions: self
                .decisions
                .iter()
                .cloned()
                .map(|decision| {
                    (
                        (decision.job_id.clone(), decision.job_digest.clone()),
                        decision,
                    )
                })
                .collect(),
            audits: self.audits.clone(),
        }
    }

    pub fn replace(&mut self, store: MemStore) {
        self.memories = store.memories;
        self.decisions = store.decisions.into_values().collect();
        self.audits = store.audits;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Consent {
    pub contribution_id: String,
    pub version: String,
    pub nonce: String,
    pub received_at: u64,
    pub purge_at: u64,
    pub withdrawn_at: Option<u64>,
    pub purged_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Attribution {
    pub memory_id: String,
    pub version: u32,
    pub account_id: Option<String>,
    pub contribution_id: Option<String>,
    pub role: String,
    pub evidence_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Work {
    pub id: String,
    pub contribution_id: Option<String>,
    pub memory_id: Option<String>,
    pub state: String,
    pub generation: u64,
    pub lease_until: Option<u64>,
    pub holder: Option<String>,
    pub enqueued_at: u64,
    pub kind: String,
    pub input_digest: Option<String>,
    #[serde(default)]
    pub not_before: u64,
    #[serde(default)]
    pub attempts: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub created_at: u64,
    pub delivered_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AppState {
    pub schema_version: u8,
    pub next_outbox_id: u64,
    #[serde(default)]
    pub legacy_imported: bool,
    pub accounts: BTreeMap<String, Account>,
    pub disabled_accounts: BTreeSet<String>,
    pub contributions: ContributionState,
    pub usage: UsageState,
    pub scores: ScoreState,
    pub identity: IdentityState,
    pub catalog: Catalog,
    pub consents: BTreeMap<String, Consent>,
    pub attributions: Vec<Attribution>,
    pub work: BTreeMap<String, Work>,
    pub outbox: Vec<OutboxEvent>,
    pub proposals: BTreeMap<String, super::worker::Proposal>,
    /// The worker's model budget gate (see `budget.rs`). Old stores have none and start with an open gate.
    #[serde(default)]
    pub model_budget: super::budget::ModelBudgetState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_outbox_id: 1,
            legacy_imported: false,
            accounts: BTreeMap::new(),
            disabled_accounts: BTreeSet::new(),
            contributions: ContributionState::default(),
            usage: UsageState::default(),
            scores: ScoreState::default(),
            identity: IdentityState::default(),
            catalog: Catalog::default(),
            consents: BTreeMap::new(),
            attributions: Vec::new(),
            work: BTreeMap::new(),
            outbox: Vec::new(),
            proposals: BTreeMap::new(),
            model_budget: super::budget::ModelBudgetState::default(),
        }
    }
}

pub fn version_id(memory: &str, version: u32) -> String {
    // Core version IDs have a 64-byte limit and do not allow colons.
    format!(
        "ver-{}",
        &wam_gatekeeper::record::sha256_hex(format!("{memory}:{version}").as_bytes())[..48]
    )
}

impl AppState {
    pub fn demo() -> Self {
        let legacy = crate::memory_repository::LocalRepository::seeded();
        let mut state = Self::from_repository(&legacy);
        for record in legacy.memories() {
            let index = &record.index;
            let payload = RecordPayload {
                l0: L0 {
                    kind: "workflow".into(),
                    site_host: record.domain.clone(),
                    url_patterns: vec!["/guide/*".into()],
                    task_category: Some(index.task_category().as_str().into()),
                    intent_key: Some(index.intent_key().into()),
                    keywords: index
                        .keywords()
                        .iter()
                        .map(|keyword| keyword.as_str().to_owned())
                        .collect(),
                },
                l1: L1 {
                    title: record.title,
                    summary: record.summary,
                    key_points: vec![],
                    pitfalls: vec![],
                },
                l2: L2 {
                    steps: ["navigate", "type", "click"]
                        .into_iter()
                        .map(|action| Step {
                            action: action.into(),
                            when: None,
                            note_code: None,
                        })
                        .collect(),
                    detail: Some(record.detail),
                    ..L2::default()
                },
            };
            let scope = ScopeKey {
                host: record.domain,
                pattern: "example.com/guide/*".into(),
                kind: "workflow".into(),
                task: index.task_category().as_str().into(),
                intent: index.intent_key().into(),
            };
            let row = MemoryRow {
                id: index.id().into(),
                scope,
                support_count: 1,
                status: Status::Active,
                confidence_band: 2,
                versions: vec![VersionRow {
                    version: 1,
                    fingerprint: wam_gatekeeper::record::candidate_fingerprint(&[
                        "navigate".into(),
                        "type".into(),
                        "click".into(),
                    ]),
                    payload,
                    verified_at: index.verified_at(),
                    status: Status::Active,
                    evidence: Evidence {
                        independent_contributors: 1,
                        success: 1,
                        ..Evidence::default()
                    },
                    job_id: "demo-seed".into(),
                }],
            };
            state.attributions.push(Attribution {
                memory_id: row.id.clone(),
                version: 1,
                account_id: record.author_account_id,
                contribution_id: None,
                role: "author".into(),
                evidence_digest: "demo-seed".into(),
            });
            state.catalog.memories.insert(row.id.clone(), row);
        }
        state
    }

    /// Adds the built-in example memories (crate::examples) that the catalog does not hold yet and returns how many
    /// it added. The repository also credits the official publisher in this transaction.
    pub(crate) fn add_examples(&mut self, examples: &[crate::examples::Example]) -> usize {
        let mut added = 0;
        for example in examples {
            if self.catalog.memories.contains_key(&example.id) {
                continue;
            }
            let l0 = &example.payload.l0;
            let route = l0
                .url_patterns
                .first()
                .cloned()
                .unwrap_or_else(|| "/*".into());
            let actions: Vec<String> = example
                .payload
                .l2
                .steps
                .iter()
                .map(|step| step.action.clone())
                .collect();
            let row = MemoryRow {
                id: example.id.clone(),
                scope: ScopeKey {
                    host: l0.site_host.clone(),
                    pattern: format!("{}{route}", l0.site_host),
                    kind: l0.kind.clone(),
                    task: l0.task_category.clone().unwrap_or_default(),
                    intent: l0.intent_key.clone().unwrap_or_default(),
                },
                support_count: 1,
                status: Status::Active,
                confidence_band: 1,
                versions: vec![VersionRow {
                    version: 1,
                    fingerprint: wam_gatekeeper::record::candidate_fingerprint(&actions),
                    payload: example.payload.clone(),
                    verified_at: crate::examples::WRITTEN_AT,
                    status: Status::Active,
                    evidence: Evidence::default(),
                    job_id: "example-seed".into(),
                }],
            };
            self.catalog.memories.insert(row.id.clone(), row);
            added += 1;
        }
        added
    }

    /// Refresh only untouched built-in rows, preserving authored/reviewed content and old payloads.
    pub(crate) fn refresh_examples(
        &mut self,
        examples: &[crate::examples::Example],
    ) -> crate::examples::RefreshReport {
        let mut report = crate::examples::RefreshReport {
            added: self.add_examples(examples),
            ..Default::default()
        };
        for example in examples {
            let memory = self
                .catalog
                .memories
                .get_mut(&example.id)
                .expect("added above");
            let protected = memory.status != Status::Active
                || memory.support_count != 1
                || memory.confidence_band != 1
                || self.attributions.iter().any(|row| {
                    row.memory_id == example.id && !super::examples::official_attribution(row)
                })
                || memory.versions.is_empty()
                || memory.versions.iter().any(|version| {
                    version.job_id != "example-seed"
                        || version.evidence != Evidence::default()
                        || !version
                            .payload
                            .l1
                            .title
                            .starts_with(crate::examples::TITLE_PREFIX)
                        || !matches!(version.status, Status::Active | Status::Superseded)
                })
                || memory
                    .versions
                    .last()
                    .is_none_or(|version| version.status != Status::Active);
            if protected {
                report.protected += 1;
                continue;
            }
            let previous = memory.versions.last().expect("checked above");
            if previous.payload == example.payload {
                continue;
            }
            let Some(next_version) = previous.version.checked_add(1) else {
                report.protected += 1;
                continue;
            };
            let actions: Vec<_> = example
                .payload
                .l2
                .steps
                .iter()
                .map(|step| step.action.clone())
                .collect();
            let l0 = &example.payload.l0;
            memory.scope = ScopeKey {
                host: l0.site_host.clone(),
                pattern: format!(
                    "{}{}",
                    l0.site_host,
                    l0.url_patterns.first().map(String::as_str).unwrap_or("/*")
                ),
                kind: l0.kind.clone(),
                task: l0.task_category.clone().unwrap_or_default(),
                intent: l0.intent_key.clone().unwrap_or_default(),
            };
            memory.versions.last_mut().expect("checked above").status = Status::Superseded;
            memory.versions.push(VersionRow {
                version: next_version,
                fingerprint: wam_gatekeeper::record::candidate_fingerprint(&actions),
                payload: example.payload.clone(),
                verified_at: crate::examples::WRITTEN_AT,
                status: Status::Active,
                evidence: Evidence::default(),
                job_id: "example-seed".into(),
            });
            report.updated += 1;
        }
        report
    }

    pub fn from_repository(repository: &impl Repository) -> Self {
        let mut state = Self::default();
        for ranking in repository.ranking_accounts() {
            let Some(snapshot) = repository.profile_for_account(ranking.key()) else {
                continue;
            };
            let profile = profile_response(&snapshot.profile, snapshot.public_opt_in);
            state.accounts.insert(
                ranking.key().into(),
                Account {
                    id: ranking.key().into(),
                    base_points: ranking.total_points(),
                    contribution_enabled: true,
                    profile: ProfilePutRequest {
                        handle: profile.handle,
                        bio: profile.bio,
                        service_links: profile.service_links,
                        public_opt_in: profile.public_opt_in,
                    },
                },
            );
        }
        state
    }

    pub fn active(&self, account: &str) -> bool {
        self.accounts.contains_key(account)
            && !self
                .disabled_accounts
                .contains(&wam_gatekeeper::record::sha256_hex(account.as_bytes()))
    }

    pub fn emit(&mut self, kind: &str, target: &str, at: u64) {
        let id = format!("outbox-{:016}", self.next_outbox_id);
        self.next_outbox_id += 1;
        self.outbox.push(OutboxEvent {
            id,
            kind: kind.into(),
            target: target.into(),
            created_at: at,
            delivered_at: None,
        });
    }

    pub fn records(
        &self,
        tasks: &TaskVocabulary,
    ) -> Result<Vec<MemoryRecord>, crate::application::ServiceError> {
        let active_accounts: HashSet<&str> = self
            .accounts
            .keys()
            .filter(|account| {
                !self
                    .disabled_accounts
                    .contains(&wam_gatekeeper::record::sha256_hex(account.as_bytes()))
            })
            .map(String::as_str)
            .collect();
        let mut authors: HashMap<(&str, u32), &str> = HashMap::new();
        for attribution in &self.attributions {
            let Some(account_id) = attribution.account_id.as_deref() else {
                continue;
            };
            if attribution.role == "author" && active_accounts.contains(account_id) {
                authors
                    .entry((attribution.memory_id.as_str(), attribution.version))
                    .or_insert(account_id);
            }
        }

        self.catalog
            .memories
            .values()
            .filter_map(|memory| memory.active().map(|version| (memory, version)))
            .map(|(memory, version)| {
                let author_account_id = authors
                    .get(&(memory.id.as_str(), version.version))
                    .map(|account_id| (*account_id).to_owned());
                memory_record_from_parts(memory, version, tasks, author_account_id)
            })
            .collect()
    }

    pub fn memory_by_version(
        &self,
        id: &str,
        requested_version_id: &str,
        tasks: &TaskVocabulary,
    ) -> Result<Option<MemoryRecord>, crate::application::ServiceError> {
        let Some(memory) = self.catalog.memories.get(id) else {
            return Ok(None);
        };
        if !memory.status.recallable() {
            return Ok(None);
        }
        let Some(version) = memory.versions.iter().find(|version| {
            version_id(id, version.version) == requested_version_id
                && matches!(
                    version.status,
                    Status::Active | Status::NeedsReverify | Status::Superseded
                )
        }) else {
            return Ok(None);
        };
        let author_account_id = self
            .attributions
            .iter()
            .find(|attribution| {
                attribution.memory_id == memory.id
                    && attribution.version == version.version
                    && attribution.role == "author"
                    && attribution
                        .account_id
                        .as_deref()
                        .is_some_and(|account| self.active(account))
            })
            .and_then(|attribution| attribution.account_id.clone());
        memory_record_from_parts(memory, version, tasks, author_account_id).map(Some)
    }
}

pub(crate) fn memory_record_from_parts(
    memory: &wam_gatekeeper::store::MemoryRow,
    version: &wam_gatekeeper::store::VersionRow,
    tasks: &TaskVocabulary,
    author_account_id: Option<String>,
) -> Result<MemoryRecord, crate::application::ServiceError> {
    let payload = &version.payload;
    let l0 = &payload.l0;
    let patterns = l0
        .url_patterns
        .iter()
        .map(|pattern| {
            build_pattern(&l0.site_host, pattern, UrlMode::Public)
                .map_err(|_| crate::application::ServiceError::Unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let keywords = if l0.keywords.is_empty() {
        Vec::new()
    } else {
        wam_core::KeywordVocabulary::new("stored", l0.keywords.iter().map(String::as_str))
            .and_then(|vocabulary| vocabulary.filter(l0.keywords.iter().map(String::as_str)))
            .map_err(|_| crate::application::ServiceError::Unavailable)?
    };
    let index = MemoryIndex::new(
        &memory.id,
        version_id(&memory.id, version.version),
        patterns,
        tasks
            .resolve(l0.task_category.as_deref().unwrap_or_default())
            .map_err(|_| crate::application::ServiceError::Unavailable)?,
        wam_gatekeeper::record::parse_kind(&l0.kind)
            .ok_or(crate::application::ServiceError::Unavailable)?,
        l0.intent_key.as_deref().unwrap_or_default(),
        keywords,
        memory.confidence_band,
        match memory.status {
            Status::Active => MemoryStatus::Active,
            Status::NeedsReverify => MemoryStatus::NeedsReverify,
            Status::Retired => MemoryStatus::Retired,
            Status::Superseded => MemoryStatus::Superseded,
        },
        version.verified_at,
    )
    .map_err(|_| crate::application::ServiceError::Unavailable)?;
    Ok(MemoryRecord {
        index,
        title: payload.l1.title.clone(),
        summary: payload.l1.summary.clone(),
        site: l0.site_host.clone(),
        domain: l0.site_host.clone(),
        verified_date: version.verified_at.to_string(),
        hint: wam_gatekeeper::record::render_l1(payload),
        detail: serde_json::to_string(&payload.l2)
            .map_err(|_| crate::application::ServiceError::Unavailable)?,
        author_account_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_refresh_appends_a_version_and_is_idempotent() {
        let mut state = AppState::default();
        let current = crate::examples::load().unwrap().remove(0);
        let mut older = current.clone();
        older.payload.l1.summary = "An older built-in workflow.".into();
        state.add_examples(&[older.clone()]);
        assert_eq!(state.add_examples(std::slice::from_ref(&current)), 0);
        assert_eq!(
            state.catalog.memories[&current.id].versions[0].payload,
            older.payload
        );

        let report = state.refresh_examples(std::slice::from_ref(&current));
        assert_eq!(report.updated, 1);
        let row = &state.catalog.memories[&current.id];
        assert_eq!(row.versions.len(), 2);
        assert_eq!(row.versions[0].payload, older.payload);
        assert_eq!(row.versions[0].status, Status::Superseded);
        assert_eq!(row.versions[1].payload, current.payload);
        assert_eq!(row.versions[1].version, 2);
        assert_ne!(version_id(&current.id, 1), version_id(&current.id, 2));
        assert_eq!(
            state.refresh_examples(&[current]),
            crate::examples::RefreshReport::default()
        );
        assert!(state.attributions.is_empty());
        assert!(state.catalog.decisions.is_empty());
    }

    #[test]
    fn example_refresh_preserves_reviewed_authored_and_inactive_rows() {
        let current = crate::examples::load().unwrap().remove(0);
        for protection in [
            "retired",
            "superseded",
            "needs-review",
            "version-status",
            "reviewed",
            "authored",
            "supported",
            "evidence",
            "renamed",
        ] {
            let mut state = AppState::default();
            let mut older = current.clone();
            older.payload.l1.summary = "An older built-in workflow.".into();
            state.add_examples(&[older]);
            let row = state.catalog.memories.get_mut(&current.id).unwrap();
            match protection {
                "retired" => row.status = Status::Retired,
                "superseded" => row.status = Status::Superseded,
                "needs-review" => row.status = Status::NeedsReverify,
                "version-status" => row.versions[0].status = Status::Retired,
                "reviewed" => row.versions[0].job_id = "reviewed-job".into(),
                "authored" => {
                    state
                        .attributions
                        .push(attribution(&current.id, 1, "author", "author"))
                }
                "supported" => row.support_count = 2,
                "evidence" => row.versions[0].evidence.success = 1,
                "renamed" => row.versions[0].payload.l1.title = "A published workflow".into(),
                _ => unreachable!(),
            }
            let before = state.catalog.memories[&current.id].clone();
            assert_eq!(
                state
                    .refresh_examples(std::slice::from_ref(&current))
                    .protected,
                1,
                "{protection}"
            );
            assert_eq!(state.catalog.memories[&current.id], before, "{protection}");
        }
    }

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            profile: ProfilePutRequest {
                handle: id.into(),
                bio: Vec::new(),
                service_links: Vec::new(),
                public_opt_in: false,
            },
            base_points: 0,
            contribution_enabled: true,
        }
    }

    fn attribution(memory_id: &str, version: u32, account_id: &str, role: &str) -> Attribution {
        Attribution {
            memory_id: memory_id.into(),
            version,
            account_id: Some(account_id.into()),
            contribution_id: None,
            role: role.into(),
            evidence_digest: "test".into(),
        }
    }

    #[test]
    fn records_select_the_first_active_author_for_the_active_version() {
        let mut state = AppState::demo();
        let (memory_id, version) = state
            .catalog
            .memories
            .values()
            .find_map(|memory| {
                memory
                    .active()
                    .map(|version| (memory.id.clone(), version.version))
            })
            .expect("demo state contains an active memory");

        for id in [
            "disabled-author",
            "first-author",
            "later-author",
            "reviewer",
        ] {
            state.accounts.insert(id.into(), account(id));
        }
        state
            .disabled_accounts
            .insert(wam_gatekeeper::record::sha256_hex(b"disabled-author"));
        state.attributions = vec![
            attribution(&memory_id, version, "reviewer", "reviewer"),
            attribution(&memory_id, version, "disabled-author", "author"),
            attribution(&memory_id, version, "first-author", "author"),
            attribution(&memory_id, version, "later-author", "author"),
        ];

        let tasks = TaskVocabulary::new("test", ["compose-draft", "navigate", "research"]).unwrap();
        let records = state.records(&tasks).unwrap();
        let record = records
            .iter()
            .find(|record| record.index.id() == memory_id)
            .expect("active demo record is returned");

        assert_eq!(record.author_account_id.as_deref(), Some("first-author"));
    }
}
