//! Family d's user actions: everything the composer does, from a keystroke to a send.
//!
//! The arms follow the `action` switch in
//! `packages/shared/session-chat-controller/native-host.ts`. What the TypeScript did with an
//! `await` the core does with an [`Effect`] and a later event, so nothing here performs I/O.

use serde_json::{json, Value};

use crate::action::{ActionKind, UserAction};
use crate::composer::layout::{
    ComposerScrollInput, COMPOSER_BOTTOM_THRESHOLD_PX, COMPOSER_SCROLL_RESET_MS,
    COMPOSER_SCROLL_THRESHOLD_PX,
};
use crate::composer::links::{classify_link_href, file_position_from_href, LinkTarget};
use crate::composer::queue::{is_queue_row_busy, move_queue_row};
use crate::composer::reference_pills::{composer_references, ReferenceKind};
use crate::composer::references::{insert_reference, native_path_reference, remove_reference};
use crate::composer::submission::{restore_undelivered_text, SubmissionMode};
use crate::composer::suggestions::{
    complete_composer_mention, composer_native_command, suggestion_popup, suggestion_replacement,
    SuggestionKind,
};
use crate::composer::transcript_menu::append_draft_text;
use crate::composer::view::{current_matches, suggestion_sources};
use crate::effect::Effect;
use crate::jsnum::js_number_of;
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// Handles one action family d owns.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    match action.kind {
        ActionKind::ComposerScroll | ActionKind::ComposerExpand => scroll(state, action, context),
        ActionKind::CompleteComposerCommand => complete_command(state),
        ActionKind::ComposerSelection => {
            selection_changed(state, text_param(action), caret_param(action));
            Vec::new()
        }
        ActionKind::SuggestionKey
        | ActionKind::SuggestionPick
        | ActionKind::SuggestionHighlight
        | ActionKind::SuggestionRetry
        | ActionKind::SuggestionDismiss => suggestion_command(state, action),
        ActionKind::MeasureComposer => {
            // The measurement arrives as `Event::Measured`, which is where the overflow is folded
            // in; a renderer that sends it as an action carries the same payload.
            if let Some(measurements) = action.param("measurements") {
                if let Ok(measurements) = serde_json::from_value(measurements.clone()) {
                    let fit = crate::composer::layout::fit_composer_controls(&measurements);
                    state.composer.overflow = crate::document::ComposerOverflow {
                        overflowed: fit.overflowed,
                        options_overflowed: fit.options_overflowed,
                    };
                }
            }
            Vec::new()
        }
        ActionKind::AppendToDraft => {
            let content = append_draft_text(string_param(action, "draft"), text_param(action));
            let caret = content.encode_utf16().count();
            vec![Effect::SetComposerText {
                content,
                caret: Some(caret),
                from_history: false,
            }]
        }
        ActionKind::EditDraft => edit_draft(state, action, context),
        ActionKind::SaveDraft => save_draft(state, action),
        ActionKind::RecallHistory => recall_history(state, action),
        ActionKind::OpenComposerReference => open_reference(action),
        ActionKind::RefreshComposerChrome => refresh_chrome(state, action),
        ActionKind::Stash => stash(state, action),
        ActionKind::RestoreReturned => restore_returned(state, action),
        ActionKind::ApplyReturned => apply_returned(action),
        ActionKind::RestoreSubmission => vec![Effect::SetComposerText {
            content: restore_undelivered_text(text_param(action), string_param(action, "current")),
            caret: None,
            from_history: false,
        }],
        ActionKind::AttachmentsStarted => {
            state.composer.pending_attachments += 1;
            Vec::new()
        }
        ActionKind::AttachPaths => attach_paths(state, action),
        ActionKind::AttachmentsFinished => attachments_finished(state, action),
        ActionKind::InsertAttachments => insert_attachments(action),
        ActionKind::RemoveAttachment => {
            let edit = remove_reference(
                text_param(action),
                usize_param(action, "start"),
                usize_param(action, "end"),
            );
            vec![Effect::SetComposerText {
                content: edit.text,
                caret: Some(edit.caret),
                from_history: false,
            }]
        }
        ActionKind::DismissIncomingDraft => {
            state.composer.incoming_draft = None;
            crate::composer::draft_sync::answered(state);
            Vec::new()
        }
        ActionKind::UseIncomingDraft => {
            crate::composer::draft_sync::answered(state);
            let effects = state
                .composer
                .incoming_draft
                .as_ref()
                .map(|draft| {
                    vec![Effect::SetComposerText {
                        content: draft.content.clone(),
                        caret: None,
                        from_history: false,
                    }]
                })
                .unwrap_or_default();
            state.composer.incoming_draft = None;
            effects
        }
        ActionKind::RetryQueue => queue_rpc(
            state,
            ChatRpcMethod::UpdateSessionChatQueuedPrompt,
            json!({ "promptId": string_param(action, "promptId"), "retry": true }),
            state.composer.transport.update_queued_prompt,
            None,
        ),
        ActionKind::RemoveQueue => remove_queue(state, action),
        ActionKind::SendQueue => queue_rpc(
            state,
            ChatRpcMethod::SendSessionChatQueuedPrompt,
            json!({ "promptId": string_param(action, "promptId") }),
            state.composer.transport.send_queued_prompt,
            None,
        ),
        ActionKind::ReorderQueue => reorder_queue(
            state,
            action
                .param("promptIds")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        ),
        ActionKind::MoveQueue => move_queue(state, action),
        ActionKind::ToggleNote => toggle_note(state),
        ActionKind::EditNote => {
            state.composer.note.edited = true;
            state.composer.note.value = text_param(action).to_string();
            Vec::new()
        }
        // `clearNote` falls through into `saveNote` in the TypeScript switch, so clearing also
        // flushes the empty body.
        ActionKind::ClearNote => {
            state.composer.note.edited = true;
            state.composer.note.value = String::new();
            save_note(state)
        }
        ActionKind::SaveNote => save_note(state),
        ActionKind::SendKey => {
            crate::composer::send::send_key(
                state,
                string_param(action, "key"),
                action
                    .param("marker")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .1
        }
        ActionKind::Interrupt => crate::composer::send::interrupt(state, context),
        ActionKind::Send | ActionKind::Queue | ActionKind::Compact => crate::composer::send::begin(
            state,
            context,
            match action.kind {
                ActionKind::Queue => SubmissionMode::Queue,
                ActionKind::Compact => SubmissionMode::Compact,
                _ => SubmissionMode::Send,
            },
            text_param(action),
            draft_version(action),
            action
                .param("imagePaths")
                .and_then(Value::as_array)
                .map(|paths| {
                    paths
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        ),
        ActionKind::Handoff => crate::composer::send::handoff(
            state,
            context,
            text_param(action),
            draft_version(action),
        ),
        ActionKind::ReceiveHandoff => crate::composer::send::receive_handoff(
            state,
            context,
            string_param(action, "handoffId"),
            string_param(action, "content"),
            string_param(action, "current"),
            draft_version(action),
        ),
        _ => Vec::new(),
    }
}

/// The revision the composer's field is on, which every send and transfer carries.
fn draft_version(action: &UserAction) -> Option<crate::composer::queue::DraftVersion> {
    action
        .param("draftVersion")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

/// The wheel and the editor's own expand, which are the only two actions handled before the error
/// bar is cleared.
///
/// **To fold into family f.** The TypeScript refuses both while the subagent viewer is open,
/// because a wheel over a modal transcript is not a composer gesture; `ExtrasState` does not carry
/// that flag yet.
fn scroll(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    // The subagent transcript is modal: a wheel over it is not a composer gesture
    // (`native-host.ts`, the first thing `action` checked).
    if crate::extras::subagent::is_open(&state.extras.subagent) {
        return Vec::new();
    }
    let now = context.now_ms;
    let composer = &mut state.composer;
    if now - composer.scroll.last_event_at > COMPOSER_SCROLL_RESET_MS {
        composer.scroll.reset();
    }
    if action.kind == ActionKind::ComposerExpand {
        if action.param("editor") == Some(&Value::Bool(true)) {
            composer.scroll.suppress(now, COMPOSER_SCROLL_RESET_MS);
        }
        composer.collapsed = false;
        return Vec::new();
    }
    let delta = action
        .param("delta")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let eligible = action.param("eligible") == Some(&Value::Bool(true));
    let at_bottom = action
        .param("distanceToEnd")
        .and_then(Value::as_f64)
        .unwrap_or_default()
        <= COMPOSER_BOTTOM_THRESHOLD_PX;
    if composer.scroll.record(&ComposerScrollInput {
        now,
        delta_px: delta.abs(),
        collapse_threshold_px: COMPOSER_SCROLL_THRESHOLD_PX,
        collapse_eligible: eligible && !composer.collapsed,
        can_scroll_in_gesture_direction: action.param("canScroll") == Some(&Value::Bool(true)),
        scrolls_toward_logical_end: delta < 0.0 && at_bottom,
    }) {
        composer.collapsed = true;
    }
    if !eligible || (delta < 0.0 && at_bottom) {
        composer.collapsed = false;
    }
    Vec::new()
}

fn complete_command(state: &ChatState) -> Vec<Effect> {
    // `suggestions.nativeCommand(chat)` reads the controller's own text, not the composer's.
    match composer_native_command(
        state.session.agent.as_deref(),
        &state.composer.suggestions.text,
    ) {
        Some(content) => vec![Effect::SetComposerText {
            content: content.to_string(),
            caret: Some(content.encode_utf16().count()),
            from_history: false,
        }],
        None => Vec::new(),
    }
}

/// `NativeComposerSuggestions.update`: which dismissals survive the new draft and caret.
pub fn selection_changed(state: &mut ChatState, text: &str, caret: usize) {
    let composer = &mut state.composer;
    let previous = crate::composer::trigger::detect_composer_trigger(
        &composer.suggestions.text,
        Some(composer.suggestions.caret),
    );
    let next = crate::composer::trigger::detect_composer_trigger(text, Some(caret));
    let previous_start = previous.as_ref().map(|trigger| trigger.start);
    let next_start = next.as_ref().map(|trigger| trigger.start);
    if next
        .as_ref()
        .map(|trigger| trigger.kind != crate::composer::trigger::TriggerKind::Skill)
        .unwrap_or(true)
        || next_start != previous_start
    {
        composer.suggestions.dismissed.skill = false;
    }
    if next
        .as_ref()
        .map(|trigger| trigger.kind != crate::composer::trigger::TriggerKind::Path)
        .unwrap_or(true)
        || next_start != previous_start
    {
        composer.suggestions.dismissed.file = false;
    }
    if crate::composer::slash_commands::slash_query(text).is_none() {
        composer.suggestions.dismissed.slash = false;
    }
    if composer.suggestions.text != text
        || previous.as_ref().map(|trigger| trigger.query.clone())
            != next.as_ref().map(|trigger| trigger.query.clone())
        || previous_start != next_start
    {
        composer.suggestions.index = 0;
    }
    composer.suggestions.text = text.to_string();
    composer.suggestions.caret = caret;
    composer.text = text.to_string();
    composer.caret = caret;
}

fn suggestion_command(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let sources = suggestion_sources(state);
    let matches = current_matches(state, &sources);
    let Some(popup) = suggestion_popup(
        &matches,
        &sources,
        &state.composer.suggestions.text,
        state.composer.suggestions.index,
    ) else {
        return Vec::new();
    };
    let key = action.param("key").and_then(Value::as_str);
    if action.kind == ActionKind::SuggestionRetry {
        // `sources.requestSkills()` and nothing else (`native-suggestions.ts:157`). It used to
        // clear the list itself and issue a read under `request_id: 0`, which
        // `allocate_request_id` never hands out, so `settle_catalog` could not match the answer
        // and Retry emptied the picker for good.
        return crate::composer::settle::load_skills(state);
    }
    if action.kind == ActionKind::SuggestionHighlight {
        // `Math.max(0, Math.min(command.index ?? 0, projection.rows.length - 1))`, in that order:
        // an empty popup answers 0, where `i64::clamp(0, -1)` panics. `suggestion_popup` does
        // return an empty one (the skill list still loading, the file list not read yet), so a
        // hover on a loading picker took the chat window down.
        let index = js_number_of(action.param("index")).unwrap_or(0.0);
        let last = popup.rows.len() as f64 - 1.0;
        state.composer.suggestions.index = index.min(last).max(0.0) as usize;
        return Vec::new();
    }
    if key == Some("escape") || action.kind == ActionKind::SuggestionDismiss {
        match popup.kind {
            SuggestionKind::Slash => state.composer.suggestions.dismissed.slash = true,
            SuggestionKind::Skill => state.composer.suggestions.dismissed.skill = true,
            SuggestionKind::File => state.composer.suggestions.dismissed.file = true,
        }
        return Vec::new();
    }
    if popup.rows.is_empty() {
        return Vec::new();
    }
    if key == Some("up") || key == Some("down") {
        let step = if key == Some("up") { -1 } else { 1 };
        let count = popup.rows.len() as i64;
        state.composer.suggestions.index =
            ((popup.selected as i64 + step + count) % count) as usize;
        return Vec::new();
    }
    // `const index = command.index ?? projection.selected`, with the JavaScript reading of a
    // number: `2.0` is 2, and an index off the end simply finds no row below.
    let index = match js_number_of(action.param("index")) {
        Some(index) if index >= 0.0 && index < usize::MAX as f64 => index as usize,
        Some(_) => usize::MAX,
        None => popup.selected,
    };
    if popup.kind == SuggestionKind::Slash {
        let Some(command) = matches.slash_matches.get(index) else {
            return Vec::new();
        };
        if command.insert_text.is_none()
            && key == Some("enter")
            && state.composer.suggestions.text == format!("/{}", command.name)
        {
            // The picker sends rather than completing: the draft is already the whole command.
            return vec![Effect::HostAction {
                action: "suggestionSend".to_string(),
                params: Box::new(json!({})),
            }];
        }
        let content = command
            .insert_text
            .map(str::to_string)
            .unwrap_or_else(|| format!("/{}", command.name));
        let caret = content.encode_utf16().count();
        return vec![Effect::SetComposerText {
            content,
            caret: Some(caret),
            from_history: false,
        }];
    }
    let Some(replacement) = suggestion_replacement(
        &matches,
        popup.kind,
        index,
        &state.composer.suggestions.text,
    ) else {
        return Vec::new();
    };
    match complete_composer_mention(
        &state.composer.suggestions.text,
        state.composer.suggestions.caret,
        &format!("{replacement} "),
    ) {
        Some((content, caret)) => vec![Effect::SetComposerText {
            content,
            caret: Some(caret),
            from_history: false,
        }],
        None => Vec::new(),
    }
}

fn edit_draft(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    let text = text_param(action);
    track_draft_attachments(state, text);
    crate::composer::draft_sync::edited(state, text, action.param("draftVersion"), context);
    let from_history = action.param("history") == Some(&Value::Bool(true));
    let history_changed = !from_history && state.composer.history.index.is_some();
    if !from_history {
        state.composer.history.reset_index();
    }
    state.composer.text = text.to_string();
    // `if (!clearedError && !historyChanged) return;`: every keystroke writes the draft, and the
    // write answering is not on its own a reason to ship a snapshot.
    if !state.core.cleared_error && !history_changed {
        state.core.skip_closing_publish = true;
    }
    vec![Effect::WriteStorage {
        key: draft_key(state),
        value: Some(text.to_string()),
        durable: false,
    }]
}

fn save_draft(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let content = string_param(action, "content");
    track_draft_attachments(state, content);
    // `pushDraft` folds `result.draft` back onto the synced draft, so the write needs an id of its
    // own to be settled by.
    let version = action.param("draftVersion").cloned().unwrap_or(Value::Null);
    vec![crate::composer::draft_sync::push(state, content, version)]
}

/// `recallHistory`: Up walks back through what this machine has sent, Down walks forward.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// The ring is read lazily, on the FIRST Up with no entry showing, and the arm awaits that read
/// (`native-host.ts:1209`). `ComposerHistory::entries` had no writer at all until then, so Up and
/// Down did nothing and `historyActive` was permanently false.
fn recall_history(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let up = action.param("direction").and_then(Value::as_str) == Some("up");
    if up && state.composer.history.index.is_none() {
        state.composer.history.loading = true;
        return vec![Effect::ReadStorage {
            key: crate::composer::storage::composer_history_key(&state.identity.session_key),
        }];
    }
    apply_recall(state, up)
}

/// The half of the arm that runs once the ring is known.
pub(crate) fn apply_recall(state: &mut ChatState, up: bool) -> Vec<Effect> {
    let recalled = if up {
        state.composer.history.recall_previous()
    } else {
        state.composer.history.recall_next()
    };
    match recalled {
        Some(draft) => {
            selection_changed(state, &draft, draft.encode_utf16().count());
            // A recall closes every picker: the text was not typed, so nothing under the caret is
            // a mention the user is writing.
            let trigger = crate::composer::trigger::detect_composer_trigger(&draft, None);
            state.composer.suggestions.dismissed =
                crate::composer::suggestions::SuggestionDismissals {
                    slash: true,
                    skill: trigger.as_ref().is_some_and(|trigger| {
                        trigger.kind == crate::composer::trigger::TriggerKind::Skill
                    }),
                    file: trigger.as_ref().is_some_and(|trigger| {
                        trigger.kind == crate::composer::trigger::TriggerKind::Path
                    }),
                };
            vec![Effect::SetComposerText {
                content: draft,
                caret: None,
                from_history: true,
            }]
        }
        None => Vec::new(),
    }
}

/// A pill opens only when its destination is a local file, as React's composer did; a web link
/// pill is inert.
fn open_reference(action: &UserAction) -> Vec<Effect> {
    let href = string_param(action, "href");
    match classify_link_href(href) {
        LinkTarget::File(path) => {
            let position = file_position_from_href(href);
            vec![Effect::Open(crate::effect::OpenTarget::File {
                path,
                line: position.map(|position| position.line),
                column: position.and_then(|position| position.column),
            })]
        }
        _ => Vec::new(),
    }
}

fn refresh_chrome(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let session_id = action
        .param("sessionId")
        .map(|value| value.as_str().map(str::to_string));
    state.composer.chrome.begin_refresh(session_id);
    let prompts_request = state.core.allocate_request_id();
    let mut effects = vec![Effect::SendRpc {
        request_id: prompts_request,
        method: ChatRpcMethod::ListStashedPrompts,
        params: Box::new(json!({})),
    }];
    let note_request = state.composer.chrome.wants_note_read().then(|| {
        let request_id = state.core.allocate_request_id();
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::ReadSessionAgentNote,
            params: Box::new(json!({})),
        });
        request_id
    });
    state
        .composer
        .chrome
        .await_refresh(prompts_request, note_request);
    effects
}

