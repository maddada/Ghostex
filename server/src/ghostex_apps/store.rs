//! Where apps live: `<gxserver state>/ghostex-apps/<id>/` with `app.json` (the record),
//! `versions/<n>.html` (each version as published), `icon.svg` or `icon.png`, and `data.json` (what
//! the app saved for itself through `ghostex.app.data`).
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! User: an app is not a Live page ("i dont want to overload the term again"); people ask for an app, a presentation or a dashboard, get it built in HTML, open it like a Live page and can install it. So an app is its own kind with its own folder, away from `visual-pages/`: its id never changes, publishing the same file again adds the next version to the same app (the app always serves its newest), and it is never deleted on its own (the 90-day sweep of Live pages does not reach it). Only an explicit delete or uninstall removes it.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rand::TryRng;
use serde_json::{json, Map, Value};

use super::{icon::IconKind, manifest::Power};

/// The largest prepared app gxserver stores, like a Live page.
pub(crate) const APP_MAX_BYTES: usize = crate::visual_pages::VISUAL_PAGE_MAX_BYTES;
/// App names are labels in the Dock, Settings and the card.
pub(super) const APP_NAME_MAX_CHARS: usize = 80;
const SESSION_REF_MAX_CHARS: usize = 200;
/// How many versions keep their HTML; older ones keep only their line in the record.
const VERSIONS_KEPT: u64 = 20;
const APPS_DIR_NAME: &str = "ghostex-apps";

/// Everything an app may keep through `ghostex.app.data`, all keys together.
pub(super) const DATA_MAX_BYTES: usize = 1024 * 1024;
pub(super) const DATA_MAX_KEYS: usize = 256;
pub(super) const DATA_KEY_MAX_CHARS: usize = 128;

/// Held while an app's record or data is read and rewritten.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn apps_dir(state_dir: &Path) -> PathBuf {
    state_dir.join(APPS_DIR_NAME)
}

fn app_dir(state_dir: &Path, id: &str) -> PathBuf {
    apps_dir(state_dir).join(id)
}

fn record_path(state_dir: &Path, id: &str) -> PathBuf {
    app_dir(state_dir, id).join("app.json")
}

fn version_path(state_dir: &Path, id: &str, version: u64) -> PathBuf {
    app_dir(state_dir, id)
        .join("versions")
        .join(format!("{version}.html"))
}

fn data_path(state_dir: &Path, id: &str) -> PathBuf {
    app_dir(state_dir, id).join("data.json")
}

/// A new app id: 128 random bits as 32 lowercase hex characters, like a Live page's.
fn new_app_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system's random generator failed");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn is_app_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn clip_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Replaces a file whole (written beside it, then renamed), so a reader never sees half.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path))
}

pub(super) fn read_record(state_dir: &Path, id: &str) -> Option<Value> {
    if !is_app_id(id) {
        return None;
    }
    let bytes = fs::read(record_path(state_dir, id)).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
}

fn write_record(state_dir: &Path, id: &str, record: &Value) -> std::io::Result<()> {
    write_whole(
        &record_path(state_dir, id),
        &serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
}

/// Every app's record, newest change first.
pub(super) fn list_records(state_dir: &Path) -> Vec<Value> {
    let Ok(entries) = fs::read_dir(apps_dir(state_dir)) else {
        return Vec::new();
    };
    let mut records: Vec<Value> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            read_record(state_dir, &id)
        })
        .collect();
    records.sort_by(|a, b| {
        let at = |record: &Value| {
            record
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        at(b).cmp(&at(a))
    });
    records
}

/// The newest version's HTML.
pub(super) fn read_current_html(state_dir: &Path, id: &str) -> Option<Vec<u8>> {
    let record = read_record(state_dir, id)?;
    let version = record.get("version").and_then(Value::as_u64)?;
    fs::read(version_path(state_dir, id, version)).ok()
}

/// The app's icon file and its kind.
pub(super) fn read_icon(state_dir: &Path, id: &str) -> Option<(IconKind, Vec<u8>)> {
    let record = read_record(state_dir, id)?;
    let kind = record
        .get("icon")
        .and_then(Value::as_str)
        .and_then(|name| name.strip_prefix("icon."))
        .and_then(IconKind::from_extension)?;
    let bytes = fs::read(app_dir(state_dir, id).join(format!("icon.{}", kind.extension()))).ok()?;
    Some((kind, bytes))
}

/// The app made from `source_path`, if there is one.
fn app_for_source(state_dir: &Path, source_path: &str) -> Option<String> {
    list_records(state_dir).into_iter().find_map(|record| {
        (record.get("sourcePath").and_then(Value::as_str) == Some(source_path))
            .then(|| record.get("id").and_then(Value::as_str).map(str::to_string))
            .flatten()
    })
}

