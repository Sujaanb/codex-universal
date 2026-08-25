use std::sync::Arc;
use std::time::Duration;

use codex_app_server_protocol::Model;
use codex_app_server_protocol::ModelServiceTier;
use codex_app_server_protocol::ModelUpgradeInfo;
use codex_app_server_protocol::ReasoningEffortOption;
use codex_core::config::Config;
use codex_core::ThreadManager;
use codex_http_client::HttpClientFactory;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;

/// Cache lifetime for third-party provider catalogs.
const UNIVERSAL_CATALOG_TTL: Duration = Duration::from_secs(300);

pub async fn supported_models(
    thread_manager: Arc<ThreadManager>,
    include_hidden: bool,
    http_client_factory: HttpClientFactory,
) -> Vec<Model> {
    thread_manager
        .list_models(RefreshStrategy::OnlineIfUncached, http_client_factory)
        .await
        .into_iter()
        .filter(|preset| include_hidden || preset.show_in_picker)
        .map(model_from_preset)
        .collect()
}

// Codex Universal extension
pub async fn supported_models_with_universal(
    thread_manager: Arc<ThreadManager>,
    include_hidden: bool,
    http_client_factory: HttpClientFactory,
    config: &Config,
) -> Vec<Model> {
    let mut models = supported_models(thread_manager, include_hidden, http_client_factory).await;
    models.extend(universal_models(config).await);
    models
}

/// Codex Universal extension: builds picker entries for every model exposed by
/// the configured universal providers (OpenRouter, OpenCode Zen, ...).
///
/// Catalog fetch failures never fail the request; the provider simply
/// contributes no entries.
pub async fn universal_models(config: &Config) -> Vec<Model> {
    let entries =
        codex_universal_catalog::collect_universal_models(&config.model_providers, UNIVERSAL_CATALOG_TTL)
            .await;
    entries.into_iter().map(universal_model_from_entry).collect()
}

fn universal_model_from_entry(entry: codex_universal_catalog::UniversalModel) -> Model {
    Model {
        id: entry.id.clone(),
        model: entry.id,
        upgrade: None,
        upgrade_info: None,
        availability_nux: None,
        display_name: entry.display_name,
        description: entry.description.unwrap_or_default(),
        model_specialty: None,
        hidden: false,
        supported_reasoning_efforts: Vec::new(),
        default_reasoning_effort: ReasoningEffort::Medium,
        input_modalities: codex_protocol::openai_models::default_input_modalities(),        supports_personality: false,
        multi_agent_version: None,
        additional_speed_tiers: Vec::new(),
        service_tiers: Vec::new(),
        default_service_tier: None,
        is_default: false,
    }
}

fn model_from_preset(preset: ModelPreset) -> Model {
    Model {
        id: preset.id.to_string(),
        model: preset.model.to_string(),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: preset.upgrade.as_ref().map(|upgrade| ModelUpgradeInfo {
            model: upgrade.id.clone(),
            upgrade_copy: upgrade.upgrade_copy.clone(),
            model_link: upgrade.model_link.clone(),
            migration_markdown: upgrade.migration_markdown.clone(),
            retirement_at: upgrade
                .retirement_at
                .as_ref()
                .map(chrono::DateTime::timestamp),
        }),
        availability_nux: preset.availability_nux.map(Into::into),
        display_name: preset.display_name.to_string(),
        description: preset.description.to_string(),
        model_specialty: preset.model_specialty,
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: reasoning_efforts_from_preset(
            preset.supported_reasoning_efforts,
        ),
        default_reasoning_effort: preset.default_reasoning_effort,
        input_modalities: preset.input_modalities,
        supports_personality: preset.supports_personality,
        multi_agent_version: preset.multi_agent_version.map(Into::into),
        additional_speed_tiers: preset.additional_speed_tiers,
        service_tiers: preset
            .service_tiers
            .into_iter()
            .map(|service_tier| ModelServiceTier {
                id: service_tier.id,
                name: service_tier.name,
                description: service_tier.description,
            })
            .collect(),
        default_service_tier: preset.default_service_tier,
        is_default: preset.is_default,
    }
}

fn reasoning_efforts_from_preset(
    efforts: Vec<ReasoningEffortPreset>,
) -> Vec<ReasoningEffortOption> {
    efforts
        .into_iter()
        .map(|preset| ReasoningEffortOption {
            reasoning_effort: preset.effort,
            description: preset.description,
        })
        .collect()
}
