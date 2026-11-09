//! A send made while a picture is still on its way into the chat box.
//!
//! CDXC:Clipboard 2026-11-09 DECISION:
//! User, on the "Sends once the image is attached" line: "issue is i dont like it showed for a split second. also it wasn't styled at all ... maybe we need to just do optimistic ui here. we show it as sent instantly but then show a spinner next to it and also allow the user to take the message back if he wants until it's actually sent", then (picking option A on the paste-send page, Q2071/Q2072): "the error u showed there looks ugly tho we have card for this. also for taking back we need to use the same arrows we already have next to messages that are sent but didn't arrive at the agent". Enter (and every other send from the chat box) while a pasted, dropped or picked file is still being saved sends at once: the message leaves the chat box and its bubble is drawn in the transcript, with a picture tile still loading for each file on its way. If the save takes longer than `ATTACHING_STATUS_MS`, the bubble gets the spinner and the Back to composer arrow every client already draws beside a message that has not reached the agent; the arrow (or Up in an empty chat box) puts the text back in the chat box, and the pictures follow it there as they land. The message goes to the agent the moment its last picture is in, with the references where the caret was. A save that fails ends the send the way a refused send ends: the chat box's error card says why and the text (with the pictures that did land) is back in the box. Take back lasts only until the send leaves for the agent; after that the arrow is gone, and one that arrives too late says the agent has it.
//!
//! CDXC:Clipboard 2026-11-09 WHY:
//! The echo of a message waiting for its pictures never matches a transcript row
//! (`PendingSend::awaiting_attachments`, session/pending.rs): its text is not what will be sent
//! yet, and nothing was sent. When its last picture lands its text gains the references (so a
//! picture-only message is matched by its paths, as every picture message is) and its occurrence
//! is counted again, exactly as if Enter had been pressed then. One echo, one `sendRequestId`, from
//! Enter to the agent: the duplicate-bubble rules in session/pending.rs see what they always saw.

use serde_json::{json, Value};

use crate::composer::queue_take_back::{QueueTakeBack, QueueTakeBackStage};
use crate::composer::references::insert_path_references;
use crate::effect::Effect;
use crate::session::constants::PENDING_ID_PREFIX;
use crate::state::{ChatContext, ChatState, Submission};
use crate::wire::ChatRpcMethod;

/// How long a send waits for its pictures before its bubble says so: a usual paste lands well
/// inside it, so the bubble never flashes a spinner.
pub const ATTACHING_STATUS_MS: i64 = 300;

/// The timer that redraws once a waiting send's status is due.
pub const ATTACHING_STATUS_TIMER: &str = "composer.attachingStatus";

/// A picture or file on its way into the chat box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentSlot {
    /// The host's id for it, or one the core made (`anon:N`, `import:N`).
    pub key: String,
    /// The host named it, so only an answer naming it ends it.
    pub named: bool,
    /// The send waiting for it ([`AttachingHold::id`]), if one is.
    pub hold: Option<u64>,
}

/// What a send waiting for its pictures remembers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachingHold {
    pub id: u64,
    /// Where the references go in the send's text, in UTF-16 code units: the caret when Enter was
    /// pressed, moved on past each reference written.
    pub caret: usize,
    /// How many of its pictures are still on their way.
    pub waiting: usize,
    /// When Enter was pressed.
    pub since_ms: i64,
}

/// One message in the transcript still waiting for its pictures, as the document draws it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachingMessage {
    pub id: String,
    /// Loading tiles to draw in the bubble, one per picture still on its way.
    pub pictures: usize,
    /// The spinner and Back to composer beside the bubble, once the wait has lasted
    /// [`ATTACHING_STATUS_MS`].
    pub status: bool,
}

