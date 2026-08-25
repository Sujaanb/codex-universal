// Codex Universal extension: OpenCode Zen catalog and free-model discovery.

use super::JsonFetcher;
use serde_json::Value;

/// Live listing of models available through OpenCode Zen.
pub const OPENCODE_ZEN_MODELS_URL: &str = "https://opencode.ai/zen/v1/models";
/// Authoritative metadata catalog used by the OpenCode client itself; carries
/// per-model cost information that identifies genuinely free models.
pub const OPENCODE_MODELS_CATALOG_URL: &str = "https://models.opencode.ai/api.json";

#[derive(Debug, Clone, PartialEq)]
pub struct OpenCodeModel {
    pub model: String,
    pub name: String,
    pub free: bool,
}

/// Builds the OpenCode Zen catalog by combining:
///
/// 1. the live Zen `/models` listing (which models are actually served), and
/// 2. the models.opencode.ai catalog (which of those cost zero).
///
/// When the metadata catalog is unavailable, models are conservatively marked
/// as free only if their id ends in `-free`.
pub async fn opencode_catalog(fetcher: &JsonFetcher) -> Vec<OpenCodeModel> {
    let zen = (fetcher)(OPENCODE_ZEN_MODELS_URL.to_string())
        .await
        .unwrap_or(Value::Null);
    let metadata = (fetcher)(OPENCODE_MODELS_CATALOG_URL.to_string()).await;

    let zen_ids: Vec<String> = zen
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                .filter(|id| !id.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    zen_ids.into_iter().map(|id| {
            let (free, name) = metadata
                .as_ref()
                .and_then(|meta| meta.get("opencode"))
                .and_then(|provider| provider.get("models"))
                .and_then(|models| models.get(&id))
                .map(|model| (is_free_model(model), display_name_for(model, &id)))
                .unwrap_or_else(|| {
                    (
                        id.ends_with("-free"),
                        id.clone(),
                    )
                });
            OpenCodeModel { model: id, name, free }
        })
        .collect()
}

/// Free means zero input *and* zero output cost in authoritative metadata,
/// matching how the upstream OpenCode client gates anonymous access.
fn is_free_model(entry: &Value) -> bool {
    let cost_zero = |field: &str| match entry.pointer(field) {
        Some(Value::Number(number)) => number.as_f64() == Some(0.0),
        _ => false,
    };
    cost_zero("/cost/input") && cost_zero("/cost/output")
}

fn display_name_for(entry: &Value, fallback: &str) -> String {
    entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}
