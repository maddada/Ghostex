//! The send path: `send`, `queue`, `compact`, `handoff`, `receiveHandoff`, `interrupt`, `sendKey`.
//!
//! Port of the `send`/`queue`/`compact`, `handoff`, `receiveHandoff`, `interrupt` and `sendKey`
//! arms of `packages/shared/session-chat-controller/native-host.ts`, of
//! `deliverChatSubmission` (`submission.ts`), `sendSessionChatOptionAware` (`option-command.ts`)
//! and of the `send`, `sendKey`, `queuePrompt`, `pushDraft` and `interrupt` callbacks in
//! `controller.ts`.
//!
//! **The shape.** The TypeScript arm awaits five or six things in a row and its closing
//! `publish(controller.current())` runs after the last of them. The core cannot await, so the
//! same order is a list of [`SendPhase`]s on [`Submission`], one answer at a time: the head phase
//! is in flight, its answer runs the next one, and the arm's publish lands when the list empties
//! (`CoreState::publish_awaits`).
//!
//! **What stays with family a.** The optimistic echo, the "Ran /x" marker, the keystroke marker
//! and the Stop suppression are the pending matcher's rules, so this file calls
//! `crate::session::sends` for every one of them and never writes `state.pending` itself.
//!
//! **What stays with the host.** `draftSubmitted`, `submissionFailed` and `draftReceived` are the
//! composer field's own bookkeeping (`docs/2026-09-21/rust-chat/HOST-TODO.md` section 2), and the
//! handoff id is a `crypto.randomUUID()` the core has no source for. All three ride out as
//! [`Effect::HostAction`].

use serde_json::{json, Value};

use crate::composer::storage::{
    encode_stored_draft, StoredDraftRecord, DRAFTS_STORE, DRAFT_PARK_STORE, DRAFT_RECEIVE_STORE,
    DRAFT_SUBMITTED_STORE,
};
use crate::composer::submission::{
    classify_draft_handoff, submission_steps, HandoffDisposition, StoredDraft, SubmissionMode,
    SubmissionStep,
};
use crate::effect::Effect;
use crate::event::StorageKey;
use crate::jsnum::js_number_of;
use crate::session::sends;
use crate::session::streaming::{classify_send, SendClassification};
use crate::state::Submission;
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// One step of a submission, in the order the TypeScript runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendPhase {
    /// `composer('write', {text, version, submitted: true})`: the submitted revision is durable
    /// before anything is delivered.
    WriteDraft,
    /// `composer('flush')`: the save outbox is on disk.
    FlushDraft,
    /// `chat.draft.push(text, version)`, which is `setSessionChatDraft`.
    PushDraft,
    /// `send('/compact')`, the first half of a compact.
    SendCompact,
    /// `send(text, version)` through `sendSessionChatOptionAware`.
    SendText,
    /// `queue(text, version)`, which is `queueSessionChatPrompt`.
    QueueText,
    /// `composer('submitted', {text, version})`: clear the stored draft, record the send, flush.
    MarkSubmitted,
    /// `composer('park', {text, version})`: the draft is the terminal's now.
    ParkDraft,
}

/// `send`, `queue` and `compact`, which are one arm in the TypeScript.
///
/// `interrupt` is interrupt and send (Ctrl+Enter, the Send menu, the phone's More actions); see
/// [`interrupts_turn`].
pub fn begin(
    state: &mut ChatState,
    context: &ChatContext,
    mode: SubmissionMode,
    text: &str,
    version: Option<crate::composer::queue::DraftVersion>,
    image_paths: Vec<String>,
    interrupt: bool,
) -> Vec<Effect> {
    // A block that clears on its own does not refuse: the delivery phase waits for it instead.
    if let Some(refused) = crate::composer::document::send_refused(state) {
        state.core.fail(refused, None);
        return vec![submission_failed(mode, text)];
    }
    let version = version.map(|version| unconsumed_version(state, context, version));
    let send_request_id = submission_send_request_id(state, context, mode, text);
    let capabilities = crate::composer::document::queue(state).capabilities;
    let steps = match submission_steps(
        text,
        version.as_ref(),
        mode,
        capabilities.can_sync_draft,
        capabilities.can_queue,
    ) {
        Ok(steps) => steps,
        Err(message) => {
            state.core.fail(message, None);
            return vec![submission_failed(mode, text)];
        }
    };
    // The draft leaves with its pictures; a refusal re-inserts the text and counts them again.
    state.composer.draft_attachment_count = 0;
    // The send pushes its own final revision, so a push still waiting for a pause would only race it.
    crate::composer::draft_sync::cancel_pending_push(state);
    let mut phases = vec![SendPhase::WriteDraft, SendPhase::FlushDraft];
    for step in &steps {
        phases.push(match step {
            SubmissionStep::PushDraft { .. } => SendPhase::PushDraft,
            SubmissionStep::Send { text: sent, .. } if sent == "/compact" && phases.len() > 2 => {
                SendPhase::SendCompact
            }
            SubmissionStep::Send { .. } if mode == SubmissionMode::Compact => {
                SendPhase::SendCompact
            }
            SubmissionStep::Send { .. } => SendPhase::SendText,
            SubmissionStep::Queue { .. } => SendPhase::QueueText,
        });
    }
    phases.push(SendPhase::MarkSubmitted);
    let interrupt = interrupt && interrupts_turn(state, mode);
    if interrupt {
        raise_interrupted_toast(state, context);
    }
    let mut submission = Submission {
        text: text.to_string(),
        version,
        mode,
        cancelled: false,
        image_paths,
        phases,
        request: None,
        storage: None,
        pending_id: None,
        marker: None,
        refresh_after_send: state.session.available_agents.is_some(),
        handoff: false,
        awaiting_gate: false,
        drawn: false,
        send_request_id,
        interrupt,
    };
    draw_at_enter(state, context, &mut submission);
    start_or_wait(state, context, submission)
}

