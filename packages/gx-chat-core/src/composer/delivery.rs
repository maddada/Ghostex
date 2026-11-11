//! What becomes of a message between Enter and the agent: it is taken once, it goes again by
//! itself when time can fix what stopped it, and it stays in the chat saying why when time cannot.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "sending messages is the most important part of this app and it's broken now", after a
//! night of messages that arrived late, a notification that one "was not sent, so it is back in
//! the chat box", and the same message delivered twice. Of that notification: "you should self
//! heal, don't show me this message, why do i need to press enter again when you can press it for
//! me". His standing rule: a failure time can fix is never handed back to the user. The rules
//! every send, queue and Retry from the chat box follows:
//!
//! - **Once.** The chat box names a message at Enter with the `sendRequestId` it goes under
//!   (`take`). The same action arriving again is the same message, so the box may send it again
//!   while it cannot see it taken, and every attempt of a message carries that one id, which
//!   gxserver delivers at most once (server/src/session_chat_send_requests.rs).
//! - **Time can fix it** (Ghostex's service not answering, starting, or still sending the first
//!   attempt; an account still coming back): the send stays the message it is and goes again on
//!   the reads' ladder, with no limit while the chat is open (`retry_later`). Its row says it is
//!   waiting and why, and it is never given back to the chat box.
//! - **Time cannot fix it** (the session is gone, the text was refused): the message stays in the
//!   chat as a row that says so, with Retry, Edit and Remove (`keep_unsent`). No error line, no
//!   notification, and its words move back to the chat box only when Edit asks.
//! - A message whose call has been out for a moment says it is still sending (`call_left`).
//!
//! What still gives its text back to the chat box: Escape (the user's own cancel), a compact, a
//! handoff to the terminal, a picture that could not be saved, and a message held for another
//! computer that was returned. Supersedes the 2026-10-07 rule on `start_or_wait` that one failed
//! send takes the sends behind it back to the composer: they are their own messages and go on.
//! SEE-ALSO: `tests/send_delivery.rs` (each rule, driven as a host drives the core),
//! apps/desktop/src/app/native_chat/send_watch.rs (the box's half), startup_delivery.rs and
//! apps/mobile/app/src/chat/native/transcript/SystemRows.tsx (the row's note).

use ghostex_gx_protocol::StartupDelivery;

use crate::composer::send::SendPhase;
use crate::composer::submission::SubmissionMode;
use crate::effect::Effect;
use crate::session::constants::unreachable_retry_delay_ms;
use crate::state::{
    ChatContext, ChatState, PendingSend, SendRetry, Submission, UnsentSend, TAKEN_SEND_IDS,
};

/// Fires when a send that met a failure time can fix goes again.
pub const SEND_RETRY_TIMER: &str = "composer.sendRetry";
/// Fires when a delivery call has been out long enough for its row to say so.
pub const SEND_SLOW_TIMER: &str = "composer.sendSlow";
/// How long a delivery call is out before its row says it is still sending. gxserver answers a
/// send once it has typed it, 0.7 to 1.6 seconds.
pub const SEND_SLOW_AFTER_MS: f64 = 2_500.0;
/// The `promptId` a row that was not sent carries in its `startupDelivery`, before its echo's id:
/// Retry (`retryQueue`), Remove (`removeQueue`) and Edit (`editUnsent`) name the row by it.
pub const UNSENT_PROMPT_PREFIX: &str = "unsent:";
/// The `promptId` of a row whose send waits to go again or is still out.
pub const SENDING_PROMPT_PREFIX: &str = "sending:";

/// Takes in a send the chat box named, and says whether it is new: `false` for one this core has
/// already taken under that id, which is the same message arriving again.
pub fn take(state: &mut ChatState, named: Option<&str>) -> bool {
    let Some(named) = named.filter(|named| !named.is_empty()) else {
        return true;
    };
    let taken = &mut state.composer.taken_send_ids;
    if taken.iter().any(|id| id == named) {
        return false;
    }
    taken.push(named.to_string());
    if taken.len() > TAKEN_SEND_IDS {
        let excess = taken.len() - TAKEN_SEND_IDS;
        taken.drain(..excess);
    }
    true
}

