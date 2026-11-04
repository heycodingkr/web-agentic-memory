//! Bounded stdio framing and request lifecycle, independent of the tool dispatcher.
use crate::{LATEST_PROTOCOL_VERSION, adapt_response, handle_message, valid_request_shape};
use serde_json::{Value, json};
use std::{collections::HashMap, io};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
    task::{AbortHandle, JoinSet},
};
use wam_client::{DiagnosticError, DiagnosticOperation, HttpClient};

const MAX_FRAME_BYTES: usize = 64 * 1024;
const MAX_INFLIGHT: usize = 16;

enum Frame {
    Message(Value),
    ParseError,
    TooLarge,
}

async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Option<Frame>> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    let mut seen = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if !seen {
                return Ok(None);
            }
            break;
        }
        seen = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if !oversized {
            if bytes.len() + count > MAX_FRAME_BYTES {
                oversized = true;
            } else {
                bytes.extend_from_slice(&available[..count]);
            }
        }
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    Ok(Some(if oversized {
        Frame::TooLarge
    } else {
        serde_json::from_slice(&bytes).map_or(Frame::ParseError, Frame::Message)
    }))
}
async fn emit(
    output: &mut (impl tokio::io::AsyncWrite + Unpin),
    mut response: Value,
    version: &str,
) -> io::Result<()> {
    adapt_response(&mut response, Some(version));
    let mut bytes = serde_json::to_vec(&response).map_err(io::Error::other)?;
    bytes.push(b'\n');
    output.write_all(&bytes).await?;
    output.flush().await
}
fn request_key(message: &Value) -> Option<String> {
    message
        .get("id")
        .filter(|id| id.is_string() || id.is_i64() || id.is_u64())
        .map(Value::to_string)
}
fn protocol_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

/// Keep receiving cancellation/ping while bounded tool calls are in flight.
/// Each task gets its own failure reference while sharing HTTP/cache resources.
pub async fn serve(client: HttpClient) -> io::Result<()> {
    let (sender, mut frames) = mpsc::channel(MAX_INFLIGHT);
    let reader = tokio::spawn(async move {
        let mut input = BufReader::new(tokio::io::stdin());
        loop {
            match read_frame(&mut input).await {
                Ok(Some(frame)) => {
                    if sender.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    let mut output = tokio::io::stdout();
    let mut version = LATEST_PROTOCOL_VERSION.to_owned();
    let mut tasks = JoinSet::new();
    let mut active: HashMap<String, (u64, AbortHandle)> = HashMap::new();
    let mut sequence = 0u64;
    let mut accepting = true;
    while accepting || !tasks.is_empty() {
        tokio::select! {
            biased;
            frame = frames.recv(), if accepting => {
                let Some(frame) = frame else { accepting = false; continue; };
                let frame = frame?;
                if let Frame::Message(message) = &frame {
                    if valid_request_shape(message) {
                        if message["method"] == "notifications/cancelled" {
                            if let Some(key) = message.pointer("/params/requestId").filter(|id| id.is_string() || id.is_i64() || id.is_u64()).map(Value::to_string) {
                                if let Some((_, task)) = active.remove(&key) { task.abort(); }
                            }
                            continue;
                        }
                        if message.get("id").is_none() { continue; }
                        if message["method"] == "ping" {
                            if let Some(response) = handle_message(&client.for_request(), message.clone()).await { emit(&mut output, response, &version).await?; }
                            continue;
                        }
                        if message["method"] == "initialize" {
                            if let Some(response) = handle_message(&client.for_request(), message.clone()).await {
                                if let Some(selected) = response.pointer("/result/protocolVersion").and_then(Value::as_str) { version = selected.to_owned(); }
                                emit(&mut output, response, &version).await?;
                            }
                            continue;
                        }
                    }
                }
                let key = match &frame { Frame::Message(message) => request_key(message), _ => None };
                if key.as_ref().is_some_and(|key| active.contains_key(key)) || tasks.len() >= MAX_INFLIGHT {
                    let id = match &frame { Frame::Message(message) => message.get("id").filter(|id| id.is_string() || id.is_i64() || id.is_u64()).cloned().unwrap_or(Value::Null), _ => Value::Null };
                    emit(&mut output, protocol_error(id, -32000, "request capacity or ID conflict"), &version).await?;
                    continue;
                }
                sequence = sequence.wrapping_add(1);
                let generation = sequence;
                let task_key = key.clone();
                let request_client = client.for_request();
                let task = tasks.spawn(async move {
                    let response = match frame {
                        Frame::Message(message) => handle_message(&request_client, message).await,
                        invalid => {
                            let trace = request_client.report_diagnostic_event(DiagnosticOperation::Unknown, DiagnosticError::InvalidInput).await;
                            let mut response = if matches!(invalid, Frame::TooLarge) { protocol_error(Value::Null, -32600, "request too large") } else { crate::parse_error_response() };
                            if let Some(trace) = trace { response["error"]["data"] = json!({"trace_id":trace}); }
                            Some(response)
                        }
                    };
                    (task_key, generation, response)
                });
                if let Some(key) = key { active.insert(key, (generation, task)); }
            }
            completed = tasks.join_next(), if !tasks.is_empty() => {
                if completed.as_ref().is_some_and(|result| matches!(result, Err(error) if !error.is_cancelled())) { return Err(io::Error::other("request task failed")); }
                if let Some(Ok((key, generation, response))) = completed {
                    if let Some(key) = key {
                        if !active.get(&key).is_some_and(|(current, _)| *current == generation) { continue; }
                        active.remove(&key);
                    }
                    if let Some(response) = response { emit(&mut output, response, &version).await?; }
                }
            }
        }
    }
    reader.abort();
    Ok(())
}
