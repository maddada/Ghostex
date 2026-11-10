//! A coordinator's threads under its card: which sessions nest under which, in what order, which
//! of them an unfolded card lists, and what a folded card says about the ones it hides.
//!
//! CDXC:Sidebar 2026-11-09 DECISION:
//! Maintainer, on issue #528: "Yes please, send it! Let's nest the threads under the coordinator's card, folded by default for everyone, with the crew count as the fold toggle. No setting needed." This supersedes the 2026-11-04 decision that left a coordinator's workers out of the Unified list (reached only from the chat's Workers list and the Project sidebar). A worker is known by the link the Project sidebar nests it by (`coordinator_parent`); a worker on another computer carries none on its own computer's row, so the copy its coordinator's computer borrows (`is_borrowed_thread`) is the one drawn, with its own computer's project, Space and branch, and its own computer's copy is dropped. Scope runs first: only sessions in the active Space and not hidden are nested, and a worker whose coordinator is not in that list is its own card. Every coordinator at any depth folds on its own, folded until its crew count is clicked, and the unfolded ones are saved per client (`UnifiedSavedState::unfolded_coordinators`). Sebastian's answers on the plan: an unfolded coordinator lists its threads as the Project sidebar does, the ones busy, needing you, focused, visible or active in the last two hours, busy first, and the rest wait behind one "N older threads" row (saved per coordinator); a parked or snoozed thread is its own card on its shelf; a card whose thread needs you (approval, a question, waiting on you, stuck or failed) leaves the Moving shelf and sorts by when that thread came back. A folded card keeps the Project sidebar's overlapping dots for everything it hides (CDXC:Coordinators 2026-10-24 in sidebar_view/view.rs). Nested threads count toward no shelf, total or project header; the computer tabs, Quick Access and Running Agents count sessions as before. A search or a filter that matches a thread keeps its coordinator's card and shows the path to the thread even while folded; a filter that keeps the coordinator itself leaves its fold alone, and outer shelves and project groups stay as the user left them.
//! SEE-ALSO: sidebar_view/threads.rs `nest_threads` (the Project sidebar's tree, the two-hour rule), apps/mobile/app/src/contract/unified/tree.ts (the phone's port), apps/desktop/src/app/native_sidebar/unified/threads.rs (the drawing).

use std::collections::{BTreeSet, HashMap};

use crate::keys::SessionKey;
use crate::sidebar_view::{
    is_recent_thread, recent_thread_deadline_ms, session_is_snoozed, CoordinatorBadge,
    FoldedStatus, RowNesting, SessionRow, ThreadTally,
};

use super::build::{came_back_ms, machine_key, UnifiedFilter, UnifiedRow, UnifiedSettings};

/// What the user unfolded and listed, by [`unified_coordinator_key`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedFolds {
    /// Coordinators whose threads are shown (every coordinator starts folded).
    pub unfolded: BTreeSet<String>,
    /// Coordinators whose older threads are listed.
    pub older_shown: BTreeSet<String>,
    /// The row a reveal is bringing on screen (its `sidebar_session_id`): listed like a busy thread,
    /// with the threads above it, while its siblings' older threads stay tucked. Held by the client
    /// for the reveal, never saved.
    pub reveal: Option<String>,
}

/// The name a coordinator's fold and older-threads state are saved under: its computer, project
/// and session, so the same session id in two projects or on two computers never shares one.
pub fn unified_coordinator_key(key: &SessionKey) -> String {
    format!(
        "{}|{}|{}",
        machine_key(&key.machine),
        key.project_id,
        key.session_id
    )
}

/// What a row with threads under it draws and says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedCrew {
    /// [`unified_coordinator_key`].
    pub key: String,
    /// The threads are drawn under it now: the user unfolded it, or a search or filter shows a
    /// match inside it.
    pub open: bool,
    /// The user unfolded it (what its toggle flips).
    pub unfolded: bool,
    /// Its direct threads, listed and older together: the toggle's "Show N threads".
    pub total: u16,
    /// Its direct threads an unfold lists without the older-threads row.
    pub listed: u16,
    /// Its direct threads that wait behind the "N older threads" row.
    pub older: u16,
    /// The user listed its older threads.
    pub older_shown: bool,
    /// A search or filter keeps its matching threads drawn whatever the user's fold says, so
    /// folding it only drops the saved preference.
    pub held_open: bool,
    /// Its "N older threads" (or "Hide older threads") row is drawn after its threads: it is open
    /// on its own fold, not only on the way to a match, and has older threads.
    pub older_row: bool,
    /// The crew count: approvals, else threads waiting on you, else working ones, else every
    /// thread, grey.
    pub badge: CoordinatorBadge,
    /// While it is not open: the dots of every session under it, with its own.
    pub folded_status: FoldedStatus,
    /// The focused session is under it and not drawn, so this row carries the active highlight.
    pub contains_active: bool,
}

