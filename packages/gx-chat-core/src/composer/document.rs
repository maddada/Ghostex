//! Family d's part of the document: the queue, the draft, the composer chrome, suggestions,
//! references, the note and the host-action menu.
//!
//! The twenty keys below are the ones `publish` in
//! `packages/shared/session-chat-controller/native-host.ts` built from composer state; everything
//! they read comes out of [`ChatState`], never out of the half-built document.

use ghostex_gx_protocol::Tri;
use serde_json::Value;

use crate::composer::host_actions::composer_host_actions;
use crate::composer::layout::{can_collapse_composer, CollapseGate};
use crate::composer::policy::{
    composer_placeholder, send_blocked_reason, send_refused_reason, SendGate,
    DESKTOP_COMPOSER_PLACEHOLDER, STOP_BUTTON_COOLDOWN_MS, TOUCH_COMPOSER_PLACEHOLDER,
};
use crate::composer::queue::{
    is_queue_row_busy, queue_capabilities, queue_row_preview, QUEUE_LONG_PRESS_MS,
};
use crate::composer::suggestions::{composer_native_command, suggestion_popup};
use crate::composer::view::{current_matches, suggestion_sources};
use crate::document::{ComposerActions, Document, Draft, Interaction, Queue, QueuedPrompt};
use crate::questions::gates::{notice_visible, terminal_choice_pending};
use crate::questions::model::TerminalNotice;
use crate::state::{ChatContext, ChatState};

/// Writes family d's keys into `into`.
///
/// `queue.capabilities.supported` stays false until family a has folded a `queue` field, even an
/// empty one: that presence is the daemon capability probe, and every control hides rather than
/// calling an endpoint that would answer 404.
pub fn document(state: &ChatState, _context: &ChatContext, into: &mut Document) {
    let composer = &state.composer;
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());

    into.queue = queue(state);
    into.draft = Draft {
        can_sync: into.queue.capabilities.can_sync_draft,
        client_id: state.identity.client_id.clone(),
        synced: state.session.synced_draft.clone(),
    };
    into.returned_prompt = match &state.session.returned_prompt {
        Some(prompt) => Tri::Value(prompt.clone()),
        None => Tri::Null,
    };

    let sources = suggestion_sources(state);
    let matches = current_matches(state, &sources);
    let popup = suggestion_popup(
        &matches,
        &sources,
        &composer.suggestions.text,
        composer.suggestions.index,
    );
    into.suggestions = match &popup {
        Some(popup) => serde_json::to_value(popup)
            .map(Tri::Value)
            .unwrap_or(Tri::Null),
        None => Tri::Null,
    };
    into.composer_command =
        composer_native_command(state.session.agent.as_deref(), &composer.text).map(str::to_string);

    let collapse_eligible = can_collapse_composer(&CollapseGate {
        maximized: false,
        note_open: composer.note.open,
        suggestions_open: popup.is_some(),
        has_error: state.core.operation_error.is_some(),
        attachment_count: composer.draft_attachment_count,
        pending_attachments: composer.pending_attachments,
        queued_prompts: into.queue.prompts.len(),
    });
    into.composer_collapse_eligible = collapse_eligible;
    // `publish` drops the collapse when the box is no longer eligible, so the document never
    // claims a collapsed composer whose space is already owned by something else.
    into.composer_collapsed = collapse_eligible && composer.collapsed;

    into.composer_overflow = composer.overflow.clone();
    // `stash`, `attach` and `terminal` were `!preview` (`native-host.ts:529`): a preview chat (the
    // retired Chat Lab's) has no stash bridge, no attachment picker and no terminal to switch to,
    // so those controls stay out of the toolbar instead of doing nothing when clicked. They read
    // the host's own capability block first, so a transport narrower than the desktop's can still
    // clear one.
    let live = state.core.preview_settings.is_none();
    into.composer_actions = ComposerActions {
        summary: composer.actions.summary,
        note: composer.actions.note && state.session.agent_session_id.is_some(),
        stash: composer.actions.stash && live,
        attach: composer.actions.attach && live,
        terminal: composer.actions.terminal && live,
    };
    into.composer_chrome = composer
        .chrome
        .projection(&composer.note, state.transcript_view.summary_mode);
    into.note = composer.note.document();

    let card_visible = notice_visible(state);
    let choice_pending = terminal_choice_pending(state, notice.as_ref());
    into.send_blocked_reason = send_refused(state);
    into.composer_placeholder = composer_placeholder(
        true,
        choice_pending,
        controls_only(notice.as_ref()),
        card_visible,
        option_switching(state),
    )
    .unwrap_or(
        if state
            .session
            .boot_config
            .as_ref()
            .is_some_and(|config| config.touch_composer)
        {
            TOUCH_COMPOSER_PLACEHOLDER
        } else {
            DESKTOP_COMPOSER_PLACEHOLDER
        },
    )
    .to_string();

    into.history_active = composer.history.is_active();
    into.pending_attachments = composer.pending_attachments;
    into.incoming_draft = composer.incoming_draft.clone();
    into.interaction = Interaction {
        queue_long_press_ms: QUEUE_LONG_PRESS_MS,
        stop_button_cooldown_ms: STOP_BUTTON_COOLDOWN_MS,
    };
    into.host_actions = composer_host_actions();
    into.skills_loading = composer.sources.skills_loading;
    into.files_loading = composer.sources.files_loading;
}

