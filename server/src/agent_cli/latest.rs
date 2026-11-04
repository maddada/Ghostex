use super::catalog::Definition;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

const FOUND_TTL: Duration = Duration::from_secs(60 * 60);
const MISSING_TTL: Duration = Duration::from_secs(5 * 60);

static CACHE: LazyLock<Mutex<HashMap<String, (Instant, Option<String>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// CDXC:AgentProviders 2026-09-28 WHY:
/// "Update available" compares the installed version with the vendor's own release channel (`latestVersion` in the catalog), because npm lags behind the native installers that Ghostex now installs by default. Agents without a channel fall back to their npm package; agents with neither (Cursor's dated builds) never claim an update. Answers are cached for an hour so opening Settings does not hit every vendor.
///
/// CDXC:AgentProviders 2026-10-17 WHY:
/// Antigravity CLI has no npm package; its installers (`install.sh`, `install.ps1`) read the version from the auto-updater's per-platform manifest (`manifests/<os>_<arch>.json`), and every platform's manifest names the same release, so the catalog reads one of them.
pub(crate) async fn latest(definition: &'static Definition) -> Option<String> {
    let (url, field) = match (&definition.latest_version, &definition.npm_package) {
        (Some(source), _) => (source.url.clone(), source.json_field.clone()),
        (None, Some(package)) => (
            format!("https://registry.npmjs.org/{package}/latest"),
            Some("version".to_string()),
        ),
        (None, None) => return None,
    };
    if let Some((at, value)) = CACHE.lock().ok()?.get(&definition.agent_id).cloned() {
        let ttl = if value.is_some() {
            FOUND_TTL
        } else {
            MISSING_TTL
        };
        if at.elapsed() < ttl {
            return value;
        }
    }
    let value = tokio::task::spawn_blocking(move || fetch(&url, field.as_deref()))
        .await
        .ok()
        .flatten();
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(definition.agent_id.clone(), (Instant::now(), value.clone()));
    }
    value
}

/// The version npm's `latest` tag names for `package`, from a blocking context, cached like
/// [`latest`] (keyed by package, so it never shares an agent's entry).
pub(crate) fn npm_latest_blocking(package: &str) -> Option<String> {
    let key = format!("npm:{package}");
    if let Some((at, value)) = CACHE.lock().ok()?.get(&key).cloned() {
        let ttl = if value.is_some() {
            FOUND_TTL
        } else {
            MISSING_TTL
        };
        if at.elapsed() < ttl {
            return value;
        }
    }
    let value = fetch(
        &format!("https://registry.npmjs.org/{package}/latest"),
        Some("version"),
    );
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, (Instant::now(), value.clone()));
    }
    value
}

fn fetch(url: &str, field: Option<&str>) -> Option<String> {
    let agent: ureq::Agent = crate::http_agent::config()
        .timeout_global(Some(Duration::from_secs(4)))
        .proxy(ureq::Proxy::try_from_env())
        .build()
        .into();
    let body = agent
        .get(url)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let text = match field {
        Some(field) => serde_json::from_str::<Value>(&body)
            .ok()?
            .get(field)?
            .as_str()?
            .to_string(),
        None => body,
    };
    version_in(&text)
}

/// The first `major.minor.patch` (with any pre-release suffix) in a version line such as
/// `2.1.283 (Claude Code)`, `codex-cli 0.157.1` or `rust-v0.157.1`.
pub(crate) fn version_in(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if bytes[start].is_ascii_digit() && (start == 0 || !bytes[start - 1].is_ascii_digit()) {
            let rest = &text[start..];
            let end = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
                .unwrap_or(rest.len());
            let candidate = rest[..end].trim_end_matches(['.', '-']);
            if numbers(candidate).len() >= 3 {
                return Some(candidate.to_string());
            }
        }
        start += 1;
    }
    None
}

/// True when `latest` is a strictly higher release than `current`. Pre-release suffixes are ignored, so a
/// build ahead of the channel never shows as outdated.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let latest = numbers(latest);
    let current = numbers(current);
    if latest.len() < 3 || current.len() < 3 {
        return false;
    }
    latest[..3] > current[..3]
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .split('-')
        .next()
        .unwrap_or_default()
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect()
}
