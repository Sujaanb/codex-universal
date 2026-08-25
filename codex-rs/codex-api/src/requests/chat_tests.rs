// Codex Universal extension: Chat Completions request builder tests.

use crate::requests::chat::ChatRequestBuilder;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;
use serde_json::json;

fn user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn builds_system_and_user_messages() {
    let input = vec![user_message("hi")];
    let request = ChatRequestBuilder::new("gpt-test", "inst", &input, None).build();

    assert_eq!(request.model, "gpt-test");
    assert!(request.stream);
    assert_eq!(
        request.messages,
        vec![
            json!({"role": "system", "content": "inst"}),
            json!({"role": "user", "content": "hi"}),
        ]
    );
}

#[test]
fn groups_consecutive_tool_calls_into_a_single_assistant_message() {
    let prompt_input = vec![
        user_message("read these"),
        ResponseItem::FunctionCall {
            id: None,
            name: "read_file".to_string(),
            arguments: r#"{"path":"a.txt"}"#.to_string(),
            call_id: "call-a".to_string(),
            namespace: None,
            encrypted_function_args: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCall {
            id: None,
            name: "read_file".to_string(),
            arguments: r#"{"path":"b.txt"}"#.to_string(),
            call_id: "call-b".to_string(),
            namespace: None,
            encrypted_function_args: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("call-a".to_string()),
            name: None,
            namespace: None,
            output: FunctionCallOutputPayload::from_text("A".to_string()),
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("call-b".to_string()),
            name: None,
            namespace: None,
            output: FunctionCallOutputPayload::from_text("B".to_string()),
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let request = ChatRequestBuilder::new("gpt-test", "inst", &prompt_input, None).build();
    let messages = &request.messages;

    // system + user + assistant(tool_calls=[...]) + 2 tool outputs
    assert_eq!(messages.len(), 5);

    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");

    let tool_calls_msg = &messages[2];
    assert_eq!(tool_calls_msg["role"], "assistant");
    assert_eq!(tool_calls_msg["content"], serde_json::Value::Null);
    let tool_calls = tool_calls_msg["tool_calls"]
        .as_array()
        .expect("tool_calls array");
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0]["id"], "call-a");
    assert_eq!(tool_calls[0]["function"]["name"], "read_file");
    assert_eq!(tool_calls[1]["id"], "call-b");

    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call-a");
    assert_eq!(messages[4]["role"], "tool");
    assert_eq!(messages[4]["tool_call_id"], "call-b");
}

#[test]
fn serializes_tools_in_chat_format() {
    let tools = json!([
        {
            "type": "function",
            "name": "shell",
            "description": "run a shell command",
            "parameters": {"type": "object", "properties": {}}
        },
        {
            "type": "web_search",
            "external_web_access": true
        }
    ]);
    let input = vec![user_message("hi")];
    let request = ChatRequestBuilder::new("m", "inst", &input, Some(&tools)).build();

    let body = serde_json::to_value(&request).expect("serialize");
    // Responses-only tool kinds are dropped; function tools are nested.
    let converted_tools = body["tools"].as_array().expect("tools array");
    assert_eq!(converted_tools.len(), 1);
    assert_eq!(converted_tools[0]["type"], "function");
    assert_eq!(converted_tools[0]["function"]["name"], "shell");
    assert_eq!(
        converted_tools[0]["function"]["description"],
        "run a shell command"
    );
    assert_eq!(body["parallel_tool_calls"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
}

#[test]
fn omits_parallel_tool_calls_without_tools() {
    let input = vec![user_message("hi")];
    let request = ChatRequestBuilder::new("m", "inst", &input, None)
        .parallel_tool_calls(true)
        .build();
    let body = serde_json::to_value(&request).expect("serialize");
    assert!(body.get("tools").is_none());
    // `parallel_tool_calls: false` is omitted on the wire.
    assert!(body.get("parallel_tool_calls").is_none());
}