/// The queue as the renderer draws it: the daemon's rows, minus the ones an optimistic echo
/// already stands for, each with the single line its row shows.
pub fn queue(state: &ChatState) -> Queue {
    let capabilities = queue_capabilities(
        state.session.queue_prompts.is_some(),
        &state.composer.transport,
    );
    let prompts = state
        .session
        .queue_prompts
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|prompt| {
            // A startup send is drawn in the transcript with its own delivery status, not above the
            // composer, and a row an accepted send already echoes would be drawn twice.
            prompt.get("startupSend") != Some(&Value::Bool(true))
                && !state.pending.sends.iter().any(|entry| {
                    entry.queued_prompt_id.is_some()
                        && entry.queued_prompt_id.as_deref()
                            == prompt.get("id").and_then(Value::as_str)
                })
        })
        .map(queued_prompt)
        .collect();
    Queue {
        capabilities,
        prompts,
    }
}

fn queued_prompt(prompt: &Value) -> QueuedPrompt {
    let mut row: QueuedPrompt = serde_json::from_value(prompt.clone()).unwrap_or_default();
    row.preview = queue_row_preview(&row.text);
    row.busy = is_queue_row_busy(&row.state);
    row
}

/// `sendBlockedReason(state)`: why the send button is off, or `None`.
///
/// Public because family b's `rewindEnabled` is exactly `sendBlockedReason(...) === null`, and
/// `document::assemble` runs family b before family d. Family b reads this predicate over
/// `ChatState`, never the half-built document.
pub fn send_blocked(state: &ChatState, _context: &ChatContext) -> Option<String> {
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    send_blocked_reason(&SendGate {
        // The desktop brain never holds input on another device; the web and mobile hosts gate it
        // on their own transport, which answers through the same field.
        can_send: true,
        account_switch_busy: account_busy(state),
        conversation_locked: notice
            .as_ref()
            .is_some_and(|notice| notice.conversation_lock.is_some()),
        terminal_choice_pending: terminal_choice_pending(state, notice.as_ref()),
        notice_card_visible: notice_visible(state),
        session_option_switching: option_switching(state),
    })
    .map(str::to_string)
}

/// Why a send is refused outright, or `None`; a send that is only blocked is held instead.
pub fn send_refused(state: &ChatState) -> Option<String> {
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    send_refused_reason(&SendGate {
        can_send: true,
        conversation_locked: notice
            .as_ref()
            .is_some_and(|notice| notice.conversation_lock.is_some()),
        terminal_choice_pending: terminal_choice_pending(state, notice.as_ref()),
        notice_card_visible: notice_visible(state),
        ..SendGate::default()
    })
    .map(str::to_string)
}

/// A dialog with no rows offers controls only, so the placeholder points at them.
fn controls_only(notice: Option<&TerminalNotice>) -> bool {
    notice
        .and_then(|notice| notice.dialog.as_ref())
        .is_some_and(|dialog| dialog.rows.is_empty())
}

/// Whether an option switch is in flight.
///
/// `optionSwitching` is family e's, set by the switching callback the option dispatch installs.
/// Family c's `questions::gates` asks the same two questions through the same pair.
pub fn option_switching(state: &ChatState) -> bool {
    state.menus.option_switching
}

/// Whether an account switch is in flight, which is family e's `accountStatus.busy`.
///
/// The switch machine's own answer, not "a switch record exists": a finished switch stays on the
/// wire, and gating the composer on its presence would leave the send button off for good.
pub fn account_busy(state: &ChatState) -> bool {
    let progress = crate::menus::controls::switch_progress(state);
    state
        .menus
        .account_switch
        .status(
            progress.as_ref(),
            crate::menus::controls::switch_ready(state),
            state.menus.account_switch.now_ms.unwrap_or_default(),
        )
        .busy
}
