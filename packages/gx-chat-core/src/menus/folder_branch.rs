//! This folder's branch in the new chat's bar: the menu that switches the project's own folder to
//! another branch before the chat starts, and the line under the bar that says how the switch went
//! or asks before it goes ahead.
//!
//! CDXC:Worktrees 2026-11-16 DECISION:
//! User (Q3520 to Q3526, from Brewed Bytes on Discord): the bar's last part for This folder opens a
//! branch menu with sections Recent, Local and Remote (a remote branch only while it has no local
//! copy), a search that also offers "Create <name> from <current>", on the desktop and the phone,
//! and Option+B opens it as it does for a new worktree's base branch. A pick checks the branch out
//! at once. Another session working in the folder blocks it, with New worktree from that branch as
//! the way out; uncommitted changes ask first (switch and take them, new worktree, cancel).
//! gxserver does the checks and the checkout (`folderBranch` on `/api/draftWorktree`,
//! server/src/server/worktree_ops/folder_branch.rs); this module builds the rows and the prompt
//! both renderers draw.
//! SEE-ALSO: menus/draft_worktree.rs (`detail_part` and `pick` call in here),
//! apps/desktop/src/app/native_chat/draft_project_picker.rs and worktree_tray.rs,
//! apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx and MenuSheet.tsx.

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

pub const SECTION_RECENT: &str = "Recent";
pub const SECTION_LOCAL: &str = "Local";
pub const SECTION_REMOTE: &str = "Remote";

fn name(branch: &Value) -> Option<&str> {
    branch
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
}

fn is_remote(branch: &Value) -> bool {
    branch.get("remote").and_then(Value::as_bool) == Some(true)
}

fn command(branch: &str, remote: bool) -> Value {
    let mut command = json!({ "type": "setDraftWorktree", "folderBranch": branch });
    if remote {
        command["remote"] = json!(true);
    }
    command
}

/// The rows of This folder's branch menu, in sections: Recent (the current branch first, then the
/// ones the folder was on lately), Local (the default branch first, then the rest), Remote (only
/// those with no local branch of the same name). Each row is `{label, detail, checked, section,
/// command}`.
pub fn menu(tray: &Value) -> Vec<Value> {
    let list = |key: &str| {
        tray.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let branches = list("branches");
    let current = tray.get("currentBranch").and_then(Value::as_str);
    let default = tray.get("defaultBranch").and_then(Value::as_str);
    let local: Vec<&str> = branches
        .iter()
        .filter(|branch| !is_remote(branch))
        .filter_map(name)
        .collect();
    let note = |branch: &str, ago: &str| {
        if Some(branch) == current {
            "Current".to_string()
        } else if Some(branch) == default {
            "Default".to_string()
        } else {
            ago.to_string()
        }
    };
    let row = |branch: &str, detail: String, section: &str, remote: bool| {
        json!({
            "label": branch,
            "detail": detail,
            "checked": Some(branch) == current,
            "section": section,
            "command": command(branch, remote),
        })
    };
    let mut rows = Vec::new();
    let mut listed: Vec<String> = Vec::new();
    let mut recent = list("recentBranches");
    if recent.is_empty() {
        // An older gxserver sends no Recent list: the current branch still leads.
        if let Some(current) = current.filter(|current| local.contains(current)) {
            recent.push(json!({ "name": current, "ago": "" }));
        }
    }
    for entry in &recent {
        let Some(branch) = name(entry).filter(|branch| local.contains(branch)) else {
            continue;
        };
        let ago = entry.get("ago").and_then(Value::as_str).unwrap_or_default();
        rows.push(row(branch, note(branch, ago), SECTION_RECENT, false));
        listed.push(branch.to_string());
    }
    let mut rest: Vec<&str> = local
        .iter()
        .copied()
        .filter(|branch| !listed.iter().any(|known| known == branch))
        .collect();
    // Stable: the default branch first, the rest in gxserver's order.
    rest.sort_by_key(|branch| Some(*branch) != default);
    for branch in rest {
        rows.push(row(branch, note(branch, ""), SECTION_LOCAL, false));
    }
    for branch in branches.iter().filter(|branch| is_remote(branch)) {
        let Some(full) = name(branch) else {
            continue;
        };
        let tail = full.split_once('/').map_or(full, |(_, tail)| tail);
        if local.contains(&tail) {
            continue;
        }
        rows.push(row(full, String::new(), SECTION_REMOTE, true));
    }
    rows
}

/// What the search offers when no branch is called what was typed: `{from, command}`; the
/// renderer puts the typed name in the command's `folderBranch` and says "Create <name> from
/// <from>".
pub fn create(tray: &Value) -> Value {
    match tray.get("currentBranch").and_then(Value::as_str) {
        Some(current) => json!({
            "from": current,
            "command": { "type": "setDraftWorktree", "folderBranch": "", "create": true },
        }),
        None => Value::Null,
    }
}

/// `{"type": "setDraftWorktree", "folderBranch": name, "remote"?, "create"?, "force"?}` asks
/// gxserver to switch the folder; `{"cancelFolderSwitch": true}` drops the prompt. `None` when the
/// action is neither, so the tray's other picks run.
pub fn pick(state: &mut ChatState, action: &UserAction) -> Option<Vec<Effect>> {
    if action.param("cancelFolderSwitch").and_then(Value::as_bool) == Some(true) {
        state.menus.draft_worktree.folder_switch = None;
        state.core.request_render();
        return Some(Vec::new());
    }
    let branch = action
        .param("folderBranch")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|branch| !branch.is_empty())?
        .to_string();
    let tray = &mut state.menus.draft_worktree;
    if tray.folder_request.is_some() {
        return Some(Vec::new());
    }
    let flag = |key: &str| action.param(key).and_then(Value::as_bool) == Some(true);
    let mut request = json!({ "name": branch });
    for key in ["remote", "create", "force"] {
        if flag(key) {
            request[key] = json!(true);
        }
    }
    tray.status = None;
    tray.folder_switch =
        Some(json!({ "state": "switching", "branch": branch, "request": request }));
    let request_id = state.core.allocate_request_id();
    state.menus.draft_worktree.folder_request = Some(request_id);
    state.core.request_render();
    Some(vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftWorktree,
        params: Box::new(json!({ "folderBranch": request })),
    }])
}