/// A session that needs the user: approval, stuck, a question, waiting on them, or failed.
pub(super) fn needs_you(row: &SessionRow) -> bool {
    row.awaiting_approval
        || row.stuck
        || row.has_question()
        || row.thread_state.as_deref() == Some("waiting")
        || row.model_selection_failed
        || row.program_status_failed
}

/// 0 needs you, 1 works, 2 the rest.
fn busy_rank(row: &SessionRow) -> u8 {
    if needs_you(row) {
        0
    } else if row.thread_state.as_deref() == Some("working") || row.activity == "working" {
        1
    } else {
        2
    }
}

/// A row hangs from no coordinator in the Unified list: parked (while parking is on) or snoozed.
fn detached(row: &SessionRow, settings: UnifiedSettings, now_ms: u64) -> bool {
    (settings.enable_session_parking && row.is_parked)
        || session_is_snoozed(row.timing.snoozed_until_ms, now_ms)
}

/// Facts about a subtree, gathered once.
#[derive(Clone, Copy, Default)]
struct Subtree {
    /// Some row in it (itself included) passes the filter.
    matched: bool,
    /// Some row in it keeps it listed: busy, needs you, recent, focused or visible.
    listed: bool,
    /// Lowest `busy_rank` in it.
    rank: u8,
}

/// Puts each worker under its coordinator. `rows` are the cards the build collected (borrowed
/// copies already swapped in); returns the top-level rows, each holding its tree, with the fold,
/// match and crew of every row filled in.
pub(super) fn nest_unified_rows(
    rows: Vec<UnifiedRow>,
    folds: &UnifiedFolds,
    filter: &UnifiedFilter,
    settings: UnifiedSettings,
    now_ms: u64,
) -> Vec<UnifiedRow> {
    let index_of: HashMap<SessionKey, usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| Some((row.session.row.key.clone()?, index)))
        .collect();
    let mut parent_of: Vec<Option<usize>> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let session = &row.session.row;
            if detached(session, settings, now_ms) {
                return None;
            }
            session
                .coordinator_parent
                .as_ref()
                .and_then(|parent| index_of.get(parent).copied())
                .filter(|parent| *parent != index)
        })
        .collect();
    // A loop (two coordinators that are each other's thread) never reaches a top row.
    let reaches_top = |parent_of: &[Option<usize>], mut at: usize| {
        for _ in 0..=parent_of.len() {
            match parent_of[at] {
                Some(parent) => at = parent,
                None => return true,
            }
        }
        false
    };
    // Decided on the links as they are, so every row of a loop stays a card, as the 2026-11-04
    // list kept them.
    let looped: Vec<bool> = (0..rows.len())
        .map(|index| parent_of[index].is_some() && !reaches_top(&parent_of, index))
        .collect();
    for (parent, looped) in parent_of.iter_mut().zip(looped) {
        if looped {
            *parent = None;
        }
    }
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for (index, parent) in parent_of.iter().enumerate() {
        if let Some(parent) = parent {
            children.entry(*parent).or_default().push(index);
        }
    }
    let mut slots: Vec<Option<UnifiedRow>> = rows.into_iter().map(Some).collect();
    let mut tops = Vec::new();
    for (index, parent) in parent_of.iter().enumerate() {
        if parent.is_none() {
            if let Some(row) = assemble(index, &mut slots, &children) {
                tops.push(row);
            }
        }
    }
    let narrowing = !filter.query.trim().is_empty();
    for top in &mut tops {
        let facts = gather(top, filter, folds.reveal.as_deref(), now_ms);
        // A card kept only for what is under it shows the way to the match.
        let narrowed = narrowing || !top.matched;
        lay_out(top, folds, narrowing, narrowed, facts.matched);
        mark_active(top, false);
        lift(top);
    }
    tops
}

/// Moves the subtree of `index` out of `slots` into one row holding its threads.
fn assemble(
    index: usize,
    slots: &mut [Option<UnifiedRow>],
    children: &HashMap<usize, Vec<usize>>,
) -> Option<UnifiedRow> {
    let mut row = slots[index].take()?;
    row.threads = children
        .get(&index)
        .into_iter()
        .flatten()
        .filter_map(|child| assemble(*child, slots, children))
        .collect();
    Some(row)
}