fn host_key(action: &crate::action::UserAction) -> Option<String> {
    match action.param("id")? {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

/// `attachmentsStarted`: a picture or file began its way into the chat box.
pub fn started(state: &mut ChatState, action: &crate::action::UserAction) {
    let (key, named) = match host_key(action) {
        Some(key) => (key, true),
        None => {
            state.composer.attachment_counter += 1;
            (format!("anon:{}", state.composer.attachment_counter), false)
        }
    };
    state.composer.attachment_slots.push(AttachmentSlot {
        key,
        named,
        hold: None,
    });
}

/// The core's own import of picked files (`attachPaths`) began.
pub fn import_started(state: &mut ChatState, request_id: u64) {
    state.composer.attachment_slots.push(AttachmentSlot {
        key: format!("import:{request_id}"),
        named: true,
        hold: None,
    });
}

/// The slot an `attachmentsFinished` ends: the one it names, else the oldest the host named none
/// for.
pub fn take_slot(
    state: &mut ChatState,
    action: &crate::action::UserAction,
) -> Option<AttachmentSlot> {
    let slots = &mut state.composer.attachment_slots;
    let at = match host_key(action) {
        Some(key) => slots.iter().position(|slot| slot.key == key),
        None => slots.iter().position(|slot| !slot.named),
    }?;
    Some(slots.remove(at))
}

/// The slot of the core's own import `request_id`.
pub fn take_import_slot(state: &mut ChatState, request_id: u64) -> Option<AttachmentSlot> {
    let key = format!("import:{request_id}");
    let slots = &mut state.composer.attachment_slots;
    let at = slots.iter().position(|slot| slot.key == key)?;
    Some(slots.remove(at))
}

/// A send from the chat box takes every picture still on its way that no earlier send waits for.
/// `caret` is where their references go in `text`.
pub fn claim(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    caret: Option<usize>,
) -> Option<AttachingHold> {
    let waiting = state
        .composer
        .attachment_slots
        .iter()
        .filter(|slot| slot.hold.is_none())
        .count();
    if waiting == 0 {
        return None;
    }
    state.composer.attachment_counter += 1;
    let id = state.composer.attachment_counter;
    for slot in &mut state.composer.attachment_slots {
        if slot.hold.is_none() {
            slot.hold = Some(id);
        }
    }
    let length = text.encode_utf16().count();
    let now = context.now_millis();
    // An earlier send's timer stays as it is; `settle` arms the next one when it fires.
    state.core.timers.arm_once(
        ATTACHING_STATUS_TIMER,
        context.now_ms,
        ATTACHING_STATUS_MS as f64,
    );
    Some(AttachingHold {
        id,
        caret: caret.unwrap_or(length).min(length),
        waiting,
        since_ms: now,
    })
}

/// The pictures a send no longer waits for go to the chat box when they land.
pub fn release(state: &mut ChatState, hold: Option<&AttachingHold>) {
    let Some(hold) = hold else {
        return;
    };
    for slot in &mut state.composer.attachment_slots {
        if slot.hold == Some(hold.id) {
            slot.hold = None;
        }
    }
}

fn submission_mut(state: &mut ChatState, hold_id: u64) -> Option<&mut Submission> {
    let holds = |submission: &&mut Submission| {
        submission
            .attaching
            .as_ref()
            .is_some_and(|hold| hold.id == hold_id)
    };
    if let Some(submission) = state.composer.submitting.as_mut().filter(holds) {
        return Some(submission);
    }
    state.composer.waiting.iter_mut().find(holds)
}

/// A picture `slot` waited for landed (`paths`) or failed (`error`). `None` when no send waits for
/// it, so it goes to the chat box as before.
pub fn landed(
    state: &mut ChatState,
    context: &ChatContext,
    slot: &AttachmentSlot,
    paths: &[String],
    error: Option<&str>,
) -> Option<Vec<Effect>> {
    let hold_id = slot.hold?;
    state.core.request_publish();
    if let Some(error) = error {
        return Some(crate::composer::send::fail_attaching(
            state, context, hold_id, error,
        ));
    }
    let submission = submission_mut(state, hold_id)?;
    let hold = submission.attaching.as_mut()?;
    hold.waiting = hold.waiting.saturating_sub(1);
    if !paths.is_empty() {
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let edit = insert_path_references(&submission.text, &paths, hold.caret, hold.caret);
        submission.text = edit.text;
        hold.caret = edit.caret;
        // gxserver keeps each revision's text and refuses a send whose text differs from it, so
        // the text with its references goes as the draft's next revision.
        if let Some(version) = submission.version.as_mut() {
            version.revision += 1;
        }
    }
    let done = hold.waiting == 0;
    let text = submission.text.clone();
    let pending_id = submission.pending_id.clone();
    if done {
        submission.attaching = None;
    }
    if let Some(pending_id) = pending_id.as_deref() {
        crate::session::sends::update_attaching_send(state, pending_id, &text, done);
    }
    Some(if done {
        crate::composer::send::resume_after_attachments(state, context)
    } else {
        Vec::new()
    })
}

/// Back to composer on a message still waiting for its pictures: it never left, so it comes back
/// here and now. `None` when `message_id` is not such a message.
pub fn take_back(
    state: &mut ChatState,
    context: &ChatContext,
    message_id: &str,
) -> Option<Vec<Effect>> {
    let pending_id = message_id.strip_prefix(PENDING_ID_PREFIX)?;
    let waits = |submission: &Submission| {
        submission.attaching.is_some() && submission.pending_id.as_deref() == Some(pending_id)
    };
    let submission = if state.composer.submitting.as_ref().is_some_and(waits) {
        state.composer.submitting.take()
    } else {
        let at = state.composer.waiting.iter().position(waits)?;
        Some(state.composer.waiting.remove(at))
    }?;
    release(state, submission.attaching.as_ref());
    crate::session::sends::drop_send(state, pending_id);
    state.core.request_publish();
    let mut effects = Vec::new();
    // The same two steps as a message taken back out of the agent's queue: read the chat box,
    // then append (queue_take_back.rs).
    let text = submission.typed_text().to_string();
    if !text.trim().is_empty() && state.composer.queue_take_back.is_none() {
        let request_id = state.core.allocate_request_id();
        state.composer.queue_take_back = Some(QueueTakeBack {
            request_id,
            stage: QueueTakeBackStage::Reading,
            message_id: message_id.to_string(),
            text,
        });
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::ReadNativeComposer,
            params: Box::new(json!({})),
        });
    } else if !text.trim().is_empty() {
        effects.push(crate::composer::send::submission_failed(
            submission.mode,
            &text,
        ));
    }
    effects.extend(crate::composer::send::resume_after_attachments(
        state, context,
    ));
    Some(effects)
}

