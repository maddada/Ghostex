//! The worktree tray on top of a new chat's message box: where the chat runs, This folder, a New
//! worktree from a base branch, or an Existing worktree, picked before the first message.
//!
//! gxserver owns the facts and the move (`/api/draftWorktree`: whether the tray applies, the
//! branches, the worktrees, the remembered choice, and the move itself). This module decides what
//! the tray shows and which choice is on, remembers a pick, and turns the first message into the
//! move when the tray says a worktree: the message leaves the box, the tray shows progress, and the
//! chat opens the new session when it is ready, or puts the message back with the reason when it is
//! not. Both renderers draw the `worktreeTray` document key as it is.
//!
//! CDXC:Worktrees 2026-10-31 DECISION:
//! User (Q828 to Q832, from Brewed Bytes on Discord): a bar on top of the new chat's message box
//! picks This folder, New worktree (from a base branch you pick) or an existing worktree; the
//! branch name is automatic and never shown; the worktree is made on send, and a failure keeps the
//! message in the box with the reason and Try again; the choice is remembered per project.
//! SEE-ALSO: server/src/server/worktree_ops/draft_worktree.rs (the endpoint and the move),
//! apps/desktop/src/app/native_chat/worktree_tray.rs, apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx.

use serde_json::{json, Map, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::event::Event;
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

pub const MODE_FOLDER: &str = "folder";
pub const MODE_NEW: &str = "newWorktree";
pub const MODE_EXISTING: &str = "existingWorktree";
pub use crate::menus::cloud_box::MODE_CLOUD_BOX;
/// How often the tray reads a running move's progress.
pub const POLL_MS: f64 = 1_000.0;
const POLL_TIMER: &str = "menus.draftWorktreePoll";

/// Where a move stands, as the tray draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum MoveStatus {
    Creating {
        label: String,
    },
    Failed {
        message: String,
    },
    /// The new session is open; the draft is being closed.
    Opened,
}

/// What the tray remembers between events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftWorktreeState {
    /// The last full `/api/draftWorktree` answer.
    pub answer: Option<Value>,
    /// The full read in flight, or done for this draft.
    pub read_request: Option<u64>,
    pub read_done: bool,
    /// What this chat picked since it opened; `None` takes the project's remembered choice.
    pub mode: Option<String>,
    pub base_branch: Option<String>,
    pub worktree_key: Option<String>,
    /// The move's own calls in flight: its start, then its progress reads.
    pub start_request: Option<u64>,
    pub poll_request: Option<u64>,
    /// The message the move carries, put back in the box if it fails.
    pub message: Option<String>,
    pub status: Option<MoveStatus>,
    /// This chat sent its first message (any way: Enter, Tab to queue, Compact, a form or question
    /// answer, Send now), so the tray is gone for good ([`note_first_message`]).
    pub sent: bool,
    /// The Cloud Box picker's picks of this chat (`menus/cloud_box.rs`).
    pub cloud: crate::menus::cloud_box::CloudPicks,
    /// The running move starts a cloud box, and the box row it last reported (its boot card).
    pub cloud_start: bool,
    pub cloud_box: Option<Value>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_draft(state: &ChatState) -> bool {
    state.session.available_agents.is_some()
}

/// A box picked on the Run on row runs nowhere near this folder: no tray then, unless the tray
/// offers Cloud Box, whose picker lists that box place under More providers (D3).
fn box_picked(state: &ChatState) -> bool {
    let offered = state
        .menus
        .draft_worktree
        .answer
        .as_ref()
        .is_some_and(crate::menus::cloud_box::offered);
    !offered && crate::menus::cloud_box::agentbox_picked(state).is_some()
}

/*
CDXC:Worktrees 2026-11-02 DECISION:
User: "please as soon as i send the first message in a thread, then stop showing this bar". The tray
used to follow the draft marker, which gxserver removes only once the prompt reaches the agent, so it
stayed above the box while a first message was queued or still on its way. It now goes the moment a
first message leaves this chat and never comes back for that session: this chat latches it on the
send itself (and on every call that carries a message), and the session's own transcript or queue
holding a message says the same to a chat opened later or on another device. Only a move into a
worktree keeps it, to show that move's progress or give the message back. A chat that has sent
nothing keeps it. The phone runs this core, so its tray follows the same rule.
*/
/// Whether this session has sent a message already, by this chat or any other.
fn has_sent(state: &ChatState) -> bool {
    state.menus.draft_worktree.sent
        || state
            .messages
            .list
            .iter()
            .any(|message| message.role == ghostex_gx_protocol::ChatRole::User)
        || state
            .session
            .queue_prompts
            .as_ref()
            .is_some_and(|prompts| !prompts.is_empty())
}

/// The gxserver calls that carry a message to the agent.
fn carries_a_message(method: &ChatRpcMethod) -> bool {
    matches!(
        method,
        ChatRpcMethod::SendSessionChatMessage
            | ChatRpcMethod::QueueSessionChatPrompt
            | ChatRpcMethod::SendSessionChatQueuedPrompt
            | ChatRpcMethod::AnswerSessionChatPrompt
    )
}

/// Latches the first message: from a send this chat started (`submitted`), or from any effect of
/// this turn that hands the agent a message. Runs on every turn, before the document is built.
pub fn note_first_message(state: &mut ChatState, submitted: bool, effects: &[Effect]) {
    if state.menus.draft_worktree.sent {
        return;
    }
    let carried = effects.iter().any(
        |effect| matches!(effect, Effect::SendRpc { method, .. } if carries_a_message(method)),
    );
    if submitted || carried {
        state.menus.draft_worktree.sent = true;
        state.core.request_render();
    }
}

fn moving(state: &ChatState) -> bool {
    let tray = &state.menus.draft_worktree;
    tray.start_request.is_some()
        || matches!(
            tray.status,
            Some(MoveStatus::Creating { .. }) | Some(MoveStatus::Opened)
        )
}

