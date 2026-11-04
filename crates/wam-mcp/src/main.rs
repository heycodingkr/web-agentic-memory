use std::io::{self, BufRead, Write};
use wam_client::{DiagnosticError, DiagnosticOperation, DiagnosticSource, HttpClient};
use wam_mcp::{decode_line, handle_message, parse_error_response};

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
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                report_io_failure(&client).await;
                return Err(error.into());
            }
        };
        let response = match decode_line(&line) {
            Ok(value) => handle_message(&client, value).await,
            Err(_) => {
                let trace = client
                    .report_diagnostic_event(
                        DiagnosticOperation::Unknown,
                        DiagnosticError::InvalidInput,
                    )
                    .await;
                let mut error = parse_error_response();
                if let Some(trace) = trace {
                    error["error"]["data"] = serde_json::json!({"trace_id": trace});
                }
                Some(error)
            }
        };
        if let Some(response) = response {
            if let Err(error) = serde_json::to_writer(&mut stdout, &response) {
                report_io_failure(&client).await;
                return Err(error.into());
            }
            if let Err(error) = stdout.write_all(b"\n").and_then(|()| stdout.flush()) {
                report_io_failure(&client).await;
                return Err(error.into());
            }
        }
    }
    Ok(())
}

async fn report_io_failure(client: &HttpClient) {
    if let Some(trace) = client
        .report_diagnostic_event(
            DiagnosticOperation::Unknown,
            DiagnosticError::UnhandledError,
        )
        .await
    {
        eprintln!("trace: {trace}");
    }
}
