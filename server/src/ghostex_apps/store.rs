//! Where apps live: `<gxserver state>/ghostex-apps/<id>/` with `app.json` (the record),
//! `versions/<n>.html` (each version as published), `icon.png` (the icon as gxserver drew it,
//! icon.rs), and `data.json` (what the app saved for itself through `ghostex.app.data`). Folders are
//! made 0700 and files 0600.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! User: an app is not a Live page ("i dont want to overload the term again"); people ask for an app, a presentation or a dashboard, get it built in HTML, open it like a Live page and can install it. So an app is its own kind with its own folder, away from `visual-pages/`: its id never changes, publishing the same file again adds the next version to the same app (the app always serves its newest), and it is never deleted on its own (the 90-day sweep of Live pages does not reach it). Only an explicit delete or uninstall removes it.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rand::TryRng;
use serde_json::{json, Map, Value};

use super::manifest::Power;

/// The largest prepared app gxserver stores, like a Live page.
pub(crate) const APP_MAX_BYTES: usize = crate::visual_pages::VISUAL_PAGE_MAX_BYTES;
/// App names are labels in the Dock, Settings and the card.
pub(super) const APP_NAME_MAX_CHARS: usize = 80;
const ID_MAX_CHARS: usize = 200;
/// How many versions keep their HTML; older ones keep only their line in the record.
const VERSIONS_KEPT: u64 = 20;
const APPS_DIR_NAME: &str = "ghostex-apps";

/// Everything an app may keep through `ghostex.app.data`, all keys together.
pub(super) const DATA_MAX_BYTES: usize = 1024 * 1024;
pub(super) const DATA_MAX_KEYS: usize = 256;
pub(super) const DATA_KEY_MAX_CHARS: usize = 128;

/// Held while a publish finds the app a file belongs to and makes it, so two publishes of one file
/// make one app.
static PUBLISH_LOCK: Mutex<()> = Mutex::new(());
/// One lock per app, held while its record or data is read and rewritten, so one busy app does not
/// hold up the others (review finding 10).
static APP_LOCKS: Mutex<Option<HashMap<String, Arc<Mutex<()>>>>> = Mutex::new(None);

fn app_lock(id: &str) -> Arc<Mutex<()>> {
    let mut guard = APP_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .get_or_insert_with(HashMap::new)
        .entry(id.to_string())
        .or_default()
        .clone()
}

/// Runs `run` holding `id`'s lock.
fn locked<T>(id: &str, run: impl FnOnce() -> T) -> T {
    let lock = app_lock(id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    run()
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

/// Makes `dir` and the folders above it up to the state folder, each readable by this user only.
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    // Folders made before modes were set (phase 1) are narrowed too.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut at = Some(dir);
        while let Some(folder) = at {
            fs::set_permissions(folder, fs::Permissions::from_mode(0o700))?;
            if folder.file_name().is_some_and(|name| name == APPS_DIR_NAME) {
                break;
            }
            at = folder.parent();
        }
    }
    Ok(())
}

/// Writes a file only this user can read.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    // A file made before modes were set keeps its old mode through `open`.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)
}

/// Replaces a file whole (written beside it, then renamed), so a reader never sees half.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    write_private(&temporary, bytes).and_then(|()| fs::rename(&temporary, path))
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

/// The app's icon, the PNG gxserver drew (icon.rs).
pub(super) fn read_icon(state_dir: &Path, id: &str) -> Option<Vec<u8>> {
    read_record(state_dir, id)?;
    fs::read(app_dir(state_dir, id).join(ICON_FILE)).ok()
}

const ICON_FILE: &str = "icon.png";

/// The session whose agent published a version, from its own session token: the only chat an app
/// may send to, and whose project owns the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Publisher {
    pub(super) project_id: String,
    pub(super) session_id: String,
}

impl Publisher {
    fn to_json(&self) -> Value {
        json!({
            "projectId": clip_chars(&self.project_id, ID_MAX_CHARS),
            "sessionId": clip_chars(&self.session_id, ID_MAX_CHARS),
        })
    }

    fn from_json(value: Option<&Value>) -> Option<Publisher> {
        let value = value?;
        Some(Publisher {
            project_id: value.get("projectId")?.as_str()?.to_string(),
            session_id: value.get("sessionId")?.as_str()?.to_string(),
        })
    }
}

