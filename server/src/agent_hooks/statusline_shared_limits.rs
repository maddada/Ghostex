//! The newest 5h / 7d reading from any Claude session on one login.
//!
//! CDXC:AgentProviders 2026-11-09 WHY:
//! GitHub #514: without a saved account a session's limits are the numbers its own Claude last piped to the statusline command. A coordinator that sits idle for hours reports nothing new, so its chat kept showing the percentages from before the window reset even while other sessions on the same login had reported fresh ones. The limits belong to the login, not to the session, so a session's reading takes each window from the newest payload any session on its login stored, when that payload is newer than its own.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::Value;

use super::statusline::{
    read_statusline_payload_file, ClaudeStatuslinePayload, CLAUDE_STATUSLINE_STATE_DIRECTORY,
};

const RATE_LIMIT_WINDOWS: [&str; 2] = ["five_hour", "seven_day"];

/// The longest window is 7 days; a payload older than that cannot carry a reading worth taking.
const NEWER_PAYLOAD_MAX_AGE: Duration = Duration::from_secs(8 * 24 * 60 * 60);

/// The login a payload's session ran on: the config folder its transcript lives in (the folder that holds `projects/`) names it.
fn payload_login(
    payload: &serde_json::Map<String, Value>,
    home: &Path,
    cache: &mut HashMap<PathBuf, Option<String>>,
) -> Option<String> {
    let transcript = Path::new(payload.get("transcript_path")?.as_str()?);
    if !transcript.is_absolute() {
        return None;
    }
    let root = transcript
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "projects"))?
        .parent()?
        .to_path_buf();
    cache
        .entry(root.clone())
        .or_insert_with(|| {
            crate::accounts::session_identity::login_identity(
                crate::accounts::model::Provider::Claude,
                &root,
                home,
            )
        })
        .clone()
}

fn window_reading(payload: &serde_json::Map<String, Value>, key: &str) -> Option<(f64, f64)> {
    let window = payload.get("rate_limits")?.get(key)?;
    let used = window.get("used_percentage")?.as_f64()?;
    let resets_at = window.get("resets_at")?.as_f64()?;
    (used.is_finite() && resets_at.is_finite()).then_some((used, resets_at))
}

/// Replaces `own`'s 5h / 7d windows with the newest same-login reading, window by window. Only payload files written after `own` are read, so the session that reported last (the usual case) costs a directory listing.
pub(crate) fn adopt_same_login_rate_limits(
    hook_state_directory: &Path,
    own: &mut ClaudeStatuslinePayload,
) {
    let Some(own_modified) = own.modified else {
        return;
    };
    let now = SystemTime::now();
    let Ok(entries) = fs::read_dir(hook_state_directory.join(CLAUDE_STATUSLINE_STATE_DIRECTORY))
    else {
        return;
    };
    let mut newer: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok()?;
            let fresh = now
                .duration_since(modified)
                .map_or(true, |age| age <= NEWER_PAYLOAD_MAX_AGE);
            (modified > own_modified && fresh).then_some((modified, path))
        })
        .collect();
    if newer.is_empty() {
        return;
    }
    let home = ghostex_paths::user_home_dir();
    let mut logins = HashMap::new();
    let Some(own_login) = payload_login(&own.payload, &home, &mut logins) else {
        return;
    };
    newer.sort_by(|left, right| right.0.cmp(&left.0));
    let mut pending: Vec<&str> = RATE_LIMIT_WINDOWS.to_vec();
    for (_, path) in newer {
        if pending.is_empty() {
            break;
        }
        let Some(candidate) = read_statusline_payload_file(&path) else {
            continue;
        };
        let readings: Vec<(&str, (f64, f64))> = pending
            .iter()
            .filter_map(|key| Some((*key, window_reading(&candidate.payload, key)?)))
            .collect();
        if readings.is_empty()
            || payload_login(&candidate.payload, &home, &mut logins).as_deref()
                != Some(own_login.as_str())
        {
            continue;
        }
        for (key, (used, resets_at)) in readings {
            // A window only moves forward: a reading of an earlier window than the one this session already knows is not newer news.
            if window_reading(&own.payload, key).is_some_and(|(_, known)| resets_at < known) {
                pending.retain(|pending_key| *pending_key != key);
                continue;
            }
            let Some(limits) = own
                .payload
                .entry("rate_limits")
                .or_insert_with(|| Value::Object(Default::default()))
                .as_object_mut()
            else {
                continue;
            };
            limits.insert(
                key.to_string(),
                serde_json::json!({"used_percentage": used, "resets_at": resets_at}),
            );
            pending.retain(|pending_key| *pending_key != key);
        }
    }
}
