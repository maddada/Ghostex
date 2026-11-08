//! The one-time 11.0.1 security notice: which phones and computers that use Easy Connect must pair
//! again, decided once from the stored pairing data, shown in the middle of the screen until each
//! is paired again or the user says Done.
//!
//! At its first start on 11.0.1 or later, gxserver writes `<gxserver state>/security/upgrade-11.0.1.json`
//! before anything else touches the pairing data: each paired phone (its row id, name, platform and
//! when it was last used) and each remote computer this computer reaches through Easy Connect that
//! is not turned off (its id, name and Easy Connect address). A computer with neither gets an empty
//! record, so a phone paired later never brings the notice up.
//!
//! `/api/securityUpgradeNotice`: `op: "read"` (the default) answers the rows with their status
//! now, whether the notice is due, and how many still wait; `op: "later"` hides it for a day (twice
//! at most, then for good); `op: "done"` hides it for good. Settings > Remote keeps a "Pair again"
//! line while any row still waits.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (GitHub #495, rollout): "on first start of 11.0.1, a one-time notice explaining Ghostex got more secure and asking to re-pair machines/phones that use Easy Connect, shown ONLY to users with active Easy Connect machines/devices." Then, of the first toast: "This is a very ugly notice man. Show it in the middle of the screen and make it clearer please." The user picked (Q1726 B) the notice that holds the live pairing QR, ticks each phone off as it pairs again and gives each computer its own Pair button, and on the phone opens straight into the scanner; (Q1727) one row per pairing, same names included; (Q1728) pairings not used for over a month left out; (Q1729) Remind me later shows it again tomorrow, at most twice more, with a "Pair again" line in Settings > Remote until every row is done.
//! CDXC:Security 2026-11-08 WHY: pairing v2 rewrites the pairing data when it starts, so the list is taken before any of that runs (`snapshot_before_pairing_changes` is called before gxserver's storage opens), reading only the columns phones had before 11.0.1.
//! SEE-ALSO: apps/desktop/src/app/window/security_upgrade_modal.rs (the notice), apps/desktop/src/app/gx_store/security_notice.rs (when it opens), server/src/remote_access/ (pairing).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::paths::GxserverPaths;

pub(crate) const SECURITY_UPGRADE_NOTICE_ENDPOINT: &str = "/api/securityUpgradeNotice";
const NOTICE_FILE: &str = "upgrade-11.0.1.json";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// A pairing not used for longer than this before the update is left out of the notice (Q1728).
const UNUSED_FOR_MS: i64 = 30 * DAY_MS;
/// How many times Remind me later brings the notice back (Q1729).
const REMINDERS: u8 = 2;

fn default_reminders() -> u8 {
    REMINDERS
}

/// A phone paired before 11.0.1, as it was when the list was taken.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoticePhone {
    pub(crate) id: String,
    pub(crate) name: String,
    /// `ios`, `android`, or what the phone said.
    #[serde(default)]
    pub(crate) platform: String,
    /// When it last reached this computer, or when it was paired if it never did (RFC 3339).
    #[serde(default)]
    pub(crate) last_used_at: Option<String>,
}

/// A computer this one reached through Easy Connect before 11.0.1.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoticeComputer {
    /// Its id in Settings > Remote (`remoteMachines[].id`).
    pub(crate) id: String,
    pub(crate) name: String,
    /// Its Easy Connect address then; a new one means it was paired again.
    #[serde(default)]
    pub(crate) address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpgradeNotice {
    #[serde(default)]
    pub(crate) phone_rows: Vec<NoticePhone>,
    #[serde(default)]
    pub(crate) computer_rows: Vec<NoticeComputer>,
    /// Names only, from the first record of this file (before the rows above); read when the
    /// rows are empty.
    #[serde(default)]
    pub(crate) phones: Vec<String>,
    #[serde(default)]
    pub(crate) computers: Vec<String>,
    /// Done, or Remind me later with no reminders left.
    pub(crate) seen: bool,
    pub(crate) taken_at_ms: i64,
    /// Remind me later: not before then.
    #[serde(default)]
    pub(crate) remind_at_ms: Option<i64>,
    #[serde(default = "default_reminders")]
    pub(crate) reminders_left: u8,
}

