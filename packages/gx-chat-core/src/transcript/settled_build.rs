use std::collections::HashSet;
use std::sync::Arc;

use ghostex_gx_protocol::ChatMessage;

use crate::document::TranscriptItem;
use crate::extras::minimap::{minimap_rows, minimap_shows};
use crate::extras::minimap_rail::MinimapMarkerRow;
use crate::state::{ChatContext, ChatState, TranscriptViewState, EAGER_TAIL_ITEMS};
use crate::transcript::foreign::{is_working, order_cmp, order_key};
use crate::transcript::presentation::{
    build_items, drop_hidden_rows, item_index, latest_worked_turn, line_breaks, prune_cache,
    Builder, Projection, ProjectionCache, ProjectionScope,
};
use crate::transcript::sent_message::{learn_names, resolve_recipients_after};
use crate::transcript::thinking_card::RunningThinking;
use crate::transcript::transcript::{project_chat_segment, project_chat_segment_with};

use super::{
    active_seams, asks_resend, newest_reply_in, projection_ids, render_len, restarts_response,
    seams, sorted_positions, SettledPrefix, SharedChunks, Unresolved, ACTIVE_STRETCH_ROWS,
    ACTIVE_TAIL_ROWS,
};

/// The projection built on `settled`, or `None` when the prefix does not hold for this state and
/// the whole list has to be built.
pub(super) fn build_on(
    state: &ChatState,
    settled: &mut SettledPrefix,
    context: &ChatContext,
    cache: &mut ProjectionCache,
) -> Option<(Projection, SharedChunks)> {
    let view = &state.transcript_view;
    if settled.key != SettledPrefix::key_of(state) {
        return None;
    }
    let composed = &state.messages.composed;
    let generation = state.messages.compose_generation;
    if settled.checked_generation != generation {
        let held = settled.messages.len();
        // The composition kept the head it was checked against (`compose_after`), the prefix lies
        // wholly in that head, and every row of the new tail sorts after the prefix: the prefix
        // still leads the list unchanged, and only the rows after it need sorting.
        let part = state.messages.transcript_part.as_ref();
        let kept = part.filter(|part| {
            settled.checked_part == Some(part.revision)
                && part.len <= composed.len()
                && settled.order[..held].iter().all(|at| *at < part.len)
                && composed[part.len..]
                    .iter()
                    .all(|message| order_key(message) > settled.order_max)
        });
        let order = match kept {
            Some(part) => {
                let mut later: Vec<usize> = settled.order[held..]
                    .iter()
                    .copied()
                    .filter(|at| *at < part.len)
                    .chain(part.len..composed.len())
                    .collect();
                later.sort_by(|left, right| order_cmp(&composed[*left], &composed[*right]));
                let mut order = settled.order[..held].to_vec();
                order.extend(later);
                order
            }
            None => {
                let order = sorted_positions(composed);
                if order.len() <= held
                    || !(0..held).all(|at| composed[order[at]] == settled.messages[at])
                    || !settled
                        .scans
                        .windows(2)
                        .all(|pair| order[pair[0].at] < order[pair[1].at])
                {
                    return None;
                }
                order
            }
        };
        if order.len() <= held {
            return None;
        }
        settled.order = order;
        settled.checked_generation = generation;
        settled.checked_part = part.map(|part| part.revision);
    }
    let order = std::mem::take(&mut settled.order);
    let built = build_after(state, settled, &order, context, cache, view);
    settled.order = order;
    built
}