/// Whether an interrupt and send stops a turn: only a plain send to a working agent does. An idle
/// agent just receives the message, and Compact & Send and the queue wait for the turn anyway.
///
/// CDXC:SessionChat 2026-10-11 DECISION:
/// User: "I'm having to switch to terminal to press ctrl + return in order to interrupt and send a prompt. should be able to send this from the chat view without switching to terminal view." Ctrl+Enter in the chat box (and Interrupt & Send in the Send button's menu, and in More actions on the phone) stops the agent's running turn and sends the message at once, the way Claude Code's Ctrl+Enter does in the terminal. gxserver types the agent's own keys for it (session_chat_queue_runtime/send.rs).
pub fn interrupts_turn(state: &ChatState, mode: SubmissionMode) -> bool {
    mode == SubmissionMode::Send && crate::session::working::is_working(state)
}

/// The red "Agent was interrupted" toast, for 2 seconds.
fn raise_interrupted_toast(state: &mut ChatState, context: &ChatContext) {
    state.composer.interrupted_toast = true;
    state.core.timers.arm(
        INTERRUPTED_TOAST_TIMER,
        context.now_ms,
        crate::composer::policy::INTERRUPTED_TOAST_MS as f64,
    );
    state.core.request_publish();
}

/// CDXC:SessionChat 2026-10-07 DECISION:
/// User: "When I press Enter to send a message and there's an image (or maybe even without an image), the text I wrote doesn't instantly disappear from the chat box and appear in the chat transcript. Please fix this, I want this to be INSTANT." The echo (or the "Ran /x" marker, or the queued row) is drawn and published on the Enter itself, before the draft save, the gxserver draft push and the send that follow it; the closing publish of the send's chain (`CoreState::publish_awaits`) had held it until gxserver finished typing the message, about 1.2 seconds. A failed or refused send drops it again and the composer gets the text back with the error, as before.
fn draw_at_enter(state: &mut ChatState, context: &ChatContext, submission: &mut Submission) {
    let body = match submission.mode {
        SubmissionMode::Send => Some(submission.text.clone()),
        SubmissionMode::Compact => Some("/compact".to_string()),
        // The queued row is drawn from the submission itself (`composer::document::queue`).
        SubmissionMode::Queue => None,
    };
    if let Some(body) = body {
        let images = match submission.mode {
            SubmissionMode::Send => submission.image_paths.clone(),
            _ => Vec::new(),
        };
        let drawn = draw_agent_send(state, context, &body, &images);
        submission.pending_id = drawn.pending_id;
        submission.marker = drawn.marker;
    }
    submission.drawn = true;
    state.core.request_publish();
}

/// Starts a submission, or holds it behind the one in flight.
///
/// CDXC:SessionChat 2026-10-07 WHY:
/// The composer used to ignore Enter until the previous send settled (gxserver answers a send only after it has typed it, 1.1 to 1.6 seconds), so a quick follow-up stayed in the box. Sends now line up here in order, each with its own `sendRequestId`, and one that fails or is cancelled takes every send behind it back to the composer with it, so nothing is delivered out of order and nothing typed is lost.
fn start_or_wait(
    state: &mut ChatState,
    context: &ChatContext,
    submission: Submission,
) -> Vec<Effect> {
    if state.composer.submitting.is_some() {
        state.composer.waiting.push(submission);
        return Vec::new();
    }
    state.composer.submitting = Some(submission);
    run_head(state, context)
}

/// The submission ahead ended: the next one waiting starts.
///
/// The ones an Escape cancelled while they waited go back to the composer instead, together; a
/// send typed after that Escape still goes out.
fn start_next(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    if state.composer.submitting.is_some() {
        return Vec::new();
    }
    let mut effects = Vec::new();
    let mut returned: Option<(SubmissionMode, String)> = None;
    while !state.composer.waiting.is_empty() {
        let next = state.composer.waiting.remove(0);
        if !next.cancelled {
            state.composer.submitting = Some(next);
            break;
        }
        undo_drawn(state, next.pending_id.as_deref(), next.marker.as_ref());
        if next.handoff {
            effects.push(Effect::HostAction {
                action: "draftHandoffToTerminalFailed".to_string(),
                params: Box::new(json!({ "error": "The session chat send was cancelled." })),
            });
        } else {
            returned = Some(match returned {
                Some((mode, text)) => (mode, format!("{text}\n{}", next.text)),
                None => (next.mode, next.text),
            });
        }
    }
    if let Some((mode, text)) = returned {
        state
            .core
            .fail("The session chat send was cancelled.".to_string(), None);
        effects.push(submission_failed(mode, &text));
    }
    effects.extend(run_head(state, context));
    effects
}

/// CDXC:SessionChat 2026-10-05 WHY:
/// gxserver delivers one `sendRequestId` at most once (server session_chat_send_requests.rs). Each Enter names its submission with a fresh id, except Enter on the same text right after that submission's call failed: a timed-out call may still have been delivered (a phone's SSH call gives up after 75 seconds while gxserver keeps typing), so the retry reuses the id and gxserver answers from the first attempt instead of typing the message twice. A refusal gxserver could not settle (`sendOutcomeUnknown`) is not reused, or the same text could never be sent again. Without host entropy the id is left to gxserver: a constant id would make every later send a duplicate.
fn submission_send_request_id(
    state: &mut ChatState,
    context: &ChatContext,
    mode: SubmissionMode,
    text: &str,
) -> Option<String> {
    let retried = state
        .composer
        .failed_send
        .take()
        .filter(|failed| failed.mode == mode && failed.text == text)
        .map(|failed| failed.send_request_id);
    retried.or_else(|| (context.random_ids[0] != 0).then(|| context.random_id(0)))
}

