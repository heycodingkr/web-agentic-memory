mod lifecycle_support;
use lifecycle_support::{PassingReplay, now, provider};
use wam_contracts::{ContributionAction, MemoryOutcome, OutcomeReportRequest};
use wam_server::{
    ApplicationRepository, FileContributionStore, FileScoreStore, FileUsageStore, LocalRepository,
    WamService,
    ports::{
        ContributionStore, ReceiptMemoryAttribution, Repository, ScoreStore, SuccessfulUseAward,
        UsageStore,
    },
};
#[test]
fn legacy_profiles_scores_and_receipts_migrate_but_local_only_consent_is_not_upgraded() {
    let directory = std::env::temp_dir().join(format!(
        "wam-import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let id;
    {
        let profiles = LocalRepository::open(directory.join("profiles.json")).unwrap();
        let snapshot = profiles.profile_for_account("demo-user").unwrap();
        profiles
            .save_profile("demo-user", snapshot.profile, false)
            .unwrap();
        let contributions =
            FileContributionStore::open(directory.join("contributions.json")).unwrap();
        id = contributions
            .create(
                "demo-user",
                "https://example.org/editor".into(),
                "compose-draft".into(),
                vec![ContributionAction::Navigate, ContributionAction::Save],
                now(),
            )
            .unwrap()
            .id;
        let usage = FileUsageStore::open(directory.join("usage.json")).unwrap();
        let receipt = usage
            .record_recall(
                Some("demo-user"),
                &[ReceiptMemoryAttribution {
                    memory_id: "web:example-draft".into(),
                    version_id: "v1".into(),
                    author_account_id: Some("demo-peer".into()),
                }],
                now(),
            )
            .unwrap()
            .unwrap();
        usage
            .report_outcome(
                "demo-user",
                &receipt,
                &OutcomeReportRequest {
                    outcome: MemoryOutcome::Success,
                    used_version_ids: vec!["v1".into()],
                },
                now(),
            )
            .unwrap();
        let scores = FileScoreStore::open(directory.join("scores.json")).unwrap();
        scores
            .award_successful_use(SuccessfulUseAward {
                beneficiary_account_id: "demo-peer".into(),
                consumer_account_id: "demo-user".into(),
                memory_id: "web:example-draft".into(),
                memory_version_id: "v1".into(),
                receipt_id: receipt,
                created_at: now(),
            })
            .unwrap();
    }
    {
        let repository = ApplicationRepository::open_directory(&directory).unwrap();
        let service = WamService::durable(repository.clone()).unwrap();
        assert!(
            !repository
                .profile_for_account("demo-user")
                .unwrap()
                .public_opt_in
        );
        assert_eq!(repository.total_points_for("demo-peer").unwrap(), 2);
        assert_eq!(
            service.contribution("demo-user", &id).unwrap().status,
            "consent_required"
        );
        assert_eq!(
            repository
                .process_next(&mut provider("high"), &PassingReplay, now())
                .unwrap(),
            wam_server::durable::WorkResult::Idle
        );
        for name in [
            "profiles.json",
            "contributions.json",
            "usage.json",
            "scores.json",
            "identity.json",
        ] {
            assert!(!directory.join(name).exists());
        }
        assert!(directory.join("application.json").exists());
    }
    {
        let repository = ApplicationRepository::open_directory(&directory).unwrap();
        assert_eq!(repository.total_points_for("demo-peer").unwrap(), 2);
    }
    std::fs::remove_dir_all(directory).unwrap();
}
