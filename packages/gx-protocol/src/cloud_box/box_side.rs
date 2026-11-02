//! The box-side contract: the `ghostex box …` verbs home gxserver (or a phone) runs on a box
//! through the provider's exec API, the files it writes through the provider's file API, and the
//! JSON each verb prints. Implemented by box mode (`server/src/cloud_boxes/box_mode/`, W2).
//!
//! Every verb prints exactly one JSON object on stdout (`{"ok": true, …}` or
//! `{"ok": false, "error": "…", "code": "…"}`) and exits non-zero on failure.
//!
//! Exec command lines can end up in a provider's operation log, so nothing secret is ever on
//! one: the boot ticket carries only ids and public keys, and logins, secrets and the first
//! message travel as files written into the box's private folder ([`box_private_dir`]).

use serde::{Deserialize, Serialize};

use super::enums::CloudProviderId;

/// `ghostex box` verb names.
pub mod box_verbs {
    /// Reset a copied identity, write the allow-list, start gxserver and tailcat, print
    /// [`super::BoxBootResult`]. Safe to run again (a resume re-runs it).
    pub const BOOT: &str = "boot";
    /// Fetch, branch, copy ticked secret files into the checkout and start the agent session with
    /// the first message; reads [`super::BoxPrepareSessionRequest`] from `--request <file>`,
    /// prints [`super::BoxPrepareSessionResult`].
    pub const PREPARE_SESSION: &str = "prepare-session";
    /// Before Save as snapshot: remove the private-folder links, the secret files copied into
    /// checkouts and the box's identity (tailcat key, gxserver token, machine id). Prints
    /// [`super::BoxSnapshotPrepResult`].
    pub const PREPARE_SNAPSHOT: &str = "prepare-snapshot";
    /// After the snapshot is taken: put back what `prepare-snapshot` removed.
    pub const FINISH_SNAPSHOT: &str = "finish-snapshot";
    /// Print [`super::BoxIdleStatus`].
    pub const IDLE_STATUS: &str = "idle-status";
    /// Print [`super::BoxWorkStatus`] (D13: uncommitted or unpushed work blocks a delete).
    pub const WORK_STATUS: &str = "work-status";
    /// Print refreshed logins waiting to go home ([`super::BoxWriteBackResult`]); `--ack <ids>`
    /// drops the ones home saved (D6, Codex `auth.json`).
    pub const WRITE_BACK: &str = "write-back";
    /// `local` provider only: `--listen <addr> --to <addr>` forwards TCP so Docker can publish
    /// the box gxserver (loopback-only in the container) on the computer's loopback. Real
    /// providers use tailcat.
    pub const RELAY: &str = "relay";

    pub const ALL: [&str; 8] = [
        BOOT,
        PREPARE_SESSION,
        PREPARE_SNAPSHOT,
        FINISH_SNAPSHOT,
        IDLE_STATUS,
        WORK_STATUS,
        WRITE_BACK,
        RELAY,
    ];
}

/// The `/api/cloudBoxes` actions a box-mode gxserver answers over the box link (same JSON as the
/// verbs above), so a connected client need not exec.
pub mod box_actions {
    pub const IDLE_STATUS: &str = "boxIdleStatus";
    pub const WORK_STATUS: &str = "boxWorkStatus";
    pub const WRITE_BACK: &str = "boxWriteBack";
}

/// The box's private folder for logins and secrets: tmpfs, mode 0700, never in a snapshot.
/// Boat lets the file API write only under `/home/user` and `/tmp`, and its snapshots never save
/// `/tmp`, so Boat uses `/tmp/ghostex`; every other provider uses `/run/ghostex`.
pub fn box_private_dir(provider: &CloudProviderId) -> &'static str {
    match provider {
        CloudProviderId::Boat => "/tmp/ghostex",
        _ => "/run/ghostex",
    }
}

/// File names inside [`box_private_dir`].
pub mod box_files {
    /// [`super::BoxSecretsFile`].
    pub const SECRETS: &str = "secrets.json";
    /// [`super::BoxPrepareSessionRequest`] of the next `prepare-session`.
    pub const PREPARE_SESSION_REQUEST: &str = "prepare-session.json";
}

/// Where the Linux gxserver package is installed on a box (its `bin/` holds gxserver, `ghostex`,
/// zmx and tailcat). Snapshots keep it, so a box from a snapshot boots without a download.
pub const BOX_PACKAGE_DIR: &str = "/opt/ghostex";
/// The `ghostex` CLI on a box.
pub const BOX_GHOSTEX_BIN: &str = "/opt/ghostex/bin/ghostex";

/// The current [`BoxBootTicket::version`].
pub const BOX_BOOT_TICKET_VERSION: u64 = 1;

