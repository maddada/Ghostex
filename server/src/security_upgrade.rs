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
//! at most, then for good); `op: "done"` hides it for good; `op: "forget"` with a row's `id`
//! leaves that row out from then on. Settings > Remote keeps a "Pair again"
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
use ghostex_gx_protocol::pairing::PendingDevice;

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
    /// Rows the owner chose Forget on (`op: "forget"`): left out from then on.
    #[serde(default)]
    pub(crate) forgotten: Vec<String>,
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
            forgotten: Vec::new(),
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
    /// It scanned the new code and waits for the owner's approval, with three words to compare
    /// (pairing v2): the notice shows them and Approve.
    Pending,
}

impl RowStatus {
    fn wire(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Paired => "paired",
            Self::Removed => "removed",
            Self::Pending => "pending",
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
    /// `legacy` (the pairing from before 11.0.1) or `paired` (its own key).
    pub(crate) status: String,
    /// When it last reached this computer; `None` if it never did since it was paired.
    pub(crate) last_seen_at: Option<String>,
    /// Paired over its own SSH login (`/api/pairOverSsh`): it signs in over Tailscale or SSH.
    pub(crate) over_ssh: bool,
}

impl CurrentPhone {
    fn same_device(&self, phone: &NoticePhone) -> bool {
        self.name == phone.name && self.platform == phone.platform
    }
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
    // A table from before pairing v2 has no columns to tell a phone paired over SSH, and no such
    // phone either: every row there is a legacy pairing.
    let Ok(mut statement) = db
        .prepare(&format!(
            "SELECT id, name, platform, pairedAt, status, lastSeenAt, ({PAIRED_OVER_SSH}) FROM remote_paired_devices"
        ))
        .or_else(|_| {
            db.prepare(
                "SELECT id, name, platform, pairedAt, 'legacy', lastSeenAt, 0 FROM remote_paired_devices",
            )
        })
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
                status: row.get(4)?,
                last_seen_at: row.get(5)?,
                over_ssh: row.get::<_, i64>(6)? != 0,
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

/// A device row paired over its own SSH login (`/api/pairOverSsh`): a phone's token, but neither
/// an SSH key line nor an Easy Connect key.
const PAIRED_OVER_SSH: &str = "status = 'paired' AND apiTokenHash IS NOT NULL \
     AND COALESCE(sshKeyFingerprint, '') = '' AND tailcatClientKey IS NULL";

/*
CDXC:Security 2026-11-08 DECISION:
User (GitHub #495): "outline this in the message talking about what needs to be reconnected", and
"make the notice also open for people who have no Easy Connect pairings but whose phone has signed
in over SSH lately", with each kind of device in its own row. A phone that signs in over Tailscale,
the local network or SSH leaves no pairing row; the trace gxserver keeps is its notification
registration, which a phone on the token file makes with no `deviceId` and renews whenever it
connects (push/registration.rs). One row stands for those phones: it waits until a phone pairs
over SSH (`/api/pairOverSsh`), shows the words while one is Pending, and is done once one has
paired since the update. Computers this one reaches over SSH get a row that only says what they
may ask once; it needs nothing, so it never keeps the notice open by itself.
*/
/// What the notice knows of phones that sign in over SSH.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SshPhones {
    /// Phones whose notification registration came over the token file in the last month.
    pub(crate) recent: usize,
    /// Phones paired over SSH since the update.
    pub(crate) paired_since_update: usize,
}

fn ssh_phones(paths: &GxserverPaths, taken_at_ms: i64, now_ms: i64) -> SshPhones {
    let recent = crate::push::PushRegistrationStore::open(paths)
        .read()
        .map(|data| {
            data.registrations
                .iter()
                .filter(|registration| registration.device_id.is_none())
                .filter(|registration| {
                    chrono::DateTime::parse_from_rfc3339(&registration.registered_at)
                        .is_ok_and(|at| at.timestamp_millis() >= now_ms - UNUSED_FOR_MS)
                })
                .count()
        })
        .unwrap_or(0);
    let taken_at = chrono::DateTime::from_timestamp_millis(taken_at_ms)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default();
    let paired_since_update = crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| {
            db.query_row(
                &format!(
                    "SELECT COUNT(*) FROM remote_paired_devices WHERE {PAIRED_OVER_SSH} AND pairedAt >= ?1"
                ),
                [taken_at],
                |row| row.get::<_, i64>(0),
            )
            .ok()
        })
        .unwrap_or(0) as usize;
    SshPhones {
        recent,
        paired_since_update,
    }
}