/// Remembers a submission whose send or queue call failed, for [`submission_send_request_id`].
fn remember_failed_send(state: &mut ChatState, code: Option<&str>) {
    let Some(submission) = state.composer.submitting.as_ref() else {
        return;
    };
    let delivering = matches!(
        submission.phases.first(),
        Some(SendPhase::SendCompact | SendPhase::SendText | SendPhase::QueueText)
    );
    state.composer.failed_send = submission
        .send_request_id
        .clone()
        .filter(|_| delivering && code != Some("sendOutcomeUnknown"))
        .map(|send_request_id| crate::state::FailedSend {
            text: submission.text.clone(),
            mode: submission.mode,
            send_request_id,
        });
}

/// Whether gxserver's receipts say this revision (or a later one of the same draft) was already sent.
fn is_consumed(state: &ChatState, version: &crate::composer::queue::DraftVersion) -> bool {
    state
        .session
        .synced_draft
        .as_ref()
        .and_then(|draft| draft.get("consumedDrafts"))
        .and_then(Value::as_array)
        .is_some_and(|receipts| {
            receipts.iter().any(|receipt| {
                receipt.get("draftId").and_then(Value::as_str) == Some(&version.draft_id)
                    && js_number_of(receipt.get("revision"))
                        .is_some_and(|revision| revision >= version.revision as f64)
            })
        })
}

/// The identity a send goes out under: a fresh one when the composer's is already sent.
///
/// CDXC:Drafts 2026-09-27 WHY:
/// gxserver refuses a send whose draft revision it already consumed ("The submitted draft revision is no longer available"), which is right for a retried copy of one submission. But the composer can still hold a consumed identity when the user presses Enter: on 2026-09-27 a "continue" typed under revision 11 of one draft went out at 09:40, and at 18:16 Enter sent "continue" again under that same revision. The save before the send was silently ignored as obsolete, the send bounced, and the text only went on the second Enter because the restored text had a new identity. A user's Enter is a new submission, so it gets a new identity here; a retry of one submission keeps its own and is still refused.
fn unconsumed_version(
    state: &ChatState,
    context: &ChatContext,
    version: crate::composer::queue::DraftVersion,
) -> crate::composer::queue::DraftVersion {
    if !is_consumed(state, &version) {
        return version;
    }
    crate::composer::queue::DraftVersion {
        draft_id: context.random_id(1),
        revision: 1,
    }
}

/// Runs the head phase, or finishes the submission when the list has run out.
fn run_head(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(submission) = state.composer.submitting.as_ref() else {
        return Vec::new();
    };
    let Some(phase) = submission.phases.first().copied() else {
        let mut effects = finish(state);
        effects.extend(start_next(state, context));
        return effects;
    };
    // CDXC:SessionChat 2026-09-17 WHY:
    // Escape can arrive while a draft save is pending, before the daemon has a send to cancel.
    // Both renderers must cancel here as well so the recovered draft is not delivered after the
    // interrupt.
    if submission.cancelled
        && matches!(
            phase,
            SendPhase::SendCompact | SendPhase::SendText | SendPhase::QueueText
        )
    {
        undo_optimistic(state);
        return fail(state, "The session chat send was cancelled.");
    }
    let text = submission.text.clone();
    let version = submission.version.clone();
    let handoff = submission.handoff;
    let interrupt = submission.interrupt && phase == SendPhase::SendText;
    let queued_text = match submission.mode {
        SubmissionMode::Queue => text.trim().to_string(),
        _ => text.clone(),
    };
    match phase {
        SendPhase::WriteDraft => {
            // A send that waited behind another can start after the user has typed the next
            // message under a new identity; the stored draft is that message now, not this one.
            if !handoff && holds_newer_typing(state, &text, version.as_ref()) {
                if let Some(submission) = state.composer.submitting.as_mut() {
                    submission.phases.remove(0);
                }
                return run_head(state, context);
            }
            let key = draft_key(state);
            let record = StoredDraftRecord {
                text: text.clone(),
                updated_at: Some(context.now_millis() as f64),
                version: version.clone(),
                submitted: !handoff,
                parked: false,
            };
            state.composer.stored_draft = Some(record.clone());
            wait_storage(state, key.clone());
            vec![Effect::WriteStorage {
                key,
                value: Some(encode_stored_draft(&record)),
                durable: false,
            }]
        }
        SendPhase::FlushDraft => {
            wait_storage(state, flush_key());
            vec![Effect::FlushStorage {
                store: DRAFTS_STORE.to_string(),
            }]
        }
        SendPhase::PushDraft => {
            let request_id = state.core.allocate_request_id();
            wait_request(state, request_id);
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::SetSessionChatDraft,
                params: Box::new(json!({
                    "clientId": state.identity.client_id,
                    "content": text,
                    "draftVersion": version,
                })),
            }]
        }
        SendPhase::SendCompact | SendPhase::SendText => {
            let (body, version, images, send_request_id) = match phase {
                SendPhase::SendCompact => (
                    "/compact".to_string(),
                    None,
                    Vec::new(),
                    submission
                        .send_request_id
                        .as_ref()
                        .map(|id| format!("{id}-compact")),
                ),
                _ => (
                    text,
                    version,
                    submission.image_paths.clone(),
                    submission.send_request_id.clone(),
                ),
            };
            if !submission.drawn {
                // gxserver turns each review comment pill into its block before the agent reads
                // it (send_session_chat_message_with_draft), so the echo shows the same text the
                // transcript will record.
                let echoed = crate::composer::review_comments::expand_review_comments(&body);
                let drawn = draw_agent_send(state, context, &echoed, &images);
                adopt_send(state, drawn);
            }
            if hold_for_gate(state, context) {
                return Vec::new();
            }
            let request_id = state.core.allocate_request_id();
            wait_request(state, request_id);
            let mut effect = agent_send_rpc(
                request_id,
                &body,
                version,
                &images,
                send_request_id.as_deref(),
            );
            if let (true, Effect::SendRpc { params, .. }) = (interrupt, &mut effect) {
                params["interrupt"] = json!(true);
            }
            vec![effect]
        }
        SendPhase::QueueText => {
            let send_request_id = submission.send_request_id.clone();
            if hold_for_gate(state, context) {
                return Vec::new();
            }
            let request_id = state.core.allocate_request_id();
            wait_request(state, request_id);
            let mut params = json!({ "text": queued_text, "draftVersion": version });
            if let Some(id) = send_request_id {
                params["sendRequestId"] = json!(id);
            }
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::QueueSessionChatPrompt,
                params: Box::new(params),
            }]
        }
        SendPhase::MarkSubmitted => {
            let key = operation_key(state, DRAFT_SUBMITTED_STORE);
            wait_storage(state, key.clone());
            vec![Effect::WriteStorage {
                key,
                value: Some(json!({ "text": text, "version": version }).to_string()),
                durable: true,
            }]
        }
        SendPhase::ParkDraft => {
            let key = operation_key(state, DRAFT_PARK_STORE);
            if let Some(record) = state.composer.stored_draft.as_mut() {
                record.parked = true;
            }
            wait_storage(state, key.clone());
            vec![Effect::WriteStorage {
                key,
                value: Some(json!({ "text": text, "version": version }).to_string()),
                durable: true,
            }]
        }
    }
}