/// `ghostex box boot --ticket <ticket>`: the ticket is this JSON, base64url-encoded without
/// padding. It carries no secret (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxBootTicket {
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub version: u64,
    pub box_id: String,
    pub provider: CloudProviderId,
    /// Home gxserver's `serverId` (sessions on the box record it as their home machine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_machine_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_project_id: Option<String>,
    /// Tailcat public keys of the user's devices allowed to reach the box (Q1021 private only).
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub allow_device_keys: Vec<String>,
    /// Minutes before an idle box pauses or stops itself (Q1011).
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub idle_stop_minutes: u64,
    /// The box stops itself when idle (Boat: with the per-box key Boat writes into every box).
    /// `false` where the provider pauses idle boxes itself (Instabox) or for `local`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub self_stop: bool,
    /// Start tailcat. `false` for `local`, whose gxserver port Docker publishes on loopback.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub tailcat: bool,
}

/// What `ghostex box boot` prints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxBootResult {
    pub box_id: String,
    /// The box's machine id after the reset (differs between two boxes from one snapshot).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub machine_id: String,
    /// The identity was copied from a snapshot (or another box) and was reset.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub identity_reset: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailcat_address: Option<String>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub gxserver_port: u64,
    pub gxserver_token: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub gxserver_version: String,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub protocol_version: u64,
}

/// `secrets.json` in the box's private folder, written by home (or the phone) before
/// `prepare-session`. Values never reach a snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxSecretsFile {
    /// Agent accounts offered on the box (the ones marked "Use on cloud boxes").
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub accounts: Vec<BoxInjectedAccount>,
    /// Ticked secret files: project-relative path and contents.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub files: Vec<BoxSecretFile>,
    /// Ticked variables: name and value.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub vars: Vec<BoxSecretVar>,
}

/// An agent login injected on the box (plan §2.5; D5 box token, D6 copy and send back).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxInjectedAccount {
    /// The account id on home gxserver.
    pub account_id: String,
    /// `claude` or `codex`.
    pub agent: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    /// `claudeBoxToken`: the one-year `claude setup-token` value for `CLAUDE_CODE_OAUTH_TOKEN`.
    /// `codexAuthJson`: the account's `auth.json`, refreshed on the computer just before.
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxSecretFile {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxSecretVar {
    pub name: String,
    pub value: String,
}

/// `prepare-session.json`: what `ghostex box prepare-session` sets up.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxPrepareSessionRequest {
    pub box_id: String,
    /// The repository to fetch (the home project's origin URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    /// Where the project lives on the box (kept by snapshots), e.g. `/data/home/code/ghostex`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub checkout_path: String,
    /// Branch to fetch and start from (Q1005: the latest base branch, not the snapshot's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// The new branch, named like a worktree branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Work in a new worktree on the box (a reused box's second session, or the tray's New
    /// worktree) instead of the checkout itself.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub worktree: bool,
    pub agent_id: String,
    /// [`BoxInjectedAccount::account_id`] the session runs on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_machine_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
}

/// What `prepare-session` prints.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxPrepareSessionResult {
    /// The session's ids on the box's gxserver.
    pub project_id: String,
    pub session_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
}

/// What `prepare-snapshot` / `finish-snapshot` print.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxSnapshotPrepResult {
    /// Paths removed (prepare) or put back (finish).
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub paths: Vec<String>,
    /// gxserver and tailcat were stopped for the snapshot (prepare) or started again (finish).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub services_restarted: bool,
}

/// What `idle-status` prints (Q1011: idle = no agent turn running and no terminal open).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxIdleStatus {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub idle: bool,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub running_turns: u64,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub open_terminals: u64,
    /// ISO-8601, while idle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_since: Option<String>,
    /// When the box pauses or stops itself, while idle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stops_at: Option<String>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub idle_stop_minutes: u64,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub self_stop: bool,
}

/// What `work-status` prints (D13).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxWorkStatus {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub has_unpushed_work: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub checkouts: Vec<BoxCheckoutWork>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxCheckoutWork {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub uncommitted_files: u64,
    /// Commits not on the branch's upstream (all commits when it has none).
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub unpushed_commits: u64,
}

/// What `write-back` prints: refreshed logins the box holds that home has not saved yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxWriteBackResult {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub items: Vec<BoxWriteBackItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxWriteBackItem {
    /// Pass back with `--ack` once saved.
    pub id: String,
    pub account_id: String,
    /// `codexAuthJson`.
    pub kind: String,
    pub value: String,
    pub refreshed_at: String,
}
