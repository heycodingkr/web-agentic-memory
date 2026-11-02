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

/// Values the Meta Model API accepts for `reasoning.effort` on Muse Spark (it rejects `none`). A reasoning model
/// spends the request's output budget on hidden reasoning before it answers, so a lower effort leaves room for the
/// reply and costs less.
const REASONING_EFFORTS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];

const PRICE_VARIABLES: [&str; 3] = [
    "WAM_LLM_PRICE_INPUT_PER_M",
    "WAM_LLM_PRICE_CACHED_INPUT_PER_M",
    "WAM_LLM_PRICE_OUTPUT_PER_M",
];

/// What the provider charges, in US dollars per million tokens (the Meta console lists Muse Spark 1.3 at 1.25 for
/// input, 0.15 for cached input and 4.25 for output). Tokens the model spends on hidden reasoning are billed as
/// output. With a price list the provider reports what every reply cost (`meta["cost_usd"]`), which is what a
/// worker's spending ceiling counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pricing {
    pub input: f64,
    pub cached_input: f64,
    pub output: f64,
}

impl Pricing {
    pub fn new(input: f64, cached_input: f64, output: f64) -> Result<Self, ProviderError> {
        if [input, cached_input, output]
            .iter()
            .any(|price| !price.is_finite() || !(0.0..=1000.0).contains(price))
        {
            return Err(ProviderError::Configuration(
                "model prices must be between 0 and 1000 US dollars per million tokens".into(),
            ));
        }
        Ok(Self {
            input,
            cached_input,
            output,
        })
    }

    /// What one reply cost in US dollars, or `None` when its `usage` carries no usable token counts.
    pub fn cost_usd(&self, usage: &Value) -> Option<f64> {
        let input = usage.get("input_tokens")?.as_u64()?;
        let output = usage.get("output_tokens")?.as_u64()?;
        let cached = usage["input_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0)
            .min(input);
        let dollars = (input - cached) as f64 * self.input
            + cached as f64 * self.cached_input
            + output as f64 * self.output;
        Some(dollars / 1_000_000.0)
    }

    /// All three `WAM_LLM_PRICE_*` variables, or none of them (no cost is reported then). A partial or unreadable
    /// list is refused rather than guessed, and the error never repeats a value.
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, ProviderError> {
        let values: Vec<Option<String>> = PRICE_VARIABLES
            .iter()
            .map(|name| get(name).filter(|value| !value.trim().is_empty()))
            .collect();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let mut prices = [0.0; 3];
        for (slot, value) in prices.iter_mut().zip(&values) {
            let Some(value) = value else {
                return Err(ProviderError::Configuration(format!(
                    "set all of {} or none of them",
                    PRICE_VARIABLES.join(", ")
                )));
            };
            *slot = value.trim().parse::<f64>().map_err(|_| {
                ProviderError::Configuration(
                    "model prices must be numbers (US dollars per million tokens)".into(),
                )
            })?;
        }
        Self::new(prices[0], prices[1], prices[2]).map(Some)
    }
}

/// OpenAI Responses API-compatible HTTP provider. It uses the same JSON turn protocol as the local CLI adapters,
/// so the model never receives native browser, shell, network, or database tools.
pub struct OpenAiCompatProvider {
    name: String,
    model: String,
    api_key: String,
    endpoint: Url,
    client: Client,
    reasoning_effort: Option<&'static str>,
    pricing: Option<Pricing>,
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
            reasoning_effort: None,
            pricing: None,
        })
    }

    /// Report what every reply costs (`meta["cost_usd"]`) from its token counts and this price list.
    pub fn with_pricing(mut self, pricing: Option<Pricing>) -> Self {
        self.pricing = pricing;
        self
    }

    /// Send `reasoning.effort` with every request. `None` sends nothing and keeps the provider's default. An unknown
    /// value is refused here, without being echoed, instead of failing every request later with an HTTP 400.
    pub fn with_reasoning_effort(mut self, effort: Option<&str>) -> Result<Self, ProviderError> {
        self.reasoning_effort = effort
            .map(|value| {
                REASONING_EFFORTS
                    .iter()
                    .copied()
                    .find(|known| known.eq_ignore_ascii_case(value.trim()))
                    .ok_or_else(|| {
                        ProviderError::Configuration(format!(
                            "WAM_LLM_REASONING_EFFORT must be one of {}",
                            REASONING_EFFORTS.join(", ")
                        ))
                    })
            })
            .transpose()?;
        Ok(self)
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
        let effort = std::env::var("WAM_LLM_REASONING_EFFORT")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let pricing = Pricing::from_lookup(|name| std::env::var(name).ok())?;
        Ok(
            Self::new(name, &base_url, &api_key, &model, Duration::from_secs(180))?
                .with_reasoning_effort(effort.as_deref())?
                .with_pricing(pricing),
        )
    }
}

impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn reports_cost(&self) -> bool {
        self.pricing.is_some()
    }

    fn next_turn(&mut self, request: &TurnRequest<'_>) -> Result<ModelTurn, ProviderError> {
        let instructions = format!(
            "{}\n\n---\n{}",
            request.system,
            protocol_text(request.tools)
        );
        let mut body = json!({
            "model": self.model,
            "instructions": instructions,
            "input": render_transcript(request.transcript),
            "max_output_tokens": 2048,
            "store": false
        });
        if let Some(effort) = self.reasoning_effort {
            body["reasoning"] = json!({"effort": effort});
        }
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| send_failure(&error))?;
        let status = response.status();
        let retry_after_secs = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let mut bytes = Vec::new();
        let read = response
            .take((MAX_HTTP_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut bytes);
        if !status.is_success() {
            // The status says what happened. The body only helps to tell a spent balance from other refusals, so a
            // body that is not JSON (a gateway's HTML page), too large or cut off does not change the answer.
            let body = match read {
                Ok(_) if bytes.len() <= MAX_HTTP_RESPONSE_BYTES => {
                    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            return Err(failure_for_status(status, &body, retry_after_secs));
        }
        read.map_err(|error| read_failure(&error))?;
        if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
            return Err(ProviderError::Failed(
                "model provider response exceeded the size limit".into(),
            ));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ProviderError::Failed("model provider returned invalid JSON".into()))?;
        let text = response_output_text(&response).ok_or_else(|| ProviderError::Malformed {
            raw: String::new(),
            why: missing_output_reason(&response),
        })?;
        let (tool, args) = extract_turn(&text)?;
        let cost_usd = self.pricing.and_then(|pricing| {
            response
                .get("usage")
                .and_then(|usage| pricing.cost_usd(usage))
        });
        Ok(ModelTurn {
            tool,
            args,
            raw: text,
            meta: json!({
                "usage": response.get("usage"),
                "model": response.get("model"),
                "reasoning_effort": self.reasoning_effort,
                "cost_usd": cost_usd
            }),
        })
    }
}

/// Why no reply came at all. A request that cannot connect, times out or is cut off says nothing about this job:
/// the provider (or the network to it) is down for now. The message is fixed text, never what the error carried.
fn send_failure(error: &reqwest::Error) -> ProviderError {
    if error.is_builder() || error.is_redirect() {
        return ProviderError::Failed("HTTP model provider request could not be completed".into());
    }
    let why = if error.is_timeout() {
        "HTTP model provider timed out"
    } else {
        "HTTP model provider is unreachable"
    };
    ProviderError::Unavailable {
        why: why.into(),
        retry_after_secs: None,
    }
}

/// The reply began and then stopped (a dropped connection or the request timeout while the body was coming).
fn read_failure(error: &std::io::Error) -> ProviderError {
    let why = if error.kind() == std::io::ErrorKind::TimedOut {
        "HTTP model provider timed out"
    } else {
        "HTTP model provider response could not be read"
    };
    ProviderError::Unavailable {
        why: why.into(),
        retry_after_secs: None,
    }
}

