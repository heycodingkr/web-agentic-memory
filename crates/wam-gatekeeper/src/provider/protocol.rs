use super::{Message, ProviderError};
use crate::tools::ToolSpec;
use serde_json::{Value, json};

/// The protocol text appended to the instructions for models without native tool calling.
pub fn protocol_text(tools: &[ToolSpec]) -> String {
    let mut out = String::from(
        "TOOL PROTOCOL\nThis session has no native tool calling. On every turn reply with exactly ONE JSON object and \
         nothing else (no markdown fence, no commentary): {\"tool\": \"<tool name>\", \"args\": {...}}\nThe server runs \
         the tool and shows you the result on the next turn. Tools, each with its argument JSON Schema:\n",
    );
    for tool in tools {
        let line = json!({"name": tool.name, "kind": match tool.kind {
            crate::tools::ToolKind::Read => "read",
            crate::tools::ToolKind::Decision => "decision",
        }, "description": tool.description, "input_schema": tool.input_schema});
        out.push_str(&line.to_string());
        out.push('\n');
    }
    out
}

pub fn render_transcript(messages: &[Message]) -> String {
    let mut out = String::from("TRANSCRIPT\n");
    for message in messages {
        match message {
            Message::Job(text) => out.push_str(&format!("[JOB] {text}\n")),
            Message::Call { tool, args } => {
                out.push_str(&format!("[YOU] {}\n", json!({"tool": tool, "args": args})))
            }
            Message::Result { tool, value } => {
                out.push_str(&format!("[TOOL RESULT {tool}] {value}\n"))
            }
        }
    }
    out.push_str("Reply now with your next tool call as one JSON object.");
    out
}

/// Pulls the first balanced JSON object out of a model reply and checks that it is a tool call.
pub fn extract_turn(text: &str) -> Result<(String, Value), ProviderError> {
    let malformed = |why: &str| ProviderError::Malformed {
        raw: text.chars().take(400).collect(),
        why: why.to_owned(),
    };
    let start = text
        .find('{')
        .ok_or_else(|| malformed("no JSON object found"))?;
    let (mut depth, mut in_string, mut escaped, mut end) = (0i32, false, false, None);
    for (offset, character) in text[start..].char_indices() {
        if in_string {
            match (escaped, character) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + offset + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| malformed("the JSON object is not closed"))?;
    let value: Value = serde_json::from_str(&text[start..end])
        .map_err(|_| malformed("the JSON object does not parse"))?;
    let tool = value["tool"]
        .as_str()
        .ok_or_else(|| malformed("missing string field \"tool\""))?
        .to_owned();
    let args = match value.get("args") {
        None | Some(Value::Null) => json!({}),
        Some(args) if args.is_object() => args.clone(),
        Some(_) => return Err(malformed("\"args\" must be an object")),
    };
    Ok((tool, args))
}

pub(super) fn clip(text: &str) -> String {
    text.chars().take(300).collect()
}

/// Does an error text from a CLI or an API say that the usage limit or the credit is spent, as opposed to a passing
/// failure? The phrases are the ones the CLIs and common APIs use. A wording that is not listed is a plain failure,
/// which the queue retries with backoff and finally dead-letters: nothing is lost, it only takes longer to notice.
pub fn looks_like_quota_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "usage limit",
        "hit your limit",
        "insufficient_quota",
        "insufficient quota",
        "exceeded your current quota",
        "quota exceeded",
        "out of credits",
        "insufficient credit",
        "credit balance",
        "payment required",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

/// A failed model call: spent quota is told apart from everything else.
pub(super) fn failure(text: &str) -> ProviderError {
    if looks_like_quota_error(text) {
        ProviderError::QuotaExhausted {
            why: clip(text),
            retry_after_secs: None,
        }
    } else {
        ProviderError::Failed(clip(text))
    }
}