fn stash(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let text = text_param(action);
    if text.trim().is_empty() {
        return Vec::new();
    }
    state.composer.chrome.begin_refresh(None);
    vec![
        Effect::SendRpc {
            request_id: 0,
            method: ChatRpcMethod::SaveStashedPrompt,
            params: Box::new(json!({ "content": text })),
        },
        Effect::ClearComposerIfUnchanged {
            text: text.to_string(),
        },
    ]
}

fn restore_returned(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(returned) = action.param("returned") else {
        return Vec::new();
    };
    let Some(id) = returned.get("id").and_then(Value::as_str) else {
        return Vec::new();
    };
    let text = returned
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    state.composer.claiming_returned = Some((id.to_string(), text));
    vec![Effect::ReadStorage {
        key: crate::composer::storage::returned_prompts_key(),
    }]
}

fn apply_returned(action: &UserAction) -> Vec<Effect> {
    let text = text_param(action);
    let current = string_param(action, "current");
    let content = if current.contains(text) {
        current.to_string()
    } else {
        restore_undelivered_text(text, current)
    };
    vec![Effect::SetComposerText {
        content,
        caret: None,
        from_history: false,
    }]
}

/// `attachPaths`: the host picked files, so gxserver copies them in and hands back the references.
///
/// The arm counts the read, publishes so the composer spins at once, and resumes on the answer.
/// `settle_attachment_import` is its `try`/`finally`.
fn attach_paths(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    state.composer.pending_attachments += 1;
    // `publish(chat)` before the await: the spinner shows on this turn.
    state.core.request_publish();
    let request_id = state.core.allocate_request_id();
    state.composer.attachment_imports.push(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ImportNativeAttachments,
        params: Box::new(json!({ "paths": action.param("paths").cloned().unwrap_or(Value::Null) })),
    }]
}

