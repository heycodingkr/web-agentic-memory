use wam_core::profile::PublicHandle;
use wam_server::{ApplicationRepository, WamService, examples, ports::Repository};

#[test]
fn built_in_memories_credit_the_official_public_author_once() {
    let repository = ApplicationRepository::demo();
    let count = repository.ensure_examples().unwrap();
    let ids: Vec<_> = examples::load()
        .unwrap()
        .into_iter()
        .map(|example| example.id)
        .collect();
    let service = WamService::durable(repository.clone()).unwrap();
    let catalog = serde_json::to_value(service.catalog().unwrap()).unwrap();
    for memory in catalog["memories"].as_array().unwrap() {
        if ids.iter().any(|id| memory["memory_id"] == id.as_str()) {
            assert_eq!(memory["author_handle"], "wam_official", "{memory}");
        }
    }
    let board = service.leaderboard().unwrap();
    let official = board
        .entries
        .iter()
        .find(|entry| entry.handle == "wam_official")
        .expect("the bundled workflows' publisher appears in the public ranking");
    assert_eq!(official.total_points, count as i64 * 20);
    let profile = service.public_profile("wam_official").unwrap();
    assert!(profile.public_opt_in);
    assert!(!profile.bio.is_empty());
    assert_eq!(repository.ensure_examples().unwrap(), 0);
    assert_eq!(
        repository.refresh_examples().unwrap(),
        examples::RefreshReport::default()
    );
    assert_eq!(
        service.leaderboard().unwrap(),
        board,
        "startup and unchanged refresh must not mint points"
    );
    assert!(!catalog.to_string().contains("author_account_id"));
}

#[test]
fn catalog_authorship_tracks_public_profile_consent_and_current_handle() {
    let repository = ApplicationRepository::demo();
    let record = repository.memories().remove(0);
    let account = record.author_account_id.unwrap();
    let snapshot = repository.profile_for_account(&account).unwrap();
    let service = WamService::durable(repository.clone()).unwrap();
    let listed_author = |service: &WamService<ApplicationRepository>| {
        let catalog = serde_json::to_value(service.catalog().unwrap()).unwrap();
        catalog["memories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["memory_id"] == record.index.id())
            .unwrap()["author_handle"]
            .clone()
    };
    assert_eq!(listed_author(&service), snapshot.profile.handle().as_str());
    repository
        .save_profile(&account, snapshot.profile.clone(), false)
        .unwrap();
    assert!(
        listed_author(&service).is_null(),
        "private handles must not leak through the catalog"
    );
    repository.ensure_examples().unwrap();
    assert!(
        listed_author(&service).is_null(),
        "seeding must not change existing creators' privacy"
    );
    assert!(
        repository
            .profile_by_handle(&PublicHandle::parse(snapshot.profile.handle().as_str()).unwrap())
            .is_some()
    );
    let response = service.my_profile(&account).unwrap();
    service
        .save_my_profile(
            &account,
            wam_contracts::ProfilePutRequest {
                handle: "renamed_builder".into(),
                bio: response.bio,
                service_links: response.service_links,
                public_opt_in: true,
            },
        )
        .unwrap();
    assert_eq!(listed_author(&service), "renamed_builder");
}
