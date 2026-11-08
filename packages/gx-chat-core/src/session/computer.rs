//! The computer a chat's session runs on, when that is another computer: the calm card above the
//! chat box while it is offline, and the messages this computer holds for it until it is back.
//!
//! CDXC:RemoteMachines 2026-11-08 DECISION:
//! User, on the red "Couldn't reach Ghostex. It may be restarting." line a chat showed while its computer (a Windows PC) was simply switched off: "this notice ... sucks". Picked on the offline-notice page: the card above the chat box names the computer, "Windows PC is offline · last seen 2h ago" (Q1830 "offline", Q1831 "card", with the "Worked for …" bar below the card: "pls swap the placement of the working bar and this new card"); Retry is its only action (Q1835); a message sent meanwhile is held and goes out by itself when the computer is back (Q1832 "hold"), held on this computer's gxserver (Q1833) for at most 24 hours, then back to the box with a note (Q1834); a send that gets no answer for 10 s checks the computer at once instead of spinning until the link drops (Q1838 "check fast"); the phone does the same in this release (Q1839). The old "Couldn't reach Ghostex" words stay only for this computer's own Ghostex restarting.
//! SEE-ALSO: server/src/connected_computers/held_sends.rs (the hold itself), apps/desktop/src/app/gx_chat/computer.rs (the desktop host's half), apps/mobile/app/src/chat/rust/computer.ts (the phone's).

use ghostex_gx_protocol::StartupDelivery;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::effect::Effect;
use crate::event::Event;
use crate::state::{ChatContext, ChatState, PendingSend};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The id every held message carries, so the queue's Remove reaches [`cancel`] instead.
pub const HELD_PREFIX: &str = "held-";
const TIMER_HELD_POLL: &str = "a:computer-held-poll";
const TIMER_CHECK: &str = "a:computer-check";
/// How often the held messages are read again while any wait, so one that went out disappears.
const HELD_POLL_MS: f64 = 5_000.0;
/// A message that got no answer this long has the computer checked (Q1838 "check fast"): the link
/// to a computer that was switched off still looks up for 45 to 60 s.
const CHECK_AFTER_MS: f64 = 10_000.0;

/// What the host knows about the computer the session runs on ([`Event::ComputerChanged`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionComputer {
    /// The computer's name as the app shows it ("Windows PC").
    pub name: String,
    /// The session runs on another computer, not this one's own gxserver.
    pub remote: bool,
    /// The app can't reach it now.
    pub offline: bool,
    /// When this app was last connected to it, epoch milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ms: Option<f64>,
    /// Its gxserver's server id, when the app knows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// The app's own id for it, which this computer's gxserver also knows it by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    /// When the host's last check of the computer ([`Effect::HostAction`] `checkComputer`) got an
    /// answer: a slow agent, not a computer that is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_at_ms: Option<f64>,
}

/// One message this computer holds for the session's computer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeldMessage {
    pub id: String,
    pub text: String,
    pub image_paths: Vec<String>,
    pub held_at_ms: i64,
    /// Its delivery is running now.
    pub sending: bool,
}

/// The computer's part of the chat state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputerState {
    pub computer: Option<SessionComputer>,
    pub held: Vec<HeldMessage>,
    /// A message got no answer for [`CHECK_AFTER_MS`]: since when the computer is being checked.
    pub checking_since_ms: Option<f64>,
    /// `holdSessionChatSend` calls in flight: the request and the local id of the row it draws.
    holds: Vec<(u64, String)>,
    list_request: Option<u64>,
    /// `cancelHeldSessionChatSend` calls in flight, and whether the text goes back to the box.
    cancels: Vec<(u64, bool)>,
    /// Returned messages already put back in the box, so a second read does not do it twice.
    returned_seen: Vec<String>,
    local_counter: u64,
}

/// The session's computer, when it is another one.
fn remote(state: &ChatState) -> Option<&SessionComputer> {
    state
        .session
        .computer
        .computer
        .as_ref()
        .filter(|computer| computer.remote)
}

fn name(state: &ChatState) -> String {
    remote(state)
        .map(|computer| computer.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "The computer".to_string())
}

/// Whether a send should be held instead of tried: the session's computer is known to be offline.
pub fn holds_sends(state: &ChatState) -> bool {
    remote(state).is_some_and(|computer| computer.offline)
}