impl Default for UpgradeNotice {
    fn default() -> Self {
        Self {
            phone_rows: Vec::new(),
            computer_rows: Vec::new(),
            phones: Vec::new(),
            computers: Vec::new(),
            seen: false,
            taken_at_ms: 0,
            remind_at_ms: None,
            reminders_left: REMINDERS,
        }
    }
}

/// Where one row stands now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowStatus {
    /// Still paired the old way.
    Waiting,
    /// Paired again since the update.
    Paired,
    /// Removed from this computer (a phone in Paired devices, a computer in Settings > Remote, or
    /// a computer turned off): nothing left to pair, so it leaves the notice.
    Removed,
}

impl RowStatus {
    fn wire(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Paired => "paired",
            Self::Removed => "removed",
        }
    }
}

/// A paired phone as the state database lists it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CurrentPhone {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) platform: String,
    pub(crate) paired_at: String,
}

fn notice_path(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("security").join(NOTICE_FILE)
}

/// Takes the list once, at the first start that has this code. Never fails gxserver's start.
pub(crate) fn snapshot_before_pairing_changes(paths: &GxserverPaths) -> Result<()> {
    let path = notice_path(paths);
    if path.exists() {
        return Ok(());
    }
    let phone_rows = paired_phones_before_update(&paths.state_db_file);
    let computer_rows =
        easy_connect_computers(crate::shared_settings_file::read_shared_settings(paths).as_ref());
    let notice = UpgradeNotice {
        phones: phone_rows.iter().map(|phone| phone.name.clone()).collect(),
        computers: computer_rows
            .iter()
            .map(|computer| computer.name.clone())
            .collect(),
        phone_rows,
        computer_rows,
        taken_at_ms: chrono::Utc::now().timestamp_millis(),
        ..UpgradeNotice::default()
    };
    write(&path, &notice)
}

