// C1 wave-3 re-cluster: the titlebar Resources panel snapshot types and the titlebar Git menu state/action types, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy)]
pub(crate) struct GpuiNativeTitlebarTip {
    pub(crate) body: &'static str,
    pub(crate) icon_path: &'static str,
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceProcess {
    pub(crate) command: String,
    pub(crate) cpu: f64,
    pub(crate) pid: u32,
    pub(crate) ppid: u32,
    pub(crate) memory_mb: f64,
    pub(crate) system_pid: u32,
}

/// One TCP listener sampled for the titlebar Resources "Dev Servers" section.
#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceServer {
    pub(crate) label: String,
    pub(crate) pid: u32,
    pub(crate) port: u16,
    pub(crate) url: String,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceRow {
    pub(crate) action: GpuiNativeResourceAction,
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) children: Vec<GpuiNativeResourceChild>,
    pub(crate) cpu: f64,
    pub(crate) detail: String,
    pub(crate) icon_path: &'static str,
    pub(crate) label: String,
    pub(crate) memory_mb: f64,
    pub(crate) pids: Vec<u32>,
    pub(crate) session_id: Option<String>,
    /// True when Sleep Inactive would put this row to sleep: an awake session
    /// whose agent is idle (not working, not waiting for attention).
    pub(crate) sleep_candidate: bool,
    pub(crate) url: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiNativeResourceChild {
    pub(crate) cpu: f64,
    pub(crate) label: String,
    pub(crate) memory_mb: f64,
    pub(crate) pid: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiNativeResourceAction {
    Browser(BrowserTabId),
    Code,
    None,
    Orphan,
    Server,
    Session,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiNativeResourcesSnapshot {
    pub(crate) browser_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) code_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) inactive_terminal_sleep_count: usize,
    pub(crate) orphan_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) project_label: String,
    pub(crate) server_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) session_rows: Vec<GpuiNativeResourceRow>,
    pub(crate) sleep_all_session_count: usize,
    pub(crate) total_cpu: f64,
    pub(crate) total_memory_mb: f64,
}

/// Fixed selector set for titlebar Git menu rows. Menu selections dispatch
/// only one of these validated selectors back into the sidebar runtime's
/// `runSidebarGitAction` path; labels, branch text, and reasons from the
/// renderer never become action payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarGitMenuActionId {
    Commit,
    Push,
    Pr,
    SyncMain,
    SyncRemote,
    MultiRelease,
    Release,
}

impl GpuiTitlebarGitMenuActionId {
    pub(crate) fn from_selector(value: &str) -> Option<Self> {
        match value {
            "commit" => Some(Self::Commit),
            "push" => Some(Self::Push),
            "pr" => Some(Self::Pr),
            "syncMain" => Some(Self::SyncMain),
            "syncRemote" => Some(Self::SyncRemote),
            "multiRelease" => Some(Self::MultiRelease),
            "release" => Some(Self::Release),
            _ => None,
        }
    }

    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::Pr => "pr",
            Self::SyncMain => "syncMain",
            Self::SyncRemote => "syncRemote",
            Self::MultiRelease => "multiRelease",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarGitMenuRow {
    pub(crate) action: GpuiTitlebarGitMenuActionId,
    pub(crate) disabled: bool,
    pub(crate) label: String,
    pub(crate) primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarGitMenuState {
    pub(crate) additions: u64,
    pub(crate) ahead_count: u64,
    pub(crate) behind_count: u64,
    pub(crate) branch: Option<String>,
    pub(crate) deletions: u64,
    pub(crate) has_working_tree_changes: bool,
    pub(crate) is_busy: bool,
    pub(crate) is_repo: bool,
    pub(crate) primary_action: GpuiTitlebarGitMenuActionId,
    pub(crate) rows: Vec<GpuiTitlebarGitMenuRow>,
    pub(crate) sync_remote_disabled: bool,
}

pub(crate) fn gpui_titlebar_git_menu_state_from_payload(
    payload: &str,
) -> Option<GpuiTitlebarGitMenuState> {
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(serde_json::Value::as_str)
        != Some(GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE)
        || object.get("version").and_then(serde_json::Value::as_u64)
            != Some(GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION)
    {
        return None;
    }
    let rows_value = object.get("rows").and_then(serde_json::Value::as_array)?;
    if rows_value.len() > GPUI_TITLEBAR_GIT_MENU_MAX_ROWS {
        return None;
    }
    let mut rows = Vec::with_capacity(rows_value.len());
    for row_value in rows_value {
        let row = row_value.as_object()?;
        let action = GpuiTitlebarGitMenuActionId::from_selector(
            row.get("action").and_then(serde_json::Value::as_str)?,
        )?;
        let label = bounded_gpui_titlebar_git_menu_text(
            row.get("label").and_then(serde_json::Value::as_str)?,
            GPUI_TITLEBAR_GIT_MENU_ROW_LABEL_MAX_CHARS,
        )?;
        rows.push(GpuiTitlebarGitMenuRow {
            action,
            disabled: row.get("disabled").and_then(serde_json::Value::as_bool) == Some(true),
            label,
            primary: row.get("primary").and_then(serde_json::Value::as_bool) == Some(true),
        });
    }
    let branch = match object.get("branch") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(branch)) => {
            bounded_gpui_titlebar_git_menu_text(branch, GPUI_TITLEBAR_GIT_MENU_BRANCH_MAX_CHARS)
        }
        Some(_) => return None,
    };
    Some(GpuiTitlebarGitMenuState {
        additions: object
            .get("additions")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        ahead_count: object
            .get("aheadCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        behind_count: object
            .get("behindCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        branch,
        deletions: object
            .get("deletions")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        has_working_tree_changes: object
            .get("hasWorkingTreeChanges")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        is_busy: object.get("isBusy").and_then(serde_json::Value::as_bool) == Some(true),
        is_repo: object.get("isRepo").and_then(serde_json::Value::as_bool) == Some(true),
        primary_action: object
            .get("primaryAction")
            .and_then(serde_json::Value::as_str)
            .and_then(GpuiTitlebarGitMenuActionId::from_selector)?,
        rows,
        sync_remote_disabled: object
            .get("syncRemoteDisabled")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    })
}

pub(crate) fn bounded_gpui_titlebar_git_menu_text(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(max_chars).collect())
}
