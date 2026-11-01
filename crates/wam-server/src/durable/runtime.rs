//! Optional in-process worker for a local file store, or for small deployments.
//! PostgreSQL installations can instead run the separate wam-worker process.
use super::{ApplicationRepository, CommandReplayVerifier, ModelBudgetPolicy, WorkResult};
use crate::application::ServiceError;
use std::{sync::mpsc, thread::JoinHandle, time::Duration};

pub fn start_background(repository: ApplicationRepository) -> Result<JoinHandle<()>, ServiceError> {
    let program = std::env::var("WAM_REPLAY_PROGRAM")
        .map_err(|_| ServiceError::InvalidInput("replay_program_required"))?;
    let args = match std::env::var("WAM_REPLAY_ARGS_JSON") {
        Ok(value) => serde_json::from_str::<Vec<String>>(&value)
            .map_err(|_| ServiceError::InvalidInput("invalid_replay_arguments"))?,
        Err(_) => Vec::new(),
    };
    let provider_name =
        std::env::var("WAM_LLM_PROVIDER").unwrap_or_else(|_| "openai-compat".into());
    if provider_name == "fake" {
        return Err(ServiceError::InvalidInput("real_provider_required"));
    }
    let model = std::env::var("WAM_LLM_MODEL").ok();
    let policy = ModelBudgetPolicy::from_env()?;
    let (ready, response) = mpsc::sync_channel(1);
    let handle = std::thread::Builder::new()
        .name("wam-curation".into())
        .spawn(move || {
            let setup = (|| {
                let provider =
                    wam_gatekeeper::provider::build_provider(&provider_name, model.as_deref())
                        .map_err(|_| {
                            ServiceError::InvalidInput("invalid_provider_configuration")
                        })?;
                // A daily ceiling counts what the provider reports it cost; without that it would never trip.
                if policy.has_daily_cap() && !provider.reports_cost() {
                    return Err(ServiceError::InvalidInput(
                        "daily_budget_requires_cost_reporting",
                    ));
                }
                let replay = CommandReplayVerifier::new(program, args, Duration::from_secs(120))?;
                Ok((provider, replay))
            })();
            let (mut provider, replay) = match setup {
                Ok(setup) => {
                    let _ = ready.send(Ok(()));
                    setup
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                }
            };
            loop {
                let now = crate::application::now_unix();
                match repository.process_next_budgeted(provider.as_mut(), &replay, &policy, now) {
                    Ok(WorkResult::Idle) => {}
                    Ok(WorkResult::Retry) => match repository.model_budget_status(&policy, now) {
                        Ok(status) if !status.open => eprintln!(
                            "curation paused: {}",
                            status.closed_notice().unwrap_or_else(|| format!(
                                "model budget closed ({}); {} job(s) wait",
                                status.blocked_by.unwrap_or("unknown"),
                                status.waiting_jobs
                            ))
                        ),
                        _ => eprintln!("curation job will be retried"),
                    },
                    Ok(_) => eprintln!("curation job processed"),
                    Err(_) => eprintln!("curation commit deferred"),
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .map_err(|_| ServiceError::Unavailable)?;
    response.recv().map_err(|_| ServiceError::Unavailable)??;
    Ok(handle)
}
