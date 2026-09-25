//! Family b's rows: the list the renderer walks, and the details of the rows it draws open.
//!
//! The producers these replace are `projectChatTranscript` plus `NativeChatPresentation.update`
//! (`packages/shared/session-chat-controller/native-presentation.ts`). Its three caches are load
//! bearing (`docs/2026-09-21/rust-chat/SEAM.md` section 2c) and the port keeps them; where they
//! live is described on [`crate::state::TranscriptViewState`].

use crate::document::{RowDetails, TranscriptItem};
use crate::state::{ChatContext, ChatState, ProjectionInputs};
use crate::transcript::presentation;

/// The whole transcript list, from which family a computes the splice against what the host last
/// saw.
///
/// The input is `ChatState::messages.composed`, which family a builds: the authoritative rows plus
/// markers, terminal statuses, the streaming bubble and the pending echoes, in that order.
///
/// Pure, so it can be called from `frame_parts` with a shared borrow. [`refresh`] does the same work
/// once per turn and stores the answer, and this hands that back when it is current.
pub fn rows(state: &ChatState, context: &ChatContext) -> Vec<TranscriptItem> {
    if !state.transcript_view.items.is_empty() {
        return state.transcript_view.items.clone();
    }
    presentation::build(state, context).items
}

/// Details for the rows the renderer currently draws open, keyed by its own row key.
///
/// The open set is reported by `Event::Measured(Measurement::OpenRowDetails)`, so only the rows on
/// screen ever build their diff lines or tool output.
///
/// `native-host.ts:424` was `presentation.rowDetail(...) ?? subagentViewer.rowDetail(...)`: a row
/// opened INSIDE the subagent viewer belongs to the child transcript, whose messages the session's
/// own projection has never seen, so the viewer answers for it.
pub fn row_details(state: &ChatState, context: &ChatContext) -> RowDetails {
    let mut details = RowDetails::new();
    for open in &state.transcript_view.open_rows {
        let detail =
            presentation::row_detail(state, context, &open.kind, &open.message_id, open.index)
                .or_else(|| {
                    crate::extras::subagent_row_detail(
                        state,
                        &open.kind,
                        &open.message_id,
                        open.index,
                    )
                });
        if let Some(detail) = detail {
            details.insert(open.key.clone(), detail);
        }
    }
    details
}

/// Rebuilds the item list and the projection bookkeeping into the state.
///
/// Family a calls this once per turn, before `republish`. It is what makes the backfill observable:
/// a row that shipped as a placeholder is queued here, and the next [`advance`] projects a batch of
/// them so the following publish carries the whole row.
pub fn refresh(state: &mut ChatState, context: &ChatContext) {
    // `NativeChatPresentation.update` rebuilds only when one of its five inputs changed and hands
    // back the SAME result object otherwise, which is what `take`'s identity comparison reads. The
    // core has no identities, so the five inputs are remembered here and the revision below stands
    // in for "this is a new array".
    // `update()` runs the sticky fold before its change check, so a working blip over an unchanged
    // transcript rebuilds nothing.
    let live = crate::session::working::transcript_working(state);
    crate::transcript::turns::sticky_transcript_working(
        &state.messages.composed,
        live,
        &mut state.transcript_view.fold_settled_at,
    );
    let inputs = ProjectionInputs {
        composed: Vec::new(),
        composition_identity: state.messages.compose_generation,
        working: crate::transcript::foreign::is_working(state),
        summary: state.transcript_view.summary_mode,
        detail_revision: state.transcript_view.detail_revision,
        backfill_revision: state.transcript_view.backfill_revision,
        queue: state.session.queue_prompts.clone(),
    };
    if state.transcript_view.projection_inputs.as_ref() == Some(&inputs) {
        return;
    }
    // Built against the state's OWN cache, so every message this pass projects is kept for the
    // next one; `presentation::build` works on a copy and is for the pure readers.
    let mut cache = std::mem::take(&mut state.transcript_view.projected);
    let projection = presentation::build_scope(
        &presentation::scope(state, &state.transcript_view),
        context,
        &mut cache,
    );
    state.transcript_view.projected = cache;
    // The rail rides in the same result object as the items in `NativeChatPresentation.update`, so
    // it is produced by this pass and stored on its owner's state.
    state.extras.minimap = projection.minimap;
    let view = &mut state.transcript_view;
    view.items = projection.items;
    view.final_ids = projection.final_ids;
    view.backfill = projection.backfill;
    view.projection_inputs = Some(inputs);
    // `update` runs INSIDE `publish` in the TypeScript, so a rebuild only becomes a new array the
    // host can see on a turn that publishes. `ChatCore::republish` turns this flag into the
    // revision the frame's identity test reads.
    view.projection_rebuilt = true;
    state.transcript_view.row_details = row_details(state, context);
    // `this.scheduleBackfill()` at the end of `update`: the zero-delay timer is armed by the
    // publish that queued the placeholders, so the very next `tick` promotes them. Arming it
    // from the settle instead waited for the NEXT event's settle, so on every real chat the first
    // transcript shipped its older rows as placeholders one document longer than the TypeScript
    // brain did.
    if state.transcript_view.has_pending_backfill() {
        state.core.timers.arm_once(
            crate::transcript::settle::BACKFILL_TIMER,
            context.now_ms,
            0.0,
        );
    }
}

/// Projects the newest batch of queued placeholders.
///
/// Returns whether anything changed, which is what tells the host to publish again. The TypeScript
/// runs this on a 0 ms timer and takes the LAST `BACKFILL_BATCH` ids, newest first.
pub fn advance(state: &mut ChatState, context: &ChatContext) -> bool {
    if state.transcript_view.backfill.is_empty() {
        return false;
    }
    let view = &mut state.transcript_view;
    let batch_start = view
        .backfill
        .len()
        .saturating_sub(crate::state::BACKFILL_BATCH);
    // `for (const message of this.backfill.splice(-BACKFILL_BATCH)) this.message(message)`: the
    // queue holds the MESSAGES the projection met, not ids looked up in the composed list, because
    // a completed turn's work rows come from the `loadWork` reads and are in no list at all. Each
    // is projected NOW and cached, so the refresh below reads whole rows for them.
    let batch: Vec<ghostex_gx_protocol::ChatMessage> = view.backfill.split_off(batch_start);
    let mut cache = std::mem::take(&mut state.transcript_view.projected);
    {
        let scope = presentation::scope(state, &state.transcript_view);
        for message in &batch {
            presentation::project_cached(&mut cache, &scope, context, message);
        }
    }
    state.transcript_view.projected = cache;
    state.transcript_view.backfill_revision += 1;
    refresh(state, context);
    true
}