/// What one publish stores.
pub(super) struct NewVersion<'a> {
    pub(super) name: &'a str,
    pub(super) html: &'a str,
    pub(super) source_path: Option<&'a str>,
    pub(super) session_ref: Option<&'a str>,
    pub(super) icon: (IconKind, &'a [u8]),
    pub(super) powers: &'a [Power],
}

/// What a publish did.
pub(super) struct Published {
    pub(super) id: String,
    pub(super) version: u64,
    pub(super) created: bool,
    /// Powers this version declares that the previous one did not (the update band's ask).
    pub(super) new_powers: Vec<Power>,
}

/// Stores a new app, or the next version of the app made from the same file.
///
/// CDXC:GhostexApps 2026-11-25 DECISION:
/// User picked "ask again only when an update wants something new" (Q4976 "new"): an update keeps every power the user already allowed or refused, and a power the new version declares for the first time starts undecided, so the app asks for it (or the update band does). Grants are kept by power name on the app, not per version, and are never written here.
pub(super) fn publish(state_dir: &Path, new: NewVersion<'_>) -> std::io::Result<Published> {
    let _guard = lock();
    let existing = new
        .source_path
        .and_then(|source| app_for_source(state_dir, source));
    let created = existing.is_none();
    let id = existing.unwrap_or_else(new_app_id);
    let dir = app_dir(state_dir, &id);
    fs::create_dir_all(dir.join("versions"))?;
    let mut record = read_record(state_dir, &id).unwrap_or_else(|| {
        json!({
            "id": id,
            "createdAt": now(),
            "grants": {},
            "versions": [],
        })
    });
    let previous: Vec<String> = record
        .get("permissions")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let version = record.get("version").and_then(Value::as_u64).unwrap_or(0) + 1;
    let names: Vec<&str> = new.powers.iter().map(|power| power.name()).collect();
    let new_powers = if created {
        Vec::new()
    } else {
        new.powers
            .iter()
            .copied()
            .filter(|power| !previous.iter().any(|name| name == power.name()))
            .collect()
    };
    fs::write(version_path(state_dir, &id, version), new.html)?;
    let (kind, bytes) = new.icon;
    for other in [IconKind::Svg, IconKind::Png] {
        if other != kind {
            let _ = fs::remove_file(dir.join(format!("icon.{}", other.extension())));
        }
    }
    write_whole(&dir.join(format!("icon.{}", kind.extension())), bytes)?;
    let session_ref = new
        .session_ref
        .map(|value| clip_chars(value, SESSION_REF_MAX_CHARS));
    let stamp = now();
    record["name"] = json!(clip_chars(new.name, APP_NAME_MAX_CHARS));
    record["version"] = json!(version);
    record["updatedAt"] = json!(stamp);
    record["sessionRef"] = json!(session_ref);
    record["permissions"] = json!(names);
    record["icon"] = json!(format!("icon.{}", kind.extension()));
    if let Some(source) = new.source_path {
        record["sourcePath"] = json!(source);
    }
    if let Some(versions) = record.get_mut("versions").and_then(Value::as_array_mut) {
        versions.push(json!({
            "version": version,
            "publishedAt": stamp,
            "sessionRef": session_ref,
            "permissions": names,
        }));
    }
    write_record(state_dir, &id, &record)?;
    if version > VERSIONS_KEPT {
        let _ = fs::remove_file(version_path(state_dir, &id, version - VERSIONS_KEPT));
    }
    Ok(Published {
        id,
        version,
        created,
        new_powers,
    })
}

/// A power's state for an app: what the user decided, or `"ask"` while undecided, or
/// `"undeclared"` when the app's newest version does not declare it (then it is always refused).
pub(super) fn power_state(record: &Value, power: Power) -> &'static str {
    let declared = record
        .get("permissions")
        .and_then(Value::as_array)
        .is_some_and(|names| names.iter().any(|name| name.as_str() == Some(power.name())));
    if !declared {
        return "undeclared";
    }
    match record
        .get("grants")
        .and_then(|grants| grants.get(power.name()))
        .and_then(Value::as_str)
    {
        Some("allowed") => "allowed",
        Some("denied") => "denied",
        _ => "ask",
    }
}

