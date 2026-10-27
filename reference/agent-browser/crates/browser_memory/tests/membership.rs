mod account_fixture;
use browser_memory::{web,workflow::{self,Trace}};
use serde_json::json;
use host::settings::Settings;

#[test]
fn first_login_enables_catalog_refresh_without_requiring_a_settings_change() {
    let root=tempfile::tempdir().unwrap();
    account_fixture::login(root.path(),web::DEFAULT_ENDPOINT,"new-member");
    let config=web::Config::load(root.path());
    assert!(config.network,"authenticated first-run defaults permit background catalog refresh");
    assert!(!config.contribute,"workflow upload still needs explicit saved consent");
}
#[test]
fn guests_cannot_receive_bundled_or_cached_member_workflows() {
    let root=tempfile::tempdir().unwrap();
    assert!(web::turn_context(root.path(),"medium.com","draft").is_none(),"member workflows require an Arise login");
    assert!(!web::refresh(root.path(),true));
    assert!(web::expand(root.path(),&json!({"id":"web:medium-draft","level":2})).is_err());
}
#[test]
fn cache_and_contributions_are_scoped_to_identity_and_logout_disables_injection() {
    let root=tempfile::tempdir().unwrap();let root=root.path();
    let endpoint="http://127.0.0.1:1";
    let mut settings=Settings::default();
    settings.web_agentic_memory_endpoint=endpoint.into();
    settings.web_workflow_contribution=true;
    settings.save(&root.join("settings.toml")).unwrap();
    account_fixture::login(root,endpoint,"user-a");
    let mut catalog:serde_json::Value=serde_json::from_slice(include_bytes!("../../../assets/web_agentic_memory.json")).unwrap();
    catalog["entries"][0]["summary"]=json!("USER_A_CACHED_SENTINEL");
    let site=catalog["entries"][0]["site"].as_str().unwrap().to_owned();
    std::fs::create_dir_all(root.join("memory")).unwrap();
    std::fs::write(web::cache_path(root),serde_json::to_vec(&json!({"endpoint":endpoint,"user_id":"user-a","fetched_at":0,"catalog":catalog})).unwrap()).unwrap();
    assert!(web::turn_context(root,&site,"").unwrap().contains("USER_A_CACHED_SENTINEL"));
    let mut trace=Trace::default();
    trace.call(root,"a","browser_click","medium.com");trace.result(root,"a",false);
    trace.finish(root,"completed");
    assert_eq!(workflow::pending_count(root),1);
    let queue_a=workflow::queue_dir(root);
    trace.call(root,"inflight","browser_click","medium.com");trace.result(root,"inflight",false);
    account_fixture::login(root,endpoint,"user-b");
    trace.finish(root,"completed");
    assert_eq!(workflow::pending_count(root),0,"inflight A steps cannot become B reports");
    assert_ne!(workflow::queue_dir(root),queue_a);
    assert!(!web::turn_context(root,&site,"").unwrap_or_default().contains("USER_A_CACHED_SENTINEL"));
    std::fs::remove_file(mesh_account::arise::state_file(root,endpoint)).unwrap();
    assert!(web::turn_context(root,"medium.com","draft").is_none());
    workflow::prefetch_upload(root);
    assert!(!queue_a.exists(),"logout purges pending contributions");
    assert!(!browser_memory::Store::db_path(root).exists());
}
#[test]
fn expired_metadata_does_not_authorize_cached_workflow_injection() {
    let root=tempfile::tempdir().unwrap();
    account_fixture::login(root.path(),web::DEFAULT_ENDPOINT,"expired-user");
    let path=mesh_account::arise::state_file(root.path(),web::DEFAULT_ENDPOINT);
    let mut data:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    data["expiresAt"]=json!(0);
    std::fs::write(path,serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(web::turn_context(root.path(),"medium.com","draft").is_none());
}