/// A reply with an error status, told apart by what waiting can do about it (docs/spec/provider-outage.md):
/// spent credit or quota (the signal wins over the status code, so a 429 that says the balance is gone is still a
/// refusal), a temporary outage (429, 408 and every 5xx: waiting helps and the job is not at fault), a rejection of the
/// key or the model setting (401, 403, 404: the job is not at fault either, but only a person fixing the setting
/// helps), or an ordinary failure (the request is wrong and the job may be the cause). The status decides, never the
/// body. Messages are fixed text: nothing the provider sent is repeated.
fn failure_for_status(
    status: reqwest::StatusCode,
    body: &Value,
    retry_after_secs: Option<u64>,
) -> ProviderError {
    let code = body["error"]["code"].as_str().unwrap_or_default();
    let message = body["error"]["message"].as_str().unwrap_or_default();
    if status == reqwest::StatusCode::PAYMENT_REQUIRED
        || code.eq_ignore_ascii_case("insufficient_quota")
        || looks_like_quota_error(code)
        || looks_like_quota_error(message)
    {
        return ProviderError::QuotaExhausted {
            why: format!("HTTP {} quota exhausted", status.as_u16()),
            retry_after_secs,
        };
    }
    let why = format!("HTTP {} from model provider", status.as_u16());
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
    {
        ProviderError::Unavailable {
            why,
            retry_after_secs,
        }
    } else if matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::NOT_FOUND
    ) {
        ProviderError::Rejected { why }
    } else {
        ProviderError::Failed(why)
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

/// Why a reply carried no output text. A reasoning model that spends all of `max_output_tokens` before it answers
/// returns `status: "incomplete"` with only a `reasoning` item; say so instead of reporting a generic bad reply.
/// The provider's own reason text is never echoed: only known reasons are named.
fn missing_output_reason(response: &Value) -> String {
    if response.get("status").and_then(Value::as_str) == Some("incomplete") {
        let reason = match response["incomplete_details"]["reason"].as_str() {
            Some("max_output_tokens") => "max_output_tokens",
            Some("content_filter") => "content_filter",
            _ => "unspecified",
        };
        return format!(
            "Responses API reply was incomplete ({reason}) and contained no output text; the output budget may have gone to reasoning"
        );
    }
    "Responses API reply did not contain output text".into()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn prices_come_as_a_complete_list_or_not_at_all() {
        assert_eq!(Pricing::from_lookup(lookup(&[])).unwrap(), None);
        // Blank values count as unset, like the other WAM_LLM_* settings.
        assert_eq!(
            Pricing::from_lookup(lookup(&[("WAM_LLM_PRICE_INPUT_PER_M", "  ")])).unwrap(),
            None
        );
        assert_eq!(
            Pricing::from_lookup(lookup(&[
                ("WAM_LLM_PRICE_INPUT_PER_M", " 1.25 "),
                ("WAM_LLM_PRICE_CACHED_INPUT_PER_M", "0.15"),
                ("WAM_LLM_PRICE_OUTPUT_PER_M", "4.25"),
            ]))
            .unwrap(),
            Some(Pricing {
                input: 1.25,
                cached_input: 0.15,
                output: 4.25
            })
        );
        for partial in [
            vec![("WAM_LLM_PRICE_INPUT_PER_M", "1.25")],
            vec![
                ("WAM_LLM_PRICE_INPUT_PER_M", "1.25"),
                ("WAM_LLM_PRICE_OUTPUT_PER_M", "4.25"),
            ],
        ] {
            assert!(matches!(
                Pricing::from_lookup(lookup(&partial)),
                Err(ProviderError::Configuration(_))
            ));
        }
    }

    #[test]
    fn an_unreadable_price_is_refused_without_repeating_it() {
        let error = Pricing::from_lookup(lookup(&[
            ("WAM_LLM_PRICE_INPUT_PER_M", "cheap-sentinel"),
            ("WAM_LLM_PRICE_CACHED_INPUT_PER_M", "0.15"),
            ("WAM_LLM_PRICE_OUTPUT_PER_M", "4.25"),
        ]))
        .unwrap_err();
        assert!(matches!(error, ProviderError::Configuration(_)));
        assert!(!error.to_string().contains("cheap-sentinel"));
    }
}