/// The remote computers this one reaches over SSH (Tailscale, the local network) and not turned off.
pub(crate) fn ssh_computers(settings: Option<&Value>) -> Vec<Value> {
    settings
        .and_then(|settings| settings.get("remoteMachines"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|machine| {
            machine.get("transport").and_then(Value::as_str) != Some("easyConnect")
                && machine.get("disabled").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|machine| {
            let name = machine.get("name").and_then(Value::as_str)?.trim();
            (!name.is_empty()).then(|| {
                json!({
                    "id": machine.get("id").and_then(Value::as_str).unwrap_or(name),
                    "name": name,
                })
            })
        })
        .collect()
}

/// [`phone_statuses`], with the phones waiting for approval right now (pairing v2): a row that
/// still waits becomes `Pending` when a waiting phone has its name and platform (one waiting phone
/// per row, oldest first), and carries that phone's entry (its words and approval request).
pub(crate) fn phone_statuses_with_pending(
    phones: &[NoticePhone],
    current: &[CurrentPhone],
    pending: &[PendingDevice],
    taken_at_ms: i64,
) -> Vec<(RowStatus, Option<PendingDevice>)> {
    let mut waiting: Vec<&PendingDevice> = pending.iter().collect();
    phone_statuses(phones, current, taken_at_ms)
        .into_iter()
        .zip(phones)
        .map(|(status, phone)| {
            if status != RowStatus::Waiting {
                return (status, None);
            }
            match waiting
                .iter()
                .position(|device| device.name == phone.name && device.platform == phone.platform)
            {
                Some(at) => (RowStatus::Pending, Some(waiting.remove(at).clone())),
                None => (status, None),
            }
        })
        .collect()
}

/// SEAM (pairing v2, GitHub #495): whether each phone of the notice was paired again.
///
/// Today: a phone is paired again when a pairing made after the update has the same name and
/// platform (each new pairing ticks off one old row, newest first), and removed when its old row
/// is gone with no such pairing.
///
/// Pairing v2: a re-paired phone is a new `paired` row while the old one stays `legacy` until
/// removed, which the rule above reads; a phone waiting on the owner's approval becomes `pending`
/// with its three words in [`phone_statuses_with_pending`].
pub(crate) fn phone_statuses(
    phones: &[NoticePhone],
    current: &[CurrentPhone],
    taken_at_ms: i64,
) -> Vec<RowStatus> {
    let taken_at = chrono::DateTime::from_timestamp_millis(taken_at_ms);
    // A phone paired over its SSH login is the SSH row's, never an Easy Connect row's re-pairing.
    let mut newer: Vec<&CurrentPhone> = current
        .iter()
        .filter(|phone| !phone.over_ssh)
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
            } else {
                match current.iter().find(|now| now.id == phone.id) {
                    // The same row now holds its own key.
                    Some(now) if now.status == "paired" => RowStatus::Paired,
                    Some(_) => RowStatus::Waiting,
                    None => RowStatus::Removed,
                }
            }
        })
        .collect()
}

/// SEAM (pairing v2, GitHub #495): whether each computer of the notice was paired again: a
/// pairing with its Easy Connect address recorded since the update
/// (`remote_access::computer_pairing`, CDXC:Security 2026-11-08 FIX there), or a new address in
/// its Settings > Remote entry; or removed (gone, turned off, or moved off Easy Connect).
pub(crate) fn computer_statuses(
    computers: &[NoticeComputer],
    settings: Option<&Value>,
) -> Vec<RowStatus> {
    computer_statuses_with_pairings(computers, settings, &[])
}

/// [`computer_statuses`] with the Easy Connect addresses this computer paired with since the update.
pub(crate) fn computer_statuses_with_pairings(
    computers: &[NoticeComputer],
    settings: Option<&Value>,
    paired_addresses: &[String],
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
                Some(entry) if paired_addresses.contains(&entry.address) => RowStatus::Paired,
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
    view_with_pending(notice, current, &[], settings, now_ms)
}

