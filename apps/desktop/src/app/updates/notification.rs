//! What the top bar's update notification (its button, count and list) leaves out: Fast Computer &
//! Browser Use's patch updates, and any item the user dismissed at the version it was dismissed at.
//! Settings > Updates still lists everything.
//!
//! CDXC:Updates 2026-11-16 DECISION:
//! User: "fast computer use is updating too often, please only show it in update notification when
//! it moves by x.1 at least (dont show it for minor updates). also add a button to dismiss an
//! update that removes that notification and user won't be notified about that specific update
//! again on that machine (until newer version comes basically)". Fast Computer & Browser Use
//! (`tool:trycua`, on any computer) is listed only when the available version's major or minor part
//! is higher than the installed one (0.52.3 to 0.52.4 stays out, 0.52.x to 0.53.0 shows). A dismissal
//! is the item's id and the version it was dismissed at, kept in this computer's state folder (not
//! synced); a newer version of that item, or the item running its update, shows it again.
use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::app::helpers::ghostex_state_root;

const FILE_NAME: &str = "gpui-updates-notification-dismissed.json";
/// The item id's tail for Fast Computer & Browser Use (a remote computer's ids are prefixed).
const FAST_COMPUTER_USE_ID: &str = "tool:trycua";

fn file_path() -> PathBuf {
    ghostex_state_root().join(FILE_NAME)
}

/// `(major, minor)` of a version such as `0.52.4` or `v0.52.4-beta`.
fn major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let number = |part: Option<&str>| {
        let digits: String = part?.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().ok()
    };
    Some((number(parts.next())?, number(parts.next())?))
}

/// The version a dismissal is kept against: the version on offer, else the state (an item without
/// one, such as "Needs you").
fn dismissal_key(item: &Value) -> String {
    item["latest"]
        .as_str()
        .filter(|latest| !latest.is_empty())
        .or(item["state"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// Whether the notification should leave `item` out.
pub(super) fn hidden_from_notification(item: &Value, dismissed: &HashMap<String, String>) -> bool {
    let id = item["id"].as_str().unwrap_or_default();
    if item["state"] == "available" && id.ends_with(FAST_COMPUTER_USE_ID) {
        let installed = item["installed"].as_str().and_then(major_minor);
        let latest = item["latest"].as_str().and_then(major_minor);
        if matches!((installed, latest), (Some(installed), Some(latest)) if latest <= installed) {
            return true;
        }
    }
    item["state"] != "updating"
        && dismissed
            .get(id)
            .is_some_and(|version| *version == dismissal_key(item))
}

/// Records that `item` is dismissed at the version it shows now.
pub(super) fn dismiss(item: &Value, dismissed: &mut HashMap<String, String>) {
    if let Some(id) = item["id"].as_str() {
        dismissed.insert(id.to_string(), dismissal_key(item));
        save(dismissed);
    }
}

pub(super) fn load() -> HashMap<String, String> {
    std::fs::read_to_string(file_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(super) fn save(dismissed: &HashMap<String, String>) {
    let path = file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(dismissed) {
        let _ = std::fs::write(path, text);
    }
}
