use super::{ApplicationRepository, ReplayRequest, ReplayVerifier, state::Attribution};
use crate::{application::ServiceError, ports::Repository};
use wam_gatekeeper::{
    record::{ValidationCtx, sha256_hex, validate},
    store::{DecisionCommit, MemoryMutation, Status, Store, VersionRow},
    tools::{Candidate, Job, JobScope, Outcomes, Trigger},
};

impl ApplicationRepository {
    /// An operator rollback creates a new immutable revision of a previously
    /// approved payload. Replay happens outside the transaction. The rejected
    /// revision is retired and its awards are reversed; rollback earns no +20.
    pub fn rollback_version(
        &self,
        memory_id: &str,
        target: u32,
        expected: u32,
        replay: &dyn ReplayVerifier,
        now: u64,
    ) -> Result<String, ServiceError> {
        let job_id = format!(
            "rollback-{}-{expected}-{target}",
            &sha256_hex(memory_id.as_bytes())[..32]
        );
        let existing = self.read(|state| {
            Ok(state
                .catalog
                .decisions
                .iter()
                .find(|decision| decision.job_id == job_id)
                .and_then(|decision| decision.result.version))
        })?;
        if let Some(version) = existing {
            return Ok(super::version_id(memory_id, version));
        }
        let (memory, previous, digest) = self.read(|state| {
            let memory = state
                .catalog
                .memories
                .get(memory_id)
                .ok_or(ServiceError::NotFound)?
                .clone();
            if memory
                .versions
                .last()
                .is_none_or(|version| version.version != expected)
                || target >= expected
            {
                return Err(ServiceError::Conflict);
            }
            let previous = memory
                .versions
                .iter()
                .find(|version| version.version == target && version.status != Status::Retired)
                .ok_or(ServiceError::Conflict)?
                .clone();
            let proof = state.attributions.iter().any(|entry| {
                entry.memory_id == memory_id
                    && entry.version == target
                    && entry
                        .account_id
                        .as_deref()
                        .is_some_and(|account| state.active(account))
            });
            if !proof {
                return Err(ServiceError::Conflict);
            }
            let digest = super::worker::catalog_digest(state, &memory.scope.host);
            Ok((memory, previous, digest))
        })?;
        let baseline = memory
            .versions
            .last()
            .ok_or(ServiceError::Conflict)?
            .payload
            .clone();
        let job = Job {
            id: job_id.clone(),
            trigger: Trigger::Reverify,
            scope: JobScope {
                site_host: memory.scope.host.clone(),
                task_category: memory.scope.task.clone(),
                intent_key: memory.scope.intent.clone(),
            },
            candidate: Candidate {
                url_pattern: previous
                    .payload
                    .l0
                    .url_patterns
                    .first()
                    .cloned()
                    .ok_or(ServiceError::Conflict)?,
                action_sequence: previous
                    .payload
                    .l2
                    .steps
                    .iter()
                    .map(|step| step.action.clone())
                    .collect(),
                outcomes: Outcomes {
                    success: previous.evidence.success,
                    partial: previous.evidence.partial,
                    failed: previous.evidence.failed,
                },
                observed_traits: vec![],
                independent_contributors: 1,
                ui_fingerprint: previous.evidence.ui_fingerprint.clone(),
                observed_at: now,
            },
            batch_candidates: vec![],
            contribution_ids: vec![],
            replay_passed: false,
            lease_fence: None,
        };
        let request = ReplayRequest::new(&job, Some(baseline), Some(previous.payload.clone()));
        let evidence = replay.verify(&request)?;
        if !evidence.passes(&request) {
            return Err(ServiceError::Conflict);
        }
        let validated = validate(
            previous.payload.clone(),
            &ValidationCtx {
                host: &memory.scope.host,
                task: &memory.scope.task,
                intent_key: &memory.scope.intent,
                tasks: &self.task_vocabulary(),
                mode: wam_core::UrlMode::Public,
            },
        )
        .map_err(|_| ServiceError::Conflict)?;
        self.update(|state| {
            if super::worker::catalog_digest(state, &memory.scope.host) != digest {
                return Err(ServiceError::Conflict);
            }
            let proof: Vec<Attribution> = state
                .attributions
                .iter()
                .filter(|entry| {
                    entry.memory_id == memory_id
                        && entry.version == target
                        && entry
                            .account_id
                            .as_deref()
                            .is_some_and(|account| state.active(account))
                })
                .cloned()
                .collect();
            if proof.is_empty() {
                return Err(ServiceError::Conflict);
            }
            let mut catalog = state.catalog.store();
            // A proven prior revision can replace a failed/retired active one.
            catalog
                .memories
                .get_mut(memory_id)
                .ok_or(ServiceError::Conflict)?
                .status = Status::NeedsReverify;
            let committed = catalog
                .apply_decision(DecisionCommit {
                    job_id: job_id.clone(),
                    job_digest: request.input_digest.clone(),
                    kind: "rollback".into(),
                    reason_codes: vec!["verified_previous_revision".into()],
                    confidence_band: Some(memory.confidence_band),
                    mutation: MemoryMutation::Supersede {
                        memory_id: memory_id.into(),
                        expected_version: expected,
                        confidence_band: memory.confidence_band,
                        next: VersionRow {
                            version: 0,
                            payload: validated.payload.clone(),
                            fingerprint: validated.fingerprint.clone(),
                            verified_at: now,
                            status: Status::Active,
                            evidence: previous.evidence.clone(),
                            job_id: job_id.clone(),
                        },
                    },
                    lease_fence: None,
                    at: now,
                })
                .map_err(|_| ServiceError::Conflict)?;
            let version = committed.version.ok_or(ServiceError::Conflict)?;
            if let Some(row) = catalog.memories.get_mut(memory_id) {
                if let Some(rejected) = row
                    .versions
                    .iter_mut()
                    .find(|version| version.version == expected)
                {
                    rejected.status = Status::Retired;
                }
            }
            state.catalog.replace(catalog);
            for mut attribution in proof {
                attribution.version = version;
                state.attributions.push(attribution);
            }
            super::lifecycle::reverse_version(state, memory_id, expected, now)?;
            state.records(&self.task_vocabulary())?;
            state.emit("memory_rolled_back", memory_id, now);
            Ok(super::version_id(memory_id, version))
        })
    }
}
