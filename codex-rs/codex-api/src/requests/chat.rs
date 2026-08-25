// Codex Universal extension: Chat Completions request builder.
//
// Converts the internal Responses-shaped prompt (instructions + ResponseItem
// history + serialized tool definitions) into an OpenAI-compatible
// `/chat/completions` request body. Tool calls and tool outputs are mapped to
// the `tool_calls` / role-"tool" message conventions so third-party models can
// drive the Codex harness.

use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;

/// Assembled request body for a Chat Completions streaming call.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub parallel_tool_calls: bool,
    pub stream: bool,
    /// Ask compatible providers to include usage in the final stream chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<ChatStreamOptions>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ChatStreamOptions {
    pub include_usage: bool,
}

pub struct ChatRequestBuilder<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a [ResponseItem],
    tools: Option<&'a Value>,
    parallel_tool_calls: bool,
}

impl<'a> ChatRequestBuilder<'a> {
    pub fn new(
        model: &'a str,
        instructions: &'a str,
        input: &'a [ResponseItem],
        tools: Option<&'a Value>,
    ) -> Self {
        Self {
            model,
            instructions,
            input,
            tools,
            parallel_tool_calls: true,
        }
    }

    pub fn parallel_tool_calls(mut self, parallel_tool_calls: bool) -> Self {
        self.parallel_tool_calls = parallel_tool_calls;
        self
    }

