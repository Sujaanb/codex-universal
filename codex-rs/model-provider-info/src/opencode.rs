// Codex Universal extension: read-only discovery of OpenCode credentials.
//
// OpenCode stores provider credentials in an `auth.json` file located in its
// XDG data directory:
//
//   - Linux/macOS: `~/.local/share/opencode/auth.json`
//   - Windows:     `%USERPROFILE%\.local\share\opencode\auth.json`
//   - override:    `XDG_DATA_HOME` environment variable
//
// The relevant entry looks like:
//
//   { "opencode": { "type": "api", "key": "<zen api key>" } }
//
// This module only ever *reads* that file and returns the key as a plain
// String that callers must immediately wrap in a redacted type. Keys are
// never logged, serialized into errors, or written back to disk.

use std::path::PathBuf;

/// Environment variable that overrides the XDG data home directory.
const XDG_DATA_HOME_ENV_VAR: &str = "XDG_DATA_HOME";

/// Result of credential discovery: the API key to present to OpenCode Zen.
pub(crate) fn discover_credential() -> String {
    if let Some(key) = credential_from_env() {
        return key;
    }

    if let Some(key) =
        credential_from_auth_store(&default_auth_store_path().unwrap_or_default())
    {
        return key;
    }

    // Anonymous access. OpenCode Zen accepts this for free (zero-cost)
    // models; paid models will fail with 401.
    super::OPENCODE_ANONYMOUS_API_KEY.to_string()
}

fn credential_from_env() -> Option<String> {
    std::env::var(super::OPENCODE_API_KEY_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Default location of OpenCode's `auth.json`, honoring `XDG_DATA_HOME`.
fn default_auth_store_path() -> Option<PathBuf> {
    let data_home = match std::env::var(XDG_DATA_HOME_ENV_VAR) {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => dirs::home_dir()?.join(".local").join("share"),
    };
    Some(data_home.join("opencode").join("auth.json"))
}

/// Reads the `opencode` API credential from an auth store file.
///
/// Returns `None` when the file does not exist, cannot be parsed, or has no
/// usable `opencode` entry. All failures are silent by design: absence of
/// OpenCode must never break other providers.
fn credential_from_auth_store(path: &std::path::Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let entry = value.get("opencode")?;
    match entry.get("type").and_then(serde_json::Value::as_str) {
        Some("api") | Some("wellknown") => entry
            .get("key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(ToString::to_string),
        // OAuth-style entries are account sessions for other providers and
        // are not valid Zen API keys.
        _ => None,
    }
}

#[cfg(test)]
#[path = "opencode_tests.rs"]
mod tests;
