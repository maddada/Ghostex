//! The where pill at the left end of the chat box's pill strip: the computer's icon, the project and
//! the branch of a running session (the computer's name only in its card since 2026-11-18, the
//! decision in apps/desktop/src/app/native_chat/where_pill.rs), a hover card with everything else about where it runs, and a click
//! that opens the same details with Copy branch and Open folder.
//!
//! gxserver on the session's machine reads the facts (`/api/readSessionWhere`,
//! server/src/session_where.rs); the computer's name comes from the host
//! ([`crate::Event::ComputerChanged`]), since that machine calls itself "This computer". This
//! module reads once when the chat starts, again every [`POLL_MS`] while the chat is on screen,
//! and words the pill, the card (the project and computer as its header, then a row per other
//! fact) and its actions into the `wherePill` document key. A draft has none: its worktree tray
//! above the box says where it will run.
//!
//! CDXC:SessionChat 2026-11-17 DECISION:
//! User, from the where pill mockups: Q3781 H, the two-part pill ("I love the popup you show on hover in A", so hovering it shows A's card); Q3782 the pill always shows the project, the machine and the branch; Q3783 the card and the details show the project, machine, Cloud Box, branch, worktree, pull request, counts, folder, agent and terminal; Q3784 a click opens the details with actions (Copy branch and Open folder, no Move to another computer); Q3785 it sits at the left end of the pill strip. The phone is not part of this yet.
//! SEE-ALSO: server/src/session_where.rs (the read), apps/desktop/src/app/native_chat/where_pill.rs (the drawing).

use serde_json::{json, Map, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The poll timer, held while the chat is hidden (`session/timers.rs`).
pub const WHERE_PILL_POLL: &str = "extras.wherePill";
/// How often a chat on screen reads again: the branch, the counts and the pull request move.
pub const POLL_MS: f64 = 30_000.0;
/// How soon a read that failed is tried again.
const RETRY_MS: f64 = 60_000.0;

/// What the pill remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WherePillState {
    /// The last answer of `/api/readSessionWhere`.
    pub data: Option<Value>,
    /// The read in flight; an answer to any other id is dropped.
    pub request: Option<u64>,
    /// The first read went out.
    pub started: bool,
    /// The session's gxserver is older than the read, so there is no pill and no more reads.
    pub unsupported: bool,
}

/// The pill applies to this chat: a started chat over a running session, not a draft.
fn applies(state: &ChatState) -> bool {
    state.core.controller_started
        && state.session.available_agents.is_none()
        && !state.extras.where_pill.unsupported
}

fn read(state: &mut ChatState, request_id: u64) -> Effect {
    state.extras.where_pill.request = Some(request_id);
    state.extras.where_pill.started = true;
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionWhere,
        params: Box::new(json!({})),
    }
}

/// The first read, once the chat has started over a session.
pub fn read_if_due(state: &mut ChatState, next_request_id: impl FnOnce() -> u64) -> Option<Effect> {
    let pill = &state.extras.where_pill;
    if pill.started || pill.request.is_some() || !applies(state) {
        return None;
    }
    Some(read(state, next_request_id()))
}

/// The poll timer fired: read again.
pub fn settle_tick(state: &mut ChatState, next_request_id: impl FnOnce() -> u64) -> Vec<Effect> {
    if !state.core.timer_fired(WHERE_PILL_POLL)
        || state.extras.where_pill.request.is_some()
        || !applies(state)
    {
        return Vec::new();
    }
    vec![read(state, next_request_id())]
}

/// An answer to the pill's read, or `None` when it was not the pill's.
pub fn settle_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
    context: &ChatContext,
) -> Option<Vec<Effect>> {
    if state.extras.where_pill.request != Some(request_id) {
        return None;
    }
    state.core.claim_refusal();
    let pill = &mut state.extras.where_pill;
    pill.request = None;
    let delay = match outcome {
        RpcOutcome::Ok { result } => {
            if pill.data.as_ref() != Some(result) {
                pill.data = Some(result.clone());
                state.core.request_publish();
            }
            POLL_MS
        }
        // An older gxserver has no `readSessionWhere` and refuses it without a sentence.
        RpcOutcome::Err { message, .. } if !message.chars().any(char::is_alphabetic) => {
            pill.unsupported = true;
            pill.data = None;
            state.core.request_publish();
            return Some(Vec::new());
        }
        // A read that failed keeps the facts it had and tries again later.
        RpcOutcome::Err { .. } => RETRY_MS,
    };
    state
        .core
        .timers
        .arm(WHERE_PILL_POLL, context.now_ms, delay);
    Some(Vec::new())
}

