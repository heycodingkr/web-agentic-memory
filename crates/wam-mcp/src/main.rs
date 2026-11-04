use wam_client::{DiagnosticError, DiagnosticOperation, DiagnosticSource, HttpClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint =
        std::env::var("WAM_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned());
    let token = match wam_client::credentials::token_for_endpoint(&endpoint) {
        Ok(token) => token,
        Err(error) => {
            eprintln!("{error}");
            None
        }
    };
    let mut client = HttpClient::new(endpoint, token)?;
    client.set_diagnostic_source(DiagnosticSource::Mcp);
    if let Err(error) = client.enable_local_cache() {
        eprintln!("local cache disabled: {error}");
    }
    if let Err(error) = wam_mcp::stdio::serve(client.clone()).await {
        let trace = client
            .report_diagnostic_event(
                DiagnosticOperation::Unknown,
                DiagnosticError::UnhandledError,
            )
            .await;
        if let Some(trace) = trace {
            eprintln!("trace: {trace}");
        }
        eprintln!("stdio transport failed: {error}");
        std::process::exit(1);
    }
    Ok(())
}