/// Whether the stored draft is text typed after this submission, under another identity.
fn holds_newer_typing(
    state: &ChatState,
    text: &str,
    version: Option<&crate::composer::queue::DraftVersion>,
) -> bool {
    state.composer.stored_draft.as_ref().is_some_and(|stored| {
        !stored.text.is_empty()
            && stored.text != text
            && !stored.submitted
            && !stored.parked
            && stored.version.as_ref().map(|stored| &stored.draft_id)
                != version.map(|version| &version.draft_id)
    })
}

/// What one call to [`send_to_agent`] left behind, so the caller can undo it if the call fails.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSend {
    pub request_id: u64,
    /// Family a's optimistic echo, for a chat send.
    pub pending_id: Option<String>,
    /// The "Ran /x" marker's command and stamp, for a catalog slash command.
    pub marker: Option<(String, i64)>,
}

/// `sendSessionChatOptionAware` plus `chat.send`: the pills move, the echo or the marker is
/// recorded, and the call goes out.
///
/// The echo and the marker are recorded BEFORE the call and undone when it fails, which is what
/// makes a send read as a new row the instant the user presses Enter. Public because family e's
/// option dispatch types a command into the agent through the same seam
/// (`onDispatchCommand` in `native-host.ts`, which was `option-command.ts`).
pub fn send_to_agent(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    version: Option<crate::composer::queue::DraftVersion>,
    image_paths: &[String],
) -> (AgentSend, Vec<Effect>) {
    let mut sent = draw_agent_send(state, context, text, image_paths);
    sent.request_id = state.core.allocate_request_id();
    let effect = agent_send_rpc(sent.request_id, text, version, image_paths, None);
    (sent, vec![effect])
}

/// The half of [`send_to_agent`] the user sees at once: the pills move and the echo or the marker
/// is recorded. `request_id` is left 0 for the caller to fill when the call leaves.
fn draw_agent_send(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    image_paths: &[String],
) -> AgentSend {
    // What the agent records, which is what the echo has to match: gxserver types a Claude skill
    // pill as its bare `/name`, so a draft that opens with one is a slash command to Claude.
    let agent_text =
        crate::composer::skill_invocation::agent_skill_text(text, state.session.agent.as_deref());
    let text = agent_text.as_ref();
    let catalog = crate::menus::option_catalog::session_option_catalog(
        &state.menus.model_catalog,
        state.session.agent.as_deref(),
    );
    state
        .menus
        .options
        .reconcile_typed_command(catalog.as_ref(), text, context.now_millis());
    // `classifySessionChatSend(text, commandCatalog)`: the controller passes no skill prefix, so a
    // `$token` is prose here even under Codex.
    let classification = classify_send(text, &command_catalog(), None);
    let mut pending_id = None;
    let mut marker = None;
    match classification {
        SendClassification::Chat if !text.trim().is_empty() || !image_paths.is_empty() => {
            pending_id = Some(sends::begin_send(state, context, text, image_paths));
        }
        SendClassification::LocalCommand if image_paths.is_empty() => {
            pending_id = Some(sends::begin_shell_command_send(state, context, text));
        }
        SendClassification::Command => {
            let sent_at = sends::begin_command_marker(state, context, text);
            marker = Some((
                text.trim_matches(crate::session::text::is_js_space)
                    .to_string(),
                sent_at,
            ));
        }
        _ => {}
    }
    AgentSend {
        request_id: 0,
        pending_id,
        marker,
    }
}

/// `chat.send`'s gxserver call.
fn agent_send_rpc(
    request_id: u64,
    text: &str,
    version: Option<crate::composer::queue::DraftVersion>,
    image_paths: &[String],
    send_request_id: Option<&str>,
) -> Effect {
    let mut params = json!({
        "text": text,
        "imagePaths": if image_paths.is_empty() { Value::Null } else { json!(image_paths) },
        "draftVersion": version,
    });
    if let Some(id) = send_request_id {
        params["sendRequestId"] = json!(id);
    }
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(params),
    }
}

