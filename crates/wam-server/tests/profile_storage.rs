use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use wam_core::profile::{CreatorProfile, PublicHandle};
use wam_server::{
    LocalRepository,
    ports::{Repository, RepositoryError},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "wam-profile-storage-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn path(&self) -> PathBuf {
        self.0.join("profiles.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn renamed_profile(repository: &LocalRepository, account: &str, handle: &str) -> CreatorProfile {
    let existing = repository.profile_for_account(account).unwrap().profile;
    CreatorProfile::try_new(
        PublicHandle::parse(handle).unwrap(),
        existing.bio().clone(),
        existing.links().to_vec(),
    )
    .unwrap()
}

#[test]
fn new_accounts_and_handle_ownership_are_restored_without_login_collisions() {
    let fixture = Fixture::new();
    let repository = LocalRepository::open(fixture.path()).unwrap();
    let first = "github_1234567890aaaa";
    let second = "github_1234567890bbbb";
    repository.ensure_account(first).unwrap();
    repository.ensure_account(second).unwrap();
    let first_profile = repository.profile_for_account(first).unwrap();
    let second_profile = repository.profile_for_account(second).unwrap();
    assert_ne!(
        first_profile.profile.handle(),
        second_profile.profile.handle()
    );
    assert!(!first_profile.public_opt_in);
    assert!(!second_profile.public_opt_in);

    let changed = renamed_profile(&repository, first, "continued_builder");
    repository
        .save_profile(first, changed.clone(), false)
        .unwrap();
    assert_eq!(
        repository.save_profile(second, changed.clone(), true),
        Err(RepositoryError::HandleTaken)
    );
    drop(repository);

    let restarted = LocalRepository::open(fixture.path()).unwrap();
    assert_eq!(
        restarted.profile_for_account(first).unwrap().profile,
        changed
    );
    assert_eq!(
        restarted.profile_for_account(second).unwrap(),
        second_profile
    );
    restarted.ensure_account(first).unwrap();
    assert_eq!(restarted.ranking_accounts().len(), 5);
    assert_eq!(
        restarted.save_profile("demo-user", changed, true),
        Err(RepositoryError::HandleTaken)
    );
}

#[test]
fn failed_storage_does_not_commit_a_profile_or_its_public_visibility() {
    let fixture = Fixture::new();
    let repository = LocalRepository::open(fixture.path()).unwrap();
    let original = repository.profile_for_account("demo-user").unwrap();
    let replacement = renamed_profile(&repository, "demo-user", "unsaved_builder");
    let backup = fixture.0.join("saved.json");
    std::fs::rename(fixture.path(), &backup).unwrap();
    std::fs::create_dir(fixture.path()).unwrap();
    assert_eq!(
        repository.save_profile("demo-user", replacement, false),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        repository.profile_for_account("demo-user").unwrap(),
        original
    );
    assert_eq!(
        repository.ensure_account("github_uncommitted"),
        Err(RepositoryError::Unavailable)
    );
    assert!(
        repository
            .profile_for_account("github_uncommitted")
            .is_none()
    );
    std::fs::remove_dir(fixture.path()).unwrap();
    std::fs::rename(backup, fixture.path()).unwrap();
    drop(repository);
    let restarted = LocalRepository::open(fixture.path()).unwrap();
    assert_eq!(
        restarted.profile_for_account("demo-user").unwrap(),
        original
    );
    assert!(
        restarted
            .profile_for_account("github_uncommitted")
            .is_none()
    );
}

#[test]
fn invalid_profile_snapshots_fail_startup_without_replacing_existing_data() {
    let fixture = Fixture::new();
    drop(LocalRepository::open(fixture.path()).unwrap());
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.path()).unwrap()).unwrap();
    let mut duplicate_id = original.clone();
    duplicate_id["accounts"][1]["id"] = duplicate_id["accounts"][0]["id"].clone();
    let mut duplicate_handle = original.clone();
    duplicate_handle["accounts"][1]["profile"]["handle"] = serde_json::json!("AGENT_JASON");
    let mut unsafe_profile = original.clone();
    unsafe_profile["accounts"][0]["profile"]["service_links"][0]["url"] =
        serde_json::json!("javascript:alert(1)");
    let mut future_schema = original.clone();
    future_schema["schema_version"] = serde_json::json!(999);
    let mut corruptions: Vec<_> = [
        duplicate_id,
        duplicate_handle,
        unsafe_profile,
        future_schema,
    ]
    .into_iter()
    .map(|value| serde_json::to_vec(&value).unwrap())
    .collect();
    corruptions.push(b"truncated{".to_vec());
    for bytes in corruptions {
        std::fs::write(fixture.path(), &bytes).unwrap();
        assert!(matches!(
            LocalRepository::open(fixture.path()),
            Err(RepositoryError::Unavailable)
        ));
        assert_eq!(std::fs::read(fixture.path()).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn profile_storage_is_private_and_refuses_symlink_targets() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let repository = LocalRepository::open(fixture.path()).unwrap();
    assert_eq!(
        std::fs::metadata(fixture.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&fixture.0).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let target = fixture.0.join("target.json");
    std::fs::rename(fixture.path(), &target).unwrap();
    let before = std::fs::read(&target).unwrap();
    symlink(&target, fixture.path()).unwrap();
    assert!(matches!(
        LocalRepository::open(fixture.path()),
        Err(RepositoryError::Unavailable)
    ));
    let profile = renamed_profile(&repository, "demo-user", "ignored_builder");
    assert_eq!(
        repository.save_profile("demo-user", profile, false),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(std::fs::read(target).unwrap(), before);
}