/// Whether a refused send should be held: the session is on another computer and the call never
/// reached it.
pub fn holds_refusal(state: &ChatState, code: Option<&str>) -> bool {
    remote(state).is_some() && matches!(code, None | Some("unreachable" | "computerUnreachable"))
}

/// What a call nobody answered says for a session on another computer, in place of the words for
/// this computer's own Ghostex restarting.
pub fn unreachable_message(state: &ChatState) -> Option<String> {
    remote(state).map(|_| format!("Couldn't reach {}.", name(state)))
}

/// "2h ago", for the card's last-seen time.
fn ago(now_ms: f64, then_ms: f64) -> String {
    let minutes = ((now_ms - then_ms).max(0.0) / 60_000.0).floor() as i64;
    match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes}m ago"),
        60..=1439 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1440),
    }
}

/// The card above the chat box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerNotice {
    /// "Windows PC is offline", or "Windows PC isn't answering" while it is being checked.
    pub title: String,
    /// "last seen 2h ago", or "checking…".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// It is being checked: the renderer draws a spinner instead of Retry.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checking: bool,
}

/// The card, while the session's computer is offline or a message to it got no answer.
pub fn notice(state: &ChatState, context: &ChatContext) -> Option<ComputerNotice> {
    let computer = remote(state)?;
    let name = name(state);
    if computer.offline {
        return Some(ComputerNotice {
            title: format!("{name} is offline"),
            detail: computer
                .last_seen_ms
                .map(|seen| format!("last seen {}", ago(context.now_ms, seen))),
            checking: false,
        });
    }
    state
        .session
        .computer
        .checking_since_ms
        .map(|_| ComputerNotice {
            title: format!("{name} isn't answering"),
            detail: Some("checking\u{2026}".to_string()),
            checking: true,
        })
}

/// The held messages, drawn in the transcript like a message waiting in the queue, with "Will send
/// when Windows PC is back" under them and Cancel.
pub fn held_as_pending(state: &ChatState) -> Vec<PendingSend> {
    if state.session.computer.held.is_empty() {
        return Vec::new();
    }
    let name = name(state);
    state
        .session
        .computer
        .held
        .iter()
        .map(|held| PendingSend {
            id: format!("held:{}", held.id),
            queued_prompt_id: None,
            startup_delivery: Some(StartupDelivery {
                prompt_id: held.id.clone(),
                state: "held".to_string(),
                error_message: None,
                wait_reason: Some(if held.sending {
                    format!("Sending to {name}\u{2026}")
                } else {
                    format!("Will send when {name} is back")
                }),
            }),
            text: held.text.clone(),
            image_paths: held.image_paths.clone(),
            sent_at_ms: held.held_at_ms,
            after_message_id: None,
            after_message_timestamp: Some(held.held_at_ms),
            matching_occurrence: None,
            matching_after_timestamp: None,
            sent_while_working: false,
            shell_command: false,
            left_queue_at_ms: None,
        })
        .collect()
}

fn computer_params(state: &ChatState) -> Value {
    let computer = remote(state);
    json!({
        "serverId": computer.and_then(|computer| computer.server_id.clone()),
        "machineId": computer.and_then(|computer| computer.machine_id.clone()),
        "computerName": name(state),
    })
}

fn changed_rows(state: &mut ChatState) {
    state.messages.new_composition_identity();
    state.core.request_publish();
}

/// Holds one message: drawn at once, and handed to this computer's gxserver.
pub fn hold(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    image_paths: &[String],
    send_request_id: Option<&str>,
) -> Vec<Effect> {
    let computer = &mut state.session.computer;
    computer.local_counter += 1;
    let local_id = format!("{HELD_PREFIX}local-{}", computer.local_counter);
    computer.held.push(HeldMessage {
        id: local_id.clone(),
        text: text.to_string(),
        image_paths: image_paths.to_vec(),
        held_at_ms: context.now_millis(),
        sending: false,
    });
    computer.checking_since_ms = None;
    state.core.timers.cancel(TIMER_CHECK);
    let request_id = state.core.allocate_request_id();
    state.session.computer.holds.push((request_id, local_id));
    let mut params = computer_params(state);
    params["text"] = json!(text);
    if !image_paths.is_empty() {
        params["imagePaths"] = json!(image_paths);
    }
    if let Some(id) = send_request_id {
        params["sendRequestId"] = json!(id);
    }
    changed_rows(state);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::HoldSessionChatSend,
        params: Box::new(params),
    }]
}