/// The submission's echo or marker, so a refusal can take it back.
fn adopt_send(state: &mut ChatState, sent: AgentSend) {
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.pending_id = sent.pending_id;
        submission.marker = sent.marker;
        submission.drawn = true;
    }
}

/// Parks the head delivery phase while the send gate is shut, answering whether it did.
///
/// The TypeScript's `holdUntilSendable`: [`release_held`] runs the phase again once the gate
/// clears, and an interrupt fails it at once.
fn hold_for_gate(state: &mut ChatState, context: &ChatContext) -> bool {
    let blocked = crate::composer::document::send_blocked(state, context).is_some();
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.awaiting_gate = blocked;
        if blocked {
            submission.request = None;
            submission.storage = None;
        }
    }
    blocked
}

/// Settle hook: a delivery phase parked by [`hold_for_gate`] resumes once the gate is clear.
///
/// Runs after family e's settle, because the option dispatch that clears `optionSwitching` answers
/// there.
pub fn release_held(
    state: &mut ChatState,
    _event: &crate::event::Event,
    context: &ChatContext,
) -> Vec<Effect> {
    let parked = state
        .composer
        .submitting
        .as_ref()
        .is_some_and(|submission| submission.awaiting_gate);
    if !parked || crate::composer::document::send_blocked(state, context).is_some() {
        return Vec::new();
    }
    let effects = run_head(state, context);
    state.core.publish_after(&effects);
    effects
}

/// Undoes one [`AgentSend`] whose call never reached the agent.
pub fn undo_agent_send(state: &mut ChatState, sent: &AgentSend) {
    if let Some(pending_id) = sent.pending_id.as_deref() {
        sends::drop_send(state, pending_id);
    }
    if let Some((command, sent_at)) = sent.marker.as_ref() {
        sends::drop_command_marker(state, command, *sent_at);
    }
}

