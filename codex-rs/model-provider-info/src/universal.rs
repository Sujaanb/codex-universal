// Codex Universal extension: composite provider+model identities.
//
// The closed-source Codex Desktop picker only round-trips a single model
// string. To let it select models from any configured provider, this fork
// encodes provider identity into the model slug as `<provider>::<model>`
// (e.g. `openrouter::deepseek/deepseek-r1`, `opencode::x-preview-f-free`).
//
// The separator is deliberately distinct from OpenRouter's `/` so real model
// identifiers survive unchanged, and decoding only applies when the prefix
// matches a plausible provider id.

use std::fmt;

/// Separator between provider id and model id in a composite model slug.
pub const MODEL_IDENTITY_SEPARATOR: &str = "::";

/// A decoded composite model identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIdentity {
    pub provider_id: String,
    pub model: String,
}

impl fmt::Display for ModelIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{provider}{sep}{model}",
            provider = self.provider_id,
            sep = MODEL_IDENTITY_SEPARATOR,
            model = self.model
        )
    }
}

/// Encodes a provider id and model into a composite model slug.
pub fn encode_model_identity(provider_id: &str, model: &str) -> String {
    format!("{provider_id}{MODEL_IDENTITY_SEPARATOR}{model}")
}

/// Decodes a model slug into its provider/model parts when it uses the
/// composite form.
///
/// Returns `None` for plain slugs. The provider part must look like a Codex
/// provider key (lowercase ASCII letters, digits, `-`, `_`) so ordinary model
/// names containing `::` are never misinterpreted.
pub fn split_model_identity(slug: &str) -> Option<ModelIdentity> {
    let (provider, model) = slug.split_once(MODEL_IDENTITY_SEPARATOR)?;
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    if !provider
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return None;
    }
    Some(ModelIdentity {
        provider_id: provider.to_string(),
        model: model.to_string(),
    })
}

#[cfg(test)]
#[path = "universal_tests.rs"]
mod tests;
