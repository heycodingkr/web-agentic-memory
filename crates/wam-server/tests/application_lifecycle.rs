mod lifecycle_support;
use lifecycle_support::*;
use std::cell::Cell;
use wam_contracts::{
    ContributionReviewDecision, MemoryOutcome, OutcomeReportRequest, RecallRequest,
};
use wam_server::{
    ApplicationRepository,
    application::ServiceError,
    durable::{ReplayEvidence, ReplayRequest, ReplayVerifier, WorkResult},
    ports::{ContributionStore, Repository, ScoreStore},
};

#[test]
fn validated_publication_awards_twenty_and_real_use_awards_two_once() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    assert!(service.catalog().unwrap().memories.is_empty());
    let result = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap();
    let WorkResult::Published {
        memory_id,
        version_id,
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    assert_eq!(
        service.contribution(AUTHOR, &id).unwrap().status,
        "approved"
    );
    assert_eq!(service.catalog().unwrap().memories.len(), 1);
    assert!(
        service
            .expand(&memory_id, 2, None)
            .unwrap()
            .content
            .contains("navigate")
    );
    let recalled = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap();
    assert_eq!(recalled.hits[0].version_id, version_id);
    assert_eq!(service.dashboard(AUTHOR).unwrap().total_points, 20);
    let receipt = recalled.receipt_id.unwrap();
    let input = OutcomeReportRequest {
        outcome: MemoryOutcome::Success,
        used_version_ids: vec![version_id],
    };
    assert!(
        service
            .report_outcome(READER, &receipt, input.clone())
            .unwrap()
            .recorded
    );
    assert!(
        !service
            .report_outcome(READER, &receipt, input)
            .unwrap()
            .recorded
    );
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 22);
    assert_eq!(
        repository
            .process_next(&mut provider("high"), &PassingReplay, now())
            .unwrap(),
        WorkResult::Idle
    );
    assert!(
        repository
            .pending_outbox(100)
            .unwrap()
            .iter()
            .any(|event| event.kind == "curation_committed")
    );
}

#[test]
fn manual_approval_requires_a_replay_verified_proposal_and_low_confidence_is_reviewed() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let claim = service.claim_contribution_for_review(&id).unwrap();
    assert_eq!(
        service
            .decide_contribution_review(
                &id,
                claim.review_generation,
                ContributionReviewDecision::Approve,
                "reviewer"
            )
            .unwrap()
            .status,
        "queued"
    );
    assert!(service.catalog().unwrap().memories.is_empty());
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    assert_eq!(
        repository
            .process_next(&mut provider("low"), &PassingReplay, now())
            .unwrap(),
        WorkResult::NeedsReview
    );
    assert!(service.catalog().unwrap().memories.is_empty());
    let claim = service.claim_contribution_for_review(&id).unwrap();
    assert_eq!(
        service
            .decide_contribution_review(
                &id,
                claim.review_generation,
                ContributionReviewDecision::Approve,
                "reviewer"
            )
            .unwrap()
            .status,
        "approved"
    );
    assert_eq!(service.catalog().unwrap().memories.len(), 1);
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    assert_eq!(
        service.decide_contribution_review(
            &id,
            claim.review_generation,
            ContributionReviewDecision::Approve,
            "reviewer"
        ),
        Err(ServiceError::Conflict)
    );
}

#[test]
fn identical_actions_on_different_routes_publish_separate_memories() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap();
    let request:wam_contracts::CreateContributionRequest=serde_json::from_value(serde_json::json!({
        "url":"https://example.org/other-editor","task_category":"compose-draft","actions":["navigate","edit","save"],
        "consent":true,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION
    })).unwrap();
    service.create_contribution(SUPPORTER, request).unwrap();
    let mut payload = record();
    payload["l0"]["url_patterns"] = serde_json::json!(["/other-editor"]);
    assert!(matches!(
        repository
            .process_next(&mut scripted_record(payload, "high"), &PassingReplay, now())
            .unwrap(),
        WorkResult::Published { .. }
    ));
    assert_eq!(service.catalog().unwrap().memories.len(), 2);
    assert_eq!(repository.total_points_for(SUPPORTER).unwrap(), 20);
}

