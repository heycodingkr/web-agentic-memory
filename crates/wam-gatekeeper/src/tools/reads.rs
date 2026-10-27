use super::*;

impl<'a> ToolCtx<'a> {
    pub(super) fn job_get(&self) -> Result<Value, ToolError> {
        let c = &self.job.candidate;
        let exact = self.exact_duplicate()?;
        let batch_candidates = if self.job.batch_candidates.is_empty() {
            vec![c]
        } else {
            self.job.batch_candidates.iter().collect()
        };
        let batch_candidates: Vec<Value> = batch_candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                json!({
                    "candidate_index": index,
                    "url_pattern": candidate.url_pattern,
                    "action_sequence": candidate.action_sequence,
                    "normalized_steps": normalize_actions(&candidate.action_sequence),
                    "outcomes": candidate.outcomes,
                    "success_rate": candidate.evidence().success_rate(),
                    "observed_traits": candidate.observed_traits,
                    "independent_contributors": candidate.independent_contributors,
                    "ui_fingerprint": candidate.ui_fingerprint,
                    "observed_at": candidate.observed_at,
                })
            })
            .collect();
        Ok(json!({
            "job_id": self.job.id,
            "trigger": self.job.trigger,
            "scope": self.job.scope,
            "batch_candidates": batch_candidates,
            "decision_applies_to_whole_scope_batch": true,
            "candidate": {
                "url_pattern": c.url_pattern,
                "action_sequence": c.action_sequence,
                "normalized_steps": normalize_actions(&c.action_sequence),
                "outcomes": c.outcomes,
                "success_rate": c.evidence().success_rate(),
                "observed_traits": c.observed_traits,
                "independent_contributors": c.independent_contributors,
                "ui_fingerprint": c.ui_fingerprint,
                "observed_at": c.observed_at,
            },
            "precheck": {"exact_fingerprint_match": exact},
            "replay_passed": self.job.replay_passed,
            "read_calls_left": self.tools.max_read_calls.saturating_sub(self.read_calls),
        }))
    }

    pub(super) fn memory_search(&self, a: SearchArgs) -> Result<Value, ToolError> {
        let limit = match a.limit {
            None => 5,
            Some(value) if (1..=10).contains(&value) => value as usize,
            Some(_) => return Err(schema("limit must be 1 to 10")),
        };
        let kind = match a.kind.as_deref() {
            None => None,
            Some(raw) => Some(
                parse_kind(raw)
                    .ok_or_else(|| schema("kind must be workflow, site_note, or gotcha"))?,
            ),
        };
        if a.task_category
            .as_deref()
            .is_some_and(|task| !self.tasks.contains(task))
        {
            return Err(schema("unknown task_category"));
        }
        let wanted_pattern = match a.url_pattern.as_deref() {
            None => None,
            Some(route) => Some(
                build_pattern(self.host(), route, self.policy.mode)
                    .map_err(|why| schema(format!("invalid url_pattern: {why}")))?,
            ),
        };
        let wanted_keywords: Vec<String> = match a.keywords {
            Some(list) if !list.is_empty() => {
                if list.len() > 8 {
                    return Err(schema("keywords allows at most 8 items"));
                }
                let vocabulary = KeywordVocabulary::new("query", &list)
                    .map_err(|error| schema(format!("invalid or duplicate keywords: {error:?}")))?;
                list.iter()
                    .filter_map(|keyword| vocabulary.resolve(keyword))
                    .map(|keyword| keyword.as_str().to_owned())
                    .collect()
            }
            _ => Vec::new(),
        };

        let memories = self
            .store
            .memories_in_host(self.host())
            .map_err(storage_unavailable)?;
        let mut hits: Vec<(u32, &MemoryRow)> = Vec::new();
        for row in &memories {
            let Some(active) = row.active() else { continue };
            let l0 = &active.payload.l0;
            if kind.is_some_and(|wanted| kind_str(wanted) != l0.kind) {
                continue;
            }
            if a.task_category
                .as_deref()
                .is_some_and(|task| l0.task_category.as_deref() != Some(task))
            {
                continue;
            }
            let mut route = 1;
            if let Some(wanted) = &wanted_pattern {
                route = l0
                    .url_patterns
                    .iter()
                    .filter_map(|r| build_pattern(self.host(), r, self.policy.mode).ok())
                    .map(|pattern| relation(wanted, &pattern))
                    .max()
                    .unwrap_or(1);
            }
            let overlap = l0
                .keywords
                .iter()
                .filter(|k| wanted_keywords.contains(k))
                .count() as u32;
            hits.push((100 * route + 10 * overlap, row));
        }
        hits.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.id.cmp(&right.1.id))
        });
        hits.truncate(limit);
        let results: Vec<Value> = hits
            .into_iter()
            .filter_map(|(score, row)| {
                let version = row.active()?;
                Some(json!({
                    "id": row.id,
                    "version": version.version,
                    "status": row.status,
                    "support_count": row.support_count,
                    "score": score,
                    "l0": version.payload.l0,
                    "l1": {"title": version.payload.l1.title, "summary": version.payload.l1.summary},
                }))
            })
            .collect();
        Ok(json!({"count": results.len(), "results": results}))
    }

    pub(super) fn memory_get(&self, a: GetArgs) -> Result<Value, ToolError> {
        let row = self.visible(&a.id)?;
        let version = row.active().ok_or_else(|| {
            ToolError::new(
                ErrorCode::ScopeViolation,
                "no such memory in this job's site host",
            )
        })?;
        match a.level {
            1 => Ok(
                json!({"id": row.id, "version": version.version, "l0": version.payload.l0, "l1": version.payload.l1}),
            ),
            2 => Ok(json!({
                "id": row.id,
                "version": version.version,
                "l2": version.payload.l2,
                "evidence": version.evidence,
                "verified_at": version.verified_at,
            })),
            _ => Err(schema("level must be 1 or 2")),
        }
    }

    pub(super) fn memory_compare(&self, a: CompareArgs) -> Result<Value, ToolError> {
        let row = self.visible(&a.existing_id)?;
        let active = row.active().ok_or_else(|| {
            ToolError::new(
                ErrorCode::ScopeViolation,
                "no such memory in this job's site host",
            )
        })?;
        let c = &self.job.candidate;
        let candidate_steps = normalize_actions(&c.action_sequence);
        let existing_steps = normalize_actions(&step_actions(&active.payload.l2.steps));
        let known: BTreeSet<&str> = active
            .payload
            .l2
            .structure
            .iter()
            .flat_map(|s| s.ui_traits.iter())
            .chain(active.payload.l1.pitfalls.iter())
            .chain(
                active
                    .payload
                    .l2
                    .steps
                    .iter()
                    .filter_map(|s| s.when.as_ref()),
            )
            .map(String::as_str)
            .collect();
        let new_traits: Vec<&String> = c
            .observed_traits
            .iter()
            .filter(|t| !known.contains(t.as_str()))
            .collect();
        let scope_relation = match (
            build_pattern(self.host(), &c.url_pattern, self.policy.mode),
            build_pattern(
                self.host(),
                &active.payload.l0.url_patterns[0],
                self.policy.mode,
            ),
        ) {
            (Ok(mine), Ok(theirs)) => {
                if pattern_key(&mine) == pattern_key(&theirs) {
                    "exact"
                } else if path_covers(theirs.path(), mine.path()) {
                    "narrower"
                } else if path_covers(mine.path(), theirs.path()) {
                    "broader"
                } else {
                    "disjoint"
                }
            }
            _ => "disjoint",
        };
        let compared_candidates = if self.job.batch_candidates.is_empty() {
            vec![c]
        } else {
            self.job.batch_candidates.iter().collect()
        };
        let batch_candidate_comparisons: Vec<Value> = compared_candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let steps = normalize_actions(&candidate.action_sequence);
                let new_traits: Vec<&String> = candidate
                    .observed_traits
                    .iter()
                    .filter(|trait_code| !known.contains(trait_code.as_str()))
                    .collect();
                json!({
                    "candidate_index": index,
                    "fingerprint_equal": candidate_fingerprint(&candidate.action_sequence) == active.fingerprint,
                    "step_count": {
                        "candidate": steps.len(),
                        "existing": existing_steps.len(),
                        "delta": steps.len() as i64 - existing_steps.len() as i64,
                    },
                    "new_traits": new_traits,
                    "success_rate": {"candidate": candidate.evidence().success_rate(), "existing": active.evidence.success_rate()},
                    "outcomes": candidate.outcomes,
                    "independent_contributors": candidate.independent_contributors,
                    "freshness_days_delta": (candidate.observed_at as i64 - active.verified_at as i64) / 86_400,
                    "ui_fingerprint_changed": candidate.ui_fingerprint.is_some() && candidate.ui_fingerprint != active.evidence.ui_fingerprint,
                })
            })
            .collect();
        Ok(json!({
            "existing_id": row.id,
            "expected_version": active.version,
            "fingerprint_equal": candidate_fingerprint(&c.action_sequence) == active.fingerprint,
            "step_count": {
                "candidate": candidate_steps.len(),
                "existing": existing_steps.len(),
                "delta": candidate_steps.len() as i64 - existing_steps.len() as i64,
            },
            "new_traits": new_traits,
            "success_rate": {"candidate": c.evidence().success_rate(), "existing": active.evidence.success_rate()},
            "independent_contributors": {
                "candidate": c.independent_contributors,
                "existing": active.evidence.independent_contributors,
            },
            "freshness_days_delta": (c.observed_at as i64 - active.verified_at as i64) / 86_400,
            "ui_fingerprint_changed": c.ui_fingerprint.is_some() && c.ui_fingerprint != active.evidence.ui_fingerprint,
            "scope_relation": scope_relation,
            "existing_support_count": row.support_count,
            "batch_candidate_comparisons": batch_candidate_comparisons,
        }))
    }
}
