// Codex Universal extension: time-limited cache for provider catalog payloads.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

struct Entry {
    value: serde_json::Value,
    stored_at: Instant,
}

fn entries() -> &'static Mutex<HashMap<String, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(Mutex::default)
}

/// Returns the cached payload for `key` if it is younger than `ttl`.
pub fn get(key: &str, ttl: Duration) -> Option<serde_json::Value> {
    let map = entries().lock().ok()?;
    let entry = map.get(key)?;
    if entry.stored_at.elapsed() < ttl {
        Some(entry.value.clone())
    } else {
        None
    }
}

/// Stores a payload for `key`, leaving any existing (possibly stale) value
/// untouched when `value` is `Null` so network failures degrade gracefully.
pub fn put(key: &str, value: serde_json::Value) {
    if value.is_null() {
        return;
    }
    if let Ok(mut map) = entries().lock() {
        map.insert(
            key.to_string(),
            Entry {
                value,
                stored_at: Instant::now(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_within_ttl_and_ignores_null() {
        let key = "unit-test-key";
        assert_eq!(get(key, Duration::from_secs(60)), None);
        put(key, json!({"ok": true}));
        assert_eq!(get(key, Duration::from_secs(60)), Some(json!({"ok": true})));
        put(key, serde_json::Value::Null);
        // Null never overwrites a good entry.
        assert_eq!(get(key, Duration::from_secs(60)), Some(json!({"ok": true})));
        assert_eq!(get(key, Duration::from_secs(0)), None);
    }
}
