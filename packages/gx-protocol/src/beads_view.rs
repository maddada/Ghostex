//! `/api/beadsView`: the Beads view's snapshot of one project's Beads workspace.
//!
//! gxserver builds it from bd's own answers (`bd list --all -n 0 --json` and
//! `bd ready --explain --json --limit 0`), so every fact here is bd's. Rows are slim: no
//! description, design or notes; the details panel reads those with the `show` action.

use serde::{Deserialize, Serialize};

/// What gxserver found for the project's Beads workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BeadsWorkspaceState {
    /// A Beads workspace is there and was read.
    Ready,
    /// bd found no Beads workspace for this project (the setup card).
    #[default]
    None,
    /// bd is not installed on the computer (the Install Beads card).
    NoBd,
    /// bd could not say (an error, a timeout).
    Failed,
}

/// Whether `ready` and `blockedBy` can be trusted in this snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BeadsReadiness {
    #[default]
    Ok,
    /// bd's ready read failed; rows are still current.
    Unavailable,
    /// The ready read and the list read disagreed (a write landed between them in server mode),
    /// so readiness is held back until the next refresh.
    Paused,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsWorkspaceInfo {
    #[serde(default)]
    pub state: BeadsWorkspaceState,
    /// Stable id of the workspace (its canonical `.beads` path, hashed). A change means another
    /// workspace: the client forgets what it knew.
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub redirected: bool,
    /// `embedded` or `server` (bd's Dolt mode).
    #[serde(default)]
    pub mode: String,
    /// The name bd records as "you" on this computer (`$BEADS_ACTOR`, git user.name, `$USER`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsViewRow {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<String>,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub comment_count: u64,
    /// bd's ready answer; absent when readiness is unavailable or paused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<bool>,
    /// The beads this one waits on, from bd's blocked answer; absent when readiness is not ok.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<Vec<String>>,
    /// The beads waiting on this one (reverse of `blocks` dependencies).
    #[serde(default)]
    pub blocks: Vec<String>,
    /// Flagged for a person (`bd human`, the `human` label) and not closed.
    #[serde(default)]
    pub needs_decision: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsSyncRemote {
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsSyncRunning {
    /// `pull` or `push`.
    pub kind: String,
    pub started_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsSyncResult {
    /// `pull` or `push`.
    pub kind: String,
    pub ok: bool,
    /// `ok`, `notFastForward`, `conflict`, `noRemote`, `auth`, `network`, `timeout` or `other`.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    /// bd's own output, for Details.
    #[serde(default)]
    pub output: String,
    pub at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsSyncInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<BeadsSyncRemote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<BeadsSyncRunning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_push_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_pull_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_result: Option<BeadsSyncResult>,
}

/// The Board's built-in lanes a project may hide (Todo, In Progress and Done always show), as
/// `boardHiddenColumnsGet`/`Set` store them in the workspace's bd config.
pub const HIDEABLE_BOARD_COLUMNS: [&str; 3] = ["backlog", "test", "review"];

/// What a computer's gxserver can do for the Beads view. An older gxserver has no
/// `/api/beadsView`, so the client treats "not found" as "none of these".
pub mod capability {
    pub const CREATE_TYPE_PARENT: &str = "createTypeParent";
    pub const CREATE_BLOCKED_BY: &str = "createBlockedBy";
    pub const CLAIM: &str = "claim";
    pub const REOPEN: &str = "reopen";
    pub const DEFER: &str = "defer";
    pub const HUMAN: &str = "human";
    pub const SYNC: &str = "sync";
    pub const GENERATE_TITLE: &str = "generateTitle";
    pub const ALL: [&str; 8] = [
        CREATE_TYPE_PARENT,
        CREATE_BLOCKED_BY,
        CLAIM,
        REOPEN,
        DEFER,
        HUMAN,
        SYNC,
        GENERATE_TITLE,
    ];
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeadsViewSnapshot {
    #[serde(default)]
    pub workspace: BeadsWorkspaceInfo,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Opaque; compare for equality only. It names the gxserver run and the workspace, so a
    /// restart or another workspace never matches an old value.
    #[serde(default)]
    pub revision: String,
    /// True when the request's `knownRevision` is current; `beads` is then omitted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unchanged: bool,
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub settling: bool,
    #[serde(default)]
    pub readiness: BeadsReadiness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// bd's custom statuses (`status.custom`), in order.
    #[serde(default)]
    pub custom_statuses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beads: Option<Vec<BeadsViewRow>>,
    #[serde(default)]
    pub sync: BeadsSyncInfo,
}

impl BeadsViewSnapshot {
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == name)
    }
}
