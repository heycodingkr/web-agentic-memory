use postgres::{Client, NoTls};
use std::{env, path::PathBuf};

pub fn owned_root() -> PathBuf {
    let root =
        PathBuf::from(env::var("WAM_RELEASE_PROBE_ROOT").expect("owned fixture root required"));
    assert_eq!(
        std::fs::read_to_string(root.join(".wam-owned")).unwrap(),
        "release-verification"
    );
    root.canonicalize().unwrap()
}

pub fn database() -> (Client, String) {
    let root = owned_root();
    let raw = env::var("WAM_TEST_DATABASE_URL").expect("owned database required");
    let url = url::Url::parse(&raw).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(url.username(), "wam_verify");
    assert!(matches!(
        url.path(),
        "/wam_source" | "/wam_restored" | "/wam_profile" | "/wam_shutdown"
    ));
    let mut client = Client::connect(&raw, NoTls).expect("owned database connection");
    let data: String = client.query_one("SHOW data_directory", &[]).unwrap().get(0);
    assert_eq!(
        PathBuf::from(data).canonicalize().unwrap(),
        root.join("pgdata").canonicalize().unwrap()
    );
    (client, raw)
}
