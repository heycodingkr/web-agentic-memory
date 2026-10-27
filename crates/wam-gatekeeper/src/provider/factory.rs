use super::{CliKind, CliProvider, LlmProvider, OpenAiCompatProvider, ProviderError};
use std::time::Duration;

/// Provider selection by configuration: `--provider` / `WAM_LLM_PROVIDER` and `--model` / `WAM_LLM_MODEL`.
///
/// `claude-cli` and `codex-cli` are for local development. `openai-compat` (DeepSeek, Meta Muse) is the
/// deployment provider: it reads `WAM_LLM_BASE_URL` and `MODEL_API_KEY`.
pub fn build_provider(
    name: &str,
    model: Option<&str>,
) -> Result<Box<dyn LlmProvider>, ProviderError> {
    let timeout = Duration::from_secs(180);
    match name {
        "claude-cli" => Ok(Box::new(CliProvider::new(
            CliKind::Claude,
            model.map(str::to_owned),
            timeout,
        )?)),
        "codex-cli" => Ok(Box::new(CliProvider::new(
            CliKind::Codex,
            model.map(str::to_owned),
            timeout,
        )?)),
        "openai-compat" | "deepseek" | "muse" => OpenAiCompatProvider::from_env(name, model)
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>),
        "fake" => Err(ProviderError::NotBuilt(
            "the fake provider needs a script; use a scenario's script".into(),
        )),
        other => Err(ProviderError::NotBuilt(format!(
            "unknown provider {other:?}; expected claude-cli, codex-cli, or openai-compat"
        ))),
    }
}