/// The pill's clicks: `{type: "wherePill", do: "copyBranch" | "openFolder", groupId}`. Open folder
/// is the sidebar's own Open Folder on the session's project, whose sidebar group the renderer
/// names (`groupId`), because only the host knows the session's machine.
pub fn handle(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(data) = state.extras.where_pill.data.as_ref() else {
        return Vec::new();
    };
    match action.param("do").and_then(Value::as_str) {
        Some("copyBranch") => match text(data, "/git/branch") {
            Some(branch) => vec![
                Effect::Copy { text: branch },
                Effect::Toast {
                    level: "info".to_string(),
                    message: "Branch name copied".to_string(),
                },
            ],
            None => Vec::new(),
        },
        Some("openFolder") => {
            let group = action
                .param("groupId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|group| !group.is_empty());
            match group {
                Some(group) => vec![Effect::HostAction {
                    action: "openProjectFolder".to_string(),
                    params: Box::new(json!({ "groupId": group })),
                }],
                None => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

fn text(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn number(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
}

/// `n thing` or `n things`.
fn count(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// One row of the card: an icon name, its label, its value and an optional muted detail.
fn row(id: &str, icon: &str, label: &str, value: String, detail: Option<String>) -> Value {
    let mut row = Map::new();
    row.insert("id".into(), json!(id));
    row.insert("icon".into(), json!(icon));
    row.insert("label".into(), json!(label));
    row.insert("value".into(), json!(value));
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        row.insert("detail".into(), json!(detail));
    }
    Value::Object(row)
}

/// The computer the session runs on: `{kind, name, serverId}`. `kind` is `this`, `remote`,
/// `cloudBox` or `box`.
fn machine(state: &ChatState, data: &Value) -> Value {
    let computer = state.session.computer.computer.as_ref();
    let remote = computer.filter(|computer| computer.remote);
    let remote_name = remote
        .map(|computer| computer.name.trim().to_string())
        .filter(|name| !name.is_empty());
    let label = text(data, "/location/label");
    let (kind, name) = match data.pointer("/location/kind").and_then(Value::as_str) {
        Some("cloudBox") => ("cloudBox", "Cloud Box".to_string()),
        Some("box") => (
            "box",
            label
                .clone()
                .map_or_else(|| "A box".to_string(), |label| format!("{label} box")),
        ),
        _ if remote.is_some() => (
            "remote",
            remote_name.unwrap_or_else(|| "Another computer".to_string()),
        ),
        _ => ("this", "This computer".to_string()),
    };
    json!({
        "kind": kind,
        "name": name,
        "provider": label,
        "serverId": remote.and_then(|computer| computer.server_id.clone()),
        "offline": remote.is_some_and(|computer| computer.offline),
    })
}

/// The `wherePill` value, or `null` while there is none (a draft, no answer yet, an older
/// gxserver).
pub fn document(state: &ChatState) -> Value {
    if !applies(state) {
        return Value::Null;
    }
    let Some(data) = state.extras.where_pill.data.as_ref() else {
        return Value::Null;
    };
    let project = text(data, "/project").unwrap_or_else(|| "This project".to_string());
    let machine = machine(state, data);
    let machine_name = machine["name"].as_str().unwrap_or_default().to_string();
    let git = data.get("git").filter(|git| git.is_object());
    let branch = git.and_then(|git| text(git, "/branch"));
    let worktree = data.get("worktree").filter(|worktree| worktree.is_object());

    // The card's header names the project and the computer, so no row repeats them.
    let mut rows = Vec::new();
    if machine["kind"] == "cloudBox" {
        rows.push(row(
            "cloud",
            "cloud",
            "Runs in",
            "Cloud Box".to_string(),
            text(&machine, "/provider"),
        ));
    }
    if let Some(worktree) = worktree {
        rows.push(row(
            "worktree",
            "git-fork",
            "Worktree",
            text(worktree, "/name").unwrap_or_else(|| "A worktree".to_string()),
            text(worktree, "/parentProject").map(|parent| format!("of {parent}")),
        ));
    }
    match git {
        Some(git) => {
            let lines = match (number(git, "/additions"), number(git, "/deletions")) {
                (Some(added), Some(removed)) if added + removed > 0 => {
                    Some(format!("+{added} \u{2212}{removed}"))
                }
                _ => None,
            };
            rows.push(row(
                "branch",
                "git-branch",
                "Branch",
                branch.clone().unwrap_or_else(|| "No branch".to_string()),
                lines,
            ));
            let mut changes = Vec::new();
            if let Some(files) = number(git, "/changedFiles").filter(|files| *files > 0) {
                changes.push(format!("{} not committed", count(files, "file", "files")));
            }
            match (number(git, "/ahead"), number(git, "/behind")) {
                (Some(0), Some(0)) => changes.push("up to date".to_string()),
                (Some(ahead), Some(behind)) => {
                    if ahead > 0 {
                        changes.push(format!("{ahead} ahead"));
                    }
                    if behind > 0 {
                        changes.push(format!("{behind} behind"));
                    }
                }
                _ if branch.is_some() => changes.push("not pushed".to_string()),
                _ => {}
            }
            if !changes.is_empty() {
                let mut changes = changes.join(", ");
                if let Some(first) = changes.get(..1) {
                    changes = first.to_uppercase() + &changes[1..];
                }
                rows.push(row("counts", "git-commit", "Changes", changes, None));
            }
            if let Some(pull_request) = git.get("pullRequest").filter(|pr| pr.is_object()) {
                if let Some(number) = number(pull_request, "/number") {
                    let state_word = match text(pull_request, "/state").as_deref() {
                        Some("open") => "Open",
                        Some("draft") => "Draft",
                        Some("merged") => "Merged",
                        Some("closed") => "Closed",
                        _ => "",
                    };
                    rows.push(row(
                        "pullRequest",
                        "git-pull-request",
                        "Pull request",
                        format!("#{number}"),
                        Some(state_word.to_string()),
                    ));
                }
            }
        }
        None => rows.push(row(
            "branch",
            "git-branch",
            "Branch",
            "Not a git repository".to_string(),
            None,
        )),
    }
    if let Some(folder) = text(data, "/folderLabel").or_else(|| text(data, "/folder")) {
        rows.push(row("folder", "folder-open", "Folder", folder, None));
    }
    let agent = crate::extras::welcome::welcome_agent_name(
        text(data, "/agent")
            .or_else(|| state.session.agent.clone())
            .as_deref(),
    );
    if let Some(agent) = agent {
        rows.push(row(
            "agent",
            "user",
            "Agent",
            agent,
            // Hide account emails masks it here too, for a session on a computer whose own
            // gxserver does not hide them (server/src/session_where.rs masks there).
            text(data, "/accountEmail")
                .or_else(|| text(data, "/account"))
                .map(|account| {
                    crate::menus::native_accounts::account_text(
                        state.core.hide_account_emails,
                        &account,
                    )
                }),
        ));
    }
    if let Some(terminal) = text(data, "/terminal") {
        rows.push(row("terminal", "terminal-2", "Terminal", terminal, None));
    }

    let mut actions = Vec::new();
    if branch.is_some() {
        actions.push(json!({
            "id": "copyBranch",
            "icon": "copy",
            "label": "Copy branch",
            "action": {"type": "wherePill", "do": "copyBranch"},
        }));
    }
    if text(data, "/folder").is_some() {
        actions.push(json!({
            "id": "openFolder",
            "icon": "folder-open",
            "label": "Open folder",
            "action": {"type": "wherePill", "do": "openFolder"},
        }));
    }
    let aria = match &branch {
        Some(branch) => format!("{project} on {machine_name}, branch {branch}"),
        None => format!("{project} on {machine_name}"),
    };
    json!({
        "project": project,
        "machine": machine,
        "branch": branch,
        "worktree": worktree.is_some(),
        "ariaLabel": aria,
        "card": {
            "title": project,
            "subtitle": if machine["offline"] == Value::Bool(true) {
                format!("{machine_name} \u{b7} offline")
            } else {
                machine_name.clone()
            },
            "rows": rows,
        },
        "actions": actions,
    })
}