/// [`view`] with the phones waiting for approval: a `pending` row adds `words` and `approvalId`.
pub(crate) fn view_with_pending(
    notice: &UpgradeNotice,
    current: &[CurrentPhone],
    pending: &[PendingDevice],
    settings: Option<&Value>,
    now_ms: i64,
) -> Value {
    view_with_ssh(
        notice,
        current,
        pending,
        &SshPhones::default(),
        settings,
        now_ms,
    )
}

/// [`view_with_pending`] with the phones that sign in over SSH and the computers reached over SSH.
pub(crate) fn view_with_ssh(
    notice: &UpgradeNotice,
    current: &[CurrentPhone],
    pending: &[PendingDevice],
    ssh: &SshPhones,
    settings: Option<&Value>,
    now_ms: i64,
) -> Value {
    view_with_pairings(notice, current, pending, ssh, &[], settings, now_ms)
}

/// [`view_with_ssh`] with the Easy Connect addresses this computer paired with since the update.
pub(crate) fn view_with_pairings(
    notice: &UpgradeNotice,
    current: &[CurrentPhone],
    pending: &[PendingDevice],
    ssh: &SshPhones,
    paired_addresses: &[String],
    settings: Option<&Value>,
    now_ms: i64,
) -> Value {
    let ssh_pending = pending.iter().find(|device| device.over_ssh);
    let ssh_pending_devices: Vec<&PendingDevice> =
        pending.iter().filter(|device| device.over_ssh).collect();
    let pending: Vec<PendingDevice> = pending
        .iter()
        .filter(|device| !device.over_ssh)
        .cloned()
        .collect();
    let pending = pending.as_slice();
    // A record from before the rows were kept has names only: each name stands for an old
    // pairing still on file with that name (one pairing per name), or for nothing.
    let legacy_names = notice.phone_rows.is_empty()
        && notice.computer_rows.is_empty()
        && !(notice.phones.is_empty() && notice.computers.is_empty());
    let (phone_rows, computer_rows) = if legacy_names {
        rows_from_names(notice, current, settings)
    } else {
        (notice.phone_rows.clone(), notice.computer_rows.clone())
    };
    let forgotten = |id: &str| notice.forgotten.iter().any(|gone| gone == id);
    let mut phones = Vec::new();
    let mut computers = Vec::new();
    let recent: Vec<NoticePhone> = phone_rows
        .iter()
        .filter(|phone| !forgotten(&phone.id))
        .filter(|phone| recently_used(phone, notice.taken_at_ms))
        .cloned()
        .collect();
    let statuses = phone_statuses_with_pending(&recent, current, pending, notice.taken_at_ms);
    let leftovers = phone_leftovers(&recent, &statuses, current, &ssh_pending_devices);
    for ((phone, (status, waiting)), leftover) in recent.iter().zip(statuses).zip(leftovers) {
        if status == RowStatus::Removed {
            continue;
        }
        let mut row = json!({
            "id": phone.id,
            "name": phone.name,
            "kind": phone_kind(&phone.platform),
            "lastUsedAt": phone.last_used_at,
            "status": status.wire(),
        });
        if let Some(reason) = leftover {
            row["status"] = json!("leftover");
            row["leftover"] = json!(reason);
        }
        if let Some(waiting) = waiting {
            row["words"] = json!(waiting.words);
            row["approvalId"] = json!(waiting.approval_id);
            row["pendingId"] = json!(waiting.id);
        }
        phones.push(row);
    }
    let computer_rows: Vec<NoticeComputer> = computer_rows
        .into_iter()
        .filter(|computer| !forgotten(&computer.id))
        .collect();
    for (computer, status) in computer_rows.iter().zip(computer_statuses_with_pairings(
        &computer_rows,
        settings,
        paired_addresses,
    )) {
        if status == RowStatus::Removed {
            continue;
        }
        computers.push(json!({
            "id": computer.id,
            "name": computer.name,
            "status": status.wire(),
        }));
    }
    /*
    CDXC:Security 2026-11-09 DECISION:
    Discord #ios (11.2.0): "in macos app it's asking me to pair my phone again (but says that I need to turn Easy Connect on, but I have my iphone connected through tailscale). I clicked on remind me later, and it still works." User: "shouldn't show it if you didn't use easy pass before." A phone that signs in over Tailscale or SSH keeps connecting after the update, so it never makes the notice open, the "can't connect until you pair it again" line show, or the count go up on its own: its row shows only once such a phone asks to pair (its words wait for Approve) or has paired. This replaces the 2026-11-08 decision above that opened the notice for phones seen over SSH lately.
    */
    let ssh_phones = if ssh_pending.is_some() || ssh.paired_since_update > 0 {
        let status = if ssh_pending.is_some() {
            RowStatus::Pending
        } else {
            RowStatus::Paired
        };
        let mut row = json!({
            "id": "ssh-phones",
            "count": ssh.recent.max(ssh.paired_since_update).max(1),
            "status": status.wire(),
        });
        if let Some(device) = ssh_pending {
            row["name"] = json!(device.name);
            row["kind"] = json!(phone_kind(&device.platform));
            row["words"] = json!(device.words);
            row["approvalId"] = json!(device.approval_id);
            row["pendingId"] = json!(device.id);
        }
        vec![row]
    } else {
        Vec::new()
    };
    let ssh_computers = ssh_computers(settings);
    /*
    CDXC:Security 2026-11-09 FIX:
    User (9 Nov): "please why do i see 9 devices here??? i don't have 9 devices connected even, there's something wrong". `waiting` is what Settings > Remote's "N phones and computers still use the old pairing and can't connect" line counts, so it counts only rows that still wait to be paired again: not a device waiting for Approve (`pending`, counted apart; it already asked with the new pairing), not a leftover (never used, an older pairing of the same phone, a phone that signs in over SSH now), and not a name from a names-only record whose pairing is gone. The notice still opens by itself for a pending device.
    */
    let count = |status: &str| {
        phones
            .iter()
            .chain(computers.iter())
            .chain(ssh_phones.iter())
            .filter(|row| row["status"] == status)
            .count()
    };
    let (waiting, pending_count) = (count("waiting"), count("pending"));
    let due = !notice.seen && notice.remind_at_ms.is_none_or(|at| now_ms >= at);
    json!({
        "show": due && waiting + pending_count > 0,
        "waiting": waiting,
        "pending": pending_count,
        "leftovers": count("leftover"),
        "phones": phones,
        "computers": computers,
        "sshPhones": ssh_phones,
        "sshComputers": ssh_computers,
        "remindersLeft": notice.reminders_left,
        "takenAtMs": notice.taken_at_ms,
    })
}