/// What a send says while it waits out a failure time can fix, or `None` for one time cannot.
///
/// `unreachable` (and no code at all, the desktop's transport failures) is Ghostex's service not
/// answering; `serverStarting` is it starting; `sendInProgress` is the first attempt still being
/// typed, which the same id asks about again; `accountRecoveryNotReady` is the session's account
/// still coming back. Every other refusal is gxserver's own answer about this message.
fn waits_for(code: Option<&str>, message: &str) -> Option<String> {
    match code {
        None | Some("unreachable") => Some(
            "Couldn't reach Ghostex. It may be restarting. It sends by itself when it answers."
                .to_string(),
        ),
        Some("serverStarting") => {
            Some("Ghostex is starting. It sends by itself when it is up.".to_string())
        }
        Some("sendInProgress") => {
            Some("Ghostex is still sending this message. Checking again.".to_string())
        }
        Some("accountRecoveryNotReady") => {
            let reason = message.trim().trim_end_matches('.');
            Some(if reason.is_empty() {
                "The account is still coming back. It sends by itself then.".to_string()
            } else {
                format!("{reason}. It sends by itself then.")
            })
        }
        Some(_) => None,
    }
}

fn delivering(submission: &Submission) -> bool {
    !submission.handoff
        && matches!(
            submission.phases.first(),
            Some(
                SendPhase::PushDraft
                    | SendPhase::SendCompact
                    | SendPhase::SendText
                    | SendPhase::QueueText
            )
        )
}

/// The head submission's call failed in a way time can fix: it keeps its place and goes again when
/// its timer fires. Answers whether it did.
pub fn retry_later(
    state: &mut ChatState,
    context: &ChatContext,
    message: &str,
    code: Option<&str>,
) -> bool {
    let Some(words) = waits_for(code, message) else {
        return false;
    };
    let Some(submission) = state.composer.submitting.as_mut() else {
        return false;
    };
    // An Escape that arrived while the call was out ends it instead.
    if submission.cancelled || !delivering(submission) {
        return false;
    }
    let attempts = submission.retry.as_ref().map_or(0, |retry| retry.attempts) + 1;
    submission.request = None;
    submission.slow = false;
    submission.retry = Some(SendRetry { attempts, words });
    state.core.timers.arm(
        SEND_RETRY_TIMER,
        context.now_ms,
        unreachable_retry_delay_ms(attempts - 1) as f64,
    );
    state.core.timers.cancel(SEND_SLOW_TIMER);
    // The row says it; the error line and its Retry are for what the row cannot say.
    state.core.refusal_claimed = true;
    state.core.clear_error();
    state.core.request_publish();
    true
}

/// Whether the head submission's wait is over: its timer fired with nothing in flight.
pub fn retry_due(state: &ChatState) -> bool {
    state.core.timer_fired(SEND_RETRY_TIMER)
        && state
            .composer
            .submitting
            .as_ref()
            .is_some_and(|submission| {
                submission.retry.is_some()
                    && submission.request.is_none()
                    && !submission.awaiting_gate
            })
}

/// The head submission's delivery call just left.
pub fn call_left(state: &mut ChatState, context: &ChatContext) {
    state
        .core
        .timers
        .arm(SEND_SLOW_TIMER, context.now_ms, SEND_SLOW_AFTER_MS);
}

/// The clock half: a call out for [`SEND_SLOW_AFTER_MS`] says so on its row.
pub fn settle(state: &mut ChatState, _context: &ChatContext) {
    if !state.core.timer_fired(SEND_SLOW_TIMER) {
        return;
    }
    let Some(submission) = state.composer.submitting.as_mut() else {
        return;
    };
    if submission.request.is_some() && delivering(submission) && !submission.slow {
        submission.slow = true;
        state.core.request_publish();
    }
}