/// The messages in the transcript waiting for their pictures, oldest first.
pub fn attaching_messages(state: &ChatState, context: &ChatContext) -> Vec<AttachingMessage> {
    let now = context.now_millis();
    state
        .composer
        .submitting
        .iter()
        .chain(state.composer.waiting.iter())
        .filter_map(|submission| {
            let hold = submission.attaching.as_ref()?;
            let pending_id = submission.pending_id.as_ref()?;
            Some(AttachingMessage {
                id: format!("{PENDING_ID_PREFIX}{pending_id}"),
                pictures: hold.waiting,
                status: now - hold.since_ms >= ATTACHING_STATUS_MS,
            })
        })
        .collect()
}

/// The ids Back to composer is offered on for messages still waiting for their pictures.
pub fn takeable_ids(state: &ChatState) -> Vec<String> {
    state
        .composer
        .submitting
        .iter()
        .chain(state.composer.waiting.iter())
        .filter(|submission| submission.attaching.is_some())
        .filter_map(|submission| submission.pending_id.as_ref())
        .map(|pending_id| format!("{PENDING_ID_PREFIX}{pending_id}"))
        .collect()
}

/// Settles the status timer: the bubbles redraw with their spinner.
pub fn settle(state: &mut ChatState, context: &ChatContext) {
    if !state.core.timer_fired(ATTACHING_STATUS_TIMER) {
        return;
    }
    state.core.request_publish();
    // A later send that waits too is due after this one.
    let now = context.now_millis();
    let next = state
        .composer
        .submitting
        .iter()
        .chain(state.composer.waiting.iter())
        .filter_map(|submission| submission.attaching.as_ref())
        .map(|hold| hold.since_ms + ATTACHING_STATUS_MS - now)
        .filter(|remaining| *remaining > 0)
        .min();
    if let Some(remaining) = next {
        state
            .core
            .timers
            .arm(ATTACHING_STATUS_TIMER, context.now_ms, remaining as f64);
    }
}