#[test]
fn provider_quota_wait_is_not_a_failed_attempt_and_does_not_call_early() {
    use wam_gatekeeper::provider::{LlmProvider, ModelTurn, ProviderError, TurnRequest};
    struct Quota {
        calls: usize,
    }
    impl LlmProvider for Quota {
        fn name(&self) -> String {
            "test:quota".into()
        }
        fn next_turn(&mut self, _: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
            self.calls += 1;
            Err(ProviderError::QuotaExhausted {
                why: "test quota".into(),
                retry_after_secs: Some(120),
            })
        }
    }
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let started = now();
    let mut quota = Quota { calls: 0 };
    for attempt in 0..7 {
        let at = started + attempt * 121;
        assert_eq!(
            repository
                .process_next(&mut quota, &PassingReplay, at)
                .unwrap(),
            WorkResult::Retry
        );
        assert_eq!(
            repository
                .process_next(&mut quota, &PassingReplay, at + 1)
                .unwrap(),
            WorkResult::Idle
        );
    }
    assert_eq!(quota.calls, 7);
    assert_eq!(service.contribution(AUTHOR, &id).unwrap().status, "queued");
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    assert!(matches!(
        repository
            .process_next(&mut provider("high"), &PassingReplay, started + 7 * 121)
            .unwrap(),
        WorkResult::Published { .. }
    ));
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
}

#[test]
fn invalid_scope_secrets_and_replay_failure_never_publish_or_award() {
    for mutation in ["scope", "route", "secret", "schema"] {
        let repository = ApplicationRepository::in_memory();
        let service = prepare(&repository);
        contribution(&service, AUTHOR);
        let mut payload = record();
        match mutation {
            "scope" => payload["l0"]["site_host"] = "other.org".into(),
            "route" => payload["l0"]["url_patterns"] = serde_json::json!(["/admin"]),
            "secret" => {
                payload["l1"]["summary"] =
                    "Authorization: Bearer private-sentinel-credential".into()
            }
            _ => payload["private_trace"] = "private-sentinel-trace".into(),
        }
        assert_eq!(
            repository
                .process_next(&mut scripted_record(payload, "high"), &PassingReplay, now())
                .unwrap(),
            WorkResult::Retry
        );
        assert!(service.catalog().unwrap().memories.is_empty());
        assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    }
    struct InvalidReplay;
    impl ReplayVerifier for InvalidReplay {
        fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError> {
            let mut evidence = PassingReplay.verify(request)?;
            evidence.input_digest = "forged".into();
            Ok(evidence)
        }
    }
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    assert_eq!(
        repository
            .process_next(&mut provider("high"), &InvalidReplay, now())
            .unwrap(),
        WorkResult::NeedsReview
    );
    assert!(service.catalog().unwrap().memories.is_empty());
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
}

#[test]
fn withdrawal_during_final_replay_fences_publication_and_credit() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    struct Withdraw {
        repository: ApplicationRepository,
        calls: Cell<usize>,
    }
    impl ReplayVerifier for Withdraw {
        fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() == 2 {
                self.repository.set_consent(
                    AUTHOR,
                    false,
                    wam_contracts::CONTRIBUTION_CONSENT_VERSION,
                    now(),
                )?;
            }
            PassingReplay.verify(request)
        }
    }
    let replay = Withdraw {
        repository: repository.clone(),
        calls: Cell::new(0),
    };
    assert_eq!(
        repository.process_next(&mut provider("high"), &replay, now()),
        Err(ServiceError::Conflict)
    );
    assert!(service.catalog().unwrap().memories.is_empty());
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
}

#[test]
fn independent_support_survives_author_deletion_without_becoming_a_new_author() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published { memory_id, .. } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    contribution(&service, SUPPORTER);
    assert_eq!(
        repository
            .process_next(&mut provider("high"), &PassingReplay, now())
            .unwrap(),
        WorkResult::Supported {
            memory_id: memory_id.clone()
        }
    );
    assert_eq!(repository.total_points_for(SUPPORTER).unwrap(), 0);
    repository.delete_account(AUTHOR, now()).unwrap();
    assert_eq!(
        repository.ensure_account(AUTHOR),
        Err(wam_server::ports::RepositoryError::AccountDisabled)
    );
    assert_eq!(service.my_profile(AUTHOR), Err(ServiceError::NotFound));
    assert_eq!(service.catalog().unwrap().memories.len(), 1);
    assert!(repository.memories()[0].author_account_id.is_none());
    assert!(service.expand(&memory_id, 2, None).is_ok());
}

#[test]
fn sole_attribution_withdrawal_retires_memory_reverses_award_and_retention_is_idempotent() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let WorkResult::Published { memory_id, .. } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
    assert!(repository.withdraw_for_account(AUTHOR, &id).unwrap());
    assert!(service.catalog().unwrap().memories.is_empty());
    assert_eq!(
        service.expand(&memory_id, 1, None),
        Err(ServiceError::NotFound)
    );
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    assert!(repository.withdraw_for_account(AUTHOR, &id).unwrap());
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    let pending = contribution(&service, READER);
    let after = now() + 7 * 24 * 60 * 60 + 1;
    assert_eq!(repository.purge(after).unwrap().contributions, 1);
    assert_eq!(
        service.contribution(READER, &pending).unwrap().status,
        "expired"
    );
    assert!(
        service
            .contribution(READER, &pending)
            .unwrap()
            .actions
            .is_empty()
    );
    assert_eq!(repository.purge(after).unwrap().contributions, 0);
}