/// The session that published the app's newest version, if a session did.
pub(super) fn publisher(record: &Value) -> Option<Publisher> {
    Publisher::from_json(record.get("publisher"))
}

/// The project that owns the app (`None`: published from outside any session).
pub(super) fn owner_project(record: &Value) -> Option<String> {
    publisher(record).map(|publisher| publisher.project_id)
}

/// The app made from `source_path` by the same owner, if there is one.
///
/// CDXC:GhostexApps 2026-11-26 WHY:
/// Fable's security review (finding 2, proven live): an app was found by its file alone, so an agent of another project could publish over any app it saw listed, keep its allowed powers and read its data. An app now belongs to the project whose session published it first (from the session's own token, never the request body), and a publish of the same file from another project, or from outside any session when the app has a project, makes a new app.
fn app_for_source(state_dir: &Path, source_path: &str, project: Option<&str>) -> Option<String> {
    list_records(state_dir).into_iter().find_map(|record| {
        (record.get("sourcePath").and_then(Value::as_str) == Some(source_path)
            && owner_project(&record).as_deref() == project)
            .then(|| record.get("id").and_then(Value::as_str).map(str::to_string))
            .flatten()
    })
}

/// What one publish stores.
pub(super) struct NewVersion<'a> {
    pub(super) name: &'a str,
    pub(super) html: &'a str,
    pub(super) source_path: Option<&'a str>,
    pub(super) publisher: Option<Publisher>,
    /// The icon as gxserver drew it (icon.rs `icon_png`).
    pub(super) icon_png: &'a [u8],
    pub(super) powers: &'a [Power],
}

/// What a publish did.
pub(super) struct Published {
    pub(super) id: String,
    pub(super) version: u64,
    pub(super) created: bool,
    /// Powers this version declares that the previous one did not (the update band's ask).
    pub(super) new_powers: Vec<Power>,
    /// Powers that were allowed and are back to ask, because another session published this one.
    pub(super) reasked: Vec<Power>,
}

/// Stores a new app, or the next version of the app made from the same file.
///
/// CDXC:GhostexApps 2026-11-25 DECISION:
/// User picked "ask again only when an update wants something new" (Q4976 "new"): an update keeps every power the user already allowed or refused, and a power the new version declares for the first time starts undecided, so the app asks for it (or the update band does). Grants are kept by power name on the app, not per version, and are never written here.
///
/// CDXC:GhostexApps 2026-11-26 WHY:
/// Review finding 2: an update from another session could carry an app's allowed powers to code the user never saw run. Until phase 2's update band lets the user accept an update, an update published by any session other than the one that published the version before (or from outside any session) puts every allowed power back to ask; refusals stay.
pub(super) fn publish(state_dir: &Path, new: NewVersion<'_>) -> std::io::Result<Published> {
    let _publishing = PUBLISH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let project = new.publisher.as_ref().map(|p| p.project_id.as_str());
    let existing = new
        .source_path
        .and_then(|source| app_for_source(state_dir, source, project));
    let created = existing.is_none();
    let id = existing.unwrap_or_else(new_app_id);
    locked(&id.clone(), || publish_locked(state_dir, &id, created, new))
}

