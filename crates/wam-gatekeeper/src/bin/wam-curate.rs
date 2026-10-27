//! Dev runner: run curation scenarios against a chosen model provider.
//!
//!   wam-curate --provider fake                      deterministic, no model
//!   wam-curate --provider claude-cli --scenario better
//!   wam-curate --provider codex-cli --scenario all -v
//!
//! The provider and model also come from WAM_LLM_PROVIDER and WAM_LLM_MODEL, so switching the model for a run is
//! an environment change. claude-cli and codex-cli use the locally installed CLI and its login (local development);
//! the OpenAI-compatible API provider (DeepSeek, Meta Muse) is attached last behind the same switch.

use std::process::ExitCode;
use wam_gatekeeper::provider::{LlmProvider, build_provider};
use wam_gatekeeper::scenario::{self, Scenario, run_scenario};

fn usage() -> ExitCode {
    eprintln!(
        "usage: wam-curate [--provider fake|claude-cli|codex-cli|openai-compat] [--model MODEL] \
         [--scenario NAME|all] [--list] [-v]\nscenarios: {}",
        scenario::BUILTIN
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    );
    ExitCode::from(2)
}

fn clip(text: &str, max: usize) -> String {
    let flat = text.replace('\n', " ");
    if flat.chars().count() > max {
        format!("{}...", flat.chars().take(max).collect::<String>())
    } else {
        flat
    }
}

fn main() -> ExitCode {
    let mut provider = std::env::var("WAM_LLM_PROVIDER").unwrap_or_else(|_| "fake".into());
    let mut model = std::env::var("WAM_LLM_MODEL").ok();
    let mut selected = "all".to_owned();
    let mut verbose = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--provider" => provider = args.next().unwrap_or_default(),
            "--model" => model = args.next(),
            "--scenario" => selected = args.next().unwrap_or_default(),
            "-v" | "--verbose" => verbose = true,
            "--list" => {
                for scenario in scenario::all() {
                    println!("{:<10} {}", scenario.name, scenario.description);
                }
                return ExitCode::SUCCESS;
            }
            _ => return usage(),
        }
    }
    let scenarios: Vec<Scenario> = if selected == "all" {
        scenario::all()
    } else {
        match scenario::builtin(&selected) {
            Some(one) => vec![one],
            None => return usage(),
        }
    };
    let mut real: Option<Box<dyn LlmProvider>> = if provider == "fake" {
        None
    } else {
        match build_provider(&provider, model.as_deref()) {
            Ok(built) => Some(built),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    };

    let mut failures = 0;
    let mut total_cost = 0.0;
    println!(
        "{:<10} {:<22} {:<20} {:<28} {:>5}  result",
        "scenario", "provider", "decision", "expected", "turns"
    );
    for scenario in &scenarios {
        let mut fake = scenario.script();
        let model_ref: &mut dyn LlmProvider = match real.as_mut() {
            Some(real) => real.as_mut(),
            None => &mut fake,
        };
        let run = match run_scenario(scenario, model_ref) {
            Ok(run) => run,
            Err(error) => {
                eprintln!("{}: {error}", scenario.name);
                failures += 1;
                continue;
            }
        };
        let decision = run
            .report
            .decision
            .as_ref()
            .map_or("-", |d| d.kind.as_str());
        let expected = if scenario.expect.decisions.is_empty() {
            format!("not {}", scenario.expect.forbidden.join("/"))
        } else {
            scenario.expect.decisions.join("/")
        };
        let verdict = match &run.verdict {
            Ok(()) => "PASS".to_owned(),
            Err(why) => {
                failures += 1;
                format!("FAIL ({why})")
            }
        };
        println!(
            "{:<10} {:<22} {:<20} {:<28} {:>5}  {}",
            scenario.name,
            run.report.audit.provider,
            decision,
            clip(&expected, 27),
            run.report.turns.len(),
            verdict
        );
        for turn in &run.report.turns {
            if let Some(cost) = turn.meta["cost_usd"].as_f64() {
                total_cost += cost;
            }
            if verbose {
                println!("    -> {} {}", turn.tool, clip(&turn.args.to_string(), 160));
                println!("    <- {}", clip(&turn.result.to_string(), 160));
            }
        }
        if let Some(why) = match &run.report.outcome {
            wam_gatekeeper::curator::Outcome::Decided => None,
            other => Some(format!("{other:?}")),
        } {
            println!("    outcome: {why}");
        }
    }
    if total_cost > 0.0 {
        println!("model cost reported by the provider: ${total_cost:.4}");
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
