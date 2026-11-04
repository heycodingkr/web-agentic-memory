use clap::Parser;
use wam_cli::{Cli, Command, account::AccountError, execute, local};
use wam_client::{
    DiagnosticError, DiagnosticOperation, HttpClient,
    credentials::{OsCredentialStore, token_for_endpoint},
};

#[tokio::main]
async fn main() {
    let mut cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if !matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let endpoint = std::env::var("WAM_ENDPOINT")
                    .unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned());
                if let Ok(client) = HttpClient::new(endpoint, std::env::var("WAM_TOKEN").ok()) {
                    if let Some(trace) = client
                        .report_diagnostic_event(
                            DiagnosticOperation::Unknown,
                            DiagnosticError::InvalidInput,
                        )
                        .await
                    {
                        eprintln!("trace: {trace}");
                    }
                }
            }
            error.exit();
        }
    };
    if let Err(error) = cli.command.read_input(std::io::stdin().lock()) {
        report_cli_failure(
            &cli.endpoint,
            cli.command.diagnostic_operation(),
            DiagnosticError::InvalidInput,
        )
        .await;
        eprintln!("{error}");
        std::process::exit(2);
    }
    if let Command::Guide { action } = &cli.command {
        match wam_cli::guide::execute(action) {
            Ok(output) => print!("{output}{}", if output.ends_with('\n') { "" } else { "\n" }),
            Err(error) => {
                report_cli_failure(
                    &cli.endpoint,
                    DiagnosticOperation::Unknown,
                    DiagnosticError::UnhandledError,
                )
                .await;
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
        return;
    }
    let registration = match &cli.command {
        Command::Install {
            host,
            scope,
            local,
            mcp_bin,
            dry_run,
            json,
        } => Some((
            wam_cli::install::plan(
                *host,
                *scope,
                &cli.endpoint,
                *local,
                mcp_bin.as_deref(),
                false,
            ),
            *dry_run,
            *json,
        )),
        Command::Uninstall {
            host,
            scope,
            dry_run,
            json,
        } => Some((
            wam_cli::install::plan(*host, *scope, &cli.endpoint, false, None, true),
            *dry_run,
            *json,
        )),
        _ => None,
    };
    if let Some((plan, dry_run, json)) = registration {
        let result = match plan {
            Ok(plan) => wam_cli::install::execute(&plan, dry_run, json).await,
            Err(error) => Err(error),
        };
        match result {
            Ok((output, code)) => {
                println!("{output}");
                std::process::exit(code);
            }
            Err(error) => {
                report_cli_failure(
                    &cli.endpoint,
                    cli.command.diagnostic_operation(),
                    DiagnosticError::InvalidInput,
                )
                .await;
                eprintln!("{error}");
                std::process::exit(local::exit_code(error));
            }
        }
    }
    let operation = cli.command.diagnostic_operation();
    let mut client = match HttpClient::new(&cli.endpoint, None) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let store = match OsCredentialStore::for_endpoint(&cli.endpoint) {
        Ok(store) => store,
        Err(error) => {
            report_client_failure(&client, operation, DiagnosticError::UnhandledError).await;
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = client.enable_local_cache() {
        eprintln!("local cache disabled: {error}");
    }
    if cli.anonymous
        && matches!(
            cli.command,
            Command::Link { .. } | Command::Unlink { .. } | Command::Doctor { .. }
        )
    {
        report_client_failure(&client, operation, DiagnosticError::InvalidInput).await;
        eprintln!("--anonymous is for API reads and cannot be combined with account commands");
        std::process::exit(2);
    }
    match local::execute(&client, &store, &cli.command).await {
        Ok(Some((output, code))) => {
            println!("{output}");
            std::process::exit(code);
        }
        Ok(None) => {}
        Err(error) => {
            if client.last_trace_id().is_none() {
                let diagnostic_error = match error {
                    AccountError::Api(wam_client::ApiError::InvalidResponse) => {
                        DiagnosticError::InvalidResponse
                    }
                    AccountError::Api(wam_client::ApiError::InvalidInput)
                    | AccountError::Api(wam_client::ApiError::InvalidEndpoint)
                    | AccountError::Denied
                    | AccountError::Expired
                    | AccountError::AlreadyLinked => DiagnosticError::InvalidInput,
                    _ => DiagnosticError::UnhandledError,
                };
                let _ = client
                    .report_diagnostic_event(operation, diagnostic_error)
                    .await;
            }
            print_trace(client.last_trace_id());
            eprintln!("{error}");
            std::process::exit(match error {
                AccountError::Api(error) => local::exit_code(error),
                AccountError::Denied | AccountError::Expired => 3,
                AccountError::AlreadyLinked => 4,
                _ => 1,
            });
        }
    }
    let token = if cli.anonymous || matches!(cli.command, Command::Cache { .. }) {
        None
    } else {
        match token_for_endpoint(&cli.endpoint) {
            Ok(token) => token,
            Err(error) => {
                report_client_failure(&client, operation, DiagnosticError::InvalidInput).await;
                eprintln!("{error}");
                if local::requires_auth(&cli.command) {
                    std::process::exit(1);
                }
                None
            }
        }
    };
    if local::requires_auth(&cli.command) && token.is_none() {
        report_client_failure(&client, operation, DiagnosticError::InvalidInput).await;
        eprintln!("authentication required; run wam link");
        std::process::exit(3);
    }
    client.set_access_token(token);
    if let Command::BrowserMcp {
        browser_mcp,
        browser_args,
    } = &cli.command
    {
        client.set_diagnostic_source(wam_client::DiagnosticSource::Mcp);
        let previous_trace = client.last_trace_id();
        if let Err(error) = wam_cli::browser::serve(&client, browser_mcp, browser_args).await {
            let current_trace = client.last_trace_id();
            if current_trace.is_some() && current_trace != previous_trace {
                print_trace(current_trace);
            } else {
                report_client_failure(
                    &client,
                    DiagnosticOperation::Unknown,
                    DiagnosticError::UnhandledError,
                )
                .await;
            }
            eprintln!("browser MCP bridge: {error}");
            std::process::exit(local::exit_code(error));
        }
        return;
    }
    match execute(&client, cli.command).await {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            let trace = match client.last_trace_id() {
                Some(trace) => Some(trace),
                None => {
                    client
                        .report_diagnostic_event(
                            operation,
                            match error {
                                wam_client::ApiError::InvalidInput
                                | wam_client::ApiError::InvalidEndpoint => {
                                    DiagnosticError::InvalidInput
                                }
                                wam_client::ApiError::InvalidResponse => {
                                    DiagnosticError::InvalidResponse
                                }
                                _ => DiagnosticError::InternalError,
                            },
                        )
                        .await
                }
            };
            print_trace(trace);
            std::process::exit(local::exit_code(error));
        }
    }
}

async fn report_cli_failure(
    endpoint: &str,
    operation: DiagnosticOperation,
    error: DiagnosticError,
) {
    if let Ok(client) = HttpClient::new(endpoint, None) {
        report_client_failure(&client, operation, error).await;
    }
}

async fn report_client_failure(
    client: &HttpClient,
    operation: DiagnosticOperation,
    error: DiagnosticError,
) {
    print_trace(client.report_diagnostic_event(operation, error).await);
}

fn print_trace(trace: Option<String>) {
    if let Some(trace) = trace {
        eprintln!("trace: {trace}");
    }
}
