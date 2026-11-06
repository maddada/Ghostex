use super::{latest, mise::HeldBack, process};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

/// CDXC:AgentProviders 2026-11-06 WHY:
/// npm's `min-release-age` setting (days) makes `npm install -g pkg@latest` keep the newest release old
/// enough and still exit 0, so Gemini's update "finished" at 0.65.0 while 0.66.0 and 0.67.0 were
/// younger than the user's 10 days, and the card called it failed. Like mise's minimum_release_age
/// (mise.rs `held_back`) it is the user's safety choice and Ghostex never overrides it: the state says
/// which release npm installs now, or that npm holds every newer release back and until when.
/// `npm` is the command the update runs npm with (`npm`, or `mise exec 'node@26.12.1' -- npm`).
pub(crate) async fn held_back(
    npm: &str,
    package: &str,
    installed: &str,
    latest_version: &str,
    home: &Path,
) -> Option<HeldBack> {
    static CACHE: LazyLock<Mutex<HashMap<(String, String, String), (Instant, Option<HeldBack>)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let key = (
        npm.to_string(),
        package.to_string(),
        latest_version.to_string(),
    );
    if let Some((_, held)) = CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .filter(|(checked, _)| checked.elapsed() < Duration::from_secs(600))
    {
        return held;
    }
    let held = compute(npm, package, installed, home).await?;
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, (Instant::now(), held.clone()));
    }
    held
}

/// `None` when npm could not be asked (not cached, so the next read asks again).
async fn compute(
    npm: &str,
    package: &str,
    installed: &str,
    home: &Path,
) -> Option<Option<HeldBack>> {
    let setting = output(&format!("{npm} config get min-release-age"), home).await?;
    let days: f64 = match setting
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
    {
        Some(value) => match value.parse() {
            Ok(days) if days > 0.0 => days,
            _ => return Some(None),
        },
        None => return Some(None),
    };
    let times = output(
        &format!("{npm} view {} time --json", process::quote(package)),
        home,
    )
    .await?;
    let json = &times[times.find('{')?..=times.rfind('}')?];
    let times: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json).ok()?;
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds((days * 86_400.0) as i64);
    let published = |version: &str| {
        times
            .get(version)
            .and_then(|time| time.as_str())
            .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
            .map(|time| time.with_timezone(&chrono::Utc))
    };
    let releases = times
        .keys()
        .filter(|version| !version.contains('-') && latest::version_in(version).is_some());
    let newest_eligible = releases
        .clone()
        .filter(|version| published(version).is_some_and(|time| time <= cutoff))
        .fold(None::<&String>, |best, version| match best {
            Some(best) if !latest::is_newer(version, best) => Some(best),
            _ => Some(version),
        });
    // The oldest release npm is still waiting on, newer than what it can install now.
    let floor = newest_eligible.map_or(installed, String::as_str);
    let first_held = releases
        .filter(|version| latest::is_newer(version, floor))
        .filter_map(|version| Some((version, published(version)?)))
        .filter(|(_, time)| *time > cutoff)
        .min_by_key(|(_, time)| *time)?;
    let eligible = first_held.1 + (chrono::Utc::now() - cutoff);
    let until = eligible
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M");
    let age = if days.fract() == 0.0 {
        format!("{days:.0} days")
    } else {
        format!("{days} days")
    };
    let installable = newest_eligible
        .filter(|version| latest::is_newer(version, installed))
        .cloned();
    let message = match &installable {
        Some(version) => format!(
            "npm can update to {version} now; it holds back {} until {until} because of your npm min-release-age setting ({age}). Ghostex does not change this setting.",
            first_held.0
        ),
        None => format!(
            "npm holds back {} until {until} because of your npm min-release-age setting ({age}). Update again after that; Ghostex does not change this setting.",
            first_held.0
        ),
    };
    Some(Some(HeldBack {
        message,
        installable,
    }))
}

async fn output(script: &str, home: &Path) -> Option<String> {
    let text = Arc::new(Mutex::new(String::new()));
    let capture = text.clone();
    process::run(
        script,
        home,
        &std::collections::BTreeMap::new(),
        Duration::from_secs(10),
        move |chunk| {
            if let Ok(mut text) = capture.lock() {
                if text.len() < 4 * 1024 * 1024 {
                    text.push_str(&chunk);
                }
            }
        },
    )
    .await
    .ok()?;
    let text = text.lock().ok()?.clone();
    Some(text)
}