/// The answer to the head phase arrived.
///
/// Answers `None` when the id or the key is not the submission's, so the caller can offer it to
/// whatever else is in flight.
pub fn settle_request(
    state: &mut ChatState,
    context: &ChatContext,
    request_id: u64,
    outcome: &crate::wire::RpcOutcome,
) -> Option<Vec<Effect>> {
    if state.composer.submitting.as_ref()?.request != Some(request_id) {
        return None;
    }
    let mut effects = Vec::new();
    match outcome {
        crate::wire::RpcOutcome::Ok { result } => {
            // A receipt that names a queue row makes the echo that row's optimistic twin.
            if let (Some(pending_id), Some(queued)) = (
                state
                    .composer
                    .submitting
                    .as_ref()
                    .and_then(|submission| submission.pending_id.clone()),
                result
                    .get("queuedPromptId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ) {
                sends::adopt_queued_prompt_id(state, &pending_id, &queued);
            }
            // Every queue mutation answers with the whole authoritative queue, so an optimistic
            // step that lost a race self-corrects on the next line instead of rolling back.
            if let Some(queue) = result.get("queue").and_then(Value::as_array) {
                // `setQueuePrompts(result.queue)`: the answer's own array, equal or not.
                state.session.queue_prompts = Some(queue.clone());
                state.messages.new_composition_identity();
            }
            // `setSyncedDraft((current) => mergeSessionChatDraftState(current, result.draft))`:
            // the receipts union, and a newer local revision wins over the answer's body.
            if let Some(draft) = result.get("draft") {
                state.session.synced_draft = Some(crate::session::fold::merge_draft_state(
                    state.session.synced_draft.as_ref(),
                    draft,
                ));
                state.session.synced_draft_revision += 1;
            }
            if let Some(submission) = state.composer.submitting.as_mut() {
                let sent = submission.phases.first().copied();
                submission.request = None;
                // The echo was drawn at Enter, so it stays undoable until its own delivery
                // answered; the draft push ahead of it can still be followed by a refused send.
                if matches!(sent, Some(SendPhase::SendCompact | SendPhase::SendText)) {
                    submission.pending_id = None;
                    submission.marker = None;
                }
                if !submission.phases.is_empty() {
                    submission.phases.remove(0);
                }
                // `if (chat.availableAgents) chat.refresh()`: a draft session's identity only
                // settles once the daemon has seen the first send.
                if sent == Some(SendPhase::SendText) && submission.refresh_after_send {
                    effects.extend(crate::session::reads::request_resync(state, context));
                }
            }
            effects.extend(run_head(state, context));
        }
        crate::wire::RpcOutcome::Err { message, code, .. } => {
            remember_failed_send(state, code.as_deref());
            undo_optimistic(state);
            effects.extend(fail(state, message));
        }
    }
    state.core.publish_after(&effects);
    Some(effects)
}

/// A queue mutation answered.
///
/// `queueMutation` in `controller.ts`: every endpoint hands back the whole authoritative queue, so
/// the strip is replaced rather than patched and an optimistic step that lost a race self-corrects
/// here instead of needing a rollback path. A remove also drops the echo that had become that row.
pub fn settle_queue_mutation(
    state: &mut ChatState,
    request_id: u64,
    outcome: &crate::wire::RpcOutcome,
) -> Option<Vec<Effect>> {
    let (pending, removed) = state.composer.queue_mutation.clone()?;
    if pending != request_id {
        return None;
    }
    state.composer.queue_mutation = None;
    match outcome {
        crate::wire::RpcOutcome::Ok { result } => {
            if let Some(queue) = result.get("queue").and_then(Value::as_array) {
                // `setQueuePrompts(result.queue)`: the answer's own array, equal or not.
                state.session.queue_prompts = Some(queue.clone());
                state.messages.new_composition_identity();
            }
            if let Some(removed) = removed.as_deref() {
                crate::questions::forms::reopen_after_removed_answers(state, removed);
                sends::drop_queued_send(state, removed);
            }
        }
        // The row or its session is gone by the time Remove arrived, so the card is all there is.
        crate::wire::RpcOutcome::Err { code, .. }
            if removed.is_some() && code.as_deref() == Some("notFound") =>
        {
            if let Some(removed) = removed.as_deref() {
                sends::drop_queued_send(state, removed);
            }
        }
        crate::wire::RpcOutcome::Err { message, code, .. } => {
            state.core.fail(message.clone(), code.clone());
        }
    }
    Some(Vec::new())
}

/// The stored write or flush the head phase was waiting for answered.
pub fn settle_storage(
    state: &mut ChatState,
    context: &ChatContext,
    key: &StorageKey,
    error: Option<&str>,
) -> Option<Vec<Effect>> {
    if state.composer.submitting.as_ref()?.storage.as_ref() != Some(key) {
        return None;
    }
    let mut effects = Vec::new();
    match error {
        None => {
            if let Some(submission) = state.composer.submitting.as_mut() {
                submission.storage = None;
                if !submission.phases.is_empty() {
                    submission.phases.remove(0);
                }
            }
            effects.extend(run_head(state, context));
        }
        Some(message) => {
            undo_optimistic(state);
            effects.extend(fail(state, message));
        }
    }
    state.core.publish_after(&effects);
    Some(effects)
}

/// The last phase answered: the host adopts the new revision and clears the field.
///
/// A handoff also tells the app shell the transfer is ready; the handoff id itself is the host's,
/// because `composer('park')` mints it with `crypto.randomUUID()` and the core has no random
/// source (`docs/2026-09-21/rust-chat/SEAM.md` section 7.3).
fn finish(state: &mut ChatState) -> Vec<Effect> {
    let Some(submission) = state.composer.submitting.take() else {
        return Vec::new();
    };
    // A send waiting behind this one continues the same chain, which would otherwise hold this
    // one's closing publish until the last of them answered.
    state.core.request_publish();
    let method = if submission.handoff {
        "handoff"
    } else {
        mode_name(submission.mode)
    };
    let mut effects = vec![Effect::HostAction {
        action: "draftSubmitted".to_string(),
        params: Box::new(json!({
            "method": method,
            "text": submission.text,
            "version": submission.version,
        })),
    }];
    if submission.mode == SubmissionMode::Send
        && !submission.handoff
        && crate::composer::side_chat::sent_side_chat_opens_terminal(state, &submission.text)
    {
        effects.push(Effect::HostAction {
            action: "switchToTerminal".to_string(),
            params: Box::new(json!({})),
        });
    }
    if submission.handoff {
        effects.push(Effect::HostAction {
            action: "draftHandoffToTerminalComplete".to_string(),
            params: Box::new(json!({
                "content": submission.text,
                "draftVersion": submission.version,
            })),
        });
    }
    effects
}

/// A phase refused: the text goes back to the composer and the submission is over.
///
/// CDXC:Drafts 2026-09-22 WHY:
/// A refused handoff also has to tell the app shell, because the terminal side is already waiting
/// for a draft that is never coming (`native-host.ts:1642`, the `catch` around the whole arm).
/// Without it the terminal composer sits on a transfer that silently died.
fn fail(state: &mut ChatState, message: &str) -> Vec<Effect> {
    let Some(submission) = state.composer.submitting.take() else {
        return Vec::new();
    };
    state.core.fail(message.to_string(), None);
    let mut effects = Vec::new();
    // The sends waiting behind this one go back to the composer with it, in the order they were
    // typed, as one restore: the host answers each restore with the field it holds, so two in a
    // row would both merge into the same stale text.
    let mut text = submission.text.clone();
    let mut handoffs = usize::from(submission.handoff);
    for waiting in std::mem::take(&mut state.composer.waiting) {
        undo_drawn(
            state,
            waiting.pending_id.as_deref(),
            waiting.marker.as_ref(),
        );
        if waiting.handoff {
            // A handoff never left the field, so its text is still there.
            handoffs += 1;
        } else if !waiting.text.is_empty() {
            text = format!("{text}\n{}", waiting.text);
        }
    }
    for _ in 0..handoffs {
        effects.push(Effect::HostAction {
            action: "draftHandoffToTerminalFailed".to_string(),
            // `params: { error: operationError }`, which is the message the bar now carries.
            params: Box::new(json!({ "error": message })),
        });
    }
    effects.push(submission_failed(submission.mode, &text));
    effects
}

/// The echo and the marker go with a call that never reached the agent.
fn undo_optimistic(state: &mut ChatState) {
    let (pending_id, marker) = match state.composer.submitting.as_ref() {
        Some(submission) => (submission.pending_id.clone(), submission.marker.clone()),
        None => return,
    };
    undo_drawn(state, pending_id.as_deref(), marker.as_ref());
}

fn undo_drawn(state: &mut ChatState, pending_id: Option<&str>, marker: Option<&(String, i64)>) {
    if let Some(pending_id) = pending_id {
        sends::drop_send(state, pending_id);
    }
    if let Some((command, sent_at)) = marker {
        sends::drop_command_marker(state, command, *sent_at);
    }
}

/// `handoff`: the draft is parked here and handed to the terminal.
pub fn handoff(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    version: Option<crate::composer::queue::DraftVersion>,
) -> Vec<Effect> {
    crate::composer::draft_sync::cancel_pending_push(state);
    let submission = Submission {
        text: text.to_string(),
        version: version.clone(),
        mode: SubmissionMode::Send,
        cancelled: false,
        image_paths: Vec::new(),
        phases: vec![
            SendPhase::WriteDraft,
            SendPhase::PushDraft,
            SendPhase::ParkDraft,
        ],
        request: None,
        storage: None,
        pending_id: None,
        marker: None,
        refresh_after_send: false,
        handoff: true,
        awaiting_gate: false,
        drawn: true,
        send_request_id: None,
        interrupt: false,
    };
    // The head phase writes the unsubmitted record a handoff parks from.
    start_or_wait(state, context, submission)
}

/// `receiveHandoff`: a draft arrives from the terminal or from another client.
///
/// The disposition is decided here rather than by the host: `classifyDraftHandoff` is a pure rule
/// and the core already holds the stored entry and the composer's current text, so the host's job
/// is only the durable write and the recovery checkpoint it keeps anyway.
pub fn receive_handoff(
    state: &mut ChatState,
    context: &ChatContext,
    handoff_id: &str,
    content: &str,
    current: &str,
    version: Option<crate::composer::queue::DraftVersion>,
) -> Vec<Effect> {
    if state
        .composer
        .receiving_handoffs
        .iter()
        .any(|id| id == handoff_id)
    {
        return Vec::new();
    }
    let consumed = version
        .as_ref()
        .is_some_and(|version| is_consumed(state, version));
    state
        .composer
        .receiving_handoffs
        .push(handoff_id.to_string());
    let mut effects = Vec::new();
    if !state
        .composer
        .received_handoffs
        .iter()
        .any(|id| id == handoff_id)
        && !consumed
    {
        let stored = state
            .composer
            .stored_draft
            .as_ref()
            .map(|record| StoredDraft {
                text: record.text.clone(),
                version: record.version.clone(),
                parked: record.parked,
            });
        let parked = state
            .composer
            .stored_draft
            .as_ref()
            .is_some_and(|record| record.parked);
        let disposition =
            classify_draft_handoff(content, version.as_ref(), current, stored.as_ref(), parked);
        match disposition {
            HandoffDisposition::Conflict => {
                // The bar now offers this transfer, which the synced-draft rule must not withdraw.
                state.composer.draft_sync.offered_at = None;
                state.composer.incoming_draft = Some(crate::document::IncomingDraft {
                    content: content.to_string(),
                    version: match serde_json::to_value(&version) {
                        Ok(Value::Null) | Err(_) => ghostex_gx_protocol::Tri::Absent,
                        Ok(value) => ghostex_gx_protocol::Tri::Value(value),
                    },
                    extra: Default::default(),
                });
            }
            HandoffDisposition::Accept => {
                let record = StoredDraftRecord {
                    text: content.to_string(),
                    updated_at: Some(context.now_millis() as f64),
                    version: version.clone(),
                    submitted: false,
                    parked: false,
                };
                state.composer.stored_draft = Some(record);
                effects.push(Effect::HostAction {
                    action: "draftReceived".to_string(),
                    params: Box::new(json!({
                        "content": content,
                        "version": version,
                        "previous": current,
                    })),
                });
            }
            HandoffDisposition::Current => {}
        }
        state
            .composer
            .received_handoffs
            .push(handoff_id.to_string());
    }
    // One host round trip whatever the disposition: `composer('receive')` always writes the
    // recovery checkpoint and flushes the save outbox before it answers.
    effects.insert(
        0,
        Effect::WriteStorage {
            key: operation_key(state, DRAFT_RECEIVE_STORE),
            value: Some(
                json!({ "text": content, "version": version, "current": current }).to_string(),
            ),
            durable: true,
        },
    );
    let request_id = state.core.allocate_request_id();
    state.composer.handoff_acknowledgement = Some((request_id, handoff_id.to_string()));
    effects.push(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AcknowledgeSessionChatDraftHandoff,
        params: Box::new(json!({ "handoffId": handoff_id })),
    });
    effects
}

