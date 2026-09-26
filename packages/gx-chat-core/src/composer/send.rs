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
pub fn begin(
    state: &mut ChatState,
    context: &ChatContext,
    mode: SubmissionMode,
    text: &str,
    version: Option<crate::composer::queue::DraftVersion>,
    image_paths: Vec<String>,
) -> Vec<Effect> {
    // A block that clears on its own does not refuse: the delivery phase waits for it instead.
    if let Some(refused) = crate::composer::document::send_refused(state) {
        state.core.fail(refused, None);
        return vec![submission_failed(mode, text)];
    }
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
    state.composer.submitting = Some(Submission {
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
    });
    run_head(state, context)
}

/// Runs the head phase, or finishes the submission when the list has run out.
fn run_head(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(submission) = state.composer.submitting.as_ref() else {
        return Vec::new();
    };
    let Some(phase) = submission.phases.first().copied() else {
        return finish(state);
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
    let queued_text = match submission.mode {
        SubmissionMode::Queue => text.trim().to_string(),
        _ => text.clone(),
    };
    match phase {
        SendPhase::WriteDraft => {
            let key = draft_key(state);
            let record = StoredDraftRecord {
                text: text.clone(),
                updated_at: Some(context.now_millis() as f64),
                version: version.clone(),
                submitted: true,
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
            let (body, version, images) = match phase {
                SendPhase::SendCompact => ("/compact".to_string(), None, Vec::new()),
                _ => (text, version, submission.image_paths.clone()),
            };
            if !submission.awaiting_gate {
                let drawn = draw_agent_send(state, context, &body, &images);
                adopt_send(state, drawn);
            }
            if hold_for_gate(state, context) {
                return Vec::new();
            }
            let request_id = state.core.allocate_request_id();
            wait_request(state, request_id);
            vec![agent_send_rpc(request_id, &body, version, &images)]
        }
        SendPhase::QueueText => {
            if hold_for_gate(state, context) {
                return Vec::new();
            }
            let request_id = state.core.allocate_request_id();
            wait_request(state, request_id);
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::QueueSessionChatPrompt,
                params: Box::new(json!({ "text": queued_text, "draftVersion": version })),
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
    let effect = agent_send_rpc(sent.request_id, text, version, image_paths);
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
) -> Effect {
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(json!({
            "text": text,
            "imagePaths": if image_paths.is_empty() { Value::Null } else { json!(image_paths) },
            "draftVersion": version,
        })),
    }
}

/// The submission's echo or marker, so a refusal can take it back.
fn adopt_send(state: &mut ChatState, sent: AgentSend) {
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.pending_id = sent.pending_id;
        submission.marker = sent.marker;
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
                submission.pending_id = None;
                submission.marker = None;
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
        crate::wire::RpcOutcome::Err { message, .. } => {
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
    if submission.handoff {
        effects.push(Effect::HostAction {
            action: "draftHandoffToTerminalFailed".to_string(),
            // `params: { error: operationError }`, which is the message the bar now carries.
            params: Box::new(json!({ "error": message })),
        });
    }
    effects.push(submission_failed(submission.mode, &submission.text));
    effects
}

/// The echo and the marker go with a call that never reached the agent.
fn undo_optimistic(state: &mut ChatState) {
    let (pending_id, marker) = match state.composer.submitting.as_ref() {
        Some(submission) => (submission.pending_id.clone(), submission.marker.clone()),
        None => return,
    };
    if let Some(pending_id) = pending_id {
        sends::drop_send(state, &pending_id);
    }
    if let Some((command, sent_at)) = marker {
        sends::drop_command_marker(state, &command, sent_at);
    }
}

/// `handoff`: the draft is parked here and handed to the terminal.
pub fn handoff(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    version: Option<crate::composer::queue::DraftVersion>,
) -> Vec<Effect> {
    let record = StoredDraftRecord {
        text: text.to_string(),
        updated_at: Some(context.now_millis() as f64),
        version: version.clone(),
        submitted: false,
        parked: false,
    };
    state.composer.stored_draft = Some(record.clone());
    crate::composer::draft_sync::cancel_pending_push(state);
    state.composer.submitting = Some(Submission {
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
    });
    let key = draft_key(state);
    wait_storage(state, key.clone());
    vec![Effect::WriteStorage {
        key,
        value: Some(encode_stored_draft(&record)),
        durable: false,
    }]
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
    let consumed = version.as_ref().is_some_and(|version| {
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
    });
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

/// Escape: cancel a send that has not left, then ask the agent to stop.
pub fn interrupt(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let mut parked = false;
    if let Some(submission) = state.composer.submitting.as_mut() {
        submission.cancelled = true;
        parked = submission.awaiting_gate;
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
    sends::begin_interrupt(state, context);
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