    pub fn build(self) -> ChatCompletionRequest {
        let mut messages = Vec::<Value>::new();
        if !self.instructions.is_empty() {
            messages.push(json!({"role": "system", "content": self.instructions}));
        }

        let input = self.input;
        let mut reasoning_by_anchor_index: HashMap<usize, String> = HashMap::new();
        let mut last_emitted_role: Option<&str> = None;
        for item in input {
            match item {
                ResponseItem::Message { .. } => {}
                ResponseItem::AgentMessage { .. } => {}
                ResponseItem::FunctionCall { .. } | ResponseItem::LocalShellCall { .. } => {
                    last_emitted_role = Some("assistant");
                }
                ResponseItem::CustomToolCall { .. } => last_emitted_role = Some("assistant"),
                ResponseItem::FunctionCallOutput { .. }
                | ResponseItem::CustomToolCallOutput { .. } => last_emitted_role = Some("tool"),
                _ => {}
            }
        }

        let mut last_user_index: Option<usize> = None;
        for (idx, item) in input.iter().enumerate() {
            if let ResponseItem::Message { role, .. } = item
                && role == "user"
            {
                last_user_index = Some(idx);
            }
        }

        // When the turn does not end with a user message (e.g. tool-call
        // follow-ups), surface recent reasoning content by attaching it to the
        // adjacent assistant/tool-call item so reasoning models retain their
        // chain of thought across chat requests.
        if !matches!(last_emitted_role, Some("user")) {
            for (idx, item) in input.iter().enumerate() {
                if let Some(u_idx) = last_user_index
                    && idx <= u_idx
                {
                    continue;
                }

                if let ResponseItem::Reasoning {
                    content: Some(items),
                    ..
                } = item
                {
                    let mut text = String::new();
                    for entry in items {
                        match entry {
                            ReasoningItemContent::ReasoningText { text: segment }
                            | ReasoningItemContent::Text { text: segment } => {
                                text.push_str(segment);
                            }
                        }
                    }
                    if text.trim().is_empty() {
                        continue;
                    }

                    let mut attached = false;
                    if idx > 0
                        && let ResponseItem::Message { role, .. } = &input[idx - 1]
                        && role == "assistant"
                    {
                        reasoning_by_anchor_index
                            .entry(idx - 1)
                            .and_modify(|v| v.push_str(&text))
                            .or_insert_with(|| text.clone());
                        attached = true;
                    }

                    if !attached && idx + 1 < input.len() {
                        match &input[idx + 1] {
                            ResponseItem::FunctionCall { .. }
                            | ResponseItem::LocalShellCall { .. }
                            | ResponseItem::CustomToolCall { .. } => {
                                reasoning_by_anchor_index
                                    .entry(idx + 1)
                                    .and_modify(|v| v.push_str(&text))
                                    .or_insert_with(|| text.clone());
                            }
                            ResponseItem::Message { role, .. } if role == "assistant" => {
                                reasoning_by_anchor_index
                                    .entry(idx + 1)
                                    .and_modify(|v| v.push_str(&text))
                                    .or_insert_with(|| text.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let mut last_assistant_text: Option<String> = None;

        for (idx, item) in input.iter().enumerate() {
            match item {
                ResponseItem::Message { role, content, .. } => {
                    let mut text = String::new();
                    let mut items: Vec<Value> = Vec::new();
                    let mut saw_image = false;

                    for c in content {
                        match c {
                            ContentItem::InputText { text: t }
                            | ContentItem::OutputText { text: t } => {
                                text.push_str(t);
                                items.push(json!({"type": "text", "text": t}));
                            }
                            ContentItem::InputImage { image_url, .. } => {
                                saw_image = true;
                                items.push(
                                    json!({"type": "image_url", "image_url": {"url": image_url}}),
                                );
                            }
                            ContentItem::InputAudio { .. } => {}
                        }
                    }

                    if role == "assistant" {
                        if last_assistant_text.as_deref() == Some(text.as_str()) {
                            continue;
                        }
                        last_assistant_text = Some(text.clone());
                    }

                    // Most Chat Completions providers reject the Responses
                    // API's `developer` role; fold it into `system`.
                    let wire_role = if role == "developer" {
                        "system"
                    } else {
                        role.as_str()
                    };

                    let content_value = if wire_role != "assistant" && saw_image {
                        json!(items)
                    } else {
                        json!(text)
                    };

                    let mut msg = json!({"role": wire_role, "content": content_value});
                    if wire_role == "assistant"
                        && let Some(reasoning) = reasoning_by_anchor_index.get(&idx)
                        && let Some(obj) = msg.as_object_mut()
                    {
                        obj.insert("reasoning".to_string(), json!(reasoning));
                    }
                    messages.push(msg);
                }
                ResponseItem::AgentMessage { content, .. } => {
                    let mut text = String::new();
                    for c in content {
                        if let codex_protocol::models::AgentMessageInputContent::InputText {
                            text: t,
                        } = c
                        {
                            text.push_str(t);
                        }
                    }
                    messages.push(json!({"role": "user", "content": text}));
                }
                ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    ..
                } => {
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    let tool_call = json!({
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments,
                        }
                    });
                    push_tool_call_message(&mut messages, tool_call, reasoning);
                }
                ResponseItem::LocalShellCall {
                    id,
                    call_id,
                    status,
                    action,
                    ..
                } => {
                    // Local shell calls are Responses-native; represent them as
                    // a function-style tool call so history stays consistent.
                    let tool_call_id = call_id
                        .clone()
                        .or_else(|| id.as_ref().map(|i| i.to_string()))
                        .unwrap_or_default();
                    let tool_call = json!({
                        "id": tool_call_id,
                        "type": "function",
                        "function": {
                            "name": "local_shell",
                            "arguments": json!(action).to_string(),
                        },
                        "status": status,
                    });
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    push_tool_call_message(&mut messages, tool_call, reasoning);
                }
                ResponseItem::FunctionCallOutput {
                    call_id, output, ..
                } => {
                    let content_value = if let Some(items) = output.content_items() {
                        let mapped: Vec<Value> = items
                            .iter()
                            .map(|it| match it {
                                FunctionCallOutputContentItem::InputText { text } => {
                                    json!({"type": "text", "text": text})
                                }
                                FunctionCallOutputContentItem::InputImage { image_url, .. } => {
                                    json!({"type": "image_url", "image_url": {"url": image_url}})
                                }
                                FunctionCallOutputContentItem::InputAudio { audio_url } => {
                                    json!({"type": "input_audio", "audio_url": audio_url})
                                }
                                FunctionCallOutputContentItem::EncryptedContent { .. } => {
                                    // Encrypted content has no Chat representation.
                                    json!({"type": "text", "text": "[encrypted content omitted]"})
                                }
                            })
                            .collect();
                        json!(mapped)
                    } else {
                        json!(output.text_content().unwrap_or_default())
                    };

                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id.clone().unwrap_or_default(),
                        "content": content_value,
                    }));
                }
                ResponseItem::CustomToolCall {
                    id,
                    name,
                    input: tool_input,
                    call_id,
                    ..
                } => {
                    let tool_call = json!({
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": tool_input,
                        },
                        "internal_item_id": id,
                    });
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    push_tool_call_message(&mut messages, tool_call, reasoning);
                }
                ResponseItem::CustomToolCallOutput {
                    call_id, output, ..
                } => {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": output.text_content().unwrap_or_default(),
                    }));
                }
                // Request controls / provider-encrypted payloads have no
                // Chat Completions representation.
                ResponseItem::AdditionalTools { .. }
                | ResponseItem::ToolSearchCall { .. }
                | ResponseItem::ToolSearchOutput { .. }
                | ResponseItem::WebSearchCall { .. }
                | ResponseItem::ImageGenerationCall { .. }
                | ResponseItem::Compaction { .. }
                | ResponseItem::CompactionTrigger { .. }
                | ResponseItem::ContextCompaction { .. }
                | ResponseItem::Reasoning { .. }
                | ResponseItem::Other => {
                    continue;
                }
            }
        }

        ChatCompletionRequest {
            model: self.model.to_string(),
            messages,
            tools: self.tools.and_then(convert_tools_for_chat),
            parallel_tool_calls: self.parallel_tool_calls && self.tools.is_some(),
            stream: true,
            stream_options: Some(ChatStreamOptions {
                include_usage: true,
            }),
        }
    }
}

