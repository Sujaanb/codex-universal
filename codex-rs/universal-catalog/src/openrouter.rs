// Codex Universal extension: OpenRouter model catalog parsing.

use serde_json::Value;

pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

#[derive(Debug, Clone, PartialEq)]
pub struct OpenRouterModel {
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: Option<u64>,
    pub free: bool,
}

/// Parses the OpenRouter `GET /api/v1/models` payload.
///
/// Free models are identified by zero prompt/completion pricing, which is the
/// provider's authoritative cost metadata (not the model name).
pub fn parse_openrouter_models(payload: &Value) -> Vec<OpenRouterModel> {
    let Some(data) = payload.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };

    data.iter()
        .filter_map(|entry| {
            let model = entry.get("id").and_then(Value::as_str)?;
            if model.is_empty() {
                return None;
            }
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(model)
                .to_string();
            let description = entry
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let context_window = entry
                .get("context_length")
                .and_then(Value::as_u64);
            let free = is_free(entry);
            Some(OpenRouterModel {
                model: model.to_string(),
                name,
                description,
                context_window,
                free,
            })
        })
        .collect()
}

fn is_free(entry: &Value) -> bool {
    let Some(pricing) = entry.get("pricing") else {
        return false;
    };
    let parse = |field: &str| {
        pricing
            .get(field)
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<f64>().ok())
            .or_else(|| pricing.get(field).and_then(Value::as_f64))
            .unwrap_or(f64::INFINITY)
    };
    // Only treat a model as free when both directions are explicitly zero.
    parse("prompt") == 0.0 && parse("completion") == 0.0
}