/// The acknowledgement answered: the transfer is over on both sides.
pub fn settle_handoff_acknowledgement(
    state: &mut ChatState,
    request_id: u64,
) -> Option<Vec<Effect>> {
    let (pending, handoff_id) = state.composer.handoff_acknowledgement.clone()?;
    if pending != request_id {
        return None;
    }
    state.composer.handoff_acknowledgement = None;
    state
        .composer
        .receiving_handoffs
        .retain(|id| id != &handoff_id);
    Some(vec![Effect::HostAction {
        action: "draftHandoffToChatComplete".to_string(),
        params: Box::new(json!({ "handoffId": handoff_id })),
    }])
}

/// The timer that ends a first Escape's wait for its confirmation.
pub const INTERRUPT_CONFIRM_TIMER: &str = "composer.interruptConfirm";

/// The timer that takes the red "Agent was interrupted" toast down.
pub const INTERRUPTED_TOAST_TIMER: &str = "composer.interruptedToast";

/// Escape: cancel a send that has not left, then ask the agent to stop.
///
/// `confirm` is the renderer's "Press Escape twice to interrupt" setting.
///
/// CDXC:SessionChat 2026-09-30 DECISION:
/// User: "hitting Escape once, if it's going to interrupt, [should] show a toast at the bottom of the chat view ... saying, 'Press Escape again to interrupt.'" A second Escape within 2 seconds interrupts; a Settings toggle (`sessionChatConfirmEscapeInterrupt`, on by default) turns it off so the first Escape interrupts. Only an Escape that would stop a working agent waits: closing a terminal dialog, or an idle agent, stays one press.
pub fn interrupt(state: &mut ChatState, context: &ChatContext, confirm: bool) -> Vec<Effect> {
    if confirm
        && !state.composer.interrupt_confirm_armed
        && cancellable_dialog(state).is_none()
        && crate::session::working::is_working(state)
    {
        state.composer.interrupt_confirm_armed = true;
        state.core.timers.arm(
            INTERRUPT_CONFIRM_TIMER,
            context.now_ms,
            crate::composer::policy::INTERRUPT_CONFIRM_WINDOW_MS as f64,
        );
        state.core.request_publish();
        return Vec::new();
    }
    if std::mem::take(&mut state.composer.interrupt_confirm_armed) {
        state.core.timers.cancel(INTERRUPT_CONFIRM_TIMER);
        state.core.request_publish();
    }
    let mut parked = false;
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.cancelled = true;
        parked = submission.awaiting_gate;
    }
    for waiting in &mut state.composer.waiting {
        waiting.cancelled = true;
    }
    // A send parked behind the gate has no answer coming to notice the cancel, so it ends here.
    let mut effects = if parked {
        run_head(state, context)
    } else {
        Vec::new()
    };
    // CDXC:SessionChat 2026-09-08 DECISION:
    // User: Escape closing /usage or a similar dialog must not report "Interrupted the agent" when
    // no turn was interrupted. The dialog's cancel lane verifies the live screen and avoids the
    // stop lane's queue cancellation and activity reset.
    if let Some(dialog) = cancellable_dialog(state) {
        // Closing is optimistic, like the card's Close button (questions/actions.rs `closes_dialog`).
        state.questions.answered_notice_key = state.questions.active_notice_key.clone();
        state.core.request_publish();
        let request_id = state.core.allocate_request_id();
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::AnswerSessionChatPrompt,
            params: Box::new(json!({
                "kind": "terminalDialog",
                "dialogId": dialog,
                "dialogAction": "cancel",
            })),
        });
        return effects;
    }
    // CDXC:SessionChat 2026-10-01 DECISION:
    // User: "when the user interrupts show a red one that says 'Agent was interrupted' same spot and look as the 'Press escape again..' one". Raised by every interrupt that stopped a working turn (Escape or the Stop button, desktop and phone), for 2 seconds.
    if sends::begin_interrupt(state, context) {
        raise_interrupted_toast(state, context);
    }
    let request_id = state.core.allocate_request_id();
    effects.push(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::InterruptSessionChat,
        params: Box::new(json!({})),
    });
    effects
}

