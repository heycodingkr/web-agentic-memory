use clap::Parser;
use wam_cli::{Cli, Command, account::AccountError, execute, local, output};
use wam_client::{
    ApiError, DiagnosticError, DiagnosticOperation, HttpClient,
    credentials::{OsCredentialStore, token_for_endpoint},
};

#[tokio::main]
async fn main() {
    let json_output = std::env::args_os().any(|argument| argument == "--json");
    let mut cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                error.exit();
            }
            let endpoint = std::env::var("WAM_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned());
            let trace = report_cli_failure(
                &endpoint,
                DiagnosticOperation::Unknown,
                DiagnosticError::InvalidInput,
            )
            .await;
            if json_output {
                exit_api(ApiError::InvalidInput, true, trace.as_deref());
            }
            if let Some(trace) = trace {
                eprintln!("trace: {trace}");
            }
            error.exit();
        }
    };
    if let Err(error) = cli.command.read_input(std::io::stdin().lock()) {
        let trace = report_cli_failure(
            &cli.endpoint,
            cli.command.diagnostic_operation(),
            DiagnosticError::InvalidInput,
        )
        .await;
        exit_api(error, json_output, trace.as_deref());
    }
    if let Command::Guide { action } = &cli.command {
        match wam_cli::guide::execute(action) {
            Ok(output) => print!("{output}{}", if output.ends_with('\n') { "" } else { "\n" }),
            Err(error) => {
                let trace = report_cli_failure(
                    &cli.endpoint,
                    DiagnosticOperation::Unknown,
                    DiagnosticError::UnhandledError,
                )
                .await;
                exit_failure(
                    "instruction_update_failed",
                    &error.to_string(),
                    2,
                    json_output,
                    trace.as_deref(),
                );
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
                let trace = report_cli_failure(
                    &cli.endpoint,
                    cli.command.diagnostic_operation(),
                    DiagnosticError::InvalidInput,
                )
                .await;
                exit_api(error, json_output, trace.as_deref());
            }
        }
    }
    let operation = cli.command.diagnostic_operation();
    let mut client = match HttpClient::new(&cli.endpoint, None) {
        Ok(client) => client,
        Err(error) => exit_api(error, json_output, None),
    };
    let store = match OsCredentialStore::for_endpoint(&cli.endpoint) {
        Ok(store) => store,
        Err(error) => {
            let trace = client
                .report_diagnostic_event(operation, DiagnosticError::UnhandledError)
                .await;
            exit_api(error, json_output, trace.as_deref());
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
        let trace = client
            .report_diagnostic_event(operation, DiagnosticError::InvalidInput)
            .await;
        eprintln!("--anonymous cannot be combined with account commands");
        exit_api(ApiError::InvalidInput, json_output, trace.as_deref());
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
                    AccountError::Api(ApiError::InvalidResponse) => {
                        DiagnosticError::InvalidResponse
                    }
                    AccountError::Api(ApiError::InvalidInput | ApiError::InvalidEndpoint)
                    | AccountError::Denied
                    | AccountError::Expired
                    | AccountError::AlreadyLinked => DiagnosticError::InvalidInput,
                    _ => DiagnosticError::UnhandledError,
                };
                let _ = client
                    .report_diagnostic_event(operation, diagnostic_error)
                    .await;
            }
            let trace = client.last_trace_id();
            if let AccountError::Api(error) = error {
                exit_api(error, json_output, trace.as_deref());
            }
            let (code, exit) = match error {
                AccountError::Denied => ("authorization_denied", 3),
                AccountError::Expired => ("authorization_expired", 3),
                AccountError::AlreadyLinked => ("already_linked", 4),
                AccountError::Store(_) => ("credential_storage_unavailable", 1),
                AccountError::StorageRollbackFailed => ("credential_cleanup_required", 1),
                AccountError::Api(_) => unreachable!(),
            };
            exit_failure(
                code,
                &error.to_string(),
                exit,
                json_output,
                trace.as_deref(),
            );
        }
    }
    let token = if cli.anonymous || matches!(cli.command, Command::Cache { .. }) {
        None
    } else {
        match token_for_endpoint(&cli.endpoint) {
            Ok(token) => token,
            Err(error) => {
                let trace = client
                    .report_diagnostic_event(operation, DiagnosticError::InvalidInput)
                    .await;
                if local::requires_auth(&cli.command) {
                    exit_failure(
                        "credential_storage_unavailable",
                        &error.to_string(),
                        1,
                        json_output,
                        trace.as_deref(),
                    );
                }
                eprintln!("{error}");
                if let Some(trace) = trace {
                    eprintln!("trace: {trace}");
                }
                None
            }
        }
    };
    if local::requires_auth(&cli.command) && token.is_none() {
        let trace = client
            .report_diagnostic_event(operation, DiagnosticError::InvalidInput)
            .await;
        eprintln!("run wam link to authenticate");
        exit_api(ApiError::Unauthorized, json_output, trace.as_deref());
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
            if current_trace.is_none() || current_trace == previous_trace {
                let _ = client
                    .report_diagnostic_event(
                        DiagnosticOperation::Unknown,
                        DiagnosticError::UnhandledError,
                    )
                    .await;
            }
            exit_api(error, json_output, client.last_trace_id().as_deref());
        }
        return;
    }
    match execute(&client, cli.command).await {
        Ok(output) => println!("{output}"),
        Err(error) => {
            let trace = match client.last_trace_id() {
                Some(trace) => Some(trace),
                None => {
                    client
                        .report_diagnostic_event(
                            operation,
                            match error {
                                ApiError::InvalidInput | ApiError::InvalidEndpoint => {
                                    DiagnosticError::InvalidInput
                                }
                                ApiError::InvalidResponse => DiagnosticError::InvalidResponse,
                                _ => DiagnosticError::InternalError,
                            },
                        )
                        .await
                }
            };
            exit_api(error, json_output, trace.as_deref());
        }
    }
}

fn exit_api(error: ApiError, json: bool, trace: Option<&str>) -> ! {
    if json {
        println!("{}", output::api_failure(error, trace));
    }
    eprintln!("{error}");
    if let Some(trace) = trace {
        eprintln!("trace: {trace}");
    }
    std::process::exit(local::exit_code(error))
}
fn exit_failure(code: &str, message: &str, exit: i32, json: bool, trace: Option<&str>) -> ! {
    if json {
        println!("{}", output::failure(code, message, trace));
    }
    eprintln!("{message}");
    if let Some(trace) = trace {
        eprintln!("trace: {trace}");
    }
    std::process::exit(exit)
}
async fn report_cli_failure(
    endpoint: &str,
    operation: DiagnosticOperation,
    error: DiagnosticError,
) -> Option<String> {
    match HttpClient::new(endpoint, None) {
        Ok(client) => client.report_diagnostic_event(operation, error).await,
        Err(_) => None,
    }
}
