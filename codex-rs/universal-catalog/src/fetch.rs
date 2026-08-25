// Codex Universal extension: JSON fetch helper with a bounded timeout.

use std::time::Duration;

const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Fetches `url` and parses the body as JSON.
///
/// Returns `None` on any network or parse failure; callers treat that as
/// "no catalog data available".
pub fn fetch_json(
    url: String,
) -> futures::future::BoxFuture<'static, Option<serde_json::Value>> {
    Box::pin(async move {
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .build()
            .ok()?;
        let response = client
            .get(&url)
            .header("accept", "application/json")
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<serde_json::Value>().await.ok()
    })
}