/// Converts Responses-style flat function tools into Chat Completions
/// `{"type":"function","function":{...}}` form.
///
/// Responses-only tool kinds (`web_search`, `namespace`, `tool_search`,
/// freeform/custom) have no Chat Completions representation and are dropped
/// rather than sent, which many third-party providers reject.
fn convert_tools_for_chat(tools: &Value) -> Option<Value> {
    let items = tools.as_array()?;
    let converted: Vec<Value> = items
        .iter()
        .filter_map(|tool| {
            if tool.get("type").and_then(Value::as_str) != Some("function") {
                return None;
            }
            let name = tool.get("name")?;
            let mut function = serde_json::Map::new();
            function.insert("name".to_string(), name.clone());
            if let Some(description) = tool.get("description") {
                function.insert("description".to_string(), description.clone());
            }
            if let Some(parameters) = tool.get("parameters") {
                function.insert("parameters".to_string(), parameters.clone());
            }
            Some(json!({"type": "function", "function": Value::Object(function)}))
        })
        .collect();
    (!converted.is_empty()).then(|| Value::Array(converted))
}

fn push_tool_call_message(messages: &mut Vec<Value>, tool_call: Value, reasoning: Option<&str>) {
    // Chat Completions requires that tool calls are grouped into a single assistant message
    // (with `tool_calls: [...]`) followed by tool role responses.
    if let Some(Value::Object(obj)) = messages.last_mut()
        && obj.get("role").and_then(Value::as_str) == Some("assistant")
        && obj.get("content").is_some_and(Value::is_null)
        && let Some(tool_calls) = obj.get_mut("tool_calls").and_then(Value::as_array_mut)
    {
        tool_calls.push(tool_call);
        if let Some(reasoning) = reasoning {
            attach_reasoning(obj, reasoning);
        }
        return;
    }

    let mut msg = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [tool_call],
    });
    if let Some(reasoning) = reasoning
        && let Some(obj) = msg.as_object_mut()
    {
        attach_reasoning(obj, reasoning);
    }
    messages.push(msg);
}

fn attach_reasoning(obj: &mut serde_json::Map<String, Value>, reasoning: &str) {
    if let Some(Value::String(existing)) = obj.get_mut("reasoning") {
        if !existing.is_empty() {
            existing.push('\n');
        }
        existing.push_str(reasoning);
    } else {
        obj.insert(
            "reasoning".to_string(),
            Value::String(reasoning.to_string()),
        );
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