/// The rows of a names-only record (the first 11.0.1 build kept only names): each phone name
/// takes one old pairing still on file with that name, paired before the list was taken and not
/// yet holding its own key; each computer name takes the Easy Connect computer saved with that
/// name. A name with nothing left (the phone was removed, the computer is gone) has no row.
fn rows_from_names(
    notice: &UpgradeNotice,
    current: &[CurrentPhone],
    settings: Option<&Value>,
) -> (Vec<NoticePhone>, Vec<NoticeComputer>) {
    let taken_at = chrono::DateTime::from_timestamp_millis(notice.taken_at_ms);
    let mut old: Vec<&CurrentPhone> = current
        .iter()
        .filter(|phone| phone.status != "paired" && !phone.over_ssh)
        .filter(|phone| {
            match (
                chrono::DateTime::parse_from_rfc3339(&phone.paired_at),
                taken_at,
            ) {
                (Ok(paired), Some(taken)) => paired.with_timezone(&chrono::Utc) < taken,
                _ => true,
            }
        })
        .collect();
    let phones = notice
        .phones
        .iter()
        .filter_map(|name| {
            let at = old.iter().position(|phone| &phone.name == name)?;
            let phone = old.remove(at);
            Some(NoticePhone {
                id: phone.id.clone(),
                name: phone.name.clone(),
                platform: phone.platform.clone(),
                last_used_at: Some(
                    phone
                        .last_seen_at
                        .clone()
                        .unwrap_or_else(|| phone.paired_at.clone()),
                ),
            })
        })
        .collect();
    let mut saved = easy_connect_computers(settings);
    let computers = notice
        .computers
        .iter()
        .filter_map(|name| {
            let at = saved.iter().position(|computer| &computer.name == name)?;
            Some(saved.remove(at))
        })
        .collect();
    (phones, computers)
}