/// Records the user's decision on one power (`allowed`, `denied`, or `ask` to forget it), and
/// clears a pending ask for it. Only the authenticated route the user's own Ghostex calls after a
/// click reaches this (api.rs); no bridge call does.
pub(super) fn set_grant(
    state_dir: &Path,
    id: &str,
    power: Power,
    decision: &str,
) -> Result<Value, String> {
    let _guard = lock();
    let mut record = read_record(state_dir, id).ok_or_else(|| "No such app.".to_string())?;
    if !record.get("grants").is_some_and(Value::is_object) {
        record["grants"] = json!({});
    }
    match decision {
        "allowed" | "denied" => record["grants"][power.name()] = json!(decision),
        "ask" => {
            if let Some(grants) = record.get_mut("grants").and_then(Value::as_object_mut) {
                grants.remove(power.name());
            }
        }
        _ => return Err("A decision is allowed, denied or ask.".to_string()),
    }
    if let Some(asks) = record.get_mut("pendingAsks").and_then(Value::as_object_mut) {
        asks.remove(power.name());
    }
    write_record(state_dir, id, &record).map_err(|error| error.to_string())?;
    Ok(record)
}

/// Notes that the app asked for an undecided power, so Ghostex can show its permission card.
pub(super) fn note_pending_ask(state_dir: &Path, id: &str, power: Power) {
    let _guard = lock();
    let Some(mut record) = read_record(state_dir, id) else {
        return;
    };
    if !record.get("pendingAsks").is_some_and(Value::is_object) {
        record["pendingAsks"] = json!({});
    }
    record["pendingAsks"][power.name()] = json!(now());
    let _ = write_record(state_dir, id, &record);
}

/// Removes an app with every version, its icon and its data.
pub(super) fn delete(state_dir: &Path, id: &str) -> std::io::Result<bool> {
    if !is_app_id(id) {
        return Ok(false);
    }
    let _guard = lock();
    let dir = app_dir(state_dir, id);
    if !dir.is_dir() {
        return Ok(false);
    }
    fs::remove_dir_all(dir)?;
    Ok(true)
}

/// The app's saved data (an empty object when it saved nothing).
pub(super) fn read_data(state_dir: &Path, id: &str) -> Map<String, Value> {
    fs::read(data_path(state_dir, id))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// Changes the app's saved data with `change`, refusing the result when it goes over the limits.
pub(super) fn change_data(
    state_dir: &Path,
    id: &str,
    change: impl FnOnce(&mut Map<String, Value>),
) -> Result<(), String> {
    let _guard = lock();
    if read_record(state_dir, id).is_none() {
        return Err("No such app.".to_string());
    }
    let mut data = read_data(state_dir, id);
    change(&mut data);
    if data.len() > DATA_MAX_KEYS {
        return Err(format!("An app may keep at most {DATA_MAX_KEYS} keys."));
    }
    let bytes = serde_json::to_vec(&Value::Object(data)).unwrap_or_default();
    if bytes.len() > DATA_MAX_BYTES {
        return Err(format!(
            "An app may keep at most {} KiB of data.",
            DATA_MAX_BYTES / 1024
        ));
    }
    write_whole(&data_path(state_dir, id), &bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version<'a>(source: &'a str, powers: &'a [Power]) -> NewVersion<'a> {
        NewVersion {
            name: "Morning brief",
            html: "<html></html>",
            source_path: Some(source),
            session_ref: Some("P1:S1"),
            icon: (IconKind::Svg, b"<svg viewBox=\"0 0 1 1\"></svg>"),
            powers,
        }
    }

    #[test]
    fn same_file_is_the_same_app_and_updates_keep_grants() {
        let dir = tempfile::tempdir().unwrap();
        let first = publish(dir.path(), version("/a/app.html", &[Power::Send])).unwrap();
        assert!(first.created);
        set_grant(dir.path(), &first.id, Power::Send, "allowed").unwrap();
        let second = publish(
            dir.path(),
            version("/a/app.html", &[Power::Send, Power::Notifications]),
        )
        .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.version, 2);
        assert_eq!(second.new_powers, vec![Power::Notifications]);
        let record = read_record(dir.path(), &first.id).unwrap();
        assert_eq!(power_state(&record, Power::Send), "allowed");
        assert_eq!(power_state(&record, Power::Notifications), "ask");
        assert_eq!(power_state(&record, Power::Ontop), "undeclared");
        let other = publish(dir.path(), version("/b/app.html", &[])).unwrap();
        assert_ne!(other.id, first.id);
    }

    #[test]
    fn data_stays_inside_its_limits() {
        let dir = tempfile::tempdir().unwrap();
        let app = publish(dir.path(), version("/a/app.html", &[])).unwrap();
        change_data(dir.path(), &app.id, |data| {
            data.insert("count".to_string(), json!(3));
        })
        .unwrap();
        assert_eq!(read_data(dir.path(), &app.id).get("count"), Some(&json!(3)));
        let big = "x".repeat(DATA_MAX_BYTES);
        assert!(change_data(dir.path(), &app.id, |data| {
            data.insert("big".to_string(), json!(big));
        })
        .is_err());
        assert_eq!(read_data(dir.path(), &app.id).len(), 1);
    }
}
