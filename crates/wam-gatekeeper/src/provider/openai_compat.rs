use super::{
    LlmProvider, ModelTurn, ProviderError, TurnRequest, extract_turn, looks_like_quota_error,
    protocol_text, render_transcript,
};
use reqwest::blocking::Client;
use reqwest::header::RETRY_AFTER;
use serde_json::{Value, json};
use std::io::Read;
use std::time::{Duration, SystemTime};
use url::{Host, Url};

const MAX_HTTP_RESPONSE_BYTES: usize = 1024 * 1024;

/// OpenAI Responses API-compatible HTTP provider. It uses the same JSON turn protocol as the local CLI adapters,
/// so the model never receives native browser, shell, network, or database tools.
pub struct OpenAiCompatProvider {
    name: String,
    model: String,
    api_key: String,
    endpoint: Url,
    client: Client,
}

impl OpenAiCompatProvider {
    pub fn new(
        name: &str,
        base_url: &str,
        api_key: &str,
        model: &str,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let endpoint = responses_endpoint(base_url)?;
        if api_key.is_empty() || api_key.bytes().any(|byte| !(0x21..=0x7e).contains(&byte)) {
            return Err(ProviderError::Configuration(
                "MODEL_API_KEY must be non-empty printable ASCII without whitespace".into(),
            ));
        }
        if model.trim().is_empty() {
            return Err(ProviderError::Configuration(
                "WAM_LLM_MODEL is required".into(),
            ));
        }
        let client = Client::builder()
            .timeout(timeout)
            .user_agent(concat!("wam-gatekeeper/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ProviderError::Configuration("could not initialize HTTP client".into()))?;
        Ok(Self {
            name: format!("{name}:{model}"),
            model: model.to_owned(),
            api_key: api_key.to_owned(),
            endpoint,
            client,
        })
    }

    pub(super) fn from_env(name: &str, model: Option<&str>) -> Result<Self, ProviderError> {
        let base_url = std::env::var("WAM_LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.meta.ai/v1".to_owned());
        let api_key = std::env::var("MODEL_API_KEY")
            .map_err(|_| ProviderError::Configuration("MODEL_API_KEY is required".into()))?;
        let model = model
            .map(str::to_owned)
            .or_else(|| std::env::var("WAM_LLM_MODEL").ok())
            .ok_or_else(|| ProviderError::Configuration("WAM_LLM_MODEL is required".into()))?;
        Self::new(name, &base_url, &api_key, &model, Duration::from_secs(180))
    }
}

impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        let instructions = format!(
            "{}\n\n---\n{}",
            request.system,
            protocol_text(request.tools)
        );
        let body = json!({
            "model": self.model,
            "instructions": instructions,
            "input": render_transcript(request.transcript),
            "max_output_tokens": 2048,
            "store": false
        });
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|_| ProviderError::Failed("HTTP model provider is unreachable".into()))?;
        let status = response.status();
        let retry_after_secs = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let mut bytes = Vec::new();
        response
            .take((MAX_HTTP_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ProviderError::Failed("could not read model provider response".into()))?;
        if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
            return Err(ProviderError::Failed(
                "model provider response exceeded the size limit".into(),
            ));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ProviderError::Failed("model provider returned invalid JSON".into()))?;
        if !status.is_success() {
            let code = response["error"]["code"].as_str().unwrap_or_default();
            let message = response["error"]["message"].as_str().unwrap_or_default();
            if status == reqwest::StatusCode::PAYMENT_REQUIRED
                || code.eq_ignore_ascii_case("insufficient_quota")
                || looks_like_quota_error(code)
                || looks_like_quota_error(message)
            {
                return Err(ProviderError::QuotaExhausted {
                    why: format!("HTTP {} quota exhausted", status.as_u16()),
                    retry_after_secs,
                });
            }
            return Err(ProviderError::Failed(format!(
                "HTTP {} from model provider",
                status.as_u16()
            )));
        }
        let text = response_output_text(&response).ok_or_else(|| ProviderError::Malformed {
            raw: String::new(),
            why: "Responses API reply did not contain output text".into(),
        })?;
        let (tool, args) = extract_turn(&text)?;
        Ok(ModelTurn {
            tool,
            args,
            raw: text,
            meta: json!({"usage": response.get("usage"), "model": response.get("model")}),
        })
    }
}

fn responses_endpoint(base_url: &str) -> Result<Url, ProviderError> {
    let mut url = Url::parse(base_url)
        .map_err(|_| ProviderError::Configuration("WAM_LLM_BASE_URL must be a valid URL".into()))?;
    let local_http = match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    if !(url.scheme() == "https" || (url.scheme() == "http" && local_http))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ProviderError::Configuration(
            "WAM_LLM_BASE_URL must use HTTPS (HTTP is allowed for loopback fixtures) and contain no credentials, query, or fragment".into(),
        ));
    }
    url.path_segments_mut()
        .map_err(|_| ProviderError::Configuration("WAM_LLM_BASE_URL cannot be a base URL".into()))?
        .pop_if_empty()
        .push("responses");
    Ok(url)
}

fn response_output_text(response: &Value) -> Option<String> {
    let mut text = String::new();
    for item in response.get("output")?.as_array()? {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        for block in item.get("content")?.as_array()? {
            if block.get("type").and_then(Value::as_str) == Some("output_text") {
                if let Some(value) = block.get("text").and_then(Value::as_str) {
                    text.push_str(value);
                }
            }
        }
    }
    (!text.is_empty()).then_some(text)
}

fn parse_retry_after(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds);
    }
    httpdate::parse_http_date(value)
        .ok()?
        .duration_since(SystemTime::now())
        .ok()
        .map(|duration| duration.as_secs())
}
