// Codex Universal extension: Chat Completions SSE stream parser.
//
// Converts OpenAI-compatible `chat/completions` streaming chunks into the same
// internal `ResponseEvent` stream produced by the Responses API parser, so the
// Codex agent loop can consume either transport transparently.

use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

pub(crate) fn spawn_chat_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    _turn_state: Option<Arc<OnceLock<String>>>,
) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        process_chat_sse(stream_response.bytes, tx_event, idle_timeout, telemetry).await;
    });
    ResponseStream {
        rx_event,
        upstream_request_id: None,
    }
}

/// Processes Server-Sent Events from a Chat Completions streaming response.
///
/// The upstream protocol terminates a streaming response with a final sentinel
/// event (`data: [DONE]`). Some providers emit `data: DONE` (without brackets)
/// instead; both are treated as sentinels.
///
/// On servers that keep the HTTP connection open after emitting the sentinel
/// (notably wiremock on Windows), skipping the sentinel would mean never
/// emitting `ResponseEvent::Completed`, so the sentinel always flushes and
/// completes the stream.
pub async fn process_chat_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<std::sync::Arc<dyn SseTelemetry>>,
) where
    S: Stream<Item = Result<bytes::Bytes, codex_client::TransportError>> + Unpin,
{
    let mut stream = stream.eventsource();

    #[derive(Default, Debug)]
    struct ToolCallState {
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    }

    let mut tool_calls: HashMap<usize, ToolCallState> = HashMap::new();
    let mut tool_call_order: Vec<usize> = Vec::new();
    let mut tool_call_order_seen: HashSet<usize> = HashSet::new();
    let mut tool_call_index_by_id: HashMap<String, usize> = HashMap::new();
    let mut next_tool_call_index = 0usize;
    let mut last_tool_call_index: Option<usize> = None;
    let mut assistant_item: Option<ResponseItem> = None;
    let mut reasoning_item: Option<ResponseItem> = None;
    let mut usage: Option<TokenUsage> = None;
    let mut completed_sent = false;

    macro_rules! flush_and_complete {
        () => {
            if let Some(reasoning) = reasoning_item.take() {
                let _ = tx_event
                    .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
                    .await;
            }

            if let Some(assistant) = assistant_item.take() {
                let _ = tx_event
                    .send(Ok(ResponseEvent::OutputItemDone(assistant)))
                    .await;
            }

            for index in tool_call_order.drain(..) {
                let Some(state) = tool_calls.remove(&index) else {
                    continue;
                };
                tool_call_order_seen.remove(&index);
                let ToolCallState {
                    id,
                    name,
                    arguments,
                } = state;
                let Some(name) = name else {
                    debug!("Skipping tool call at index {index} because name is missing");
                    continue;
                };
                let item = ResponseItem::FunctionCall {
                    id: None,
                    name,
                    namespace: None,
                    arguments,
                    encrypted_function_args: None,
                    call_id: id.unwrap_or_else(|| format!("tool-call-{index}")),
                    internal_chat_message_metadata_passthrough: None,
                };
                let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
            }

            let _ = tx_event
                .send(Ok(ResponseEvent::Completed {
                    response_id: String::new(),
                    token_usage: usage.take(),
                    end_turn: None,
                }))
                .await;
        };
    }

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                return;
            }
            Ok(None) => {
                if !completed_sent {
                    flush_and_complete!();
                }
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        trace!("SSE event: {}", sse.data);

        let data = sse.data.trim();

        if data.is_empty() {
            continue;
        }

        if data == "[DONE]" || data == "DONE" {
            flush_and_complete!();
            return;
        }

        let value: serde_json::Value = match serde_json::from_str(data) {
            Ok(val) => val,
            Err(err) => {
                debug!(
                    "Failed to parse ChatCompletions SSE event: {err}, data: {}",
                    data
                );
                continue;
            }
        };

        if let Some(chunk_usage) = parse_chunk_usage(&value) {
            usage = Some(chunk_usage);
        }

        // Provider-side errors can arrive as a JSON body without choices.
        if value.get("error").is_some() && value.get("choices").is_none() {
            let message = value["error"]
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("provider returned an error");
            let _ = tx_event
                .send(Err(ApiError::Stream(message.to_string())))
                .await;
            return;
        }

        let Some(choices) = value.get("choices").and_then(|c| c.as_array()) else {
            continue;
        };

        for choice in choices {
            if let Some(delta) = choice.get("delta") {
                if let Some(reasoning) = delta.get("reasoning") {
                    if let Some(text) = reasoning.as_str() {
                        append_reasoning_text(&tx_event, &mut reasoning_item, text.to_string())
                            .await;
                    } else if let Some(text) = reasoning.get("text").and_then(|v| v.as_str()) {
                        append_reasoning_text(&tx_event, &mut reasoning_item, text.to_string())
                            .await;
                    } else if let Some(text) = reasoning.get("content").and_then(|v| v.as_str()) {
                        append_reasoning_text(&tx_event, &mut reasoning_item, text.to_string())
                            .await;
                    }
                } else if let Some(reasoning_content) =
                    delta.get("reasoning_content").and_then(|v| v.as_str())
                {
                    append_reasoning_text(&tx_event, &mut reasoning_item, reasoning_content.into())
                        .await;
                }

                if let Some(content) = delta.get("content") {
                    if content.is_array() {
                        for item in content.as_array().unwrap_or(&vec![]) {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                append_assistant_text(
                                    &tx_event,
                                    &mut assistant_item,
                                    text.to_string(),
                                )
                                .await;
                            }
                        }
                    } else if let Some(text) = content.as_str()
                        && !text.is_empty()
                    {
                        append_assistant_text(&tx_event, &mut assistant_item, text.to_string())
                            .await;
                    }
                }

                if let Some(tool_call_values) = delta.get("tool_calls").and_then(|c| c.as_array()) {
                    for tool_call in tool_call_values {
                        let mut index = tool_call
                            .get("index")
                            .and_then(serde_json::Value::as_u64)
                            .map(|i| i as usize);

                        let mut call_id_for_lookup = None;
                        if let Some(call_id) = tool_call.get("id").and_then(|i| i.as_str())
                            && !call_id.is_empty()
                        {
                            call_id_for_lookup = Some(call_id.to_string());
                            if let Some(existing) = tool_call_index_by_id.get(call_id) {
                                index = Some(*existing);
                            }
                        }

                        if index.is_none() && call_id_for_lookup.is_none() {
                            index = last_tool_call_index;
                        }

                        let index = index.unwrap_or_else(|| {
                            while tool_calls.contains_key(&next_tool_call_index) {
                                next_tool_call_index += 1;
                            }
                            let idx = next_tool_call_index;
                            next_tool_call_index += 1;
                            idx
                        });

                        let call_state = tool_calls.entry(index).or_default();
                        if tool_call_order_seen.insert(index) {
                            tool_call_order.push(index);
                        }

                        if let Some(id) = tool_call.get("id").and_then(|i| i.as_str())
                            && !id.is_empty()
                        {
                            call_state.id.get_or_insert_with(|| id.to_string());
                            tool_call_index_by_id.entry(id.to_string()).or_insert(index);
                        }

                        if let Some(func) = tool_call.get("function") {
                            if let Some(fname) = func.get("name").and_then(|n| n.as_str())
                                && !fname.is_empty()
                            {
                                call_state.name.get_or_insert_with(|| fname.to_string());
                            }
                            if let Some(arguments) = func.get("arguments").and_then(|a| a.as_str())
                            {
                                call_state.arguments.push_str(arguments);
                            }
                        }

                        last_tool_call_index = Some(index);
                    }
                }
            }

            if let Some(message) = choice.get("message")
                && let Some(reasoning) = message.get("reasoning")
            {
                if let Some(text) = reasoning.as_str() {
                    append_reasoning_text(&tx_event, &mut reasoning_item, text.to_string()).await;
                } else if let Some(text) = reasoning.get("text").and_then(|v| v.as_str()) {
                    append_reasoning_text(&tx_event, &mut reasoning_item, text.to_string()).await;
                } else if let Some(text) = reasoning.get("content").and_then(|v| v.as_str()) {
                    append_reasoning_text(&tx_event, &mut reasoning_item, text.to_string()).await;
                }
            }

            let finish_reason = choice.get("finish_reason").and_then(|r| r.as_str());
            match finish_reason {
                Some("stop") => {
                    if !completed_sent {
                        completed_sent = true;
                        flush_and_complete!();
                    }
                }
                Some("length") => {
                    let _ = tx_event.send(Err(ApiError::ContextWindowExceeded)).await;
                    return;
                }
                Some("tool_calls") | Some("function_call") => {
                    if let Some(reasoning) = reasoning_item.take() {
                        let _ = tx_event
                            .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
                            .await;
                    }

                    if let Some(assistant) = assistant_item.take() {
                        let _ = tx_event
                            .send(Ok(ResponseEvent::OutputItemDone(assistant)))
                            .await;
                    }

                    for index in tool_call_order.drain(..) {
                        let Some(state) = tool_calls.remove(&index) else {
                            continue;
                        };
                        tool_call_order_seen.remove(&index);
                        let ToolCallState {
                            id,
                            name,
                            arguments,
                        } = state;
                        let Some(name) = name else {
                            debug!("Skipping tool call at index {index} because name is missing");
                            continue;
                        };
                        let item = ResponseItem::FunctionCall {
                            id: None,
                            name,
                            namespace: None,
                            arguments,
                            encrypted_function_args: None,
                            call_id: id.unwrap_or_else(|| format!("tool-call-{index}")),
                            internal_chat_message_metadata_passthrough: None,
                        };
                        let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Maps a Chat Completions `usage` object onto Codex's token usage type.
fn parse_chunk_usage(value: &serde_json::Value) -> Option<TokenUsage> {
    let usage = value.get("usage")?;
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(serde_json::Value::as_i64)?;
    let completion_tokens = usage
        .get("completion_tokens")
        .and_then(serde_json::Value::as_i64)?;
    let total_tokens = usage
        .get("total_tokens")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(prompt_tokens.saturating_add(completion_tokens));
    let cached_input_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let reasoning_output_tokens = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);

    Some(TokenUsage {
        input_tokens: prompt_tokens,
        cached_input_tokens,
        cache_write_input_tokens: 0,
        output_tokens: completion_tokens,
        reasoning_output_tokens,
        total_tokens,
        codex_rollout_budget_units: None,
    })
}

async fn append_assistant_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    assistant_item: &mut Option<ResponseItem>,
    text: String,
) {
    if assistant_item.is_none() {
        let item = ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        *assistant_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }

    if let Some(ResponseItem::Message { content, .. }) = assistant_item {
        content.push(ContentItem::OutputText { text: text.clone() });
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputTextDelta(text.clone())))
            .await;
    }
}

async fn append_reasoning_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    text: String,
) {
    if reasoning_item.is_none() {
        let item = ResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content: Some(vec![]),
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        };
        *reasoning_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }

    if let Some(ResponseItem::Reasoning {
        content: Some(content),
        ..
    }) = reasoning_item
    {
        let content_index = content.len() as i64;
        content.push(ReasoningItemContent::ReasoningText { text: text.clone() });

        let _ = tx_event
            .send(Ok(ResponseEvent::ReasoningContentDelta {
                delta: text.clone(),
                content_index,
            }))
            .await;
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