/// Why each still-waiting phone row needs nothing after all, or `None` for a row that counts:
/// `ssh` (the same phone signs in over Tailscale or SSH now: paired over its SSH login, or asking
/// to), `neverUsed` (its pairing never reached this computer), `duplicate` (an older pairing of a
/// phone with a newer one in the list, or one already paired again or asking to).
fn phone_leftovers(
    phones: &[NoticePhone],
    statuses: &[(RowStatus, Option<PendingDevice>)],
    current: &[CurrentPhone],
    ssh_pending: &[&PendingDevice],
) -> Vec<Option<&'static str>> {
    let used = |phone: &NoticePhone| phone.last_used_at.clone().unwrap_or_default();
    phones
        .iter()
        .zip(statuses)
        .enumerate()
        .map(|(index, (phone, (status, _)))| {
            if *status != RowStatus::Waiting {
                return None;
            }
            let over_ssh = current
                .iter()
                .any(|now| now.over_ssh && now.same_device(phone))
                || ssh_pending
                    .iter()
                    .any(|device| device.name == phone.name && device.platform == phone.platform);
            if over_ssh {
                return Some("ssh");
            }
            let row = current.iter().find(|now| now.id == phone.id);
            if row.is_some_and(|row| row.last_seen_at.is_none()) {
                return Some("neverUsed");
            }
            let newer_or_done = phones.iter().zip(statuses).enumerate().any(
                |(other, (same, (other_status, _)))| {
                    other != index
                        && same.name == phone.name
                        && same.platform == phone.platform
                        && match other_status {
                            RowStatus::Paired | RowStatus::Pending => true,
                            RowStatus::Waiting => {
                                let seen = current
                                    .iter()
                                    .find(|now| now.id == same.id)
                                    .is_some_and(|now| now.last_seen_at.is_some());
                                seen && (used(same), other) > (used(phone), index)
                            }
                            RowStatus::Removed => false,
                        }
                },
            );
            newer_or_done.then_some("duplicate")
        })
        .collect()
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
    pending: &[PendingDevice],
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> crate::server::RoutedResponse {
    use axum::http::StatusCode;
    let params = crate::domain::read_domain_rpc_params(body).unwrap_or_default();
    match handle_endpoint_with_pending(paths, pending, &params) {
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

/// `op`: `read` (default), `later`, `done` (`dismiss` reads as `done`) or `forget` (with `id`). Answers the view.
pub(crate) fn handle_endpoint(
    paths: &GxserverPaths,
    params: &serde_json::Map<String, Value>,
) -> Result<Value> {
    handle_endpoint_with_pending(paths, &[], params)
}

/// [`handle_endpoint`] with the phones waiting for approval (`remote_access::waiting_devices`).
pub(crate) fn handle_endpoint_with_pending(
    paths: &GxserverPaths,
    pending: &[PendingDevice],
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
        // Forget on a row that needs nothing: that old record leaves the notice, nothing else.
        Some("forget") => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .context("forget needs the row's id")?;
            if !notice.forgotten.iter().any(|gone| gone == id) {
                notice.forgotten.push(id.to_string());
                write(&notice_path(paths), &notice)?;
            }
        }
        _ => {}
    }
    let paired_addresses: Vec<String> =
        crate::remote_access::computer_pairing::read_paired_computers(paths)
            .into_iter()
            .filter(|paired| paired.paired_at_ms >= notice.taken_at_ms)
            .map(|paired| paired.address)
            .collect();
    Ok(view_with_pairings(
        &notice,
        &current_phones(paths),
        pending,
        &ssh_phones(paths, notice.taken_at_ms, now_ms),
        &paired_addresses,
        crate::shared_settings_file::read_shared_settings(paths).as_ref(),
        now_ms,
    ))
}