/// Sets each row's own `matched`, sorts every row's threads busy first, and returns the facts of
/// the subtree.
fn gather(
    row: &mut UnifiedRow,
    filter: &UnifiedFilter,
    reveal: Option<&str>,
    now_ms: u64,
) -> Subtree {
    row.matched = filter.keeps(row);
    let session = &row.session;
    let mut facts = Subtree {
        matched: row.matched,
        listed: is_recent_thread(&session.row, now_ms)
            || needs_you(&session.row)
            || session.is_focused
            || session.is_visible
            || reveal == Some(session.row.sidebar_session_id.as_str()),
        rank: busy_rank(&session.row),
    };
    let mut ranked: Vec<(Subtree, UnifiedRow)> = std::mem::take(&mut row.threads)
        .into_iter()
        .map(|mut child| (gather(&mut child, filter, reveal, now_ms), child))
        .collect();
    ranked.sort_by(|(left_facts, left), (right_facts, right)| {
        left_facts
            .rank
            .cmp(&right_facts.rank)
            .then_with(|| right.came_back_ms.cmp(&left.came_back_ms))
            .then_with(|| {
                left.session
                    .row
                    .sidebar_session_id
                    .cmp(&right.session.row.sidebar_session_id)
            })
    });
    for (child_facts, _) in &ranked {
        facts.matched |= child_facts.matched;
        facts.listed |= child_facts.listed;
        facts.rank = facts.rank.min(child_facts.rank);
    }
    // Each child's facts ride on it for `lay_out` through `shown` (listed) and `matched`.
    row.threads = ranked
        .into_iter()
        .map(|(child_facts, mut child)| {
            child.shown = child_facts.listed;
            child.subtree_matched = child_facts.matched;
            child
        })
        .collect();
    facts
}

/// Decides, top down, which rows are open and which threads are drawn. `narrowed` is a search or
/// a filter that kept the card only for what is under it: a row the search did not match shows
/// only the threads leading to a match. On entry each child's `shown` says it is listed (not an
/// older thread) and `subtree_matched` that a match is in it.
fn lay_out(
    row: &mut UnifiedRow,
    folds: &UnifiedFolds,
    query: bool,
    narrowing: bool,
    subtree_matched: bool,
) {
    row.subtree_matched = subtree_matched;
    if row.threads.is_empty() {
        row.crew = None;
        return;
    }
    let Some(key) = row.session.row.key.as_ref().map(unified_coordinator_key) else {
        return;
    };
    let unfolded = folds.unfolded.contains(&key);
    let older_shown = folds.older_shown.contains(&key);
    // This row narrows to its matches only when the search or filter did not match it itself.
    let narrowed = narrowing && !row.matched;
    let listed = row.threads.iter().filter(|child| child.shown).count();
    let total = row.threads.len();
    let match_below = row.threads.iter().any(|child| child.subtree_matched);
    // A text search opens a folded row whose threads match, even one it matched itself (a
    // coordinator and its thread both named "API"); a filter pick that keeps the row itself leaves
    // its fold alone.
    let forced = !narrowed && query && !unfolded && match_below;
    let open = if narrowed {
        match_below
    } else {
        unfolded || forced
    };
    let mut tally = ThreadTally::default();
    let bump = |count: &mut u16| *count = count.saturating_add(1);
    for child in &mut row.threads {
        let session = &child.session.row;
        tally.total = tally.total.saturating_add(1);
        match session.thread_state.as_deref() {
            Some("waiting") => bump(&mut tally.waiting),
            Some("working") => bump(&mut tally.working),
            _ => {}
        }
        let listed_child = child.shown;
        child.shown = if narrowed || forced {
            child.subtree_matched
        } else {
            listed_child || older_shown || (narrowing && child.subtree_matched)
        };
    }
    tally.approval = approvals_below(row);
    let mut folded_status = FoldedStatus::default();
    if !open {
        let mut stack: Vec<&UnifiedRow> = vec![row];
        while let Some(next) = stack.pop() {
            folded_status.add(&next.session.row);
            stack.extend(next.threads.iter());
        }
    }
    row.crew = Some(UnifiedCrew {
        key,
        open,
        unfolded,
        total: u16::try_from(total).unwrap_or(u16::MAX),
        listed: u16::try_from(listed).unwrap_or(u16::MAX),
        older: u16::try_from(total - listed).unwrap_or(u16::MAX),
        older_shown,
        held_open: match_below && (narrowed || query),
        older_row: open && !narrowed && !forced && total > listed,
        badge: RowNesting {
            threads: tally,
            ..RowNesting::default()
        }
        .coordinator_badge(),
        folded_status,
        contains_active: false,
    });
    for child in &mut row.threads {
        let matched = child.subtree_matched;
        lay_out(child, folds, query, narrowing, matched);
    }
}

