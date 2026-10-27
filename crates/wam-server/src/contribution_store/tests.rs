use super::FileContributionStore;
use crate::ports::ContributionStore;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, path::PathBuf};
use wam_contracts::{ContributionAction, ContributionRecordResponse, ContributionReviewDecision};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_store_path() -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "wam-contribution-store-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ))
        .join("contributions.json")
}

fn add(store: &dyn ContributionStore, account: &str) -> ContributionRecordResponse {
    store
        .create(
            account,
            "https://example.com/guide".to_owned(),
            "compose-draft".to_owned(),
            vec![ContributionAction::Navigate, ContributionAction::Save],
            100,
        )
        .expect("store contribution")
}

#[test]
fn file_store_keeps_receipts_and_next_id_across_reopen() {
    let path = temp_store_path();
    let first = {
        let store = FileContributionStore::open(&path).expect("open store");
        add(&store, "account-a")
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().expect("parent directory"))
                .expect("directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    let reopened = FileContributionStore::open(&path).expect("reopen store");
    assert_eq!(
        reopened
            .get_for_account("account-a", &first.id)
            .expect("read receipt"),
        Some(first.clone())
    );
    assert_eq!(
        reopened
            .get_for_account("account-b", &first.id)
            .expect("owner-scoped lookup"),
        None
    );
    let second = add(&reopened, "account-a");
    assert_eq!(second.id, "local-000002");

    assert!(
        reopened
            .withdraw_for_account("account-a", &first.id)
            .expect("withdraw")
    );
    drop(reopened);

    let reopened = FileContributionStore::open(&path).expect("reopen after withdrawal");
    assert_eq!(
        reopened
            .get_for_account("account-a", &first.id)
            .expect("read withdrawn receipt"),
        None
    );
    assert_eq!(
        reopened
            .list_for_account("account-a")
            .expect("list remaining receipt"),
        vec![second]
    );
    let _ = fs::remove_dir_all(path.parent().expect("temp directory"));
}

#[test]
fn file_store_withdrawal_is_owner_scoped() {
    let path = temp_store_path();
    let store = FileContributionStore::open(&path).expect("open store");
    let record = add(&store, "account-a");

    assert!(
        !store
            .withdraw_for_account("account-b", &record.id)
            .expect("owner-scoped withdrawal")
    );
    assert_eq!(
        store
            .get_for_account("account-a", &record.id)
            .expect("owner can still read"),
        Some(record)
    );
    let _ = fs::remove_dir_all(path.parent().expect("temp directory"));
}

#[test]
fn file_store_persists_review_generation_and_rejects_stale_transitions() {
    let path = temp_store_path();
    let record = {
        let store = FileContributionStore::open(&path).expect("open store");
        add(&store, "account-a")
    };
    let claimed = {
        let store = FileContributionStore::open(&path).expect("reopen store");
        let claimed = store
            .claim_for_review(&record.id)
            .expect("claim transaction")
            .expect("queued record");
        assert_eq!(claimed.review_generation, 1);
        assert!(
            store
                .claim_for_review(&record.id)
                .expect("duplicate claim")
                .is_none()
        );
        claimed
    };

    let reopened = FileContributionStore::open(&path).expect("restore review state");
    let restored = reopened.all_for_review().expect("list review records");
    assert_eq!(restored[0].review_generation, claimed.review_generation);
    assert_eq!(restored[0].record.status, "in_review");
    assert!(reopened.all_for_queue().expect("active queue").len() == 1);
    assert!(
        reopened
            .decide_for_review(
                &record.id,
                0,
                ContributionReviewDecision::Approve,
                "reviewer-a",
                10,
            )
            .expect("stale decision")
            .is_none()
    );
    assert_eq!(
        reopened
            .decide_for_review(
                &record.id,
                claimed.review_generation,
                ContributionReviewDecision::Approve,
                "reviewer-a",
                11
            )
            .expect("valid decision")
            .expect("reviewed record")
            .status,
        "approved"
    );
    assert!(reopened.all_for_queue().expect("resolved queue").is_empty());
    drop(reopened);

    let reopened = FileContributionStore::open(&path).expect("restore final decision");
    assert_eq!(
        reopened.all_for_review().expect("review history")[0]
            .record
            .status,
        "approved"
    );
    let event = reopened
        .review_audit_history()
        .expect("review audit history")
        .pop()
        .expect("decision event");
    assert_eq!(event.reviewer, "reviewer-a");
    assert_eq!(event.decided_at, 11);
    assert!(
        reopened
            .withdraw_for_account("account-a", &record.id)
            .expect("withdraw reviewed receipt")
    );
    assert_eq!(
        reopened
            .review_audit_history()
            .expect("audit survives withdrawal")
            .len(),
        1
    );
    let _ = fs::remove_dir_all(path.parent().expect("temp directory"));
}

#[test]
fn file_store_migrates_received_receipts_into_the_durable_review_queue() {
    let path = temp_store_path();
    fs::create_dir_all(path.parent().expect("temp directory")).expect("create directory");
    fs::write(
        &path,
        r#"{"schema_version":1,"next_id":2,"contributions":[{"account_id":"account-a","record":{"id":"local-000001","url_pattern":"https://example.com/guide","task_category":"compose-draft","actions":["navigate","save"],"status":"received","submitted_at":100}}]}"#,
    )
    .expect("write legacy store");

    let store = FileContributionStore::open(&path).expect("migrate legacy store");
    assert_eq!(
        store
            .get_for_account("account-a", "local-000001")
            .expect("read migrated receipt")
            .expect("receipt exists")
            .status,
        "queued"
    );
    assert_eq!(store.all_for_queue().expect("queue restore").len(), 1);
    let serialized: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read migrated store"))
            .expect("parse migrated state");
    assert_eq!(serialized["schema_version"], 2);
    assert_eq!(serialized["contributions"][0]["review_generation"], 0);
    let _ = fs::remove_dir_all(path.parent().expect("temp directory"));
}