/// Reads the tray once per draft. Runs on every event.
fn observe(state: &mut ChatState) -> Vec<Effect> {
    if !is_draft(state) {
        // A promoted session has no tray; a move that opened its new session is over too.
        if state.menus.draft_worktree.answer.is_some() && !moving(state) {
            state.menus.draft_worktree = DraftWorktreeState::default();
            state.core.request_render();
        }
        return Vec::new();
    }
    let tray = &state.menus.draft_worktree;
    if !state.core.controller_started || tray.read_request.is_some() || tray.read_done {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.menus.draft_worktree.read_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftWorktree,
        params: Box::new(json!({})),
    }]
}

fn move_status(answer: &Value) -> Option<(String, Value)> {
    let status = answer.get("move").filter(|value| value.is_object())?;
    Some((text(status, "status")?, status.clone()))
}

/// The message goes back to the box and the tray says why.
fn give_back(state: &mut ChatState, message: String) -> Vec<Effect> {
    let tray = &mut state.menus.draft_worktree;
    tray.start_request = None;
    tray.poll_request = None;
    tray.status = Some(MoveStatus::Failed { message });
    state.core.request_render();
    match tray.message.take() {
        Some(text) => vec![Effect::HostAction {
            action: "submissionFailed".to_string(),
            params: Box::new(json!({ "method": "send", "text": text })),
        }],
        None => Vec::new(),
    }
}

/// Folds what a running move reported: keep reading, give the message back, or open the new chat.
fn follow(state: &mut ChatState, context: &ChatContext, answer: &Value) -> Vec<Effect> {
    match move_status(answer) {
        Some((status, value)) if status == "creating" => {
            if let Some(row) = value.get("cloudBox").filter(|row| row.is_object()) {
                state.menus.draft_worktree.cloud_box = Some(row.clone());
                state.core.request_render();
            }
            state.core.timers.arm(POLL_TIMER, context.now_ms, POLL_MS);
            Vec::new()
        }
        Some((status, value)) if status == "failed" => give_back(
            state,
            text(&value, "message").unwrap_or_else(|| "Could not create the worktree.".to_string()),
        ),
        Some((status, value)) if status == "done" => {
            let (Some(project_id), Some(session_id)) =
                (text(&value, "projectId"), text(&value, "sessionId"))
            else {
                return give_back(state, "Could not open the new chat.".to_string());
            };
            let tray = &mut state.menus.draft_worktree;
            tray.poll_request = None;
            tray.status = Some(MoveStatus::Opened);
            if let Some(row) = value.get("cloudBox").filter(|row| row.is_object()) {
                tray.cloud_box = Some(row.clone());
            }
            let sent = tray.message.take().unwrap_or_default();
            // A cloud box session opens on the box's own machine (`cloudbox:<boxId>`).
            let mut open = json!({ "projectId": project_id, "sessionId": session_id });
            if let Some(machine_id) = text(&value, "machineId") {
                open["machineId"] = json!(machine_id);
            }
            state.core.request_render();
            let request_id = state.core.allocate_request_id();
            vec![
                Effect::HostAction {
                    action: "draftSubmitted".to_string(),
                    params: Box::new(json!({ "method": "send", "text": sent })),
                },
                Effect::HostAction {
                    action: "openAgentSession".to_string(),
                    params: Box::new(open),
                },
                // The draft this chat was is empty now: gxserver closes it.
                Effect::SendRpc {
                    request_id,
                    method: ChatRpcMethod::DraftWorktree,
                    params: Box::new(json!({ "finish": true })),
                },
            ]
        }
        // gxserver no longer knows the move: it restarted while the worktree was being made.
        _ => give_back(
            state,
            "Ghostex restarted before the worktree was ready. Try again.".to_string(),
        ),
    }
}

/// Family e's settle hook for the tray.
pub fn settle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    if let Event::RpcSettled {
        request_id,
        outcome,
    } = event
    {
        let tray = &state.menus.draft_worktree;
        let id = Some(*request_id);
        if tray.read_request == id {
            let tray = &mut state.menus.draft_worktree;
            tray.read_request = None;
            tray.read_done = true;
            // An older gxserver without the endpoint leaves no answer, so no tray.
            if let RpcOutcome::Ok { result } = outcome.as_ref() {
                tray.answer = Some(result.clone());
            }
            state.core.request_render();
        } else if tray.start_request == id {
            state.menus.draft_worktree.start_request = None;
            effects.extend(match outcome.as_ref() {
                RpcOutcome::Ok { result } => follow(state, context, result),
                RpcOutcome::Err { message, .. } => give_back(state, message.clone()),
            });
        } else if tray.poll_request == id {
            state.menus.draft_worktree.poll_request = None;
            effects.extend(match outcome.as_ref() {
                RpcOutcome::Ok { result } => follow(state, context, result),
                // A read that did not get through is tried again; the move runs on regardless.
                RpcOutcome::Err { .. } => {
                    state.core.timers.arm(POLL_TIMER, context.now_ms, POLL_MS);
                    Vec::new()
                }
            });
        }
    }
    if state.core.timer_fired(POLL_TIMER) && state.menus.draft_worktree.poll_request.is_none() {
        let request_id = state.core.allocate_request_id();
        state.menus.draft_worktree.poll_request = Some(request_id);
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::DraftWorktree,
            params: Box::new(json!({ "light": true })),
        });
    }
    effects.extend(observe(state));
    effects
}

/// The remembered choice the answer carries.
fn remembered(answer: &Value, key: &str) -> Option<String> {
    answer
        .get("remembered")
        .and_then(|memory| text(memory, key))
}

