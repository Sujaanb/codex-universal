use super::credential_from_auth_store;
use std::fs;
use std::path::PathBuf;

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codex-universal-opencode-test-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn reads_api_key_from_auth_store() {
    let dir = temp_dir("api-key");
    let path = dir.join("auth.json");
    fs::write(
        &path,
        r#"{"opencode": {"type": "api", "key": "test-zen-key"}}"#,
    )
    .expect("write auth store");

    assert_eq!(
        credential_from_auth_store(&path),
        Some("test-zen-key".to_string())
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn ignores_oauth_entries() {
    let dir = temp_dir("oauth");
    let path = dir.join("auth.json");
    fs::write(
        &path,
        r#"{"opencode": {"type": "oauth", "refresh": "r", "access": "a", "expires": 1}}"#,
    )
    .expect("write auth store");

    assert_eq!(credential_from_auth_store(&path), None);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn returns_none_for_missing_file_or_garbage() {
    let dir = temp_dir("missing");
    let missing = dir.join("does-not-exist.json");
    assert_eq!(credential_from_auth_store(&missing), None);

    let garbage = dir.join("garbage.json");
    fs::write(&garbage, "not json at all").expect("write garbage");
    assert_eq!(credential_from_auth_store(&garbage), None);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn never_leaks_key_in_error_paths() {
    // Credential discovery is infallible by design: any failure simply
    // yields None so callers fall back to anonymous access. There is no
    // error type that could carry the key.
    let dir = temp_dir("no-leak");
    let path = dir.join("auth.json");
    fs::write(&path, "{}").expect("write empty store");
    assert_eq!(credential_from_auth_store(&path), None);
    let _ = fs::remove_dir_all(dir);
}