/// The head submission's call was refused for good: the message stays in the chat as a row that
/// says why, and the submission is over. Answers whether it did; `false` leaves the refusal to the
/// caller (a compact, a handoff and a Side chat have no row of their own to keep).
pub fn keep_unsent(
    state: &mut ChatState,
    context: &ChatContext,
    message: &str,
    code: Option<&str>,
) -> bool {
    let keeps = state
        .composer
        .submitting
        .as_ref()
        .is_some_and(|submission| {
            delivering(submission)
                && !submission.cancelled
                && matches!(
                    submission.mode,
                    SubmissionMode::Send | SubmissionMode::Queue
                )
                && submission.phases.first() != Some(&SendPhase::SendCompact)
                && !submission.typed_text().trim().is_empty()
                && crate::composer::side_chat::side_chat_question(&submission.text).is_none()
        });
    if !keeps {
        return false;
    }
    let Some(submission) = state.composer.submitting.take() else {
        return false;
    };
    crate::composer::attaching::release(state, submission.attaching.as_ref());
    // The echo drawn at Enter becomes the row, under its own id; a queue had none.
    let drawn = submission.pending_id.as_deref().and_then(|pending_id| {
        let at = state
            .pending
            .sends
            .iter()
            .position(|entry| entry.id == pending_id)?;
        Some(state.pending.sends.remove(at))
    });
    let mut echo = match drawn {
        Some(echo) => echo,
        // Drawn as a send made now would be, so it sits after the chat it was typed under.
        None => {
            let id = crate::session::sends::begin_send(
                state,
                context,
                submission.typed_text(),
                &submission.image_paths,
            );
            let at = state
                .pending
                .sends
                .iter()
                .position(|entry| entry.id == id)
                .unwrap_or(0);
            state.pending.sends.remove(at)
        }
    };
    let reason = message.trim();
    echo.queued_prompt_id = None;
    echo.sent_while_working = false;
    echo.awaiting_attachments = false;
    echo.startup_delivery = Some(StartupDelivery {
        prompt_id: format!("{UNSENT_PROMPT_PREFIX}{}", echo.id),
        state: "failed".to_string(),
        error_message: Some(if reason.is_empty() {
            "Not sent.".to_string()
        } else {
            format!("Not sent. {reason}")
        }),
        wait_reason: None,
    });
    state.composer.unsent.push(UnsentSend {
        echo,
        typed: submission.typed_text().to_string(),
        text: submission.text.clone(),
        mode: submission.mode,
        image_paths: submission.image_paths.clone(),
        // gxserver could not say what became of that attempt, so the id can never be sent again.
        send_request_id: submission
            .send_request_id
            .clone()
            .filter(|_| code != Some("sendOutcomeUnknown")),
        interrupt: submission.interrupt,
    });
    state.core.timers.cancel(SEND_SLOW_TIMER);
    state.core.refusal_claimed = true;
    state.core.clear_error();
    state.messages.new_composition_identity();
    state.core.request_publish();
    true
}

/// Whether `prompt_id` names a row that was not sent (`UNSENT_PROMPT_PREFIX`).
pub fn is_unsent(prompt_id: &str) -> bool {
    prompt_id.starts_with(UNSENT_PROMPT_PREFIX)
}

fn take_unsent(state: &mut ChatState, prompt_id: &str) -> Option<UnsentSend> {
    let id = prompt_id.strip_prefix(UNSENT_PROMPT_PREFIX)?;
    let at = state
        .composer
        .unsent
        .iter()
        .position(|unsent| unsent.echo.id == id)?;
    let unsent = state.composer.unsent.remove(at);
    state.messages.new_composition_identity();
    state.core.request_publish();
    Some(unsent)
}