fn branches(answer: &Value) -> Vec<Value> {
    answer
        .get("branches")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn worktrees(answer: &Value) -> Vec<Value> {
    answer
        .get("worktrees")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The choice on: this chat's own pick, else the project's remembered one, else This folder.
fn selected_mode(state: &ChatState, answer: &Value) -> String {
    let cloud_offered = crate::menus::cloud_box::offered(answer);
    let mode = state
        .menus
        .draft_worktree
        .mode
        .clone()
        .or_else(|| {
            (cloud_offered
                && (crate::menus::cloud_box::remembered(answer)
                    || crate::menus::cloud_box::agentbox_picked(state).is_some()))
            .then(|| MODE_CLOUD_BOX.to_string())
        })
        .or_else(|| remembered(answer, "mode"))
        .unwrap_or_else(|| MODE_FOLDER.to_string());
    match mode.as_str() {
        MODE_CLOUD_BOX if cloud_offered => mode,
        MODE_NEW => mode,
        MODE_EXISTING if !worktrees(answer).is_empty() => mode,
        _ => MODE_FOLDER.to_string(),
    }
}

/// The base branch on: the pick, else the remembered one while it still exists, else the
/// folder's current branch, else the first listed.
fn selected_base(state: &ChatState, answer: &Value) -> Option<String> {
    let listed = branches(answer);
    let exists = |name: &str| {
        listed
            .iter()
            .any(|branch| branch.get("name").and_then(Value::as_str) == Some(name))
    };
    state
        .menus
        .draft_worktree
        .base_branch
        .clone()
        .or_else(|| remembered(answer, "baseBranch").filter(|name| exists(name)))
        .or_else(|| text(answer, "currentBranch"))
        .or_else(|| listed.first().and_then(|branch| text(branch, "name")))
}

/// The tray as the renderers draw it, or `None` when it is hidden.
///
/// `{mode, segments: [{mode, label}], currentBranch, baseBranch, branches, worktrees, worktreeKey,
/// worktreeLabel, busy, status: null | {state: "creating" | "failed" | "opened", label | message},
/// where, detail}`. `where` and `detail` are the breadcrumb's last two parts ([`where_part`],
/// [`detail_part`]); the renderers put the project and the computer before them.
pub fn tray(state: &ChatState) -> Option<Value> {
    let tray = &state.menus.draft_worktree;
    let answer = tray.answer.as_ref()?;
    let moving = moving(state);
    if !moving && (!is_draft(state) || box_picked(state) || has_sent(state)) {
        return None;
    }
    let available = answer.get("available").and_then(Value::as_bool) == Some(true);
    let movable = available || answer.get("movable").and_then(Value::as_bool) == Some(true);
    if !movable {
        return None;
    }
    // CDXC:Worktrees 2026-10-31 DECISION:
    // User (Q847): "we need to add picking the project and machine in the bar that we added today". The renderers put a project chip first in the tray whenever `movable`; picking from it moves the draft to that project and machine, and the chip is all the tray shows in a folder that is not a git checkout, which has nothing else to pick. A chat being moved into a worktree keeps it, disabled through `busy`.
    // SEE-ALSO: packages/gx-core/src/unified_sidebar/choices.rs (the choices), apps/desktop/src/app/native_chat/draft_project_picker.rs.
    let mode = if available {
        selected_mode(state, answer)
    } else {
        MODE_FOLDER.to_string()
    };
    let worktree_list = worktrees(answer);
    let mut segments = Vec::new();
    if available {
        segments.push(json!({ "mode": MODE_FOLDER, "label": "This folder" }));
        segments.push(json!({ "mode": MODE_NEW, "label": "New worktree" }));
    }
    if !worktree_list.is_empty() {
        segments.push(json!({ "mode": MODE_EXISTING, "label": "Existing" }));
    }
    if crate::menus::cloud_box::offered(answer) {
        segments.push(json!({
            "mode": MODE_CLOUD_BOX,
            "label": crate::menus::cloud_box::SEGMENT_LABEL,
            "icon": crate::menus::cloud_box::ICON,
        }));
    }
    let worktree_key = tray.worktree_key.clone().filter(|key| {
        worktree_list
            .iter()
            .any(|worktree| worktree.get("worktreeKey").and_then(Value::as_str) == Some(key))
    });
    let worktree_label = worktree_key.as_ref().and_then(|key| {
        worktree_list
            .iter()
            .find(|worktree| worktree.get("worktreeKey").and_then(Value::as_str) == Some(key))
            .and_then(|worktree| text(worktree, "name"))
    });
    let card = |label: &str, opening: bool| {
        tray.cloud_box
            .as_ref()
            .filter(|_| tray.cloud_start)
            .map(|row| crate::menus::cloud_box::card(label, tray.message.as_deref(), row, opening))
            .unwrap_or(Value::Null)
    };
    let status = match &tray.status {
        Some(MoveStatus::Creating { label }) => {
            json!({ "state": "creating", "label": label, "card": card(label, false) })
        }
        Some(MoveStatus::Failed { message }) => json!({ "state": "failed", "message": message }),
        Some(MoveStatus::Opened) => json!({
            "state": "opened",
            "label": if tray.cloud_start { "Opening the chat on the box…" } else { "Opening the new chat…" },
            "card": card("Opening the chat on the box…", true),
        }),
        None => Value::Null,
    };
    let mut value = Map::new();
    value.insert("mode".to_string(), json!(mode));
    value.insert("segments".to_string(), Value::Array(segments));
    value.insert(
        "projectName".to_string(),
        answer.get("projectName").cloned().unwrap_or(Value::Null),
    );
    value.insert(
        "currentBranch".to_string(),
        answer.get("currentBranch").cloned().unwrap_or(Value::Null),
    );
    value.insert(
        "baseBranch".to_string(),
        json!(selected_base(state, answer)),
    );
    value.insert("branches".to_string(), Value::Array(branches(answer)));
    value.insert("worktrees".to_string(), Value::Array(worktree_list));
    value.insert("worktreeKey".to_string(), json!(worktree_key));
    value.insert("worktreeLabel".to_string(), json!(worktree_label));
    value.insert("busy".to_string(), json!(moving));
    value.insert("status".to_string(), status);
    if mode == MODE_CLOUD_BOX {
        if let Some(facts) = crate::menus::cloud_box::facts(answer) {
            let picked = crate::menus::cloud_box::resolved(state, facts);
            value.insert(
                "cloudBox".to_string(),
                json!({
                    "chip": crate::menus::cloud_box::chip(state, &picked),
                    "menu": crate::menus::cloud_box::menu(state, facts, &picked),
                }),
            );
        }
    }
    let mut value = Value::Object(value);
    value["where"] = where_part(&value);
    value["detail"] = detail_part(&value);
    Some(value)
}

/*
CDXC:Worktrees 2026-11-04 DECISION:
User (Q1227 "D", note: "dont have a border around the breadcrumb ... make hovering over one of those items show the full text if it's truncated ... on narrow screen ... show the bread crumbs on 2 levels but they must be aligned well please like a grid"; Q1228 "Set 1"): the tray is one borderless breadcrumb, project › computer › where › detail, instead of chips and a 4-way switch that was 448px wide and ran off a narrow pane. Each part opens its own menu. The where menu lists the offered choices with the number keys 1 to 4 (Option+W opens it), and the detail is what the choice needs next: the folder's branch (only shown), a new worktree's base branch, the existing worktree, or the Cloud Box pick (Option+B opens it). The core builds both parts so the computer and the phone draw the same words, icons and menus.
SEE-ALSO: apps/desktop/src/app/native_chat/worktree_tray.rs and tray_layout.rs (the one-row or two-row grid), apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx and trayLayout.ts.
*/

/// What a choice is called in the breadcrumb, in its menu, its glyph (the renderers'
/// `titlebar/<glyph>.svg` and the phone's `Glyph`) and its menu line.
fn choice_words(mode: &str, tray: &Value) -> (&'static str, &'static str, &'static str, String) {
    match mode {
        MODE_NEW => (
            "New worktree",
            "New worktree",
            "git-fork",
            format!(
                "Made when you send, from {}",
                text(tray, "baseBranch").unwrap_or_else(|| "main".to_string())
            ),
        ),
        MODE_EXISTING => {
            let count = tray
                .get("worktrees")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            (
                "Existing",
                "Existing worktree",
                "stack",
                format!(
                    "{count} worktree{} in this project",
                    if count == 1 { "" } else { "s" }
                ),
            )
        }
        MODE_CLOUD_BOX => (
            crate::menus::cloud_box::SEGMENT_LABEL,
            crate::menus::cloud_box::SEGMENT_LABEL,
            crate::menus::cloud_box::ICON,
            "A box in the cloud that clones the project".to_string(),
        ),
        _ => (
            "This folder",
            "This folder",
            "folder-open",
            match text(tray, "currentBranch") {
                Some(branch) => format!("The project's own checkout, on {branch}"),
                None => "The project's own checkout".to_string(),
            },
        ),
    }
}

/// The breadcrumb's where part: `null` when there is nothing to choose (a folder that is not a
/// git checkout), else `{label, glyph, menu}`, the menu one row per offered choice with its number
/// key (`key`, shown as `detail`).
pub fn where_part(tray: &Value) -> Value {
    let Some(segments) = tray
        .get("segments")
        .and_then(Value::as_array)
        .filter(|segments| !segments.is_empty())
    else {
        return Value::Null;
    };
    let mode = text(tray, "mode").unwrap_or_else(|| MODE_FOLDER.to_string());
    let menu: Vec<Value> = segments
        .iter()
        .filter_map(|segment| text(segment, "mode"))
        .enumerate()
        .map(|(index, choice)| {
            let (_, label, glyph, description) = choice_words(&choice, tray);
            let key = (index + 1).to_string();
            json!({
                "label": label,
                "description": description,
                "glyph": glyph,
                "detail": key,
                "key": key,
                "checked": choice == mode,
                "command": { "type": "setDraftWorktree", "mode": choice },
            })
        })
        .collect();
    let (label, _, glyph, _) = choice_words(&mode, tray);
    json!({ "label": label, "glyph": glyph, "menu": menu })
}

/// The breadcrumb's last part, what the choice on needs next, or `null`:
/// `{kind, label, glyph, prefix?, tooltip?, star?, placeholder?, menuTitle?, menuWidth?, menu}`.
/// `kind` is `branch` (This folder's branch, only shown: `menu` is `null`), `baseBranch`,
/// `worktree` or `cloudBox`. While a move runs or failed there is none: the status line says it.
pub fn detail_part(tray: &Value) -> Value {
    if !tray.get("status").is_some_and(Value::is_null)
        || tray
            .get("segments")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    {
        return Value::Null;
    }
    let list = |key: &str| {
        tray.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    match text(tray, "mode").as_deref() {
        Some(MODE_NEW) => {
            let base = text(tray, "baseBranch").unwrap_or_else(|| "main".to_string());
            let menu: Vec<Value> = list("branches")
                .iter()
                .filter_map(|branch| {
                    let name = text(branch, "name")?;
                    Some(json!({
                        "label": name,
                        "detail": if branch.get("current").and_then(Value::as_bool) == Some(true) { "Current" } else { "" },
                        "checked": name == base,
                        "command": { "type": "setDraftWorktree", "baseBranch": name },
                    }))
                })
                .collect();
            json!({
                "kind": "baseBranch",
                "prefix": "from",
                "label": base,
                "glyph": "git-branch",
                "tooltip": format!("Base branch: {base}"),
                "menuTitle": "Start the worktree from",
                "menu": menu,
            })
        }
        Some(MODE_EXISTING) => {
            let key = text(tray, "worktreeKey");
            let worktrees = list("worktrees");
            let picked = worktrees
                .iter()
                .find(|worktree| text(worktree, "worktreeKey") == key && key.is_some());
            let menu: Vec<Value> = worktrees
                .iter()
                .filter_map(|worktree| {
                    let worktree_key = text(worktree, "worktreeKey")?;
                    Some(json!({
                        "label": worktree.get("name").cloned().unwrap_or(Value::Null),
                        "detail": worktree.get("branch").cloned().unwrap_or(Value::Null),
                        "checked": Some(&worktree_key) == key.as_ref(),
                        "command": { "type": "setDraftWorktree", "worktreeKey": worktree_key },
                    }))
                })
                .collect();
            let label =
                text(tray, "worktreeLabel").unwrap_or_else(|| "Pick a worktree".to_string());
            let tooltip = match picked.and_then(|worktree| text(worktree, "branch")) {
                Some(branch) => format!("Worktree: {label}, on {branch}"),
                None => format!("Worktree: {label}"),
            };
            json!({
                "kind": "worktree",
                "label": label,
                "glyph": "git-branch",
                "tooltip": tooltip,
                "placeholder": picked.is_none(),
                "menuTitle": "Existing worktrees",
                "menu": menu,
            })
        }
        Some(MODE_CLOUD_BOX) => {
            let Some(cloud) = tray.get("cloudBox").filter(|cloud| cloud.is_object()) else {
                return Value::Null;
            };
            let label = cloud["chip"]
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(crate::menus::cloud_box::SEGMENT_LABEL)
                .to_string();
            let star = cloud["chip"].get("star").and_then(Value::as_bool) == Some(true);
            json!({
                "kind": "cloudBox",
                "label": label,
                "glyph": if star { "star-filled" } else { crate::menus::cloud_box::ICON },
                "star": star,
                "tooltip": format!("Cloud Box: {label}"),
                "menuTitle": crate::menus::cloud_box::SEGMENT_LABEL,
                "menuWidth": 320,
                "menu": cloud.get("menu").cloned().unwrap_or(Value::Null),
            })
        }
        _ => match text(tray, "currentBranch") {
            Some(branch) => json!({
                "kind": "branch",
                "label": branch,
                "glyph": "git-branch",
                "tooltip": format!("This folder is on {branch}"),
                "menu": Value::Null,
            }),
            None => Value::Null,
        },
    }
}

/// `{"type": "setDraftWorktree", "mode"?, "baseBranch"?, "worktreeKey"?}`: a pick on the tray,
/// remembered for the project (the mode, and a new worktree's base branch).
pub fn pick(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(current) = tray(state) else {
        return Vec::new();
    };
    if current.get("busy").and_then(Value::as_bool) == Some(true) {
        return Vec::new();
    }
    let param = |key: &str| {
        action
            .param(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let tray_state = &mut state.menus.draft_worktree;
    let mut mode = text(&current, "mode").unwrap_or_else(|| MODE_FOLDER.to_string());
    if let Some(picked) = param("mode").filter(|picked| {
        current
            .get("segments")
            .and_then(Value::as_array)
            .is_some_and(|segments| {
                segments
                    .iter()
                    .any(|segment| segment.get("mode").and_then(Value::as_str) == Some(picked))
            })
    }) {
        mode = picked;
        tray_state.mode = Some(mode.clone());
    }
    if let Some(branch) = param("baseBranch") {
        tray_state.base_branch = Some(branch);
        mode = MODE_NEW.to_string();
        tray_state.mode = Some(mode.clone());
    }
    if let Some(key) = param("worktreeKey") {
        tray_state.worktree_key = Some(key);
        mode = MODE_EXISTING.to_string();
        tray_state.mode = Some(mode.clone());
    }
    // A new pick clears the last failure's reason.
    tray_state.status = None;
    state.core.request_render();
    let mut effects = Vec::new();
    // Leaving Cloud Box while an agentbox place is picked moves the draft back to this computer.
    if mode != MODE_CLOUD_BOX && crate::menus::cloud_box::agentbox_picked(state).is_some() {
        effects.extend(crate::menus::run_location::switch_to(
            state,
            crate::menus::run_location::LOCAL_RUN_LOCATION,
        ));
    }
    effects.push(remember_effect(state, &mode));
    effects
}

/// The call that remembers the tray's choice: the mode for the project, a new worktree's base
/// branch, and the Cloud Box picks for the draft.
fn remember_effect(state: &mut ChatState, mode: &str) -> Effect {
    let mut remember = json!({ "mode": mode });
    if mode == MODE_NEW {
        if let Some(base) = tray(state).and_then(|tray| text(&tray, "baseBranch")) {
            remember["baseBranch"] = json!(base);
        }
    }
    if mode == MODE_CLOUD_BOX {
        if let Some(facts) = state
            .menus
            .draft_worktree
            .answer
            .as_ref()
            .and_then(crate::menus::cloud_box::facts)
        {
            let picked = crate::menus::cloud_box::resolved(state, facts);
            remember["cloudBox"] = crate::menus::cloud_box::start_picks(&picked);
        }
    }
    // Remembering is fire and forget: the answer re-reads the tray, which the pick already drew.
    let request_id = state.core.allocate_request_id();
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftWorktree,
        params: Box::new(json!({ "remember": remember })),
    }
}

/// `{"type": "setDraftCloudBox", "provider"?, "snapshotId"?, "size"?, "reuseBoxId"?, "blank"?}`: a
/// pick in the Cloud Box picker. A `null` clears that pick (a new box instead of a reused one).
pub fn pick_cloud(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(current) = tray(state) else {
        return Vec::new();
    };
    if current.get("busy").and_then(Value::as_bool) == Some(true) {
        return Vec::new();
    }
    let has = |key: &str| action.params.contains_key(key);
    let value = |key: &str| {
        action
            .param(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let picks = &mut state.menus.draft_worktree.cloud;
    if has("provider") {
        picks.provider = value("provider");
    }
    if has("snapshotId") {
        picks.snapshot_id = value("snapshotId");
    }
    if has("size") {
        picks.size = value("size");
    }
    if has("reuseBoxId") {
        picks.reuse_box_id = Some(value("reuseBoxId"));
    }
    if let Some(blank) = action.param("blank").and_then(Value::as_bool) {
        picks.blank = Some(blank);
    }
    let tray_state = &mut state.menus.draft_worktree;
    tray_state.mode = Some(MODE_CLOUD_BOX.to_string());
    tray_state.status = None;
    state.core.request_render();
    let mut effects = Vec::new();
    if crate::menus::cloud_box::agentbox_picked(state).is_some() {
        effects.extend(crate::menus::run_location::switch_to(
            state,
            crate::menus::run_location::LOCAL_RUN_LOCATION,
        ));
    }
    effects.push(remember_effect(state, MODE_CLOUD_BOX));
    effects
}

/// The first message of a chat whose tray says a worktree: the move instead of an ordinary send.
/// `None` when the tray is hidden or says This folder, so the ordinary send goes ahead.
pub fn intercept_send(
    state: &mut ChatState,
    text_to_send: &str,
    image_paths: &[String],
) -> Option<Vec<Effect>> {
    let current = tray(state)?;
    let mode = text(&current, "mode")?;
    if mode == MODE_FOLDER {
        return None;
    }
    if mode == MODE_CLOUD_BOX {
        return intercept_cloud_send(state, &current, text_to_send, image_paths);
    }
    if current.get("busy").and_then(Value::as_bool) == Some(true) {
        // One move at a time: the message goes straight back.
        return Some(vec![Effect::HostAction {
            action: "submissionFailed".to_string(),
            params: Box::new(json!({ "method": "send", "text": text_to_send })),
        }]);
    }
    state.menus.draft_worktree.message = Some(text_to_send.to_string());
    state.menus.draft_worktree.cloud_start = false;
    let worktree_key = text(&current, "worktreeKey");
    if mode == MODE_EXISTING && worktree_key.is_none() {
        return Some(give_back(
            state,
            "Pick an existing worktree first.".to_string(),
        ));
    }
    let base = text(&current, "baseBranch");
    let label = match (mode.as_str(), base.as_deref()) {
        (MODE_EXISTING, _) => format!(
            "Opening {}…",
            text(&current, "worktreeLabel").unwrap_or_else(|| "the worktree".to_string())
        ),
        (_, Some(base)) => format!("Creating a worktree from {base}…"),
        _ => "Creating a worktree…".to_string(),
    };
    let tray_state = &mut state.menus.draft_worktree;
    tray_state.status = Some(MoveStatus::Creating { label });
    let request_id = state.core.allocate_request_id();
    tray_state.start_request = Some(request_id);
    state.core.request_render();
    let mut start = json!({
        "mode": mode,
        "text": text_to_send,
        "imagePaths": image_paths,
    });
    if mode == MODE_NEW {
        start["baseBranch"] = json!(base);
    } else {
        start["worktreeKey"] = json!(worktree_key);
    }
    Some(vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftWorktree,
        params: Box::new(json!({ "start": start })),
    }])
}

/// The first message of a Cloud Box draft: gxserver keeps it, starts the box and the tray follows
/// its boot card. An agentbox place picked under More providers sends as always (its first send
/// starts that box, `server/src/session_chat_queue_runtime/box_first_send.rs`).
fn intercept_cloud_send(
    state: &mut ChatState,
    current: &Value,
    text_to_send: &str,
    image_paths: &[String],
) -> Option<Vec<Effect>> {
    if crate::menus::cloud_box::agentbox_picked(state).is_some() {
        return None;
    }
    if current.get("busy").and_then(Value::as_bool) == Some(true) {
        return Some(vec![Effect::HostAction {
            action: "submissionFailed".to_string(),
            params: Box::new(json!({ "method": "send", "text": text_to_send })),
        }]);
    }
    let facts = state
        .menus
        .draft_worktree
        .answer
        .as_ref()
        .and_then(crate::menus::cloud_box::facts)?
        .clone();
    let picked = crate::menus::cloud_box::resolved(state, &facts);
    state.menus.draft_worktree.message = Some(text_to_send.to_string());
    if let Some(reason) = crate::menus::cloud_box::refusal(&facts, &picked) {
        return Some(give_back(state, reason));
    }
    let label = crate::menus::cloud_box::start_label(&picked);
    let tray_state = &mut state.menus.draft_worktree;
    tray_state.status = Some(MoveStatus::Creating { label });
    tray_state.cloud_start = true;
    tray_state.cloud_box = None;
    let request_id = state.core.allocate_request_id();
    tray_state.start_request = Some(request_id);
    state.core.request_render();
    Some(vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftWorktree,
        params: Box::new(json!({ "start": {
            "mode": MODE_CLOUD_BOX,
            "text": text_to_send,
            "imagePaths": image_paths,
            "cloudBox": crate::menus::cloud_box::start_picks(&picked),
        }})),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionKind;

    fn draft_state() -> ChatState {
        let mut state = ChatState::default();
        state.session.available_agents = Some(json!([]));
        state.core.controller_started = true;
        state
    }

    fn answered(state: &mut ChatState, remembered: Value) {
        state.menus.draft_worktree.read_done = true;
        state.menus.draft_worktree.answer = Some(json!({
            "available": true,
            "currentBranch": "main",
            "branches": [{"name": "main", "current": true}, {"name": "develop"}],
            "worktrees": [{"name": "Ghostex-old", "branch": "old", "worktreeKey": "k-old"}],
            "remembered": remembered,
            "move": null,
        }));
    }

    fn action(params: Value) -> UserAction {
        let mut action = UserAction::new(ActionKind::SetDraftWorktree);
        action.params = params.as_object().cloned().unwrap_or_default();
        action
    }

    fn sent_rpc(effects: &[Effect]) -> Option<(u64, Value)> {
        effects.iter().find_map(|effect| match effect {
            Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::DraftWorktree,
                params,
            } => Some((*request_id, (**params).clone())),
            _ => None,
        })
    }

    #[test]
    fn the_projects_remembered_choice_is_on_until_this_chat_picks() {
        let mut state = draft_state();
        answered(
            &mut state,
            json!({ "mode": "newWorktree", "baseBranch": "develop" }),
        );
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["mode"], MODE_NEW);
        assert_eq!(shown["baseBranch"], "develop");
        assert_eq!(shown["segments"].as_array().map(Vec::len), Some(3));

        let effects = pick(&mut state, &action(json!({ "mode": "folder" })));
        assert_eq!(tray(&state).expect("tray")["mode"], MODE_FOLDER);
        let (_, params) = sent_rpc(&effects).expect("remember call");
        assert_eq!(params, json!({ "remember": { "mode": "folder" } }));

        let effects = pick(&mut state, &action(json!({ "baseBranch": "main" })));
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["mode"], MODE_NEW);
        assert_eq!(shown["baseBranch"], "main");
        let (_, params) = sent_rpc(&effects).expect("remember call");
        assert_eq!(
            params,
            json!({ "remember": { "mode": "newWorktree", "baseBranch": "main" } })
        );
    }

    #[test]
    fn the_breadcrumb_names_the_choice_and_numbers_its_menu() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "folder" }));
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["where"]["label"], "This folder");
        let menu = shown["where"]["menu"].as_array().expect("where menu");
        let keys: Vec<_> = menu.iter().map(|row| row["key"].clone()).collect();
        assert_eq!(keys, vec![json!("1"), json!("2"), json!("3")]);
        assert_eq!(menu[0]["checked"], true);
        assert_eq!(
            menu[0]["description"],
            "The project's own checkout, on main"
        );
        assert_eq!(
            menu[1]["command"],
            json!({"type": "setDraftWorktree", "mode": "newWorktree"})
        );
        assert_eq!(menu[2]["label"], "Existing worktree");
        assert_eq!(menu[2]["description"], "1 worktree in this project");
        // This folder's branch is only shown.
        assert_eq!(shown["detail"]["kind"], "branch");
        assert_eq!(shown["detail"]["label"], "main");
        assert!(shown["detail"]["menu"].is_null());

        let _ = pick(&mut state, &action(json!({ "mode": "newWorktree" })));
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["where"]["label"], "New worktree");
        assert_eq!(shown["detail"]["kind"], "baseBranch");
        assert_eq!(shown["detail"]["prefix"], "from");
        assert_eq!(shown["detail"]["label"], "main");
        let branches = shown["detail"]["menu"].as_array().expect("branches");
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0]["detail"], "Current");
        assert_eq!(branches[0]["checked"], true);

        let _ = pick(&mut state, &action(json!({ "mode": "existingWorktree" })));
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["where"]["label"], "Existing");
        assert_eq!(shown["detail"]["label"], "Pick a worktree");
        assert_eq!(shown["detail"]["placeholder"], true);
        let _ = pick(&mut state, &action(json!({ "worktreeKey": "k-old" })));
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["detail"]["label"], "Ghostex-old");
        assert_eq!(shown["detail"]["placeholder"], false);
        assert_eq!(shown["detail"]["tooltip"], "Worktree: Ghostex-old, on old");
    }

    #[test]
    fn a_running_move_shows_no_detail_and_a_plain_folder_no_where() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "newWorktree" }));
        let _ = intercept_send(&mut state, "go", &[]).expect("the move");
        let shown = tray(&state).expect("tray");
        assert!(shown["where"].is_object());
        assert!(shown["detail"].is_null());

        let mut plain = draft_state();
        plain.menus.draft_worktree.read_done = true;
        plain.menus.draft_worktree.answer =
            Some(json!({ "available": false, "movable": true, "projectName": "notes" }));
        let shown = tray(&plain).expect("tray");
        assert!(shown["where"].is_null());
        assert!(shown["detail"].is_null());
    }

    #[test]
    fn a_remembered_base_branch_that_is_gone_falls_back_to_the_current_one() {
        let mut state = draft_state();
        answered(
            &mut state,
            json!({ "mode": "newWorktree", "baseBranch": "deleted" }),
        );
        assert_eq!(tray(&state).expect("tray")["baseBranch"], "main");
    }

    #[test]
    fn this_folder_sends_as_always_and_a_worktree_send_starts_the_move() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "folder" }));
        assert!(intercept_send(&mut state, "hello", &[]).is_none());

        let _ = pick(&mut state, &action(json!({ "mode": "newWorktree" })));
        let effects =
            intercept_send(&mut state, "SPX-1 fix it", &["/a.png".to_string()]).expect("the move");
        let (_, params) = sent_rpc(&effects).expect("start call");
        assert_eq!(
            params,
            json!({ "start": {
                "mode": "newWorktree",
                "text": "SPX-1 fix it",
                "imagePaths": ["/a.png"],
                "baseBranch": "main",
            }})
        );
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["busy"], true);
        assert_eq!(shown["status"]["state"], "creating");
        assert_eq!(shown["status"]["label"], "Creating a worktree from main…");
    }

    #[test]
    fn a_failure_gives_the_message_back_with_the_reason() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "newWorktree" }));
        let context = ChatContext::default();
        let effects = intercept_send(&mut state, "fix it", &[]).expect("the move");
        let (request_id, _) = sent_rpc(&effects).expect("start call");
        let effects = settle(
            &mut state,
            &Event::RpcSettled {
                request_id,
                outcome: Box::new(RpcOutcome::Ok {
                    result: json!({ "move": { "status": "creating", "label": "…" } }),
                }),
            },
            &context,
        );
        assert!(effects.is_empty());
        assert!(state.core.timers.is_armed(POLL_TIMER));
        state.menus.draft_worktree.poll_request = Some(99);
        let effects = settle(
            &mut state,
            &Event::RpcSettled {
                request_id: 99,
                outcome: Box::new(RpcOutcome::Ok {
                    result: json!({ "move": { "status": "failed", "message": "Worktree setup command failed." } }),
                }),
            },
            &context,
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::HostAction { action, params }
                if action == "submissionFailed" && params["text"] == "fix it"
        )));
        let shown = tray(&state).expect("tray");
        assert_eq!(shown["busy"], false);
        assert_eq!(shown["status"]["message"], "Worktree setup command failed.");
    }

    #[test]
    fn a_finished_move_opens_the_new_chat_and_closes_the_draft() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "existingWorktree" }));
        let _ = pick(&mut state, &action(json!({ "worktreeKey": "k-old" })));
        let effects = intercept_send(&mut state, "carry on", &[]).expect("the move");
        let (request_id, params) = sent_rpc(&effects).expect("start call");
        assert_eq!(params["start"]["worktreeKey"], "k-old");
        let effects = settle(
            &mut state,
            &Event::RpcSettled {
                request_id,
                outcome: Box::new(RpcOutcome::Ok {
                    result: json!({ "move": { "status": "done", "projectId": "P2", "sessionId": "S2" } }),
                }),
            },
            &ChatContext::default(),
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::HostAction { action, params }
                if action == "openAgentSession" && params["sessionId"] == "S2"
        )));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SendRpc { params, .. } if params["finish"] == true
        )));
    }

    #[test]
    fn an_existing_worktree_must_be_picked_before_sending() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "existingWorktree" }));
        let effects = intercept_send(&mut state, "go", &[]).expect("refused");
        assert!(sent_rpc(&effects).is_none());
        assert_eq!(
            tray(&state).expect("tray")["status"]["message"],
            "Pick an existing worktree first."
        );
    }

    #[test]
    fn the_tray_goes_with_the_first_message_however_it_is_sent_and_never_comes_back() {
        // A draft that has sent nothing keeps it.
        let mut state = draft_state();
        answered(&mut state, json!({}));
        note_first_message(&mut state, false, &[]);
        assert!(tray(&state).is_some());

        // Enter, Tab to queue or Compact: the send itself, before any call answers.
        note_first_message(&mut state, true, &[]);
        assert!(tray(&state).is_none());
        // Still a draft (gxserver has not promoted it yet), and still gone.
        assert!(is_draft(&state));
        note_first_message(&mut state, false, &[]);
        assert!(tray(&state).is_none());

        // A form or question answer, or Send now: the call that carries it.
        for method in [
            ChatRpcMethod::AnswerSessionChatPrompt,
            ChatRpcMethod::SendSessionChatQueuedPrompt,
            ChatRpcMethod::QueueSessionChatPrompt,
            ChatRpcMethod::SendSessionChatMessage,
        ] {
            let mut state = draft_state();
            answered(&mut state, json!({}));
            note_first_message(
                &mut state,
                false,
                &[Effect::SendRpc {
                    request_id: 1,
                    method,
                    params: Box::new(json!({})),
                }],
            );
            assert!(tray(&state).is_none());
        }

        // A chat opened later, or on the phone, sees the session's queued or recorded message.
        let mut state = draft_state();
        answered(&mut state, json!({}));
        state.session.queue_prompts = Some(vec![json!({ "id": "q1", "text": "hi" })]);
        assert!(tray(&state).is_none());
        let mut state = draft_state();
        answered(&mut state, json!({}));
        state.messages.list.push(
            serde_json::from_value(json!({
                "id": "m1",
                "role": "user",
                "blocks": [],
                "timestamp": null,
                "source": "transcript",
            }))
            .expect("a user row"),
        );
        assert!(tray(&state).is_none());
    }

    #[test]
    fn a_move_into_a_worktree_keeps_the_tray_for_its_progress_and_its_failure() {
        let mut state = draft_state();
        answered(&mut state, json!({ "mode": "newWorktree" }));
        let effects = intercept_send(&mut state, "fix it", &[]).expect("the move");
        note_first_message(&mut state, false, &effects);
        assert_eq!(
            tray(&state).expect("progress")["status"]["state"],
            "creating"
        );
        let _ = give_back(&mut state, "Worktree setup command failed.".to_string());
        assert_eq!(
            tray(&state).expect("the reason")["status"]["state"],
            "failed"
        );
    }

    #[test]
    fn no_tray_for_a_started_chat_or_a_folder_that_is_not_git() {
        let mut state = draft_state();
        answered(&mut state, json!({}));
        state.session.available_agents = None;
        assert!(tray(&state).is_none());
        let mut state = draft_state();
        state.menus.draft_worktree.answer = Some(json!({ "available": false }));
        assert!(tray(&state).is_none());
    }

    #[test]
    fn a_folder_that_is_not_git_keeps_the_tray_for_its_project_chip_and_sends_as_always() {
        let mut state = draft_state();
        state.menus.draft_worktree.answer = Some(json!({
            "available": false,
            "movable": true,
            "projectName": "notes",
            // Remembered from when the folder was a checkout: it can't apply now.
            "remembered": { "mode": MODE_NEW },
        }));
        let current = tray(&state).expect("the chip's tray");
        assert_eq!(current["projectName"], "notes");
        assert_eq!(current["mode"], MODE_FOLDER);
        assert_eq!(current["segments"], json!([]));
        assert!(intercept_send(&mut state, "hello", &[]).is_none());
    }

    #[test]
    fn a_git_folder_names_its_project_beside_the_choices() {
        let mut state = draft_state();
        answered(&mut state, json!({}));
        state
            .menus
            .draft_worktree
            .answer
            .as_mut()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("projectName".to_string(), json!("acme"));
        let current = tray(&state).expect("the tray");
        assert_eq!(current["projectName"], "acme");
        assert!(current["segments"].as_array().unwrap().len() >= 2);
    }
}
