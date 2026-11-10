//! Family b's rows: the list the renderer walks, and the details of the rows it draws open.
//!
//! The producers these replace are `projectChatTranscript` plus `NativeChatPresentation.update`
//! (`packages/shared/session-chat-controller/native-presentation.ts`). Its three caches are load
//! bearing (`docs/2026-09-21/rust-chat/SEAM.md` section 2c) and the port keeps them; where they
//! live is described on [`crate::state::TranscriptViewState`].

use crate::document::{RowDetails, TranscriptItem};
use crate::state::{ChatContext, ChatState, ProjectionInputs};
use crate::transcript::items::TranscriptItems;
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
    rows_shared(state, context).to_vec()
}

/// [`rows`] without copying the projection's list when it has one.
pub fn rows_shared(state: &ChatState, context: &ChatContext) -> std::sync::Arc<TranscriptItems> {
    if !state.transcript_view.items.is_empty() {
        return std::sync::Arc::clone(&state.transcript_view.items);
    }
    std::sync::Arc::new(TranscriptItems::from_items(
        presentation::build(state, context).items,
    ))
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
    crate::session::working::track_quiet_card(state);
    let holds_open = crate::session::working::quiet_holds_turn_open(state);
    let live = crate::session::working::transcript_working(state) || holds_open;
    // A turn held open has not settled, whatever a working blip before its card recorded, so
    // answering the card does not draw it as finished until its next row.
    if holds_open {
        state.transcript_view.fold_settled_at = None;
    }
    crate::transcript::turns::sticky_transcript_working(
        &state.messages.composed,
        live,
        &mut state.transcript_view.fold_settled_at,
    );
    let inputs = ProjectionInputs {
        composed: Vec::new(),
        composition_identity: state.messages.compose_generation,
        working: crate::transcript::foreign::is_working(state),
        hides_running: crate::session::working::hides_running_response(state),
        quiet_finished_before: crate::session::working::quiet_finished_before(state),
        summary: state.transcript_view.summary_mode,
        detail_revision: state.transcript_view.detail_revision,
        backfill_revision: state.transcript_view.backfill_revision,
        queue: (*state.session.queue_prompts).clone(),
        answers_on_the_way: crate::questions::open_questions::answers_on_the_way(state)
            .into_iter()
            .map(|answers| answers.page_id)
            .collect(),
        line_breaks: presentation::line_breaks(state),
        live_side_question: crate::transcript::side_question::live_side_question(state),
        approval_wait: crate::transcript::approval_wait::approval_wait(state),
        session_directory_revision: state.transcript_view.session_directory_revision,
        agent_message_deliveries: state.session.agent_message_deliveries.clone(),
    };
    if state.transcript_view.projection_inputs.as_ref() == Some(&inputs) {
        return;
    }
    // Built against the state's OWN cache, so every message this pass projects is kept for the
    // next one; `presentation::build` works on a copy and is for the pure readers.
    let mut cache = std::mem::take(&mut state.transcript_view.projected);
    // The settled turns are reused when they still stand (`transcript/settled.rs`); otherwise the
    // list is settled afresh and built on that, which makes the same list as the whole build, and
    // only a list that cannot be settled is built whole.
    let reused = state
        .transcript_view
        .settled
        .take()
        .and_then(|settled| crate::transcript::settled::build(state, settled, context, &mut cache))
        .or_else(|| {
            let scope = presentation::scope(state, &state.transcript_view);
            crate::transcript::settled::settle(state, context, &mut cache, &scope).and_then(
                |settled| crate::transcript::settled::build(state, settled, context, &mut cache),
            )
        });
    let (projection, settled_rows) = match reused {
        Some((projection, rows, settled)) => {
            state.transcript_view.settled = Some(settled);
            (projection, rows)
        }
        None => {
            let scope = presentation::scope(state, &state.transcript_view);
            let projection = presentation::build_scope(&scope, context, &mut cache);
            state.transcript_view.settled = None;
            (projection, Vec::new())
        }
    };
    state.transcript_view.projected = cache;
    // The rail rides in the same result object as the items in `NativeChatPresentation.update`, so
    // it is produced by this pass and stored on its owner's state.
    state.extras.minimap = projection.minimap;
    let view = &mut state.transcript_view;
    let mut items = projection.items;
    // Status line in the chat keeps the time on the turn's own fold and leaves the card above the
    // chat box without a worked line (`transcript/chat_look.rs`).
    view.worked_line = if view.chat_look.status_in_transcript {
        None
    } else {
        projection.worked_turn.map(|(id, label)| {
            // A grouped line's time spans the whole reply, so the card says that one.
            without_worked_time(&mut items, &id)
                .filter(|group| group != "Worked")
                .unwrap_or(label)
        })
    };
    view.items = std::sync::Arc::new(TranscriptItems::with_settled(&settled_rows, items));
    view.final_ids = projection.final_ids;
    view.answered_pages = projection.answered_pages;
    // Written only when they differ, so their stamps (`state::Stamped`) keep the document's kept
    // parts while a reply streams.
    view.question_forms
        .set_if_changed(projection.question_forms);
    view.page_answers.set_if_changed(projection.page_answers);
    view.askers.set_if_changed(projection.askers);
    view.backfill = projection.backfill;
    if view.backfill.is_empty() {
        // The run is over: a row that changes from here on queues again like any other.
        view.backfilled.clear();
    }
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
    // A pass that left nothing queued drops the timer the settle armed before it ran, which
    // otherwise fired once more on an empty queue and published a whole document for nothing.
    if state.transcript_view.has_pending_backfill() {
        state.core.timers.arm_once(
            crate::transcript::settle::BACKFILL_TIMER,
            context.now_ms,
            0.0,
        );
    } else {
        state
            .core
            .timers
            .cancel(crate::transcript::settle::BACKFILL_TIMER);
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
    view.backfilled
        .extend(batch.iter().map(|message| message.id.clone()));
    let mut cache = std::mem::take(&mut state.transcript_view.projected);
    {
        let scope = crate::transcript::settled::scope(state, &state.transcript_view);
        for message in &batch {
            presentation::project_cached(&mut cache, &scope, context, message);
        }
    }
    state.transcript_view.projected = cache;
    state.transcript_view.backfill_revision += 1;
    refresh(state, context);
    true
}

/// The finished turn whose "Worked for 2m 13s" the working card shows drops it from its own fold, which
/// reads "Previous turn" beside its counts instead, so the time is on screen once. The OMP wake-ups'
/// turns after it (`presentation::latest_worked_turn`) keep their own times. Under Agent work
/// Reduced the turn's line is the whole reply's, so this returns that line's own label, which the
/// card shows in the turn's place.
///
/// CDXC:SessionChat 2026-10-18 DECISION:
/// User (Q257, the card when the turn ends: "worked line"): "then you need to not show the same text up top in the collapsed section that mentions how many tool calls were done in this case if you get me (because otherwise we'd have "worked 2m 13s" twice on screen". While the working card shows a turn's Worked line, that turn's fold drops its time. Then (2026-10-18), seeing the fold left with only its chevron and counts: "Make this say 'Previous turn' on the left side after the chevron". Supersedes the 2026-10-17 fold that showed only its counts.
fn without_worked_time(items: &mut [TranscriptItem], turn_id: &str) -> Option<String> {
    let (label, button) = items.iter_mut().rev().find_map(|item| match item {
        TranscriptItem::CompletedWork { id, label, .. } if id == turn_id => Some((label, None)),
        // Agent work Reduced (and Quiet): the line that merged the turn says it for the whole
        // reply, and so does Quiet's work button, which drops its time the same way.
        TranscriptItem::GroupedWork {
            turns,
            label,
            work_button,
            ..
        } if turns.iter().any(|turn| turn.id == turn_id) => Some((label, Some(work_button))),
        _ => None,
    })?;
    let previous = std::mem::replace(label, "Previous turn".to_string());
    let button = button?;
    if let Some(time) = crate::transcript::grouped_work::short_time(&previous) {
        *button = crate::transcript::grouped_work::without_time(button, time);
    }
    Some(previous)
}
