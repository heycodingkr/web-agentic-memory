//! Official publication credit for bundled examples, including legacy stores.

use super::{
    adapters::append_event,
    state::{Account, AppState, Attribution},
};
use crate::{
    application::ServiceError,
    examples::{AUTHOR_ACCOUNT_ID, AUTHOR_HANDLE, Example},
};
use wam_contracts::{BioBlock, BioInline, ProfilePutRequest, ServiceLink};
use wam_gatekeeper::{
    record::sha256_hex,
    store::{Evidence, Status},
};

impl AppState {
    pub(crate) fn credit_example_authorship(
        &mut self,
        examples: &[Example],
    ) -> Result<(), ServiceError> {
        if self
            .disabled_accounts
            .contains(&sha256_hex(AUTHOR_ACCOUNT_ID.as_bytes()))
        {
            return Ok(());
        }
        let targets: Vec<_> = examples
            .iter()
            .filter_map(|example| self.catalog.memories.get(&example.id))
            .filter(|memory| memory.status.recallable())
            .flat_map(|memory| {
                memory
                    .versions
                    .iter()
                    .map(move |version| (&memory.id, version))
            })
            .filter(|(_, version)| {
                version.job_id == "example-seed"
                    && version.evidence == Evidence::default()
                    && version
                        .payload
                        .l1
                        .title
                        .starts_with(crate::examples::TITLE_PREFIX)
                    && matches!(version.status, Status::Active | Status::Superseded)
            })
            .filter(|(memory, version)| {
                !self.attributions.iter().any(|entry| {
                    &entry.memory_id == *memory
                        && entry.version == version.version
                        && !official_attribution(entry)
                })
            })
            .map(|(memory, version)| (memory.clone(), version.version, version.verified_at))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }
        if !self.accounts.contains_key(AUTHOR_ACCOUNT_ID)
            && self
                .accounts
                .values()
                .any(|account| account.profile.handle == AUTHOR_HANDLE)
        {
            return Err(ServiceError::Conflict);
        }
        self.accounts.entry(AUTHOR_ACCOUNT_ID.into()).or_insert_with(|| Account {
            id: AUTHOR_ACCOUNT_ID.into(),
            base_points: 0,
            contribution_enabled: true,
            profile: ProfilePutRequest {
                handle: AUTHOR_HANDLE.into(),
                bio: vec![BioBlock::Paragraph { content: vec![BioInline::Text {
                    text: "Official Web Agentic Memory account publishing the built-in example workflows. Publication credit recognizes this work; examples remain labeled and are not independently verified site knowledge.".into(),
                }] }],
                service_links: vec![ServiceLink { label: "Web Agentic Memory".into(), url: "https://webagenticmemory.com".into() }],
                public_opt_in: true,
            },
        });
        for (memory, version, written_at) in targets {
            if !self
                .attributions
                .iter()
                .any(|entry| entry.memory_id == memory && entry.version == version)
            {
                self.attributions.push(Attribution {
                    memory_id: memory.clone(),
                    version,
                    account_id: Some(AUTHOR_ACCOUNT_ID.into()),
                    contribution_id: None,
                    role: "author".into(),
                    evidence_digest: "example-seed".into(),
                });
            }
            append_event(
                self,
                AUTHOR_ACCOUNT_ID,
                (&memory, version),
                "approval",
                20,
                format!("example-publication:{memory}:{version}"),
                written_at,
            )?;
        }
        Ok(())
    }
}

pub(super) fn official_attribution(entry: &Attribution) -> bool {
    entry.account_id.as_deref() == Some(AUTHOR_ACCOUNT_ID)
        && entry.role == "author"
        && entry.contribution_id.is_none()
        && entry.evidence_digest == "example-seed"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_authorship_and_scores_are_persisted_once_across_reopen() {
        let directory = std::env::temp_dir().join(format!(
            "wam-official-author-{}",
            crate::diagnostics::random_id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("application.json");
        let repository = crate::ApplicationRepository::open(&path).unwrap();
        let examples = crate::examples::load().unwrap();
        repository
            .update(|state| {
                state.add_examples(&examples);
                assert!(
                    state.attributions.is_empty(),
                    "legacy seed has no attribution"
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            repository.ensure_examples().unwrap(),
            0,
            "existing payloads are not replaced"
        );
        let before = crate::WamService::durable(repository.clone())
            .unwrap()
            .leaderboard()
            .unwrap();
        assert_eq!(before.entries[0].handle, AUTHOR_HANDLE);
        assert_eq!(before.entries[0].total_points, examples.len() as i64 * 20);
        drop(repository);
        let reopened = crate::ApplicationRepository::open(&path).unwrap();
        reopened.ensure_examples().unwrap();
        assert_eq!(
            crate::WamService::durable(reopened.clone())
                .unwrap()
                .leaderboard()
                .unwrap(),
            before
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn changed_example_versions_keep_both_authors_and_get_one_credit_each() {
        let current = crate::examples::load().unwrap().remove(0);
        let mut older = current.clone();
        older.payload.l1.summary = "An older example workflow.".into();
        let mut state = AppState::default();
        state.add_examples(&[older]);
        state
            .credit_example_authorship(std::slice::from_ref(&current))
            .unwrap();
        assert_eq!(
            state
                .refresh_examples(std::slice::from_ref(&current))
                .updated,
            1
        );
        state
            .credit_example_authorship(std::slice::from_ref(&current))
            .unwrap();
        state.credit_example_authorship(&[current]).unwrap();
        assert_eq!(state.attributions.len(), 2);
        assert!(state.attributions.iter().all(official_attribution));
        assert_eq!(state.scores.events.len(), 2);
        assert_eq!(
            crate::score_store::total_points_for(&state.scores, AUTHOR_ACCOUNT_ID),
            40
        );
    }

    #[test]
    fn reseeding_preserves_private_profiles_withdrawals_and_other_authors() {
        let example = crate::examples::load().unwrap().remove(0);
        let examples = [example];
        let mut state = AppState::default();
        state.add_examples(&examples);
        state.credit_example_authorship(&examples).unwrap();
        state
            .accounts
            .get_mut(AUTHOR_ACCOUNT_ID)
            .unwrap()
            .profile
            .public_opt_in = false;
        state.credit_example_authorship(&examples).unwrap();
        assert!(!state.accounts[AUTHOR_ACCOUNT_ID].profile.public_opt_in);
        state.attributions[0].account_id = None;
        state.credit_example_authorship(&examples).unwrap();
        assert!(
            state.attributions[0].account_id.is_none(),
            "withdrawn attribution must not reappear"
        );
        assert_eq!(state.scores.events.len(), 1);
        state.attributions[0].account_id = Some("another-author".into());
        state.credit_example_authorship(&examples).unwrap();
        assert_eq!(
            state.attributions[0].account_id.as_deref(),
            Some("another-author")
        );
        state.accounts.remove(AUTHOR_ACCOUNT_ID);
        state
            .disabled_accounts
            .insert(sha256_hex(AUTHOR_ACCOUNT_ID.as_bytes()));
        state.credit_example_authorship(&examples).unwrap();
        assert!(
            !state.accounts.contains_key(AUTHOR_ACCOUNT_ID),
            "deleted publisher must not be recreated"
        );
    }
}