/// Retry on a row that was not sent: the same message goes again, under its first id when
/// gxserver can still take that once, as the row it is.
pub fn retry_unsent(state: &mut ChatState, context: &ChatContext, prompt_id: &str) -> Vec<Effect> {
    let Some(unsent) = take_unsent(state, prompt_id) else {
        return Vec::new();
    };
    let send_request_id = unsent
        .send_request_id
        .clone()
        .or_else(|| (context.random_ids[0] != 0).then(|| context.random_id(0)));
    let (phase, pending_id) = match unsent.mode {
        SubmissionMode::Queue => (SendPhase::QueueText, None),
        _ => {
            // Its echo is back as a send on its way, where it stood.
            let mut echo = unsent.echo.clone();
            echo.startup_delivery = None;
            let pending_id = echo.id.clone();
            state.pending.sends.push(echo);
            (SendPhase::SendText, Some(pending_id))
        }
    };
    let submission = Submission {
        text: unsent.text.clone(),
        // The draft this message left the chat box as is long gone; it goes as the text it is.
        version: None,
        mode: unsent.mode,
        cancelled: false,
        image_paths: unsent.image_paths.clone(),
        phases: vec![phase],
        request: None,
        storage: None,
        pending_id,
        marker: None,
        refresh_after_send: false,
        handoff: false,
        awaiting_gate: false,
        drawn: true,
        send_request_id,
        interrupt: unsent.interrupt,
        draft_unsaved: false,
        typed: (unsent.typed != unsent.text).then(|| unsent.typed.clone()),
        attaching: None,
        retry: None,
        slow: false,
        resend: true,
    };
    let effects = crate::composer::send::start_or_wait(state, context, submission);
    state.core.publish_after(&effects);
    effects
}

/// Edit on a row that was not sent: the row goes and its words are the chat box's again.
pub fn edit_unsent(state: &mut ChatState, prompt_id: &str) -> Vec<Effect> {
    match take_unsent(state, prompt_id) {
        Some(unsent) => vec![crate::composer::send::submission_failed(
            unsent.mode,
            &unsent.typed,
        )],
        None => Vec::new(),
    }
}

/// Remove on a row that was not sent.
pub fn remove_unsent(state: &mut ChatState, prompt_id: &str) -> Vec<Effect> {
    take_unsent(state, prompt_id);
    Vec::new()
}

/// The rows of the messages that were not sent, oldest first, for the composition.
pub fn unsent_rows(state: &ChatState) -> Vec<PendingSend> {
    state
        .composer
        .unsent
        .iter()
        .map(|unsent| unsent.echo.clone())
        .collect()
}

/// What the echo of the head submission says under its bubble, as the `startupDelivery` its drawn
/// row carries: held by the gate (`send::held_echo`), waiting to go again, or still out. `None`
/// for a send on its ordinary way.
pub fn echo_note(state: &ChatState) -> Option<(String, StartupDelivery)> {
    if let Some((pending_id, words)) = crate::composer::send::held_echo(state) {
        let delivery = StartupDelivery {
            prompt_id: format!(
                "{}{pending_id}",
                crate::session::composition::HELD_BY_GATE_PREFIX
            ),
            state: "waiting".to_string(),
            error_message: None,
            wait_reason: Some(words),
        };
        return Some((pending_id, delivery));
    }
    let submission = state.composer.submitting.as_ref()?;
    let pending_id = submission.pending_id.clone()?;
    let (state_name, wait_reason) = match (&submission.retry, submission.slow) {
        (Some(retry), _) => ("waiting", Some(retry.words.clone())),
        (None, true) => ("sending", None),
        (None, false) => return None,
    };
    let delivery = StartupDelivery {
        prompt_id: format!("{SENDING_PROMPT_PREFIX}{pending_id}"),
        state: state_name.to_string(),
        error_message: None,
        wait_reason,
    };
    Some((pending_id, delivery))
}

/// What the document's `sendHeld` says: why the head message has not gone yet, for the host's
/// send log. The gate's words, or a retry's.
pub fn held_words(state: &ChatState) -> Option<String> {
    crate::composer::send::held_echo(state)
        .map(|(_, words)| words)
        .or_else(|| {
            state
                .composer
                .submitting
                .as_ref()
                .and_then(|submission| submission.retry.as_ref())
                .map(|retry| retry.words.clone())
        })
}
