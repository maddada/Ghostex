//! `NativeSubagentViewer.project()`: the child transcript the viewer paints, and the details of the
//! rows it draws open.
//!
//! Ported from `packages/shared/session-chat-controller/native-subagent.ts:165`. The viewer builds a
//! second `NativeChatPresentation` and runs the SAME projection over the page it read, always in
//! normal mode, with verbose and summarized off and no deferred-work rows of its own. Only two
//! things differ from the session's own pass: the messages, and the `agentPath` a nested Task row is
//! resolved against.
//!
//! Its cache lives on [`crate::state::SubagentState::view`] and is thrown away by every
//! `restart()`, which is what the TypeScript's `this.presentation = this.projector()` does.

use ghostex_gx_protocol::ChatMessage;
use serde_json::Value;

use crate::document::TranscriptItem;
use crate::extras::subagent::{agent_path, messages};
use crate::state::{ChatContext, ChatState, ProjectionInputs, TranscriptViewState};
use crate::transcript::line_breaks::AgentLineBreaks;
use crate::transcript::presentation::{build_scope, row_detail_scope, ProjectionScope};

/// The page's messages, as the rows the projection reads.
///
/// A message the wire shape cannot describe is dropped rather than failing the whole page: the
/// TypeScript hands `page.messages` straight to `update()` and a row it cannot read simply draws
/// nothing.
fn page_messages(page: &Value) -> Vec<ChatMessage> {
    messages(page)
        .into_iter()
        .filter_map(|message| serde_json::from_value::<ChatMessage>(message).ok())
        .collect()
}

/// The scope one projection pass runs under.
///
/// `workingDirectory` is MODULE level in `native-presentation.ts` (`setWorkingDirectory` writes a
/// file-scoped `let`), so both instances shorten their file-change paths against the same
/// directory; the session's copy is the one with a writer.
fn scope<'a>(
    view: &'a TranscriptViewState,
    messages: &'a [ChatMessage],
    working: bool,
    working_directory: Option<&'a str>,
    line_breaks: AgentLineBreaks,
) -> ProjectionScope<'a> {
    ProjectionScope {
        messages,
        working,
        // A subagent transcript is always read in normal mode.
        summary: false,
        deferred: &view.deferred,
        agent_path: &view.agent_path,
        working_directory,
        line_breaks,
    }
}

/// Rebuilds the viewer's item list when one of its inputs moved.
///
/// Called once per event from family f's settle. The TypeScript called `project()` from exactly
/// two places (a restart and a finished read), and both are a change to the page, the agent path
/// or the working flag, so an inputs guard is the same decision without the call sites.
pub fn refresh(state: &mut ChatState, context: &ChatContext) {
    let Some(page) = state.extras.subagent.page.clone() else {
        // `if (!page) { this.items = []; return; }`: the caches are NOT cleared here, only by a
        // restart, which is what makes reopening the same target cheap.
        state.extras.subagent.view.items.clear();
        state.extras.subagent.view.projection_inputs = None;
        return;
    };
    let rows = page_messages(&page);
    let next_path = agent_path(&state.extras.subagent);
    // `setAgentPath` throws every cached projection away when the conversation changes, because a
    // row pointing back at the transcript being shown is not a link.
    if state.extras.subagent.view.agent_path != next_path {
        state.extras.subagent.view.agent_path = next_path;
        state.extras.subagent.view.fold_settled_at = None;
        state.extras.subagent.view.invalidate();
    }
    let directory = state.transcript_view.working_directory.clone();
    if state.extras.subagent.view.working_directory != directory {
        state.extras.subagent.view.working_directory = directory;
        state.extras.subagent.view.invalidate();
    }
    // The viewer's own `NativeChatPresentation.update` applies the sticky fold with its own memory.
    let working = crate::transcript::turns::sticky_transcript_working(
        &rows,
        state.extras.subagent.working,
        &mut state.extras.subagent.view.fold_settled_at,
    );
    let inputs = ProjectionInputs {
        composed: rows.clone(),
        composition_identity: 0,
        working,
        summary: false,
        detail_revision: 0,
        backfill_revision: state.extras.subagent.view.backfill_revision,
        queue: None,
        line_breaks: crate::transcript::presentation::line_breaks(state),
    };
    if state.extras.subagent.view.projection_inputs.as_ref() == Some(&inputs) {
        return;
    }
    let line_breaks = crate::transcript::presentation::line_breaks(state);
    let mut cache = std::mem::take(&mut state.extras.subagent.view.projected);
    let projection = {
        let view = &state.extras.subagent.view;
        build_scope(
            &scope(
                view,
                &rows,
                working,
                view.working_directory.as_deref(),
                line_breaks,
            ),
            context,
            &mut cache,
        )
    };
    let view = &mut state.extras.subagent.view;
    view.projected = cache;
    view.items = projection.items;
    view.final_ids = projection.final_ids;
    view.backfill = projection.backfill;
    view.projection_inputs = Some(inputs);
    let details = row_details(state, context);
    state.extras.subagent.view.row_details = details;
}

/// `transcriptItems()`: the list, on its own channel beside the main transcript.
///
/// Pure, the way `crate::document::frame_parts` needs it to be: [`refresh`] does the work once per
/// event and this hands back what it stored.
pub fn rows(state: &ChatState, _context: &ChatContext) -> Vec<TranscriptItem> {
    state.extras.subagent.view.items.clone()
}

/// `rowDetail(kind, messageId, index)` against the child transcript.
///
/// `None` when the id belongs to the session's own transcript, which is what makes
/// `presentation.rowDetail(...) ?? subagentViewer.rowDetail(...)` pick the right one.
pub fn row_detail(state: &ChatState, kind: &str, message_id: &str, index: usize) -> Option<Value> {
    state.extras.subagent.page.as_ref()?;
    row_detail_scope(
        &state.extras.subagent.view.projected,
        kind,
        message_id,
        index,
    )
}

/// The details of the open rows that belong to the child transcript.
fn row_details(state: &ChatState, _context: &ChatContext) -> crate::document::RowDetails {
    let mut details = crate::document::RowDetails::new();
    for open in &state.transcript_view.open_rows {
        if let Some(detail) = row_detail(state, &open.kind, &open.message_id, open.index) {
            details.insert(open.key.clone(), detail);
        }
    }
    details
}

/// Promotes the newest batch of the viewer's placeholders, the way its own `onBackfill` does.
///
/// Returns whether anything moved, which is what asks for the publish the TypeScript's `changed()`
/// makes.
pub fn advance(state: &mut ChatState, context: &ChatContext) -> bool {
    if state.extras.subagent.view.backfill.is_empty() {
        return false;
    }
    let Some(page) = state.extras.subagent.page.clone() else {
        return false;
    };
    let rows = page_messages(&page);
    let view = &mut state.extras.subagent.view;
    let batch_start = view
        .backfill
        .len()
        .saturating_sub(crate::state::BACKFILL_BATCH);
    let batch: Vec<ChatMessage> = view.backfill.split_off(batch_start);
    let mut cache = std::mem::take(&mut view.projected);
    {
        let view = &state.extras.subagent.view;
        let scope = scope(
            view,
            &rows,
            state.extras.subagent.working,
            view.working_directory.as_deref(),
            crate::transcript::presentation::line_breaks(state),
        );
        for message in &batch {
            crate::transcript::presentation::project_cached(&mut cache, &scope, context, message);
        }
    }
    let view = &mut state.extras.subagent.view;
    view.projected = cache;
    view.backfill_revision += 1;
    refresh(state, context);
    true
}