#[test]
fn retiring_a_version_also_blocks_new_awards_from_previously_issued_receipts() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published {
        memory_id,
        version_id,
    } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    let receipt = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap()
        .receipt_id
        .unwrap();
    repository.retire_version(&memory_id, 1, now()).unwrap();
    assert_eq!(
        service.expand_version(&memory_id, 1, Some(&version_id), None),
        Err(ServiceError::NotFound)
    );
    service
        .report_outcome(
            READER,
            &receipt,
            OutcomeReportRequest {
                outcome: MemoryOutcome::Success,
                used_version_ids: vec![version_id],
            },
        )
        .unwrap();
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
}

#[test]
fn repeated_submission_nonce_is_idempotent_and_cannot_change_payload_or_restore_withdrawal() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    let mut request: wam_contracts::CreateContributionRequest = serde_json::from_value(serde_json::json!({
        "url":"https://example.org/editor","task_category":"compose-draft","actions":["navigate","edit","save"],
        "consent":true,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION,"nonce":"request-1"
    })).unwrap();
    let first = service
        .create_contribution(AUTHOR, request.clone())
        .unwrap();
    assert_eq!(
        service
            .create_contribution(AUTHOR, request.clone())
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(service.pending_gatekeeper_jobs().unwrap(), 1);
    request.url = "https://example.org/other".into();
    assert_eq!(
        service.create_contribution(AUTHOR, request.clone()),
        Err(ServiceError::Conflict)
    );
    request.url = "https://example.org/editor".into();
    assert!(repository.withdraw_for_account(AUTHOR, &first.id).unwrap());
    assert_eq!(
        service.create_contribution(AUTHOR, request),
        Err(ServiceError::Conflict)
    );
}

#[test]
fn version_authors_are_frozen_and_rollback_appends_revision_and_reverses_only_rejected_version() {
    let repository = ApplicationRepository::in_memory();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published {
        memory_id,
        version_id: first,
    } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    let old_receipt = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap()
        .receipt_id
        .unwrap();
    improvement(&service, SUPPORTER);
    let WorkResult::Published {
        version_id: second, ..
    } = repository
        .process_next(&mut replacement(&memory_id, 1), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    let recalled_version = service
        .expand_version(&memory_id, 1, Some(&first), None)
        .expect("an approved superseded version remains readable by its pinned ID");
    assert_eq!(recalled_version.version_id, first);
    let new_receipt = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap()
        .receipt_id
        .unwrap();
    service
        .report_outcome(
            READER,
            &old_receipt,
            OutcomeReportRequest {
                outcome: MemoryOutcome::Success,
                used_version_ids: vec![first],
            },
        )
        .unwrap();
    service
        .report_outcome(
            READER,
            &new_receipt,
            OutcomeReportRequest {
                outcome: MemoryOutcome::Success,
                used_version_ids: vec![second],
            },
        )
        .unwrap();
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 22);
    assert_eq!(repository.total_points_for(SUPPORTER).unwrap(), 22);
    let restored = repository
        .rollback_version(&memory_id, 1, 2, &PassingReplay, now())
        .unwrap();
    assert_eq!(
        restored,
        repository
            .rollback_version(&memory_id, 1, 2, &PassingReplay, now())
            .unwrap()
    );
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 22);
    assert_eq!(repository.total_points_for(SUPPORTER).unwrap(), 0);
    assert_eq!(repository.memories()[0].index.version_id(), restored);
    assert_eq!(
        repository.memories()[0].author_account_id.as_deref(),
        Some(AUTHOR)
    );
}

#[test]
fn file_store_keeps_published_versions_scores_and_deletion_across_restart() {
    let directory = std::env::temp_dir().join(format!(
        "wam-lifecycle-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = directory.join("application.json");
    {
        let repository = ApplicationRepository::open(&path).unwrap();
        let service = prepare(&repository);
        contribution(&service, AUTHOR);
        repository
            .process_next(&mut provider("high"), &PassingReplay, now())
            .unwrap();
        assert!(ApplicationRepository::open(&path).is_err());
    }
    {
        let repository = ApplicationRepository::open(&path).unwrap();
        let service = prepare(&repository);
        assert_eq!(service.catalog().unwrap().memories.len(), 1);
        assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 20);
        repository.delete_account(AUTHOR, now()).unwrap();
    }
    {
        let repository = ApplicationRepository::open(&path).unwrap();
        assert!(repository.memories().is_empty());
        assert!(repository.profile_for_account(AUTHOR).is_none());
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains(AUTHOR));
        assert!(!raw.contains("private-sentinel"));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
