use clap::Parser;
use wam_cli::{Cli, execute};
use wam_client::HttpClient;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let token = std::env::var("WAM_TOKEN").ok();
    let client = match HttpClient::new(cli.endpoint, token) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    match execute(&client, cli.command).await {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            let code = match error {
                wam_client::ApiError::InvalidInput => 2,
                wam_client::ApiError::Unauthorized => 3,
                wam_client::ApiError::Forbidden => 4,
                wam_client::ApiError::Conflict => 4,
                wam_client::ApiError::Transport | wam_client::ApiError::Server => 5,
                _ => 1,
            };
            std::process::exit(code);
        }
    }
}
