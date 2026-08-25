use super::openrouter::parse_openrouter_models;
use super::opencode::opencode_catalog;
use serde_json::json;

fn fetcher_from_bodies(
    bodies: Vec<(String, serde_json::Value)>,
) -> super::JsonFetcher {
    let bodies = std::sync::Arc::new(bodies.into_iter().collect::<std::collections::HashMap<_, _>>());
    std::sync::Arc::new(move |url| {
        let bodies = std::sync::Arc::clone(&bodies);
        Box::pin(async move { bodies.get(&url).cloned() })
    })
}

#[test]
fn parses_openrouter_models_and_free_metadata() {
    let payload = json!({
        "data": [
            {
                "id": "deepseek/deepseek-r1",
                "name": "DeepSeek R1",
                "description": "reasoning model",
                "context_length": 128000,
                "pricing": {"prompt": "0.0005", "completion": "0.002"}
            },
            {
                "id": "meta/llama-3-free",
                "name": "Llama 3 Free",
                "context_length": 8192,
                "pricing": {"prompt": "0", "completion": "0"}
            }
        ]
    });

    let models = parse_openrouter_models(&payload);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].model, "deepseek/deepseek-r1");
    assert!(!models[0].free);
    assert!(models[1].free);
}

#[test]
fn openrouter_garbage_payload_is_empty() {
    assert!(parse_openrouter_models(&json!({"error": "nope"})).is_empty());
}

#[tokio::test]
async fn opencode_catalog_marks_free_models() {
    let fetcher = fetcher_from_bodies(vec![
        (
            super::opencode::OPENCODE_ZEN_MODELS_URL.to_string(),
            json!({
                "object": "list",
                "data": [
                    {"id": "x-preview-f-free"},
                    {"id": "mimo-v2.5-free"},
                    {"id": "claude-opus-4-6"}
                ]
            }),
        ),
        (
            super::opencode::OPENCODE_MODELS_CATALOG_URL.to_string(),
            json!({
                "opencode": {
                    "models": {
                        "x-preview-f-free": {"name": "Ox Alpha Free", "cost": {"input": 0, "output": 0}},
                        "claude-opus-4-6": {"name": "Claude Opus", "cost": {"input": 5, "output": 25}}
                    }
                }
            }),
        ),
    ]);

    let catalog = opencode_catalog(&fetcher).await;
    assert_eq!(catalog.len(), 3);

    let ox = catalog.iter().find(|m| m.model == "x-preview-f-free").expect("ox");
    assert!(ox.free);
    assert_eq!(ox.name, "Ox Alpha Free");

    // Metadata unavailable for this id: falls back to the `-free` suffix.
    let mimo = catalog.iter().find(|m| m.model == "mimo-v2.5-free").expect("mimo");
    assert!(mimo.free);

    let paid = catalog.iter().find(|m| m.model == "claude-opus-4-6").expect("paid");
    assert!(!paid.free);
}

#[tokio::test]
async fn collect_returns_empty_without_providers_or_network() {
    use super::collect_with_fetcher;
    use codex_model_provider_info::built_in_model_providers;
    use std::time::Duration;

    let providers = built_in_model_providers(None);
    // Offline fetcher returns None everywhere; collection must not fail.
    let none_fetcher: super::JsonFetcher = std::sync::Arc::new(|_url| {
        Box::pin(async move { None })
    });
    let models = collect_with_fetcher(&providers, Duration::from_secs(1), none_fetcher).await;
    assert!(models.is_empty());
}

#[tokio::test]
async fn live_openrouter_fetch_diagnostics() {
    let body = reqwest::get(super::openrouter::OPENROUTER_MODELS_URL).await;
    match body {
        Err(e) => println!("FETCH ERR: {e}"),
        Ok(resp) => {
            println!("STATUS: {}", resp.status());
            let json = resp.json::<serde_json::Value>().await;
            match json {
                Err(e) => println!("PARSE ERR: {e}"),
                Ok(v) => {
                    let models = super::openrouter::parse_openrouter_models(&v);
                    println!("PARSED MODELS: {}", models.len());
                    if let Some(first) = models.first() {
                        println!("FIRST: {} free={}", first.model, first.free);
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn live_collect_diagnostics() {
    use std::collections::HashMap;
    let providers = codex_model_provider_info::built_in_model_providers(None);
    println!("provider ids: {:?}", providers.keys().collect::<Vec<_>>());
    let models =
        super::collect_with_fetcher(&providers, std::time::Duration::from_secs(1), super::default_fetcher()).await;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in &models {
        *counts.entry(m.id.split("::").next().unwrap().to_string()).or_default() += 1;
    }
    println!("counts: {counts:?}");
}

#[tokio::test]
async fn live_fetch_json_diagnostics() {
    let value = (super::default_fetcher())(super::openrouter::OPENROUTER_MODELS_URL.to_string()).await;
    match value {
        None => println!("FETCH_JSON: None"),
        Some(v) => {
            let models = super::openrouter::parse_openrouter_models(&v);
            println!("FETCH_JSON parsed: {}", models.len());
        }
    }
}