/// Cancel on a held message (the queue's Remove for a [`HELD_PREFIX`] id): it is taken off hold
/// and its text goes back to the chat box.
pub fn cancel(state: &mut ChatState, id: &str) -> Vec<Effect> {
    let computer = &mut state.session.computer;
    let Some(index) = computer.held.iter().position(|held| held.id == id) else {
        return Vec::new();
    };
    if computer.held[index].sending {
        return Vec::new();
    }
    let held = computer.held.remove(index);
    changed_rows(state);
    let mut effects = vec![restore(&held.text)];
    // A hold still on its way to gxserver is cancelled once it answers (`settle`).
    if held.id.starts_with(&format!("{HELD_PREFIX}local-")) {
        return effects;
    }
    effects.push(cancel_call(state, &held.id, false));
    effects
}

fn cancel_call(state: &mut ChatState, id: &str, restores: bool) -> Effect {
    let request_id = state.core.allocate_request_id();
    state.session.computer.cancels.push((request_id, restores));
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::CancelHeldSessionChatSend,
        params: Box::new(json!({ "id": id })),
    }
}

/// The text goes back to the chat box, the way a refused send's does.
fn restore(text: &str) -> Effect {
    Effect::HostAction {
        action: "submissionFailed".to_string(),
        params: Box::new(json!({ "method": "send", "text": text })),
    }
}

fn read_held(state: &mut ChatState) -> Vec<Effect> {
    if remote(state).is_none() || state.session.computer.list_request.is_some() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.session.computer.list_request = Some(request_id);
    let mut params = computer_params(state);
    params["computerName"] = Value::Null;
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ListHeldSessionChatSends,
        params: Box::new(params),
    }]
}

/// The host's news about the computer.
pub fn changed(
    state: &mut ChatState,
    context: &ChatContext,
    computer: Option<SessionComputer>,
) -> Vec<Effect> {
    let first = state.session.computer.computer.is_none();
    let was_offline = holds_sends(state);
    if state.session.computer.computer == computer {
        return Vec::new();
    }
    state.session.computer.computer = computer;
    state.core.request_publish();
    let mut effects = Vec::new();
    if remote(state).is_none() {
        state.session.computer.checking_since_ms = None;
        return effects;
    }
    let answered = remote(state)
        .and_then(|computer| computer.answered_at_ms)
        .zip(state.session.computer.checking_since_ms)
        .is_some_and(|(answered, since)| answered >= since);
    if answered {
        state.session.computer.checking_since_ms = None;
    }
    if holds_sends(state) {
        state.session.computer.checking_since_ms = None;
        // A message still waiting on the dead link is held instead (Q1838): a send keeps its id,
        // so a copy that did reach the computer is answered there from the first attempt.
        effects.extend(crate::composer::send::hold_in_flight(state, context));
    }
    // The first news, and every return, shows what this computer still holds for it.
    if first || (was_offline && !holds_sends(state)) {
        effects.extend(read_held(state));
    }
    effects
}

/// A delivering call left for the computer: check it if no answer comes in time.
pub fn arm_check(state: &mut ChatState, context: &ChatContext) {
    if remote(state).is_some_and(|computer| !computer.offline) {
        state
            .core
            .timers
            .arm(TIMER_CHECK, context.now_ms, CHECK_AFTER_MS);
    }
}

/// Family a's settle for the computer: its calls' answers and its two timers.
pub fn settle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    if state.core.timer_fired(TIMER_CHECK) {
        if crate::composer::send::delivering(state) && !holds_sends(state) {
            state.session.computer.checking_since_ms = Some(context.now_ms);
            state.core.request_publish();
            effects.push(Effect::HostAction {
                action: "checkComputer".to_string(),
                params: Box::new(json!({})),
            });
        }
    } else if state.session.computer.checking_since_ms.is_some()
        && !crate::composer::send::delivering(state)
    {
        // The message was answered after all.
        state.session.computer.checking_since_ms = None;
        state.core.request_publish();
    }
    if state.core.timer_fired(TIMER_HELD_POLL) {
        effects.extend(read_held(state));
    }
    if let Event::RpcSettled {
        request_id,
        outcome,
    } = event
    {
        effects.extend(settle_call(state, *request_id, outcome, context));
    }
    effects
}

