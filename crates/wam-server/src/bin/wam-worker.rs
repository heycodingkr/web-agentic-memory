//! Process the application's durable curation queue and retention work.
use std::{path::PathBuf, time::Duration};
use wam_gatekeeper::provider::build_provider;
use wam_server::{
    ApplicationRepository,
    durable::{CommandReplayVerifier, ModelBudgetPolicy, WorkResult},
};

const USAGE: &str = "usage: wam-worker [--once] [--purge-only] [--budget-status] [--resume-budget]";

/// One line for the operator when a provider refusal or an outage has closed the model budget gate.
fn announce_closed_budget(
    repository: &ApplicationRepository,
    policy: &ModelBudgetPolicy,
    now: u64,
) {
    if let Some(notice) = repository
        .model_budget_status(policy, now)
        .ok()
        .and_then(|status| status.closed_notice())
    {
        eprintln!("{notice}");
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut once = false;
    let mut purge_only = false;
    let mut budget_status = false;
    let mut resume_budget = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--once" => once = true,
            "--purge-only" => {
                purge_only = true;
                once = true;
            }
            "--budget-status" => budget_status = true,
            "--resume-budget" => resume_budget = true,
            _ => return Err(USAGE.into()),
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
    let policy = ModelBudgetPolicy::from_env().map_err(|_| {
        "the model budget settings are invalid: WAM_LLM_DAILY_BUDGET_USD must be a positive dollar \
         amount, WAM_LLM_QUOTA_PAUSE_SECS, WAM_LLM_OUTAGE_PAUSE_SECS and \
         WAM_LLM_REJECTED_PAUSE_SECS 30..=86400 with their *_MAX_SECS at least that, \
         WAM_LLM_OUTAGE_FREE_WAITS 0..=1000"
    })?;
    if budget_status || resume_budget {
        if resume_budget {
            let released = repository.resume_model_budget(clock())?;
            println!("model budget reopened; {released} parked job(s) released");
        }
        if budget_status {
            println!(
                "{}",
                serde_json::to_string(&repository.model_budget_status(&policy, clock())?)?
            );
        }
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
    if policy.has_daily_cap() && !provider.reports_cost() {
        return Err(
            "WAM_LLM_DAILY_BUDGET_USD needs a provider that reports what it costs: set \
             WAM_LLM_PRICE_INPUT_PER_M, WAM_LLM_PRICE_CACHED_INPUT_PER_M and WAM_LLM_PRICE_OUTPUT_PER_M"
                .into(),
        );
    }
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
        match repository.process_next_budgeted(provider.as_mut(), &replay, &policy, clock()) {
            Ok(WorkResult::Idle) if once => return Ok(()),
            Ok(result) => {
                if result != WorkResult::Idle {
                    println!("curation result: {result:?}");
                }
                if result == WorkResult::Retry {
                    announce_closed_budget(&repository, &policy, clock());
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
