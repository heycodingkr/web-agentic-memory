use super::*;

impl<'a> ToolCtx<'a> {
    pub(super) fn decide_skip(&mut self, a: SkipArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        if a.reason != "duplicate_support" && a.reason != "merge_evidence" {
            return Err(schema("reason must be duplicate_support or merge_evidence"));
        }
        let row = self.visible(&a.existing_id)?;
        self.finish(
            &a.reason,
            a.reason_codes,
            None,
            MemoryMutation::AddSupport {
                memory_id: row.id,
                count: self.job.candidate.independent_contributors.max(1),
            },
        )
    }

    pub(super) fn decide_create(&mut self, a: CreateArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        let band = self.band(&a.confidence_band)?;
        let validated = self.validate_record(a.record)?;
        let scope = self.scope_of(&validated);
        if let Some(existing) = self
            .store
            .find_by_scope(&scope)
            .map_err(storage_unavailable)?
        {
            return Err(ToolError::new(
                ErrorCode::DuplicateExists,
                format!(
                    "an active memory already covers this scope: {}; skip or replace instead",
                    existing.id
                ),
            ));
        }
        if self.job.candidate.independent_contributors < self.policy.min_independent_create {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "not enough independent contributors to create a memory yet; escalate or skip",
            ));
        }
        let first = VersionRow {
            version: 1,
            payload: validated.payload,
            fingerprint: validated.fingerprint,
            verified_at: self.now,
            status: Status::Active,
            evidence: self.job.candidate.evidence(),
            job_id: self.job.id.clone(),
        };
        self.finish(
            "create_new",
            a.reason_codes,
            Some(a.confidence_band),
            MemoryMutation::Create {
                scope,
                version: first,
                confidence_band: band,
            },
        )
    }

    pub(super) fn decide_replace(&mut self, a: ReplaceArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        let band = self.band(&a.confidence_band)?;
        let existing = self.visible(&a.existing_id)?;
        let active = existing
            .active()
            .ok_or_else(|| {
                ToolError::new(
                    ErrorCode::ScopeViolation,
                    "no such memory in this job's site host",
                )
            })?
            .clone();
        if active.version != a.expected_version {
            return Err(ToolError::new(
                ErrorCode::VersionConflict,
                format!(
                    "the active version is {}, not {}; read again and decide again",
                    active.version, a.expected_version
                ),
            ));
        }
        let validated = self.validate_record(a.record)?;
        if self.scope_of(&validated) != existing.scope {
            return Err(ToolError::new(
                ErrorCode::ScopeViolation,
                "a replacement must keep the memory's scope (host, primary route, kind, task, intent)",
            ));
        }
        let c = &self.job.candidate;
        if candidate_fingerprint(&c.action_sequence) == active.fingerprint
            || validated.fingerprint == active.fingerprint
        {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "the candidate is identical to the active version; use decide_skip",
            ));
        }
        if !(self.job.replay_passed
            || c.independent_contributors >= self.policy.replace_min_independent)
        {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "replacing needs a passed replay or enough independent contributors; skip or escalate",
            ));
        }
        if c.evidence().success_rate() < active.evidence.success_rate() {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "the candidate's success rate is lower than the active version's",
            ));
        }
        let next = VersionRow {
            version: 0,
            payload: validated.payload,
            fingerprint: validated.fingerprint,
            verified_at: self.now,
            status: Status::Active,
            evidence: c.evidence(),
            job_id: self.job.id.clone(),
        };
        self.finish(
            "supersede",
            a.reason_codes,
            Some(a.confidence_band),
            MemoryMutation::Supersede {
                memory_id: existing.id,
                expected_version: a.expected_version,
                next,
                confidence_band: band,
            },
        )
    }

    pub(super) fn decide_retire(&mut self, a: RetireArgs) -> Result<Value, ToolError> {
        self.check_reason_codes(&a.reason_codes)?;
        let existing = self.visible(&a.existing_id)?;
        let version = existing.active().map_or(0, |v| v.version);
        if version != a.expected_version {
            return Err(ToolError::new(
                ErrorCode::VersionConflict,
                format!(
                    "the active version is {version}, not {}; read again and decide again",
                    a.expected_version
                ),
            ));
        }
        let c = &self.job.candidate;
        if !(self.job.trigger == Trigger::Reverify || c.outcomes.failed > c.outcomes.success) {
            return Err(ToolError::new(
                ErrorCode::EvidenceInsufficient,
                "retiring needs a reverify trigger or failures that outweigh successes; escalate instead",
            ));
        }
        self.finish(
            "retire",
            a.reason_codes,
            None,
            MemoryMutation::Retire {
                memory_id: existing.id,
                expected_version: version,
            },
        )
    }
}