/// Sessions under `row`, at any depth, waiting for approval: they turn every crew count above them
/// red, as in the Project sidebar (CDXC:SessionStatus 2026-10-27 in sidebar_view/view.rs).
fn approvals_below(row: &UnifiedRow) -> u16 {
    row.threads
        .iter()
        .map(|child| u16::from(child.session.row.awaiting_approval) + approvals_below(child))
        .fold(0, u16::saturating_add)
}

/// Gives the nearest drawn row above a focused session that is not drawn the active highlight.
/// `hidden` says this row itself is not drawn. Returns whether a focused session that is not drawn
/// is in this subtree and no drawn row has claimed it yet.
fn mark_active(row: &mut UnifiedRow, hidden: bool) -> bool {
    let open = row.crew.as_ref().is_some_and(|crew| crew.open);
    let mut pending = hidden && row.session.is_focused;
    for child in &mut row.threads {
        let child_hidden = hidden || !open || !child.shown;
        pending |= mark_active(child, child_hidden);
    }
    if pending && !hidden {
        if let Some(crew) = row.crew.as_mut() {
            crew.contains_active = true;
        }
        return false;
    }
    pending
}

/// A card with a thread that needs you leaves Moving and sorts by when that thread came back.
fn lift(top: &mut UnifiedRow) {
    let mut newest: Option<i64> = None;
    let mut stack: Vec<&UnifiedRow> = top.threads.iter().collect();
    while let Some(next) = stack.pop() {
        if needs_you(&next.session.row) {
            let at = came_back_ms(&next.session.row);
            newest = Some(newest.map_or(at, |known| known.max(at)));
        }
        stack.extend(next.threads.iter());
    }
    let Some(at) = newest else {
        return;
    };
    top.came_back_ms = top.came_back_ms.max(at);
    if top.shelf == super::build::UnifiedShelf::Working {
        top.shelf = super::build::UnifiedShelf::Active;
    }
}

/// When a thread under one of `tops` stops being recent or wakes from a snooze, so the list is
/// rebuilt then; `None` when nothing in it changes with time.
pub(super) fn tree_deadline_ms(tops: &[UnifiedRow], now_ms: u64) -> Option<i64> {
    let mut next: Option<i64> = None;
    let mut keep = |at: Option<i64>| {
        if let Some(at) = at.filter(|at| *at > now_ms as i64) {
            next = Some(next.map_or(at, |known| known.min(at)));
        }
    };
    let mut stack: Vec<(&UnifiedRow, bool)> = tops.iter().map(|row| (row, false)).collect();
    while let Some((row, nested)) = stack.pop() {
        let session = &row.session.row;
        if nested {
            keep(recent_thread_deadline_ms(session));
        }
        // A snoozed worker nests again when it wakes.
        if session.coordinator_parent.is_some() {
            keep(session.timing.snoozed_until_ms);
        }
        stack.extend(row.threads.iter().map(|child| (child, true)));
    }
    next
}

/// Every row of a tree, depth first, the top one included.
pub(super) fn walk_tree<'a>(top: &'a UnifiedRow, out: &mut Vec<&'a UnifiedRow>) {
    out.push(top);
    for child in &top.threads {
        walk_tree(child, out);
    }
}

/// The keys of the coordinators above the row of `sidebar_session_id` in `top`'s tree, outermost
/// first, when the row is in it.
pub(super) fn ancestors_of(top: &UnifiedRow, sidebar_session_id: &str) -> Option<Vec<String>> {
    if top.session.row.sidebar_session_id == sidebar_session_id {
        return Some(Vec::new());
    }
    let own = top.crew.as_ref()?.key.clone();
    top.threads.iter().find_map(|child| {
        let mut chain = ancestors_of(child, sidebar_session_id)?;
        chain.insert(0, own.clone());
        Some(chain)
    })
}

/// Whether the row of `sidebar_session_id` is drawn in `top`'s tree as it is laid out now.
pub(super) fn is_drawn(top: &UnifiedRow, sidebar_session_id: &str) -> bool {
    if top.session.row.sidebar_session_id == sidebar_session_id {
        return true;
    }
    let open = top.crew.as_ref().is_some_and(|crew| crew.open);
    open && top
        .threads
        .iter()
        .any(|child| child.shown && is_drawn(child, sidebar_session_id))
}
