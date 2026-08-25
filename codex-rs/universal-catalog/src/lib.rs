// Codex Universal extension: dynamic model catalogs for third-party providers.
//!
//! Fetches and caches provider model listings (OpenRouter, OpenCode Zen, and
//! any OpenAI-compatible `/models` endpoint) so the unmodified Codex Desktop
//! picker can offer models from every configured provider. All network
//! failures are non-fatal: a provider that is offline simply contributes no
//! entries.

mod cache;
mod fetch;
mod openrouter;
mod opencode;

use codex_model_provider_info::ModelProviderInfo;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Maximum number of catalog entries contributed by a single provider.
const MAX_MODELS_PER_PROVIDER: usize = 500;

/// One model offered by a universal provider, already encoded with its
/// provider identity for round-tripping through the Desktop picker.
#[derive(Debug, Clone, PartialEq)]
pub struct UniversalModel {
    /// Composite identity (`provider::model`) to use as the Codex model slug.
    pub id: String,
    /// Human-facing name shown in pickers, e.g. `[OpenRouter] DeepSeek R1`.
    pub display_name: String,
    pub description: Option<String>,
    pub context_window: Option<u64>,
    /// Whether the provider metadata indicates zero-cost usage.
    pub free: bool,
}

impl UniversalModel {
    pub fn sort_key(&self) -> (std::cmp::Reverse<bool>, String) {
        // Free models first, then alphabetical by display name.
        (std::cmp::Reverse(self.free), self.display_name.clone())
    }
}

/// Fetches a URL and returns the parsed JSON body, or `None` on any failure.
pub type JsonFetcher =
    Arc<dyn Fn(String) -> futures::future::BoxFuture<'static, Option<serde_json::Value>> + Send + Sync>;

/// Collects catalog entries for all supported universal providers in
/// `model_providers`, using `cache_ttl`-limited caching per endpoint.
pub async fn collect_universal_models(
    model_providers: &HashMap<String, ModelProviderInfo>,
    cache_ttl: Duration,
) -> Vec<UniversalModel> {
    collect_with_fetcher(model_providers, cache_ttl, default_fetcher()).await
}

pub fn default_fetcher() -> JsonFetcher {
    Arc::new(|url| Box::pin(fetch::fetch_json(url)))
}

pub async fn collect_with_fetcher(
    model_providers: &HashMap<String, ModelProviderInfo>,
    cache_ttl: Duration,
    fetcher: JsonFetcher,
) -> Vec<UniversalModel> {
    let mut models = Vec::new();

    if model_providers.contains_key(codex_model_provider_info::OPENROUTER_PROVIDER_ID) {
        let payload =
            cached_fetch("openrouter", openrouter::OPENROUTER_MODELS_URL, cache_ttl, &fetcher).await;
        for entry in openrouter::parse_openrouter_models(&payload)
            .into_iter()
            .take(MAX_MODELS_PER_PROVIDER)
        {
            models.push(UniversalModel {
                id: encode("openrouter", &entry.model),
                display_name: format!("[OpenRouter] {}", entry.name),
                description: entry.description.clone(),
                context_window: entry.context_window,
                free: entry.free,
            });
        }
    }

    if model_providers.contains_key(codex_model_provider_info::OPENCODE_PROVIDER_ID) {
        for entry in opencode::opencode_catalog(&fetcher).await.into_iter().take(MAX_MODELS_PER_PROVIDER) {
            models.push(UniversalModel {
                id: encode("opencode", &entry.model),
                display_name: format!("[OpenCode] {}", entry.name),
                description: None,
                context_window: None,
                free: entry.free,
            });
        }
    }

    models.sort_by_key(UniversalModel::sort_key);
    models
}

fn encode(provider_id: &str, model: &str) -> String {
    codex_model_provider_info::universal::encode_model_identity(provider_id, model)
}

async fn cached_fetch(
    key: &str,
    url: &str,
    ttl: Duration,
    fetcher: &JsonFetcher,
) -> serde_json::Value {
    if let Some(value) = cache::get(key, ttl) {
        return value;
    }
    let value = (fetcher)(url.to_string()).await.unwrap_or(serde_json::Value::Null);
    cache::put(key, value.clone());
    value
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
