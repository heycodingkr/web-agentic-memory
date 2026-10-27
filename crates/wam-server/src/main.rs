use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use wam_server::{
    DemoReviewerTokenAuthenticator, DemoTokenAuthenticator, FileContributionStore,
    FileIdentityStore, FileScoreStore, FileUsageStore, IdentityAuthenticator, IdentityService,
    LocalRepository, WamService, router_with_identity,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind: SocketAddr = std::env::var("WAM_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
        .parse()?;
    let data_dir =
        PathBuf::from(std::env::var("WAM_DATA_DIR").unwrap_or_else(|_| ".wam-data".to_owned()));
    let contributions = Arc::new(FileContributionStore::open(
        data_dir.join("contributions.json"),
    )?);
    let usage = Arc::new(FileUsageStore::open(data_dir.join("usage.json"))?);
    let scores = Arc::new(FileScoreStore::open(data_dir.join("scores.json"))?);
    let service = Arc::new(WamService::with_all_stores(
        LocalRepository::open(data_dir.join("profiles.json"))?,
        contributions,
        usage,
        scores,
    )?);
    let identity_store = Arc::new(FileIdentityStore::open(data_dir.join("identity.json"))?);
    let identity = Arc::new(IdentityService::from_env(identity_store)?);
    let auth = Arc::new(IdentityAuthenticator::new(
        DemoTokenAuthenticator::from_env(),
        Arc::clone(&identity),
    ));
    let app = router_with_identity(
        service,
        auth,
        Arc::new(DemoReviewerTokenAuthenticator::from_env()),
        identity,
    );
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("wam-server listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