fn publish_locked(
    state_dir: &Path,
    id: &str,
    created: bool,
    new: NewVersion<'_>,
) -> std::io::Result<Published> {
    let id = id.to_string();
    let dir = app_dir(state_dir, &id);
    make_private_dir(&dir.join("versions"))?;
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
    let same_session = !created && new.publisher.is_some() && publisher(&record) == new.publisher;
    let mut reasked = Vec::new();
    if !created && !same_session {
        if let Some(grants) = record.get_mut("grants").and_then(Value::as_object_mut) {
            for power in super::manifest::ALL_POWERS {
                if grants.get(power.name()).and_then(Value::as_str) == Some("allowed") {
                    grants.remove(power.name());
                    reasked.push(power);
                }
            }
        }
        // "Always send" is the user's for the code they saw; a new hand's version asks again.
        if let Some(object) = record.as_object_mut() {
            object.remove("alwaysSend");
        }
    }
    write_private(&version_path(state_dir, &id, version), new.html.as_bytes())?;
    let _ = fs::remove_file(dir.join("icon.svg"));
    write_whole(&dir.join(ICON_FILE), new.icon_png)?;
    let publisher = new.publisher.as_ref().map(Publisher::to_json);
    let stamp = now();
    record["name"] = json!(clip_chars(new.name, APP_NAME_MAX_CHARS));
    record["version"] = json!(version);
    record["updatedAt"] = json!(stamp);
    record["publisher"] = json!(publisher);
    if let Some(object) = record.as_object_mut() {
        // Written by phase 1 before the review: a target the publisher chose (finding 1).
        object.remove("sessionRef");
    }
    record["permissions"] = json!(names);
    record["icon"] = json!(ICON_FILE);
    if let Some(source) = new.source_path {
        record["sourcePath"] = json!(source);
    }
    if let Some(versions) = record.get_mut("versions").and_then(Value::as_array_mut) {
        versions.push(json!({
            "version": version,
            "publishedAt": stamp,
            "publisher": publisher,
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
        reasked,
    })
}

/// Whether the user turned on "always send" for the app: its messages go to its chat without a
/// click on each. Nothing sets it yet; phase 2's app settings will (Q5006).
pub(super) fn always_send(record: &Value) -> bool {
    record.get("alwaysSend").and_then(Value::as_bool) == Some(true)
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
    locked(id, || set_grant_locked(state_dir, id, power, decision))
}

fn set_grant_locked(
    state_dir: &Path,
    id: &str,
    power: Power,
    decision: &str,
) -> Result<Value, String> {
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

/// Notes that the app asked for an undecided power, so Ghostex can show its permission card. Only
/// the first ask is written; asking again while it is pending changes nothing (review finding 8).
pub(super) fn note_pending_ask(state_dir: &Path, id: &str, power: Power) {
    locked(id, || {
        let Some(mut record) = read_record(state_dir, id) else {
            return;
        };
        if record
            .get("pendingAsks")
            .and_then(|asks| asks.get(power.name()))
            .is_some()
        {
            return;
        }
        if !record.get("pendingAsks").is_some_and(Value::is_object) {
            record["pendingAsks"] = json!({});
        }
        record["pendingAsks"][power.name()] = json!(now());
        let _ = write_record(state_dir, id, &record);
    });
}

/// Removes an app with every version, its icon and its data.
pub(super) fn delete(state_dir: &Path, id: &str) -> std::io::Result<bool> {
    if !is_app_id(id) {
        return Ok(false);
    }
    locked(id, || {
        let dir = app_dir(state_dir, id);
        if !dir.is_dir() {
            return Ok(false);
        }
        fs::remove_dir_all(dir)?;
        Ok(true)
    })
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
    locked(id, || change_data_locked(state_dir, id, change))
}

fn change_data_locked(
    state_dir: &Path,
    id: &str,
    change: impl FnOnce(&mut Map<String, Value>),
) -> Result<(), String> {
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

    fn session(project: &str, session: &str) -> Option<Publisher> {
        Some(Publisher {
            project_id: project.to_string(),
            session_id: session.to_string(),
        })
    }

    fn version<'a>(
        source: &'a str,
        powers: &'a [Power],
        publisher: Option<Publisher>,
    ) -> NewVersion<'a> {
        NewVersion {
            name: "Morning brief",
            html: "<html></html>",
            source_path: Some(source),
            publisher,
            icon_png: b"png",
            powers,
        }
    }

    #[test]
    fn same_file_is_the_same_app_and_updates_keep_grants() {
        let dir = tempfile::tempdir().unwrap();
        let first = publish(
            dir.path(),
            version("/a/app.html", &[Power::Send], session("P1", "S1")),
        )
        .unwrap();
        assert!(first.created);
        set_grant(dir.path(), &first.id, Power::Send, "allowed").unwrap();
        let second = publish(
            dir.path(),
            version(
                "/a/app.html",
                &[Power::Send, Power::Notifications],
                session("P1", "S1"),
            ),
        )
        .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.version, 2);
        assert_eq!(second.new_powers, vec![Power::Notifications]);
        assert!(second.reasked.is_empty());
        let record = read_record(dir.path(), &first.id).unwrap();
        assert_eq!(power_state(&record, Power::Send), "allowed");
        assert_eq!(power_state(&record, Power::Notifications), "ask");
        assert_eq!(power_state(&record, Power::Ontop), "undeclared");
        assert_eq!(publisher(&record), session("P1", "S1"));
        let other = publish(dir.path(), version("/b/app.html", &[], session("P1", "S1"))).unwrap();
        assert_ne!(other.id, first.id);
    }

    #[test]
    fn another_project_or_no_session_cannot_take_an_app_over() {
        // Review finding 2: same file, another project's token.
        let dir = tempfile::tempdir().unwrap();
        let mine = publish(
            dir.path(),
            version("/a/app.html", &[Power::Send], session("P1", "S1")),
        )
        .unwrap();
        set_grant(dir.path(), &mine.id, Power::Send, "allowed").unwrap();
        let theirs = publish(
            dir.path(),
            version("/a/app.html", &[Power::Send], session("P2", "S9")),
        )
        .unwrap();
        assert!(theirs.created);
        assert_ne!(theirs.id, mine.id);
        let outside = publish(dir.path(), version("/a/app.html", &[Power::Send], None)).unwrap();
        assert!(outside.created);
        assert_ne!(outside.id, mine.id);
        let record = read_record(dir.path(), &mine.id).unwrap();
        assert_eq!(record["version"], json!(1));
        assert_eq!(power_state(&record, Power::Send), "allowed");
        assert_eq!(publisher(&record), session("P1", "S1"));
    }

    #[test]
    fn an_update_from_another_session_asks_again_for_what_was_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let app = publish(
            dir.path(),
            version(
                "/a/app.html",
                &[Power::Send, Power::Notifications],
                session("P1", "S1"),
            ),
        )
        .unwrap();
        set_grant(dir.path(), &app.id, Power::Send, "allowed").unwrap();
        set_grant(dir.path(), &app.id, Power::Notifications, "denied").unwrap();
        let update = publish(
            dir.path(),
            version(
                "/a/app.html",
                &[Power::Send, Power::Notifications],
                session("P1", "S2"),
            ),
        )
        .unwrap();
        assert_eq!(update.id, app.id);
        assert_eq!(update.reasked, vec![Power::Send]);
        let record = read_record(dir.path(), &app.id).unwrap();
        assert_eq!(power_state(&record, Power::Send), "ask");
        assert_eq!(power_state(&record, Power::Notifications), "denied");
        assert_eq!(publisher(&record), session("P1", "S2"));
        // An app published from outside any session never keeps an allowed power over an update.
        let local = publish(dir.path(), version("/c/app.html", &[Power::Send], None)).unwrap();
        set_grant(dir.path(), &local.id, Power::Send, "allowed").unwrap();
        let again = publish(dir.path(), version("/c/app.html", &[Power::Send], None)).unwrap();
        assert_eq!(again.id, local.id);
        assert_eq!(again.reasked, vec![Power::Send]);
    }

    #[test]
    fn only_the_first_ask_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let app = publish(
            dir.path(),
            version("/a/app.html", &[Power::Send], session("P1", "S1")),
        )
        .unwrap();
        note_pending_ask(dir.path(), &app.id, Power::Send);
        let first = read_record(dir.path(), &app.id).unwrap()["pendingAsks"]["send"].clone();
        std::thread::sleep(std::time::Duration::from_millis(5));
        note_pending_ask(dir.path(), &app.id, Power::Send);
        let second = read_record(dir.path(), &app.id).unwrap()["pendingAsks"]["send"].clone();
        assert_eq!(first, second);
    }

    #[cfg(unix)]
    #[test]
    fn files_are_this_users_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let app = publish(dir.path(), version("/a/app.html", &[], session("P1", "S1"))).unwrap();
        change_data(dir.path(), &app.id, |data| {
            data.insert("k".to_string(), json!(1));
        })
        .unwrap();
        let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(apps_dir(dir.path())), 0o700);
        assert_eq!(mode(app_dir(dir.path(), &app.id)), 0o700);
        assert_eq!(mode(app_dir(dir.path(), &app.id).join("versions")), 0o700);
        assert_eq!(mode(record_path(dir.path(), &app.id)), 0o600);
        assert_eq!(mode(version_path(dir.path(), &app.id, 1)), 0o600);
        assert_eq!(mode(data_path(dir.path(), &app.id)), 0o600);
        assert_eq!(mode(app_dir(dir.path(), &app.id).join(ICON_FILE)), 0o600);
    }

    #[test]
    fn data_stays_inside_its_limits() {
        let dir = tempfile::tempdir().unwrap();
        let app = publish(dir.path(), version("/a/app.html", &[], session("P1", "S1"))).unwrap();
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
