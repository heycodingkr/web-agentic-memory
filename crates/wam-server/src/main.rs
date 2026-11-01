use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use wam_server::{
    ApplicationRepository, DemoReviewerTokenAuthenticator, DemoTokenAuthenticator,
    FileIdentityStore, IdentityAuthenticator, IdentityService, InMemoryRateLimitStore, IpNetwork,
    RateLimitStore, RedisRateLimitStore, WamService, router_with_identity_and_rate_limit_store,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let production = std::env::var("WAM_ENV").is_ok_and(|value| value == "production");
    let bind: SocketAddr = std::env::var("WAM_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
        .parse()?;
    let repository = match std::env::var("WAM_DATABASE_URL") {
        Ok(url) => ApplicationRepository::connect(&url)?,
        Err(_) if production => {
            return Err("WAM_DATABASE_URL is required in production".into());
        }
        Err(_) => {
            let data_dir =
                PathBuf::from(std::env::var("WAM_DATA_DIR").unwrap_or_else(|_| ".wam-data".into()));
            ApplicationRepository::open_directory(&data_dir)?
        }
    };
    // Example memories are for local development: on by default only with the local file store outside production.
    let examples_default = !production && std::env::var_os("WAM_DATABASE_URL").is_none();
    let examples_setting = std::env::var("WAM_EXAMPLES").ok();
    if wam_server::examples::enabled(examples_default, examples_setting.as_deref()) {
        if wam_server::examples::refresh_requested(examples_setting.as_deref()) {
            let report = repository.refresh_examples()?;
            eprintln!(
                "built-in example memories: {} added, {} updated, {} protected",
                report.added, report.updated, report.protected
            );
        } else {
            let added = repository.ensure_examples()?;
            eprintln!("built-in example memories: {added} added (WAM_EXAMPLES=off turns them off)");
        }
    }
    let service = Arc::new(WamService::durable(repository.clone())?);
    let identity_store = Arc::new(FileIdentityStore::from_repository(repository.clone()));
    let identity = Arc::new(IdentityService::from_env(identity_store)?);
    let auth = Arc::new(IdentityAuthenticator::new(
        DemoTokenAuthenticator::from_env(),
        Arc::clone(&identity),
    ));
    let rate_limit_key_secret = match std::env::var("WAM_RATE_LIMIT_KEY_SECRET") {
        Ok(secret) if secret.len() >= 32 => secret,
        _ if production => {
            return Err(
                "WAM_RATE_LIMIT_KEY_SECRET must contain at least 32 bytes in production".into(),
            );
        }
        _ => "wam-local-development-only".to_owned(),
    };
    let rate_limits: Arc<dyn RateLimitStore> = match std::env::var("WAM_RATE_LIMIT_REDIS_URL") {
        Ok(url) => {
            if production && !is_local_redis(&url)? && !url.starts_with("rediss://") {
                return Err("production Redis must use rediss:// outside localhost".into());
            }
            Arc::new(RedisRateLimitStore::connect(&url).await?)
        }
        Err(_) if production => {
            return Err("WAM_RATE_LIMIT_REDIS_URL is required in production".into());
        }
        Err(_) => Arc::new(InMemoryRateLimitStore::default()),
    };
    let trusted_proxy_ranges = std::env::var("WAM_TRUSTED_PROXY_CIDRS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::parse::<IpNetwork>)
        .collect::<Result<Vec<_>, _>>()?;
    let retention_limits = Arc::clone(&rate_limits);
    let retention_secret = rate_limit_key_secret.clone();
    let app = router_with_identity_and_rate_limit_store(
        service,
        auth,
        Arc::new(DemoReviewerTokenAuthenticator::from_env()),
        identity,
        rate_limits,
        rate_limit_key_secret,
        trusted_proxy_ranges,
    );
    let diagnostics = if let Ok(url) =
        std::env::var("WAM_LOG_DATABASE_URL").or_else(|_| std::env::var("WAM_DATABASE_URL"))
    {
        wam_server::diagnostics::Diagnostics::postgres(&url)?
    } else {
        let path = std::env::var_os("WAM_DIAGNOSTICS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var("WAM_DATA_DIR").unwrap_or_else(|_| ".wam-data".into()))
                    .join("diagnostics.sqlite")
            });
        wam_server::diagnostics::Diagnostics::sqlite(path)?
    };
    let shutdown_diagnostics = diagnostics.clone();
    let retention_diagnostics = diagnostics.clone();
    let app = wam_server::diagnostics::observe(app, diagnostics);
    let retention = repository.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let repository = retention.clone();
            let diagnostic_store = retention_diagnostics.clone();
            let limits = Arc::clone(&retention_limits);
            let secret = retention_secret.clone();
            let runtime = tokio::runtime::Handle::current();
            let result = tokio::task::spawn_blocking(move || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let _ = diagnostic_store.purge(now);
                repository.purge(now)?;
                runtime.block_on(repository.deliver_account_deletions(
                    limits.as_ref(),
                    &secret,
                    now,
                ))
            })
            .await;
            if !matches!(result, Ok(Ok(_))) {
                eprintln!("retention or account projection cleanup deferred");
            }
        }
    });
    let _worker = if std::env::var("WAM_RUN_WORKER").is_ok_and(|value| value == "1") {
        Some(wam_server::durable::runtime::start_background(
            repository.clone(),
        )?)
    } else {
        None
    };
    let (stream_shutdown, stream_shutdown_rx) = tokio::sync::watch::channel(false);
    let app = app.layer(axum::Extension(
        wam_server::mcp_transport::McpStreamShutdown(stream_shutdown_rx),
    ));
    let shutdown = shutdown_signal()?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("wam-server listening on {}", listener.local_addr()?);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown.await;
        let _ = stream_shutdown.send(true);
    })
    .await?;
    let _ = tokio::task::spawn_blocking(move || shutdown_diagnostics.flush()).await;
    Ok(())
}

fn shutdown_signal() -> std::io::Result<impl std::future::Future<Output = ()>> {
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    Ok(async move {
        #[cfg(unix)]
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
    })
}

fn is_local_redis(value: &str) -> Result<bool, url::ParseError> {
    let parsed = url::Url::parse(value)?;
    Ok(matches!(
        parsed.host_str(),
        Some("localhost" | "127.0.0.1" | "::1")
    ))
}
