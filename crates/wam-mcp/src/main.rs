use std::io::{self, BufRead, Write};
use wam_client::HttpClient;
use wam_mcp::{decode_line, handle_message, parse_error_response};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint =
        std::env::var("WAM_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned());
    let token = std::env::var("WAM_TOKEN").ok();
    let client = HttpClient::new(endpoint, token)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let response = match decode_line(&line) {
            Ok(value) => handle_message(&client, value).await,
            Err(_) => Some(parse_error_response()),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut stdout, &response)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