/// The open terminal dialog's id, when it offers a cancel action.
fn cancellable_dialog(state: &ChatState) -> Option<String> {
    let dialog = state.session.terminal_notice.as_ref()?.get("dialog")?;
    let offers_cancel = dialog
        .get("actions")
        .and_then(Value::as_array)
        .is_some_and(|actions| {
            actions
                .iter()
                .any(|action| action.as_str() == Some("cancel"))
        });
    offers_cancel
        .then(|| dialog.get("id").and_then(Value::as_str))
        .flatten()
        .map(str::to_string)
}

/// `sendKey`: a raw keystroke, with its marker recorded only after the write is accepted.
pub fn send_key(state: &mut ChatState, key: &str, marker: &str) -> (Option<u64>, Vec<Effect>) {
    // `transport.sendKey` is optional and `chat.sendKey` is only offered when the host has it, so
    // a host without the endpoint drops the keystroke rather than calling something that 404s.
    if !state.menus.can_send_key {
        return (None, Vec::new());
    }
    let request_id = state.core.allocate_request_id();
    state.composer.key_send = Some((request_id, key.to_string(), marker.to_string()));
    (
        Some(request_id),
        vec![Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::SendSessionChatMessage,
            params: Box::new(json!({ "key": key })),
        }],
    )
}

/// The keystroke was accepted: a non-empty marker becomes a chat row.
///
/// Multi-key setting adjustments pass an empty marker so their implementation keystrokes do not.
pub fn settle_key_send(
    state: &mut ChatState,
    context: &ChatContext,
    request_id: u64,
    outcome: &crate::wire::RpcOutcome,
) -> Option<Vec<Effect>> {
    let (pending, key, marker) = state.composer.key_send.clone()?;
    if pending != request_id {
        return None;
    }
    state.composer.key_send = None;
    match outcome {
        crate::wire::RpcOutcome::Ok { .. } => {
            sends::record_key_marker(state, context, &key, &marker);
        }
        crate::wire::RpcOutcome::Err { message, code, .. } => {
            state.core.fail(message.clone(), code.clone());
        }
    }
    Some(Vec::new())
}

/// The composer's text goes back when a submission did not leave.
fn submission_failed(mode: SubmissionMode, text: &str) -> Effect {
    Effect::HostAction {
        action: "submissionFailed".to_string(),
        params: Box::new(json!({ "method": mode_name(mode), "text": text })),
    }
}

fn mode_name(mode: SubmissionMode) -> &'static str {
    match mode {
        SubmissionMode::Send => "send",
        SubmissionMode::Queue => "queue",
        SubmissionMode::Compact => "compact",
    }
}

fn wait_storage(state: &mut ChatState, key: StorageKey) {
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.storage = Some(key);
        submission.request = None;
    }
}

fn wait_request(state: &mut ChatState, request_id: u64) {
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.request = Some(request_id);
        submission.storage = None;
    }
}

/// `SESSION_CHAT_DEFAULT_COMMAND_CATALOG`, which is what the controller is constructed with.
fn command_catalog() -> Vec<String> {
    crate::session::constants::DEFAULT_COMMAND_CATALOG
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

fn draft_key(state: &ChatState) -> StorageKey {
    crate::composer::storage::draft_key(&state.identity.session_key)
}

/// The flush's own answer key: a store with no suffix, which is what [`Effect::FlushStorage`]
/// answers on.
fn flush_key() -> StorageKey {
    StorageKey {
        store: DRAFTS_STORE.to_string(),
        suffix: String::new(),
    }
}

/// One of the host's own draft operations, keyed by the session it belongs to.
fn operation_key(state: &ChatState, store: &str) -> StorageKey {
    StorageKey {
        store: store.to_string(),
        suffix: state.identity.session_key.clone(),
    }
}
