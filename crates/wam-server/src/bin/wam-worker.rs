//! Process the application's durable curation queue and retention work.
use std::{path::PathBuf, time::Duration};
use wam_gatekeeper::provider::build_provider;
use wam_server::{
    ApplicationRepository,
    durable::{CommandReplayVerifier, WorkResult},
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut once = false;
    let mut purge_only = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--once" => once = true,
            "--purge-only" => {
                purge_only = true;
                once = true;
            }
            _ => return Err("usage: wam-worker [--once] [--purge-only]".into()),
        }
    }
    let repository = if let Ok(url) = std::env::var("WAM_DATABASE_URL") {
        ApplicationRepository::connect(&url)?
    } else {
        if std::env::var("WAM_ENV").is_ok_and(|value| value == "production") {
            return Err("WAM_DATABASE_URL is required in production".into());
        }
        let directory =
            PathBuf::from(std::env::var("WAM_DATA_DIR").unwrap_or_else(|_| ".wam-data".into()));
        ApplicationRepository::open_directory(&directory)?
    };
    let clock = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    };
    if purge_only {
        let report = repository.purge(clock())?;
        println!(
            "purged contributions={}, receipts={}",
            report.contributions, report.receipts
        );
        return Ok(());
    }
    let name = std::env::var("WAM_LLM_PROVIDER").unwrap_or_else(|_| "openai-compat".into());
    if name == "fake" {
        return Err(
            "the live worker requires a real provider; scripted providers belong in tests".into(),
        );
    }
    let model = std::env::var("WAM_LLM_MODEL").ok();
    let mut provider = build_provider(&name, model.as_deref())
        .map_err(|_| "model provider configuration is invalid")?;
    let replay_program = std::env::var("WAM_REPLAY_PROGRAM")
        .map_err(|_| "WAM_REPLAY_PROGRAM must point to a trusted browser replay executable")?;
    let replay_args = match std::env::var("WAM_REPLAY_ARGS_JSON") {
        Ok(value) => serde_json::from_str::<Vec<String>>(&value)
            .map_err(|_| "WAM_REPLAY_ARGS_JSON must be a string array")?,
        Err(_) => Vec::new(),
    };
    let replay = CommandReplayVerifier::new(replay_program, replay_args, Duration::from_secs(120))?;
    loop {
        repository.purge(clock())?;
        match repository.process_next(provider.as_mut(), &replay, clock()) {
            Ok(WorkResult::Idle) if once => return Ok(()),
            Ok(result) => {
                if result != WorkResult::Idle {
                    println!("curation result: {result:?}");
                }
                if once {
                    return Ok(());
                }
            }
            Err(_) if once => {
                return Err("curation could not commit; the durable job can be retried".into());
            }
            Err(_) => eprintln!("curation deferred after a storage or lease conflict"),
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