fn attachments_finished(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    state.composer.pending_attachments = state.composer.pending_attachments.saturating_sub(1);
    if let Some(error) = action.param("error").and_then(Value::as_str) {
        state.core.fail(error, None);
    }
    match action.param("paths").and_then(Value::as_array) {
        Some(paths) if !paths.is_empty() => vec![Effect::HostAction {
            action: "attachmentReferences".to_string(),
            params: Box::new(json!({ "paths": paths })),
        }],
        _ => Vec::new(),
    }
}

fn insert_attachments(action: &UserAction) -> Vec<Effect> {
    let mut text = text_param(action).to_string();
    let mut caret = usize_param(action, "start");
    let mut end = usize_param(action, "end");
    for path in action
        .param("paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let reference = native_path_reference(path, &text);
        let edit = insert_reference(&text, &reference, caret, end);
        text = edit.text;
        caret = edit.caret;
        end = caret;
    }
    vec![Effect::SetComposerText {
        content: text,
        caret: Some(caret),
        from_history: false,
    }]
}

fn remove_queue(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let prompt_id = string_param(action, "promptId");
    let document = crate::composer::document::queue(state);
    let mut edit = None;
    if action.param("edit") == Some(&Value::Bool(true)) {
        let Some(row) = document
            .prompts
            .iter()
            .find(|prompt| prompt.id == prompt_id)
        else {
            return Vec::new();
        };
        if !document.capabilities.can_edit || is_queue_row_busy(&row.state) {
            return Vec::new();
        }
        edit = Some(row.text.clone());
    }
    let prompt_id = prompt_id.to_string();
    let effects = queue_rpc(
        state,
        ChatRpcMethod::RemoveSessionChatQueuedPrompt,
        json!({ "promptId": prompt_id }),
        state.composer.transport.remove_queued_prompt,
        Some(prompt_id.clone()),
    );
    let sent = effects.first().and_then(|effect| match effect {
        Effect::SendRpc { request_id, .. } => Some(*request_id),
        _ => None,
    });
    if let (Some(original), Some(request_id)) = (edit, sent) {
        crate::composer::queue_edit::begin(state, request_id, original);
    }
    effects
}