/// Folds gxserver's answer to a switch: the tray's fresh facts and how the switch went. `false`
/// when `request_id` is not the switch's.
pub fn settle(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> bool {
    let tray = &mut state.menus.draft_worktree;
    if tray.folder_request != Some(request_id) {
        return false;
    }
    tray.folder_request = None;
    let request = tray
        .folder_switch
        .as_ref()
        .map(|switch| switch["request"].clone())
        .unwrap_or(Value::Null);
    let mut switch = match outcome {
        RpcOutcome::Ok { result } => {
            if result.get("available").is_some() {
                let mut answer = result.clone();
                if let Some(object) = answer.as_object_mut() {
                    object.remove("folderSwitch");
                }
                tray.answer = Some(answer);
            }
            result
                .get("folderSwitch")
                .cloned()
                .filter(Value::is_object)
                .unwrap_or_else(|| {
                    json!({ "state": "failed", "message": "This Ghostex can't switch the folder's branch yet. Update it on that computer." })
                })
        }
        RpcOutcome::Err { message, .. } => json!({ "state": "failed", "message": message }),
    };
    switch["request"] = request;
    tray.folder_switch = Some(switch);
    state.core.request_render();
    true
}

fn action(label: &str, glyph: &str, command: Value) -> Value {
    json!({ "label": label, "glyph": glyph, "command": command })
}

fn worktree_way_out(branch: &str) -> Value {
    action(
        &format!("New worktree from {branch}"),
        "git-fork",
        json!({ "type": "setDraftWorktree", "mode": "newWorktree", "baseBranch": branch }),
    )
}

fn cancel(label: &str) -> Value {
    action(
        label,
        "x",
        json!({ "type": "setDraftWorktree", "cancelFolderSwitch": true }),
    )
}

/// The line under the bar about a switch, or `null`: `{tone: "busy" | "done" | "warn" | "error",
/// message, actions: [{label, glyph, command}]}`.
pub fn prompt(state: &ChatState) -> Value {
    let Some(switch) = state.menus.draft_worktree.folder_switch.as_ref() else {
        return Value::Null;
    };
    let branch = switch["branch"].as_str().unwrap_or_default();
    match switch["state"].as_str() {
        Some("switching") => json!({
            "tone": "busy",
            "message": format!("Switching this folder to {branch}…"),
            "actions": [],
        }),
        Some("done") => json!({
            "tone": "done",
            "message": format!("This folder is now on {branch}"),
            "actions": [],
        }),
        Some("blocked") => {
            let session = switch["session"].as_str().unwrap_or("Another session");
            json!({
                "tone": "warn",
                "message": format!("{session} is working in this folder, so it can't switch to {branch} now."),
                "actions": [worktree_way_out(branch), cancel("Cancel")],
            })
        }
        Some("confirm") => {
            let changes = switch["changes"].as_u64().unwrap_or(0);
            let mut force = switch["request"].clone();
            force["force"] = json!(true);
            let mut command = json!({ "type": "setDraftWorktree", "folderBranch": branch });
            for key in ["remote", "force"] {
                if force[key] == true {
                    command[key] = json!(true);
                }
            }
            json!({
                "tone": "warn",
                "message": format!(
                    "{changes} uncommitted change{} here would come along to {branch}.",
                    if changes == 1 { "" } else { "s" }
                ),
                "actions": [
                    action("Switch and take them", "git-branch", command),
                    worktree_way_out(branch),
                    cancel("Cancel"),
                ],
            })
        }
        _ => json!({
            "tone": "error",
            "message": switch["message"].as_str().unwrap_or("Could not switch the branch."),
            "actions": [cancel("Dismiss")],
        }),
    }
}
