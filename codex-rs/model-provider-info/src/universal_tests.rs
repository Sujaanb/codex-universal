use crate::universal::encode_model_identity;
use crate::universal::split_model_identity;
use pretty_assertions::assert_eq;

#[test]
fn encodes_and_decodes_composite_identities() {
    let encoded = encode_model_identity("openrouter", "deepseek/deepseek-r1");
    assert_eq!(encoded, "openrouter::deepseek/deepseek-r1");

    assert_eq!(
        split_model_identity(&encoded),
        Some(crate::universal::ModelIdentity {
            provider_id: "openrouter".to_string(),
            model: "deepseek/deepseek-r1".to_string(),
        })
    );
}

#[test]
fn plain_slugs_do_not_decode() {
    assert_eq!(split_model_identity("gpt-5.3-codex"), None);
    assert_eq!(split_model_identity("x-preview-f-free"), None);
    assert_eq!(split_model_identity(""), None);
    // A bare separator is not a composite identity.
    assert_eq!(split_model_identity("::"), None);
    assert_eq!(split_model_identity("::model"), None);
}

#[test]
fn non_provider_prefixes_are_rejected() {
    // Uppercase / spaces / slashes cannot be provider keys, so these slugs are
    // treated as plain model names.
    assert_eq!(split_model_identity("Some::thing"), None);
    assert_eq!(split_model_identity("a/b::c"), None);
}

#[test]
fn opencode_free_model_round_trips() {
    let slug = encode_model_identity("opencode", "x-preview-f-free");
    let identity = split_model_identity(&slug).expect("composite identity");
    assert_eq!(identity.model, "x-preview-f-free");
    assert_eq!(identity.to_string(), "opencode::x-preview-f-free");
}
