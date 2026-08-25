// Codex Universal extension: Chat Completions SSE parser tests.

use crate::common::ResponseEvent;
use crate::error::ApiError;
use crate::sse::chat::process_chat_sse;
use assert_matches::assert_matches;
use codex_protocol::models::ResponseItem;
use futures::TryStreamExt;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

fn build_body(events: &[serde_json::Value]) -> String {
    let mut body = String::new();
    for e in events {
        body.push_str(&format!("event: message\ndata: {e}\n\n"));
    }
    body
}

async fn collect_events(body: &str) -> Vec<ResponseEvent> {
    collect_results(body)
        .await
        .into_iter()
        .map(|ev| ev.expect("stream error"))
        .collect()
}

async fn collect_results(body: &str) -> Vec<Result<ResponseEvent, ApiError>> {
    let reader = ReaderStream::new(std::io::Cursor::new(body.to_string()))
        .map_err(|err| codex_client::TransportError::Network(err.to_string()));
    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);
    tokio::spawn(process_chat_sse(
        reader,
        tx,
        Duration::from_millis(1000),
        None,
    ));

    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        out.push(ev);
    }
    out
}

/// Regression test: the stream should complete when we see a `[DONE]` sentinel.
///
/// This is important for tests/mocks that don't immediately close the underlying
/// connection after emitting the sentinel.
#[tokio::test]
async fn completes_on_done_sentinel_without_json() {
    let events = collect_events("event: message\ndata: [DONE]\n\n").await;
    assert_matches!(&events[..], [ResponseEvent::Completed { .. }]);
}

#[tokio::test]
async fn concatenates_tool_call_arguments_across_deltas() {
    let delta_name = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "id": "call_a",
                    "index": 0,
                    "function": { "name": "do_a" }
                }]
            }
        }]
    });

    let delta_args_1 = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "function": { "arguments": "{ \"foo\":" }
                }]
            }
        }]
    });

    let delta_args_2 = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "function": { "arguments": "1}" }
                }]
            }
        }]
    });

    let finish = json!({
        "choices": [{
            "finish_reason": "tool_calls"
        }]
    });

    let body = build_body(&[delta_name, delta_args_1, delta_args_2, finish]);
    let events = collect_events(&body).await;
    assert_matches!(
        &events[..],
        [
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id, name, arguments, .. }),
            ResponseEvent::Completed { .. }
        ] if call_id == "call_a" && name == "do_a" && arguments == "{ \"foo\":1}"
    );
}

#[tokio::test]
async fn emits_multiple_tool_calls() {
    let delta_a = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "id": "call_a",
                    "function": { "name": "do_a", "arguments": "{\"foo\":1}" }
                }]
            }
        }]
    });

    let delta_b = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "id": "call_b",
                    "function": { "name": "do_b", "arguments": "{\"bar\":2}" }
                }]
            }
        }]
    });

    let finish = json!({
        "choices": [{
            "finish_reason": "tool_calls"
        }]
    });

    let body = build_body(&[delta_a, delta_b, finish]);
    let events = collect_events(&body).await;
    assert_matches!(
        &events[..],
        [
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id: call_a, name: name_a, arguments: args_a, .. }),
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id: call_b, name: name_b, arguments: args_b, .. }),
            ResponseEvent::Completed { .. }
        ] if call_a == "call_a" && name_a == "do_a" && args_a == "{\"foo\":1}" && call_b == "call_b" && name_b == "do_b" && args_b == "{\"bar\":2}"
    );
}

#[tokio::test]
async fn merges_tool_calls_by_index_when_id_missing_on_subsequent_deltas() {
    let delta_with_id = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_a",
                    "function": { "name": "do_a", "arguments": "{ \"foo\":" }
                }]
            }
        }]
    });

    let delta_without_id = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "function": { "arguments": "1}" }
                }]
            }
        }]
    });

    let finish = json!({
        "choices": [{
            "finish_reason": "tool_calls"
        }]
    });

    let body = build_body(&[delta_with_id, delta_without_id, finish]);
    let events = collect_events(&body).await;
    assert_matches!(
        &events[..],
        [
            ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id, name, arguments, .. }),
            ResponseEvent::Completed { .. }
        ] if call_id == "call_a" && name == "do_a" && arguments == "{ \"foo\":1}"
    );
}

#[tokio::test]
async fn emits_text_and_reasoning_deltas() {
    let delta = json!({
        "choices": [{
            "delta": {
                "reasoning_content": "thinking",
                "content": "hello"
            }
        }]
    });
    let done = json!({
        "choices": [{
            "finish_reason": "stop"
        }]
    });

    let events = collect_events(&build_body(&[delta, done])).await;
    assert_matches!(
        &events[..],
        [
            ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. }),
            ResponseEvent::ReasoningContentDelta { delta: reasoning_delta, .. },
            ResponseEvent::OutputItemAdded(ResponseItem::Message { .. }),
            ResponseEvent::OutputTextDelta(text_delta),
            ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. }),
            ResponseEvent::OutputItemDone(ResponseItem::Message { .. }),
            ResponseEvent::Completed { .. }
        ] if reasoning_delta == "thinking" && text_delta == "hello"
    );
}

#[tokio::test]
async fn extracts_usage_from_stream_chunks() {
    let chunk = json!({
        "choices": [{ "delta": {} }],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "total_tokens": 120,
            "prompt_tokens_details": { "cached_tokens": 40 },
            "completion_tokens_details": { "reasoning_tokens": 5 }
        }
    });
    let done = json!({
        "choices": [{ "finish_reason": "stop" }]
    });

    let events = collect_events(&build_body(&[chunk, done])).await;
    assert_matches!(
        &events[..],
        [.., ResponseEvent::Completed { token_usage: Some(usage), .. }]
        if usage.input_tokens == 100
            && usage.cached_input_tokens == 40
            && usage.output_tokens == 20
            && usage.reasoning_output_tokens == 5
            && usage.total_tokens == 120
    );
}

#[tokio::test]
async fn surfaces_provider_error_payloads() {
    let error = json!({
        "error": {
            "message": "model is overloaded",
            "code": 503
        }
    });

    let events = collect_results(&build_body(&[error])).await;
    assert!(
        events.iter().any(
            |ev| matches!(ev, Err(ApiError::Stream(message)) if message.contains("overloaded"))
        )
    );
}

#[tokio::test]
async fn length_finish_reason_reports_context_window_exceeded() {
    let chunk = json!({
        "choices": [{ "delta": {}, "finish_reason": "length" }]
    });

    let events = collect_results(&build_body(&[chunk])).await;
    assert_matches!(&events[..], [Err(ApiError::ContextWindowExceeded)]);
}