/// [`build_on`] once the prefix is known to lead the list, which `order` sorts.
fn build_after(
    state: &ChatState,
    settled: &mut SettledPrefix,
    order: &[usize],
    context: &ChatContext,
    cache: &mut ProjectionCache,
    view: &TranscriptViewState,
) -> Option<(Projection, SharedChunks)> {
    let composed = &state.messages.composed;
    let working = is_working(state);
    let summary = settled.key.summary;
    // The side-question card above the composer hides the newest row asking its question; while
    // it is open nothing joins the prefix, so that row is in the stretch or already settled.
    let live_side_question = crate::transcript::side_question::live_side_question(state);
    let approval_wait = crate::transcript::approval_wait::approval_wait(state);
    let rows_after = |settled: &SettledPrefix| -> Vec<ChatMessage> {
        order[settled.messages.len()..]
            .iter()
            .map(|at| composed[*at].clone())
            .collect()
    };
    // The running turn's chunks stand only while it works, in the verbose list.
    if settled.has_active() && (!working || summary) {
        settled.drop_active();
    }
    let mut stretch = rows_after(settled);
    if !settled.can_follow(&stretch) {
        if !settled.has_active() {
            return None;
        }
        settled.drop_active();
        stretch = rows_after(settled);
        if !settled.can_follow(&stretch) {
            return None;
        }
    }
    // A turn closed after the running turn's chunks: they go, and the turn joins whole below.
    if settled.has_active() && !seams(&stretch, summary).is_empty() {
        settled.drop_active();
        stretch = rows_after(settled);
        if !settled.can_follow(&stretch) {
            return None;
        }
    }
    let mut tail = project_chat_segment(&stretch, working, &settled.redone);
    // A prompt or a wake after them restarts the response, which folds what came before it.
    if settled.has_active() && restarts_response(&tail.0) {
        settled.drop_active();
        stretch = rows_after(settled);
        if !settled.can_follow(&stretch) {
            return None;
        }
        tail = project_chat_segment(&stretch, working, &settled.redone);
    }
    let later = settled.later_scans(order, &stretch);
    let scans = later.as_ref().unwrap_or(&settled.prefix_scans);
    let (askers, question_tags) = settled.askers(state, scans, later.is_some(), &stretch);
    let page_answers = scans.answers.clone();
    let forms = scans.marked_forms();
    let mut scope = ProjectionScope {
        messages: composed,
        working,
        summary: view.summary_mode,
        deferred: &view.deferred,
        agent_path: &view.agent_path,
        working_directory: view.working_directory.as_deref(),
        line_breaks: line_breaks(state),
        live_side_question: live_side_question.clone(),
        approval_wait: approval_wait.clone(),
        agent_fleet: state.session.agent_fleet.as_ref(),
        session_directory: &view.session_directory,
        question_tags,
        askers,
        agent_message_deliveries: state.session.agent_message_deliveries.as_ref(),
        backfilled: &view.backfilled,
        question_forms: std::cell::OnceCell::from(forms),
        tool_cards: view.tool_cards,
        picture_digests: &view.picture_digests,
    };

    // Turns a newer accepted prompt has closed since the prefix was settled join it, one chunk
    // each, so the stretch built below is only the newest turn.
    let mut fresh: Vec<Unresolved> = Vec::new();
    while let Some(&next) = seams(&stretch, summary)
        .first()
        .filter(|_| live_side_question.is_none())
    {
        let rest = project_chat_segment(&stretch[next..], working, &settled.redone);
        let (projection, own_redone) =
            project_chat_segment(&stretch[..next], false, &settled.redone);
        if !own_redone.is_empty() {
            break;
        }
        let start = settled.messages.len();
        // The eager tail as it stands with this turn joined: the stretch's rows are all still here.
        let eager_from = (settled.render_count() + render_len(&tail.0, summary))
            .saturating_sub(EAGER_TAIL_ITEMS);
        // Summary mode: the reply that opens is the newest in the turns after this one, else its
        // own, else the prefix's.
        let first_turn = settled.summary_turns();
        let newest_reply = summary
            .then(|| {
                newest_reply_in(&rest.0)
                    .map(|at| first_turn + projection.summary_turns.len() + at)
                    .or_else(|| newest_reply_in(&projection).map(|at| first_turn + at))
                    .or_else(|| settled.newest_reply(None))
            })
            .flatten();
        let mut built = settled.build_chunk(
            &projection,
            (start, start + next),
            settled.chunks.len(),
            None,
            None,
            newest_reply,
            eager_from,
            &scope,
            context,
            cache,
        );
        if settled
            .absorb(&stretch[..next], &projection, &mut built, order)
            .is_none()
        {
            break;
        }
        stretch.drain(..next);
        fresh.push(built);
        if !settled.can_follow(&stretch) {
            return None;
        }
        tail = rest;
    }
    // The running turn's thinking cards fall on the rows after the newest turn opener as the list
    // arrived; one of the settled turns' rows there would carry a card it was built without.
    let running = RunningThinking::new(composed, working);
    if settled.chunks.iter().any(|chunk| {
        !chunk.active
            && chunk
                .thinking
                .iter()
                .any(|(id, _)| running.card(id).is_some())
    }) {
        return None;
    }
    // A long running turn settles all but its newest rows into active chunks, which every later
    // pass reuses while the turn keeps working.
    if working
        && !summary
        && live_side_question.is_none()
        && stretch.len() >= ACTIVE_STRETCH_ROWS
        && !restarts_response(&tail.0)
    {
        let keep = stretch.len() - ACTIVE_TAIL_ROWS;
        if let Some(&cut) = active_seams(&stretch)
            .iter()
            .rev()
            .find(|cut| **cut <= keep)
        {
            let (projection, own_redone) =
                project_chat_segment(&stretch[..cut], true, &settled.redone);
            if own_redone.is_empty() && !asks_resend(&stretch[..cut]) {
                let start = settled.messages.len();
                let eager_from = (settled.render_count() + render_len(&tail.0, summary))
                    .saturating_sub(EAGER_TAIL_ITEMS);
                let mut built = settled.build_chunk(
                    &projection,
                    (start, start + cut),
                    settled.chunks.len(),
                    None,
                    Some(&running),
                    None,
                    eager_from,
                    &scope,
                    context,
                    cache,
                );
                if settled
                    .absorb(&stretch[..cut], &projection, &mut built, order)
                    .is_some()
                {
                    stretch.drain(..cut);
                    fresh.push(built);
                    if !settled.can_follow(&stretch) {
                        return None;
                    }
                    tail = project_chat_segment(&stretch, working, &settled.redone);
                }
            }
        }
    }
    if !fresh.is_empty() {
        settled.settle_scans();
    }
    let (projection, own_redone) = match &live_side_question {
        None => tail,
        Some(question) => {
            let (projection, own_redone, hid) =
                project_chat_segment_with(&stretch, working, &settled.redone, Some(question));
            let settled_asks = settled.chunks.iter().any(|chunk| {
                chunk
                    .side_rows
                    .iter()
                    .any(|row| crate::transcript::side_question::asks_side_question(row, question))
            });
            if !hid && settled_asks {
                return None;
            }
            (projection, own_redone)
        }
    };
    if !own_redone.is_empty() || projection_ids(&projection).any(|id| settled.ids.contains(id)) {
        return None;
    }
    let eager_from = (settled.render_count() + render_len(&projection, summary))
        .saturating_sub(EAGER_TAIL_ITEMS);

    // Settled chunks whose own inputs moved are built again.
    let tags_moved = settled.checked_tags != scope.question_tags;
    let newest_reply = summary
        .then(|| {
            settled
                .newest_reply(newest_reply_in(&projection).map(|at| settled.summary_turns() + at))
        })
        .flatten();
    let moved: Vec<usize> = settled
        .chunks
        .iter()
        .enumerate()
        .filter(|(index, chunk)| {
            !fresh.iter().any(|built| built.index == *index)
                && SettledPrefix::chunk_moved(
                    chunk,
                    view,
                    cache,
                    &scope,
                    eager_from,
                    tags_moved,
                    &running,
                    newest_reply,
                )
        })
        .map(|(index, _)| index)
        .collect();
    for index in moved {
        let built = settled.rebuild(
            index,
            eager_from,
            &running,
            newest_reply,
            &scope,
            context,
            cache,
        )?;
        fresh.push(built);
    }
    settled.checked_tags = scope.question_tags.clone();

    // Drawn against the prefix as it stands once this pass's chunks are in place.
    let prior_used: HashSet<String>;
    let prior = if fresh
        .iter()
        .any(|built| built.chunk.used != settled.chunks[built.index].used)
    {
        prior_used = settled
            .chunks
            .iter()
            .enumerate()
            .flat_map(|(index, chunk)| {
                fresh
                    .iter()
                    .find(|built| built.index == index)
                    .map_or(&chunk.used, |built| &built.chunk.used)
                    .iter()
                    .cloned()
            })
            .collect();
        &prior_used
    } else {
        &settled.used
    };
    let mut builder = Builder::new(&scope, context, cache, eager_from, Some(prior), None);
    let mut items = build_items(
        &mut builder,
        &projection,
        &running,
        settled.render_count(),
        newest_reply,
    );
    if builder.used.iter().any(|id| settled.ids.contains(id)) {
        return None;
    }
    let tail_queued = std::mem::take(&mut builder.backfill);
    let tail_used = std::mem::take(&mut builder.used);
    drop(builder);
    drop_hidden_rows(&mut items, &projection.hidden_rows);
    let tail_learned = learn_names(&items);
    settled.name_chunks(
        fresh,
        &tail_learned,
        eager_from,
        &running,
        newest_reply,
        &scope,
        context,
        cache,
    )?;
    let layers = [&settled.names, &tail_learned];
    resolve_recipients_after(
        &mut items,
        scope.agent_fleet,
        scope.session_directory,
        scope.agent_message_deliveries,
        &layers,
        false,
    );
    let worked_turn = latest_worked_turn(&projection.items, working);
    // The rail counts its dashes, not the turns: a turn the user did not open gets none.
    let minimap = {
        let turns: Vec<_> = projection
            .summary_turns
            .iter()
            .map(|turn| (&turn.user, turn.final_message.as_ref()))
            .collect();
        let mut rows: Vec<MinimapMarkerRow> = settled
            .chunks
            .iter()
            .flat_map(|chunk| chunk.minimap.iter().cloned())
            .collect();
        rows.extend(minimap_rows(
            &turns,
            &item_index(&items, settled.item_count()),
        ));
        if minimap_shows(rows.len()) {
            rows
        } else {
            Vec::new()
        }
    };
    // The call an approval waits on is the newest one: in the stretch, or the prefix has to be
    // built with the mark.
    let before: Vec<&[TranscriptItem]> = settled
        .chunks
        .iter()
        .map(|chunk| chunk.items.as_slice())
        .collect();
    if !crate::transcript::approval_wait::open_waiting_call_in(
        &mut items,
        &before,
        cache,
        scope.approval_wait.as_ref(),
    ) {
        return None;
    }
    // `prune_cache`'s own bound, checked first so the prefix's ids are only gathered when it
    // prunes.
    if cache.len() > scope.messages.len() * 2 + 64 {
        let mut all_used = settled.used.clone();
        all_used.extend(tail_used);
        prune_cache(cache, &scope, &all_used);
    }

    let answered_pages: Vec<String> = page_answers
        .iter()
        .map(|answers| answers.page_id.clone())
        .collect();
    let askers = scope
        .askers
        .iter()
        .cloned()
        .map(|mut asker| {
            asker.answered = answered_pages.contains(&asker.id) || asker.answered_in_words();
            asker
        })
        .collect();
    let question_forms = scope.question_forms.take().unwrap_or_default();
    let mut final_ids: Vec<String> = settled
        .chunks
        .iter()
        .flat_map(|chunk| chunk.final_ids.iter().cloned())
        .collect();
    final_ids.extend(projection.final_ids);
    let mut backfill: Vec<ChatMessage> = settled
        .chunks
        .iter()
        .flat_map(|chunk| chunk.queued.iter().cloned())
        .collect();
    backfill.extend(tail_queued);
    let chunks = settled
        .chunks
        .iter()
        .map(|chunk| Arc::clone(&chunk.items))
        .collect();
    Some((
        Projection {
            items,
            final_ids,
            answered_pages,
            question_forms,
            page_answers,
            askers,
            backfill,
            minimap,
            worked_turn,
        },
        chunks,
    ))
}