/// The paired phones in the state database, read before its migrations or anything else run,
/// from the columns phones had before 11.0.1 only. A database that is not there yet (a new
/// install) has none.
fn paired_phones_before_update(db_file: &Path) -> Vec<NoticePhone> {
    if !db_file.exists() {
        return Vec::new();
    }
    let Ok(db) =
        rusqlite::Connection::open_with_flags(db_file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return Vec::new();
    };
    let Ok(mut statement) = db.prepare(
        "SELECT id, name, platform, COALESCE(lastSeenAt, pairedAt) FROM remote_paired_devices ORDER BY pairedAt",
    ) else {
        return Vec::new();
    };
    statement
        .query_map([], |row| {
            Ok(NoticePhone {
                id: row.get(0)?,
                name: row.get(1)?,
                platform: row.get(2)?,
                last_used_at: row.get(3)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// The paired phones now, for the status of each row.
fn current_phones(paths: &GxserverPaths) -> Vec<CurrentPhone> {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return Vec::new();
    };
    let Ok(mut statement) =
        db.prepare("SELECT id, name, platform, pairedAt FROM remote_paired_devices")
    else {
        return Vec::new();
    };
    statement
        .query_map([], |row| {
            Ok(CurrentPhone {
                id: row.get(0)?,
                name: row.get(1)?,
                platform: row.get(2)?,
                paired_at: row.get(3)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// The remote computers saved with the Easy Connect transport and not turned off.
pub(crate) fn easy_connect_computers(settings: Option<&Value>) -> Vec<NoticeComputer> {
    settings
        .and_then(|settings| settings.get("remoteMachines"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|machine| {
            machine.get("transport").and_then(Value::as_str) == Some("easyConnect")
                && machine.get("disabled").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|machine| {
            let text = |key: &str| {
                machine
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string()
            };
            let name = text("name");
            (!name.is_empty()).then(|| NoticeComputer {
                id: text("id"),
                name,
                address: text("easyConnectAddress"),
            })
        })
        .collect()
}

/// SEAM (pairing v2, GitHub #495): whether each phone of the notice was paired again.
///
/// Today: a phone is paired again when a pairing made after the update has the same name and
/// platform (each new pairing ticks off one old row, newest first), and removed when its old row
/// is gone with no such pairing.
///
/// What pairing v2 (branch `ghostex/d18d40af`) plugs in here: its re-paired phone is a new
/// `paired` row while the old one stays `legacy` until removed, which the rule above already
/// reads; a phone that scanned the code and waits on the owner's approval (its `pending` entry in
/// `/api/pairedDevices`, with the three words) should become a fourth status, `pending`, with
/// those words, so the notice can show them and the Approve button beside its row.
pub(crate) fn phone_statuses(
    phones: &[NoticePhone],
    current: &[CurrentPhone],
    taken_at_ms: i64,
) -> Vec<RowStatus> {
    let taken_at = chrono::DateTime::from_timestamp_millis(taken_at_ms);
    let mut newer: Vec<&CurrentPhone> = current
        .iter()
        .filter(|phone| !phones.iter().any(|old| old.id == phone.id))
        .filter(|phone| {
            match (
                chrono::DateTime::parse_from_rfc3339(&phone.paired_at),
                taken_at,
            ) {
                (Ok(paired), Some(taken)) => paired.with_timezone(&chrono::Utc) >= taken,
                _ => true,
            }
        })
        .collect();
    newer.sort_by(|a, b| b.paired_at.cmp(&a.paired_at));
    phones
        .iter()
        .map(|phone| {
            if let Some(at) = newer
                .iter()
                .position(|new| new.name == phone.name && new.platform == phone.platform)
            {
                newer.remove(at);
                RowStatus::Paired
            } else if current.iter().any(|now| now.id == phone.id) {
                RowStatus::Waiting
            } else {
                RowStatus::Removed
            }
        })
        .collect()
}

/// SEAM (pairing v2, GitHub #495): whether each computer of the notice was paired again: a new
/// Easy Connect address in its Settings > Remote entry, or removed (gone, turned off, or moved off
/// Easy Connect). Pairing v2 keeps this computer-side rule; the desktop pairs a computer by
/// pasting that computer's new code into the entry.
pub(crate) fn computer_statuses(
    computers: &[NoticeComputer],
    settings: Option<&Value>,
) -> Vec<RowStatus> {
    let now = easy_connect_computers(settings);
    computers
        .iter()
        .map(|computer| {
            match now
                .iter()
                .find(|entry| entry.id == computer.id && !computer.id.is_empty())
            {
                None => RowStatus::Removed,
                Some(entry) if entry.address != computer.address => RowStatus::Paired,
                Some(_) => RowStatus::Waiting,
            }
        })
        .collect()
}

/// A phone used within the month before the update (Q1728); one with no date counts as used.
fn recently_used(phone: &NoticePhone, taken_at_ms: i64) -> bool {
    phone
        .last_used_at
        .as_deref()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .is_none_or(|at| at.timestamp_millis() >= taken_at_ms - UNUSED_FOR_MS)
}

/// The kind a phone row names: "iPhone", "Android phone", or the platform as it was given.
fn phone_kind(platform: &str) -> String {
    match platform.trim().to_ascii_lowercase().as_str() {
        "ios" | "iphone" => "iPhone".to_string(),
        "ipados" | "ipad" => "iPad".to_string(),
        "android" => "Android phone".to_string(),
        "" => "Phone".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn read(paths: &GxserverPaths) -> UpgradeNotice {
    std::fs::read_to_string(notice_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| UpgradeNotice {
            seen: true,
            ..UpgradeNotice::default()
        })
}

fn write(path: &Path, notice: &UpgradeNotice) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create the security folder")?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(notice)?)
        .context("write the security upgrade notice")
}

/// The notice as the desktop draws it: each row with its status now, whether it is due, and how
/// many rows still wait.
pub(crate) fn view(
    notice: &UpgradeNotice,
    current: &[CurrentPhone],
    settings: Option<&Value>,
    now_ms: i64,
) -> Value {
    // A record from before the rows were kept has names only: one waiting row per name.
    let legacy_names = notice.phone_rows.is_empty()
        && notice.computer_rows.is_empty()
        && !(notice.phones.is_empty() && notice.computers.is_empty());
    let mut phones = Vec::new();
    let mut computers = Vec::new();
    if legacy_names {
        phones.extend(
            notice.phones.iter().map(
                |name| json!({ "id": name, "name": name, "kind": "Phone", "status": "waiting" }),
            ),
        );
        computers.extend(
            notice
                .computers
                .iter()
                .map(|name| json!({ "id": name, "name": name, "status": "waiting" })),
        );
    } else {
        let recent: Vec<NoticePhone> = notice
            .phone_rows
            .iter()
            .filter(|phone| recently_used(phone, notice.taken_at_ms))
            .cloned()
            .collect();
        for (phone, status) in
            recent
                .iter()
                .zip(phone_statuses(&recent, current, notice.taken_at_ms))
        {
            if status == RowStatus::Removed {
                continue;
            }
            phones.push(json!({
                "id": phone.id,
                "name": phone.name,
                "kind": phone_kind(&phone.platform),
                "lastUsedAt": phone.last_used_at,
                "status": status.wire(),
            }));
        }
        for (computer, status) in notice
            .computer_rows
            .iter()
            .zip(computer_statuses(&notice.computer_rows, settings))
        {
            if status == RowStatus::Removed {
                continue;
            }
            computers.push(json!({
                "id": computer.id,
                "name": computer.name,
                "status": status.wire(),
            }));
        }
    }
    let waiting = phones
        .iter()
        .chain(computers.iter())
        .filter(|row| row["status"] == "waiting")
        .count();
    let due = !notice.seen && notice.remind_at_ms.is_none_or(|at| now_ms >= at);
    json!({
        "show": due && waiting > 0,
        "waiting": waiting,
        "phones": phones,
        "computers": computers,
        "remindersLeft": notice.reminders_left,
        "takenAtMs": notice.taken_at_ms,
    })
}

/// Remind me later: back tomorrow while reminders are left, then not again.
pub(crate) fn remind_later(notice: &mut UpgradeNotice, now_ms: i64) {
    if notice.reminders_left == 0 {
        notice.seen = true;
        return;
    }
    notice.reminders_left -= 1;
    notice.remind_at_ms = Some(now_ms + DAY_MS);
}

/// The `/api/securityUpgradeNotice` route.
#[inline(never)]
pub(crate) fn handle_http(
    paths: &GxserverPaths,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> crate::server::RoutedResponse {
    use axum::http::StatusCode;
    let params = crate::domain::read_domain_rpc_params(body).unwrap_or_default();
    match handle_endpoint(paths, &params) {
        Ok(result) => crate::server::routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(error) => crate::server::routed_json(
            Some(endpoint_path),
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::protocol::rpc_error("internalError", format!("{error:#}"), Some(request_id)),
        ),
    }
}

/// `op`: `read` (default), `later` or `done` (`dismiss` reads as `done`). Answers the view.
pub(crate) fn handle_endpoint(
    paths: &GxserverPaths,
    params: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut notice = read(paths);
    match params.get("op").and_then(Value::as_str) {
        Some("later") => {
            remind_later(&mut notice, now_ms);
            write(&notice_path(paths), &notice)?;
        }
        Some("done" | "dismiss") => {
            notice.seen = true;
            write(&notice_path(paths), &notice)?;
        }
        _ => {}
    }
    Ok(view(
        &notice,
        &current_phones(paths),
        crate::shared_settings_file::read_shared_settings(paths).as_ref(),
        now_ms,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAKEN: i64 = 1_794_100_000_000;

    fn paths() -> (tempfile::TempDir, GxserverPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(dir.path().to_path_buf()));
        (dir, paths)
    }

    fn iso(ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .to_rfc3339()
    }

    fn phone(id: &str, name: &str, platform: &str, used: i64) -> NoticePhone {
        NoticePhone {
            id: id.into(),
            name: name.into(),
            platform: platform.into(),
            last_used_at: Some(iso(used)),
        }
    }

    fn now_phone(id: &str, name: &str, platform: &str, paired: i64) -> CurrentPhone {
        CurrentPhone {
            id: id.into(),
            name: name.into(),
            platform: platform.into(),
            paired_at: iso(paired),
        }
    }

    #[test]
    fn only_active_easy_connect_computers_count() {
        let computers = easy_connect_computers(Some(&json!({
            "remoteMachines": [
                { "id": "m1", "name": "Studio", "transport": "easyConnect", "easyConnectAddress": "tc1x" },
                { "id": "m2", "name": "Old box", "transport": "easyConnect", "disabled": true },
                { "id": "m3", "name": "Server", "sshHost": "server.local" },
            ]
        })));
        assert_eq!(
            computers,
            vec![NoticeComputer {
                id: "m1".into(),
                name: "Studio".into(),
                address: "tc1x".into()
            }]
        );
        assert!(easy_connect_computers(None).is_empty());
    }

    #[test]
    fn same_named_phones_are_one_row_each_and_tick_off_one_by_one() {
        let phones = vec![
            phone("a", "iPhone", "ios", TAKEN - DAY_MS),
            phone("b", "iPhone", "ios", TAKEN - 2 * DAY_MS),
            phone("c", "Pixel 8", "android", TAKEN - DAY_MS),
        ];
        let mut current = vec![
            now_phone("a", "iPhone", "ios", TAKEN - 90 * DAY_MS),
            now_phone("b", "iPhone", "ios", TAKEN - 90 * DAY_MS),
        ];
        assert_eq!(
            phone_statuses(&phones, &current, TAKEN),
            vec![RowStatus::Waiting, RowStatus::Waiting, RowStatus::Removed]
        );
        // One iPhone pairs again after the update: one old row ticks off, not both.
        current.push(now_phone("new1", "iPhone", "ios", TAKEN + 1_000));
        assert_eq!(
            phone_statuses(&phones, &current, TAKEN),
            vec![RowStatus::Paired, RowStatus::Waiting, RowStatus::Removed]
        );
        // A pairing from before the update never counts as paired again.
        let older = vec![now_phone("x", "Pixel 8", "android", TAKEN - 1_000)];
        assert_eq!(
            phone_statuses(&phones[2..], &older, TAKEN),
            vec![RowStatus::Removed]
        );
    }

    #[test]
    fn a_computer_is_paired_again_when_its_address_changes() {
        let computers = vec![
            NoticeComputer {
                id: "m1".into(),
                name: "Studio".into(),
                address: "tc1old".into(),
            },
            NoticeComputer {
                id: "m2".into(),
                name: "Omarchy".into(),
                address: "tc1same".into(),
            },
            NoticeComputer {
                id: "m3".into(),
                name: "Gone".into(),
                address: "tc1".into(),
            },
        ];
        let settings = json!({ "remoteMachines": [
            { "id": "m1", "name": "Studio", "transport": "easyConnect", "easyConnectAddress": "tc1new" },
            { "id": "m2", "name": "Omarchy", "transport": "easyConnect", "easyConnectAddress": "tc1same" },
        ]});
        assert_eq!(
            computer_statuses(&computers, Some(&settings)),
            vec![RowStatus::Paired, RowStatus::Waiting, RowStatus::Removed]
        );
    }

    #[test]
    fn pairings_unused_for_a_month_are_left_out() {
        let notice = UpgradeNotice {
            phone_rows: vec![
                phone("a", "iPhone", "ios", TAKEN - DAY_MS),
                phone("b", "iPhone", "ios", TAKEN - 40 * DAY_MS),
            ],
            taken_at_ms: TAKEN,
            ..UpgradeNotice::default()
        };
        let current = vec![
            now_phone("a", "iPhone", "ios", TAKEN - 90 * DAY_MS),
            now_phone("b", "iPhone", "ios", TAKEN - 90 * DAY_MS),
        ];
        let view = view(&notice, &current, None, TAKEN);
        assert_eq!(view["phones"].as_array().unwrap().len(), 1);
        assert_eq!(view["phones"][0]["id"], "a");
        assert_eq!(view["phones"][0]["kind"], "iPhone");
        assert_eq!(view["waiting"], 1);
        assert_eq!(view["show"], true);
    }

    #[test]
    fn remind_me_later_comes_back_tomorrow_twice_then_stops() {
        let mut notice = UpgradeNotice {
            phone_rows: vec![phone("a", "iPhone", "ios", TAKEN)],
            taken_at_ms: TAKEN,
            ..UpgradeNotice::default()
        };
        let current = vec![now_phone("a", "iPhone", "ios", TAKEN - DAY_MS)];
        let shown =
            |notice: &UpgradeNotice, at: i64| view(notice, &current, None, at)["show"] == true;
        assert!(shown(&notice, TAKEN));
        remind_later(&mut notice, TAKEN);
        assert!(!shown(&notice, TAKEN + DAY_MS - 1));
        assert!(shown(&notice, TAKEN + DAY_MS));
        remind_later(&mut notice, TAKEN + DAY_MS);
        assert!(shown(&notice, TAKEN + 2 * DAY_MS));
        remind_later(&mut notice, TAKEN + 2 * DAY_MS);
        assert!(!shown(&notice, TAKEN + 10 * DAY_MS));
        // Settings > Remote still counts what waits.
        assert_eq!(
            view(&notice, &current, None, TAKEN + 10 * DAY_MS)["waiting"],
            1
        );
    }

    #[test]
    fn a_new_install_never_shows_it() {
        let (_dir, paths) = paths();
        snapshot_before_pairing_changes(&paths).unwrap();
        assert_eq!(
            handle_endpoint(&paths, &serde_json::Map::new()).unwrap()["show"],
            false
        );
    }

    #[test]
    fn a_record_with_names_only_still_shows_them() {
        let notice = UpgradeNotice {
            phones: vec!["Pixel".into()],
            computers: vec!["Studio".into()],
            taken_at_ms: TAKEN,
            ..UpgradeNotice::default()
        };
        let view = view(&notice, &[], None, TAKEN);
        assert_eq!(view["waiting"], 2);
        assert_eq!(view["phones"][0]["name"], "Pixel");
    }

    #[test]
    fn phones_are_read_from_a_database_from_before_pairing_v2_and_done_hides_it() {
        let (_dir, paths) = paths();
        std::fs::create_dir_all(paths.state_db_file.parent().unwrap()).unwrap();
        let db = rusqlite::Connection::open(&paths.state_db_file).unwrap();
        db.execute_batch(
            "CREATE TABLE remote_paired_devices (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, platform TEXT NOT NULL,
               sshKeyFingerprint TEXT NOT NULL, tailcatClientKey TEXT, pairedAt TEXT NOT NULL,
               lastSeenAt TEXT, apiTokenHash TEXT
             );",
        )
        .unwrap();
        db.execute(
            "INSERT INTO remote_paired_devices (id, name, platform, sshKeyFingerprint, pairedAt, lastSeenAt)
             VALUES ('old', 'iPhone', 'ios', 'SHA256:x', ?1, ?2)",
            rusqlite::params![iso(TAKEN - 90 * DAY_MS), iso(chrono::Utc::now().timestamp_millis())],
        )
        .unwrap();
        drop(db);
        snapshot_before_pairing_changes(&paths).unwrap();
        let notice = read(&paths);
        assert_eq!(notice.phone_rows.len(), 1);
        assert_eq!(notice.phone_rows[0].platform, "ios");
        let read_view = handle_endpoint(&paths, &serde_json::Map::new()).unwrap();
        assert_eq!(read_view["show"], true);
        assert_eq!(read_view["phones"][0]["status"], "waiting");
        let mut done = serde_json::Map::new();
        done.insert("op".into(), json!("done"));
        assert_eq!(handle_endpoint(&paths, &done).unwrap()["show"], false);
    }
}