/// The strip's own order, applied optimistically and then confirmed.
fn reorder_queue(state: &mut ChatState, visible: Vec<String>) -> Vec<Effect> {
    if !state.composer.transport.reorder_queue || state.session.queue_prompts.is_none() {
        state.core.fail("This session cannot queue prompts.", None);
        return Vec::new();
    }
    let order = reorder_optimistically(state, &visible);
    queue_rpc(
        state,
        ChatRpcMethod::ReorderSessionChatQueue,
        json!({ "promptIds": order }),
        state.composer.transport.reorder_queue,
        None,
    )
}

fn move_queue(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let prompts = crate::composer::document::queue(state).prompts;
    let prompt_id = string_param(action, "promptId");
    let target_id = string_param(action, "targetId");
    let from = prompts.iter().position(|prompt| prompt.id == prompt_id);
    let to = prompts.iter().position(|prompt| prompt.id == target_id);
    let (Some(from), Some(to)) = (from, to) else {
        return Vec::new();
    };
    if is_queue_row_busy(&prompts[from].state) {
        return Vec::new();
    }
    let order: Vec<String> = move_queue_row(&prompts, from as isize, to as isize)
        .into_iter()
        .map(|prompt| prompt.id)
        .collect();
    reorder_queue(state, order)
}

/// A queue mutation, refused outright when the capability is off rather than calling an endpoint
/// that would answer 404.
///
/// Every mutation answers with the WHOLE authoritative queue, so an optimistic step that lost a
/// race self-corrects on the next answer instead of needing a rollback path (`queueMutation` in
/// `controller.ts`). The answer is adopted in `crate::composer::send::settle_queue_mutation`.
fn queue_rpc(
    state: &mut ChatState,
    method: ChatRpcMethod,
    params: Value,
    available: bool,
    removed: Option<String>,
) -> Vec<Effect> {
    if !available || state.session.queue_prompts.is_none() {
        state.core.fail("This session cannot queue prompts.", None);
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.composer.queue_mutation = Some((request_id, removed));
    vec![Effect::SendRpc {
        request_id,
        method,
        params: Box::new(params),
    }]
}

/// The order the strip settles into before the answer lands.
///
/// `reorder` in `controller.ts` moves the rows itself first, because the drag must land where the
/// user dropped it rather than a round trip later, and the ids the strip shows are only the
/// VISIBLE ones: `sessionChatFullQueueOrder` folds them back into the whole queue.
fn reorder_optimistically(state: &mut ChatState, visible: &[String]) -> Vec<String> {
    let Some(queue) = state.session.queue_prompts.clone() else {
        return visible.to_vec();
    };
    let order = crate::session::startup_sends::full_queue_order(&queue, visible);
    let mut next = queue;
    for (target, id) in order.iter().enumerate() {
        let from = next
            .iter()
            .position(|prompt| prompt.get("id").and_then(Value::as_str) == Some(id.as_str()));
        if let Some(from) = from {
            next = move_queue_row(&next, from as isize, target as isize);
        }
    }
    state.session.queue_prompts = Some(next);
    // `setQueuePrompts((current) => { let next = [...current]; ... })`: a copy either way.
    state.messages.new_composition_identity();
    order
}

fn toggle_note(state: &mut ChatState) -> Vec<Effect> {
    if state.composer.note.open {
        let effects = save_note(state);
        state.composer.note.open = false;
        return effects;
    }
    state.composer.note.open = true;
    state.composer.note.loading = true;
    state.composer.note.edited = false;
    // `publish(chat)` before the await: the sheet opens and spins on this turn.
    state.core.request_publish();
    let request_id = state.core.allocate_request_id();
    state.composer.note.read_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionAgentNote,
        params: Box::new(json!({})),
    }]
}

fn save_note(state: &mut ChatState) -> Vec<Effect> {
    match state.composer.note.begin_flush() {
        Some((_, next)) => {
            let request_id = state.core.allocate_request_id();
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::SaveSessionAgentNote,
                params: Box::new(json!({ "note": next })),
            }]
        }
        None => Vec::new(),
    }
}

/// The draft leaves with its pictures, and a refusal counts them again.
fn track_draft_attachments(state: &mut ChatState, text: &str) {
    state.composer.draft_attachment_count = composer_references(text)
        .into_iter()
        .filter(|reference| reference.kind == ReferenceKind::Image)
        .count() as u32;
}

fn draft_key(state: &ChatState) -> crate::event::StorageKey {
    crate::composer::storage::draft_key(&state.identity.session_key)
}

fn text_param(action: &UserAction) -> &str {
    string_param(action, "text")
}

fn string_param<'a>(action: &'a UserAction, name: &str) -> &'a str {
    action
        .param(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn caret_param(action: &UserAction) -> usize {
    usize_param(action, "caret")
}

fn usize_param(action: &UserAction, name: &str) -> usize {
    action
        .param(name)
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize
}
