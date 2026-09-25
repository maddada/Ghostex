//! The branch switcher: this conversation's family and the menu it opens.
//!
//! Ports of `packages/shared/session-chat-presentation/fork-branches.ts` and
//! `packages/shared/session-chat-controller/native-fork-branches.ts`.
//!
//! CDXC:SessionFork 2026-09-18 SEE-ALSO:
//! The branch switcher's copy and row rules, drawn by
//! `apps/desktop/src/app/native_chat/fork_branches.rs`. The renderer may not re-derive a title, a
//! subtitle, the lifecycle tone, or the family gate.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One branch as `/api/sessionForkBranches` reports it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkBranch {
    pub project_id: String,
    pub session_id: String,
    #[serde(default)]
    pub title: String,
    /// `running`, `sleeping`, `stopped`, or whatever a newer daemon reports.
    #[serde(default)]
    pub lifecycle_state: String,
    #[serde(default)]
    pub current: bool,
    /// This branch is an earlier thread the current one grew out of.
    #[serde(default)]
    pub ancestor: bool,
    /// Epoch milliseconds, or 0 when the daemon reported none.
    #[serde(default)]
    pub last_active_ms: f64,
}

/// One branch is not a family: there is nothing to switch to.
pub const SESSION_CHAT_FORK_BRANCH_FAMILY_MIN: usize = 2;

pub const SESSION_CHAT_FORK_BRANCH_MENU_LABEL: &str = "Branches";

pub const SESSION_CHAT_FORK_BRANCH_CURRENT_LABEL: &str = "Current";

/// The lifecycle dot's meaning; each renderer owns the colour it paints for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForkBranchTone {
    Running,
    Sleeping,
    Stopped,
}

impl ForkBranchTone {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Stopped => "stopped",
        }
    }
}

/// `sessionChatForkBranchTone`.
pub fn fork_branch_tone(branch: &ForkBranch) -> ForkBranchTone {
    if branch.lifecycle_state == "running" {
        return ForkBranchTone::Running;
    }
    if branch.lifecycle_state == "sleeping" {
        ForkBranchTone::Sleeping
    } else {
        ForkBranchTone::Stopped
    }
}

/// `sessionChatForkBranchTitle`.
pub fn fork_branch_title(branch: &ForkBranch) -> String {
    if branch.title.is_empty() {
        "Untitled session".to_string()
    } else {
        branch.title.clone()
    }
}

/// `sessionChatForkBranchKey`, the identity the family is keyed by.
pub fn fork_branch_key(branch: &ForkBranch) -> String {
    format!("{}:{}", branch.project_id, branch.session_id)
}

/// `formatRelativeTimeLabel(iso, { nowMs })` for a branch's last activity.
///
/// Only the compact sidebar form is reachable from here (`allowJustNow` defaults on), so this is
/// the whole of `packages/core-ui/relative-time.ts` the chat needs.
fn relative_time_label(last_active_ms: f64, now_ms: f64) -> String {
    let diff_ms = (now_ms - last_active_ms).max(0.0);
    let seconds = (diff_ms / 1000.0).floor();
    if seconds < 5.0 {
        return "just now".to_string();
    }
    if seconds < 60.0 {
        return format!("{seconds}s ago", seconds = seconds as i64);
    }
    let minutes = (seconds / 60.0).floor();
    if minutes < 60.0 {
        return format!("{minutes}m ago", minutes = minutes as i64);
    }
    let hours = (minutes / 60.0).floor();
    if hours < 24.0 {
        return format!("{hours}h ago", hours = hours as i64);
    }
    format!("{days}d ago", days = (hours / 24.0).floor() as i64)
}

/// `sessionChatForkBranchSubtitle`.
///
/// CDXC:SessionFork 2026-09-03:
/// Picking a STOPPED branch revives that same registry row (the hosts wake it before focusing),
/// so the row says so instead of looking inert.
pub fn fork_branch_subtitle(branch: &ForkBranch, now_ms: f64) -> String {
    let last_active = if branch.last_active_ms.is_finite() && branch.last_active_ms > 0.0 {
        relative_time_label(branch.last_active_ms, now_ms)
    } else {
        String::new()
    };
    [
        if branch.ancestor {
            "Earlier thread"
        } else {
            ""
        },
        if branch.lifecycle_state == "stopped" && !branch.current {
            "Resumes when opened"
        } else {
            ""
        },
        last_active.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// `sessionChatForkBranchTooltip`.
pub fn fork_branch_tooltip(count: usize) -> String {
    format!("This conversation has {count} branches that share earlier history.")
}

/// One projected row.
#[derive(Clone, Debug, PartialEq)]
pub struct ForkBranchRow {
    pub key: String,
    pub title: String,
    /// "Earlier thread · Resumes when opened · 4h ago", already joined; empty when nothing
    /// applies.
    pub subtitle: String,
    pub tone: ForkBranchTone,
    pub current: bool,
}

/// `sessionChatForkBranchRows`: the whole control, or `None` when the daemon reported no family.
///
/// The strip renders nothing at all until there are two or more rows, which is why an unforked
/// session shows no empty row. The rows keep the daemon's own order (newest activity first).
pub fn fork_branch_rows(branches: &[ForkBranch], now_ms: f64) -> Option<Vec<ForkBranchRow>> {
    if branches.len() < SESSION_CHAT_FORK_BRANCH_FAMILY_MIN {
        return None;
    }
    Some(
        branches
            .iter()
            .map(|branch| ForkBranchRow {
                key: fork_branch_key(branch),
                title: fork_branch_title(branch),
                subtitle: fork_branch_subtitle(branch, now_ms),
                tone: fork_branch_tone(branch),
                current: branch.current,
            })
            .collect(),
    )
}

/// `NativeForkBranches.project()`: the `forkBranches` document key, or `null`.
///
/// Ready-made `option_menu` rows: the heading, then one row per branch with the lifecycle dot the
/// branch list paints beside its title.
pub fn fork_branches_projection(branches: &[ForkBranch], now_ms: f64) -> Value {
    let Some(rows) = fork_branch_rows(branches, now_ms) else {
        return Value::Null;
    };
    let mut menu = vec![json!({
        "id": "fork-branches-heading",
        "heading": true,
        "label": SESSION_CHAT_FORK_BRANCH_MENU_LABEL,
    })];
    for (row, branch) in rows.iter().zip(branches.iter()) {
        let mut item = serde_json::Map::new();
        item.insert("id".into(), json!(row.key));
        item.insert("label".into(), json!(row.title));
        if !row.subtitle.is_empty() {
            item.insert("description".into(), json!(row.subtitle));
        }
        if row.current {
            item.insert(
                "detail".into(),
                json!(SESSION_CHAT_FORK_BRANCH_CURRENT_LABEL),
            );
        }
        item.insert("dot".into(), json!(row.tone.as_str()));
        item.insert("disabled".into(), json!(row.current));
        item.insert(
            "command".into(),
            json!({
                "type": "selectForkBranch",
                "projectId": branch.project_id,
                "sessionId": branch.session_id,
                "lifecycleState": branch.lifecycle_state,
            }),
        );
        menu.push(Value::Object(item));
    }
    json!({
        "count": rows.len(),
        "tooltip": fork_branch_tooltip(rows.len()),
        "menu": menu,
    })
}
