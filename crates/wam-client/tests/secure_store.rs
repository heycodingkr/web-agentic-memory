use wam_client::credentials::{CredentialStore, LinkedCredential, OsCredentialStore};

#[test]
#[ignore = "requires an unlocked OS secure store; uses and removes one disposable entry"]
fn native_secure_store_round_trip_never_creates_a_plaintext_file() {
    let namespace = std::env::temp_dir().join(format!(
        "wam-keychain-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = OsCredentialStore::new("https://wam-secure-store.invalid", &namespace).unwrap();
    struct Cleanup(OsCredentialStore);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.delete();
        }
    }
    let cleanup = Cleanup(store);
    assert!(cleanup.0.load().unwrap().is_none());
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).unwrap();
    let credential = LinkedCredential {
        device_id: "device_test".into(),
        access_token: random.iter().map(|byte| format!("{byte:02x}")).collect(),
        scopes: vec!["read:memories".into()],
        expires_at: u64::MAX,
    };
    cleanup.0.save(&credential).unwrap();
    let loaded = cleanup.0.load().unwrap().unwrap();
    assert!(loaded.access_token == credential.access_token);
    assert_eq!(loaded.device_id, credential.device_id);
    assert!(
        !namespace.exists(),
        "credentials must not create plaintext config files"
    );
    cleanup.0.delete().unwrap();
    assert!(cleanup.0.load().unwrap().is_none());
}
