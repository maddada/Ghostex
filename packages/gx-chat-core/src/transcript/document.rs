//! Family b's part of the document: the transcript modes, the per-turn deferred work, the rewind
//! sheet and the saved-prompt marks.
//!
//! The producers are `publish` in `packages/shared/session-chat-controller/native-host.ts` (the
//! eight keys below) and `NativeChatMessageActions.projection` in `native-message-actions.ts` (the
//! rewind sheet).

use ghostex_gx_protocol::Tri;
use serde_json::{json, Value};

use crate::document::Document;
use crate::state::{ChatContext, ChatState, RewindRequest};
use crate::transcript::presentation::agent_supports_rewind;
use crate::transcript::{presentation, rows};

const PREVIEW_LINE_LIMIT: usize = 3;

/// The first few lines of the prompt, so the quote stays a glance and not a re-read.
fn prompt_preview(prompt: &str) -> String {
    let trimmed = crate::transcript::jsstr::js_trim_end(prompt);
    let lines = crate::transcript::jsstr::split_newlines(trimmed);
    let head = lines
        .iter()
        .take(PREVIEW_LINE_LIMIT)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > PREVIEW_LINE_LIMIT {
        format!("{head}\n\u{2026}")
    } else {
        head
    }
}

/*
CDXC:SessionChat 2026-09-18 SEE-ALSO:
The transcript's per-message actions for GPUI chat: Rewind (the confirmation in front of
`/api/rewindSessionChat`, React's session-chat-rewind-dialog.tsx) and Save prompt (React's
session-chat-save-prompt-button.tsx). The wording, the refusal handling, and the "put the prompt back
in the composer" rule have to match those two files; only the rendering differs.
*/
fn rewind_projection(request: &RewindRequest) -> Value {
    json!({
        "messageId": request.message_id,
        "preview": prompt_preview(&request.prompt),
        "description": if request.agent == "codex" {
            "Codex continues in a new conversation from this point and puts this message back in the composer for editing."
        } else if request.agent == "opencode" {
            "Rewind the conversation to this point and put this message back in the composer for editing. Files stay as they are."
        } else {
            "We only rewind using \"Restore conversation\" in the Chat View currently. Switch to Terminal View and use /rewind to resume using another option."
        },
        "busy": request.busy,
        "completed": request.completed,
        "synchronizationPending": request.synchronization_pending,
        "error": request.error,
        "submitLabel": if request.synchronization_pending {
            if request.busy { "Synchronizing" } else { "Retry synchronization" }
        } else if request.busy {
            "Rewinding"
        } else {
            "Rewind"
        },
        "cancelLabel": if request.completed || request.synchronization_pending { "Close" } else { "Cancel" },
    })
}

/// Writes family b's keys into `into`.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    let view = &state.transcript_view;
    into.summary_mode = view.summary_mode;
    into.verbose_override = view.verbose_override;
    into.final_ids = if view.items.is_empty() {
        presentation::build(state, context).final_ids
    } else {
        view.final_ids.clone()
    };
    into.deferred_work = view.deferred_work.clone();
    /*
    The user rail's own actions. `rewindAvailable` is React's `rewindToMessage` gate (a host that can
    reach `/api/rewindSessionChat` and an agent whose rewind Ghostex drives); `rewindEnabled` is its
    live `canRewind` gate, the same condition that lets the composer send, because the daemon types
    the rewind into that same pane. A preview backend answers no rewind route at all, which is why
    the Chat Lab offers the action in neither chat.
    */
    into.rewind_available = state.core.preview_settings.is_none()
        && agent_supports_rewind(state.session.agent.as_deref());
    // `rewindEnabled: sendBlockedReason(state) === null`. That is family d's rule, and
    // `document::assemble` runs family b first, so it is read off the state family d's settle
    // cached it on rather than out of a half-built document.
    into.rewind_enabled = state.composer.send_blocked_reason.is_none();
    into.rewind = match &view.rewind {
        Some(request) => Tri::Value(rewind_projection(request)),
        None => Tri::Null,
    };
    into.saved_prompts = view
        .saved_prompts
        .iter()
        .map(|(id, status)| (id.clone(), Value::String(status.clone())))
        .collect();
    let _ = rows::rows;
}