fn settle_call(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
    context: &ChatContext,
) -> Vec<Effect> {
    let computer = &mut state.session.computer;
    if let Some(at) = computer.holds.iter().position(|(id, _)| *id == request_id) {
        let (_, local_id) = computer.holds.remove(at);
        let still_shown = computer.held.iter().position(|held| held.id == local_id);
        return match (outcome, still_shown) {
            (RpcOutcome::Ok { result }, Some(index)) => {
                if let Some(held) = held_of(&result["held"]) {
                    computer.held[index] = held;
                }
                arm_poll(state, context);
                changed_rows(state);
                Vec::new()
            }
            // Cancelled before gxserver answered: take it off hold there too.
            (RpcOutcome::Ok { result }, None) => result["held"]["id"]
                .as_str()
                .map(|id| vec![cancel_call(state, &id.to_string(), false)])
                .unwrap_or_default(),
            (RpcOutcome::Err { message, .. }, index) => {
                let mut effects = Vec::new();
                if let Some(index) = index {
                    let held = computer.held.remove(index);
                    effects.push(restore(&held.text));
                }
                state.core.fail(format!("Not sent: {message}"), None);
                changed_rows(state);
                effects
            }
        };
    }
    if computer.list_request == Some(request_id) {
        computer.list_request = None;
        let RpcOutcome::Ok { result } = outcome else {
            arm_poll(state, context);
            return Vec::new();
        };
        return adopt_list(state, result, context);
    }
    if let Some(at) = computer
        .cancels
        .iter()
        .position(|(id, _)| *id == request_id)
    {
        let (_, restores) = computer.cancels.remove(at);
        if let (true, RpcOutcome::Ok { result }) = (restores, outcome) {
            if let Some(text) = result["text"].as_str() {
                return vec![restore(text)];
            }
        }
    }
    Vec::new()
}

fn held_of(value: &Value) -> Option<HeldMessage> {
    Some(HeldMessage {
        id: value["id"].as_str()?.to_string(),
        text: value["text"].as_str().unwrap_or_default().to_string(),
        image_paths: value["imagePaths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        held_at_ms: value["heldAtMs"].as_i64().unwrap_or_default(),
        sending: value["sending"] == true,
    })
}

/// gxserver's list: the rows to draw, and any message that came back unsent (past its day, or
/// refused by the computer), which goes back to the box with its note.
fn adopt_list(state: &mut ChatState, result: &Value, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    let mut held = Vec::new();
    for row in result["sends"].as_array().into_iter().flatten() {
        let Some(message) = held_of(row) else {
            continue;
        };
        match row["returned"].as_str() {
            Some(reason) => {
                if state.session.computer.returned_seen.contains(&message.id) {
                    continue;
                }
                state
                    .session
                    .computer
                    .returned_seen
                    .push(message.id.clone());
                state
                    .core
                    .fail(reason.to_string(), Some("heldReturned".to_string()));
                effects.push(cancel_call(state, &message.id, true));
            }
            None => held.push(message),
        }
    }
    // Rows held on this side whose hold has not answered yet stay drawn.
    let local = format!("{HELD_PREFIX}local-");
    held.extend(
        state
            .session
            .computer
            .held
            .iter()
            .filter(|row| row.id.starts_with(&local))
            .cloned(),
    );
    if state.session.computer.held != held {
        state.session.computer.held = held;
        changed_rows(state);
    }
    arm_poll(state, context);
    effects
}

fn arm_poll(state: &mut ChatState, context: &ChatContext) {
    if state.session.computer.held.is_empty() {
        state.core.timers.cancel(TIMER_HELD_POLL);
    } else {
        state
            .core
            .timers
            .arm(TIMER_HELD_POLL, context.now_ms, HELD_POLL_MS);
    }
}

/// Retry on the card: the host reconnects the computer, and the core reads what it holds again.
pub fn retry(state: &mut ChatState) -> Vec<Effect> {
    state.core.request_publish();
    let mut effects = vec![Effect::HostAction {
        action: "reconnectComputer".to_string(),
        params: Box::new(json!({})),
    }];
    effects.extend(read_held(state));
    effects
}
