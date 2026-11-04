//! Coordinator trees: each thread drawn right under its coordinator.
//!
//! CDXC:Coordinators 2026-10-07 SEE-ALSO: the user's decision in packages/gx-chat-core/src/extras/coordinator_threads.rs ("show the working or the active in the last 2 hours", "Don't actually 'hide' them please"). An expanded coordinator lists its threads that are working, waiting, or were active in the last two hours (and the one the user is looking at); the older ones, resolved threads included, wait behind one "N older threads" row at the end of its children. That row and the coordinator's fold change only when the user clicks them, and are remembered per coordinator; opening the coordinator's own chat changes neither, and opening a thread unfolds its coordinator (the 2026-10-01 decision below) but never lists the older threads. A folded coordinator shows no threads, and its badge counts what it lists.
//! SEE-ALSO: server/src/coordinators/presentation.rs (the fields), view.rs `RowNesting`, apps/desktop/src/app/native_sidebar/threads.rs (the "N older threads" row), packages/gx-chat-core/src/extras/coordinator_threads.rs (the chat panel's twin rule).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::keys::SessionKey;

use super::inputs::SectionId;
use super::ordering::{is_snoozed, section_of};
use super::view::{FoldedStatus, RowNesting, SessionRow, SessionView, ThreadTally};

/// How long after its last activity a thread that neither works nor waits is still listed.
const RECENT_THREAD_MS: i64 = 2 * 60 * 60 * 1000;

fn is_busy_thread(row: &SessionRow) -> bool {
    matches!(row.thread_state.as_deref(), Some("working" | "waiting")) || row.activity == "working"
}

/// A thread an expanded coordinator lists without its "N older threads" row.
fn is_recent_thread(row: &SessionRow, now_ms: u64) -> bool {
    is_busy_thread(row)
        || row
            .timing
            .last_interaction_ms
            .is_some_and(|at| at + RECENT_THREAD_MS > now_ms as i64)
}

/// When an idle thread stops being recent, so the list is rebuilt then.
pub(crate) fn recent_thread_deadline_ms(row: &SessionRow) -> Option<i64> {
    if row.coordinator_parent.is_none() || is_busy_thread(row) {
        return None;
    }
    row.timing
        .last_interaction_ms
        .map(|at| at + RECENT_THREAD_MS)
}

/// Every coordinator's threads among `rows`, by state. A row listed twice (a project's own rows and
/// a group's members) counts once.
pub(crate) fn tally_threads<'a>(
    rows: impl Iterator<Item = &'a Arc<SessionRow>>,
    now_ms: u64,
) -> HashMap<SessionKey, ThreadTally> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut tallies: HashMap<SessionKey, ThreadTally> = HashMap::new();
    let mut parents: HashMap<&SessionKey, &SessionKey> = HashMap::new();
    let mut approvals: Vec<&SessionKey> = Vec::new();
    let bump = |count: &mut u16| *count = count.saturating_add(1);
    for row in rows {
        let Some(parent) = row.coordinator_parent.as_ref() else {
            continue;
        };
        if row.is_browser || !seen.insert(row.sidebar_session_id.as_str()) {
            continue;
        }
        if let Some(key) = row.key.as_ref() {
            parents.insert(key, parent);
        }
        if row.awaiting_approval {
            approvals.push(parent);
        }
        let tally = tallies.entry(parent.clone()).or_default();
        if is_recent_thread(row, now_ms) {
            bump(&mut tally.total);
        }
        match row.thread_state.as_deref() {
            Some("waiting") => bump(&mut tally.waiting),
            Some("working") => bump(&mut tally.working),
            _ => {}
        }
    }
    // A session waiting for approval counts on every orchestrator above it, so a nested
    // orchestrator's stuck thread turns the top crew badge red too (CDXC:SessionStatus 2026-10-27
    // in view.rs). The walk stops at a loop or past any real depth.
    for parent in approvals {
        let mut ancestor = Some(parent);
        let mut visited: Vec<&SessionKey> = Vec::new();
        while let Some(key) = ancestor.filter(|key| !visited.contains(key) && visited.len() < 32) {
            bump(&mut tallies.entry(key.clone()).or_default().approval);
            visited.push(key);
            ancestor = parents.get(key).copied();
        }
    }
    tallies
}

/// Deeper trees are drawn flat past this, so a coordinator of coordinators still reads as a tree
/// without running out of row width.
const MAX_DEPTH: u8 = 3;
// `RowNesting::passing_lines` keeps one bit per level.
const _: () = assert!(MAX_DEPTH < u8::BITS as u8);

/// The `RowNesting::passing_lines` a row at `depth` hands to the rows directly under it: the ones
/// that run past the row itself, plus its own level's line while a sibling of it follows.
pub(crate) fn passing_lines_below(passing_lines: u8, depth: u8, last: bool) -> u8 {
    if depth > 0 && !last {
        passing_lines | (1 << depth)
    } else {
        passing_lines
    }
}

fn is_nestable_thread(session: &SessionView, enable_parking: bool, now_ms: u64) -> bool {
    let row = &session.row;
    row.coordinator_parent.is_some()
        && !row.is_browser
        && !(enable_parking && row.is_parked)
        && !is_snoozed(row, now_ms)
}

/// Reorders a group's rows (already in display order) so each thread follows its coordinator, its
/// recent threads before its older ones, and returns the section each row is drawn under: a thread
/// takes its coordinator's. The threads of a coordinator in `collapsed` (sidebar row ids), and the
/// older threads of one not in `older_shown`, stay in the list, marked folded, so the headers above
/// still count them.
///
/// CDXC:Coordinators 2026-10-01 DECISION:
/// User: "I want to be able to collapse the coordinator's row by clicking on a button next to it." A chevron on the coordinator row folds its threads away and unfolds them, remembered per coordinator across restarts with the rest of the sidebar's collapse state. A folded coordinator still shows its badge (count and tint), its threads still count toward the section and project headers, revealing one of its threads unfolds it, and for a drop the folded block is just the coordinator row.
pub(crate) fn nest_threads(
    sessions: Vec<SessionView>,
    enable_parking: bool,
    group_working: bool,
    held: &HashSet<String>,
    collapsed: &BTreeSet<String>,
    older_shown: &BTreeSet<String>,
    now_ms: u64,
) -> (Vec<SessionView>, Vec<SectionId>) {
    let base_sections: Vec<SectionId> = sessions
        .iter()
        .map(|session| {
            let group_working = group_working && !held.contains(&session.row.sidebar_session_id);
            section_of(&session.row, enable_parking, group_working, now_ms)
        })
        .collect();
    let coordinators: HashMap<&SessionKey, usize> = sessions
        .iter()
        .enumerate()
        .filter(|(_, session)| session.row.is_coordinator)
        .filter_map(|(index, session)| session.row.key.as_ref().map(|key| (key, index)))
        .collect();
    if coordinators.is_empty() {
        return (sessions, base_sections);
    }
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut parent_of: Vec<Option<usize>> = vec![None; sessions.len()];
    for (index, session) in sessions.iter().enumerate() {
        if !is_nestable_thread(session, enable_parking, now_ms) {
            continue;
        }
        let Some(parent) = session
            .row
            .coordinator_parent
            .as_ref()
            .and_then(|parent| coordinators.get(parent).copied())
            .filter(|parent| *parent != index)
        else {
            continue;
        };
        children.entry(parent).or_default().push(index);
        parent_of[index] = Some(parent);
    }
    if children.is_empty() {
        return (sessions, base_sections);
    }
    // A cycle (two coordinators that are each other's thread) would never reach a root; such rows
    // stay top-level.
    let reaches_root = |parent_of: &[Option<usize>], mut index: usize| {
        for _ in 0..=parent_of.len() {
            match parent_of[index] {
                Some(parent) => index = parent,
                None => return true,
            }
        }
        false
    };
    for index in 0..sessions.len() {
        if parent_of[index].is_some() && !reaches_root(&parent_of, index) {
            parent_of[index] = None;
        }
    }
    children.retain(|_, list| {
        list.retain(|child| parent_of[*child].is_some());
        !list.is_empty()
    });
    // The thread the user is looking at is never tucked away, however long it has been idle.
    let is_older: Vec<bool> = sessions
        .iter()
        .map(|session| {
            !session.is_focused && !session.is_visible && !is_recent_thread(&session.row, now_ms)
        })
        .collect();
    for list in children.values_mut() {
        list.sort_by_key(|child| is_older[*child]);
    }
    let shows_older: Vec<bool> = sessions
        .iter()
        .map(|session| older_shown.contains(&session.row.sidebar_session_id))
        .collect();

    let is_collapsed: Vec<bool> = sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            children.contains_key(&index) && collapsed.contains(&session.row.sidebar_session_id)
        })
        .collect();

    /// Where one row is emitted: its index, depth, whether its tree line ends at it, the tree
    /// lines that run past it and the ones it hands to its threads, its section, whether a folded
    /// coordinator hides it, and whether it waits behind its coordinator's "N older threads" row.
    struct Placed {
        index: usize,
        depth: u8,
        last: bool,
        passing_lines: u8,
        passing_lines_below: u8,
        section: SectionId,
        folded: bool,
        older_hidden: bool,
    }
    /// What `emit` reads about every row.
    struct Tree<'a> {
        children: &'a HashMap<usize, Vec<usize>>,
        is_collapsed: &'a [bool],
        is_older: &'a [bool],
        shows_older: &'a [bool],
    }
    let mut order: Vec<Placed> = Vec::with_capacity(sessions.len());
    fn emit(mut placed: Placed, tree: &Tree<'_>, order: &mut Vec<Placed>) {
        let (index, depth, section) = (placed.index, placed.depth, placed.section);
        let folded = placed.folded || tree.is_collapsed[index];
        let passing_lines = passing_lines_below(placed.passing_lines, depth, placed.last);
        placed.passing_lines_below = passing_lines;
        let list = tree.children.get(&index).map_or(&[][..], Vec::as_slice);
        let tucks_older = !tree.shows_older[index];
        let hidden = |child: usize| tucks_older && tree.is_older[child];
        let drawn = list.iter().filter(|child| !hidden(**child)).count();
        // Past the depth limit the row's threads are drawn at its own level, so the line it would
        // end runs on to them; the line the threads take is still the one handed down above.
        if depth == MAX_DEPTH && !tree.is_collapsed[index] && drawn > 0 {
            placed.last = false;
        }
        order.push(placed);
        if !list.is_empty() {
            for (position, child) in list.iter().enumerate() {
                emit(
                    Placed {
                        index: *child,
                        depth: (depth + 1).min(MAX_DEPTH),
                        last: position + 1 == drawn,
                        passing_lines,
                        passing_lines_below: 0,
                        section,
                        folded: folded || hidden(*child),
                        older_hidden: hidden(*child),
                    },
                    tree,
                    order,
                );
            }
        }
    }
    let tree = Tree {
        children: &children,
        is_collapsed: &is_collapsed,
        is_older: &is_older,
        shows_older: &shows_older,
    };
    for index in 0..sessions.len() {
        if parent_of[index].is_none() {
            emit(
                Placed {
                    index,
                    depth: 0,
                    last: false,
                    passing_lines: 0,
                    passing_lines_below: 0,
                    section: base_sections[index],
                    folded: false,
                    older_hidden: false,
                },
                &tree,
                &mut order,
            );
        }
    }

    // The drawn row that stands for the focused thread when a folded coordinator hides it: its
    // nearest ancestor that is drawn.
    let mut folded_of = vec![false; sessions.len()];
    for placed in &order {
        folded_of[placed.index] = placed.folded;
    }
    let holds_active = sessions
        .iter()
        .position(|session| session.is_focused)
        .filter(|focused| folded_of[*focused])
        .and_then(|focused| {
            let mut ancestor = parent_of[focused]?;
            while folded_of[ancestor] {
                ancestor = parent_of[ancestor]?;
            }
            Some(ancestor)
        });

    // A folded coordinator's stack: its own status and every thread under it.
    let folded_status: HashMap<usize, FoldedStatus> = (0..sessions.len())
        .filter(|index| is_collapsed[*index])
        .map(|index| {
            let mut status = FoldedStatus::default();
            let mut stack = vec![index];
            while let Some(next) = stack.pop() {
                status.add(&sessions[next].row);
                stack.extend(children.get(&next).into_iter().flatten().copied());
            }
            (index, status)
        })
        .collect();

    let mut sessions: Vec<Option<SessionView>> = sessions.into_iter().map(Some).collect();
    let mut nested = Vec::with_capacity(order.len());
    let mut sections = Vec::with_capacity(order.len());
    for Placed {
        index,
        depth,
        last,
        passing_lines,
        passing_lines_below,
        section,
        folded,
        older_hidden,
    } in order
    {
        let Some(mut session) = sessions[index].take() else {
            continue;
        };
        session.nesting = RowNesting {
            depth,
            last_child: depth > 0 && last,
            passing_lines,
            lines_below: passing_lines_below,
            thread_count: children
                .get(&index)
                .map_or(0, |list| u16::try_from(list.len()).unwrap_or(u16::MAX)),
            collapsed: is_collapsed[index],
            folded,
            older_threads: children.get(&index).map_or(0, |list| {
                u16::try_from(list.iter().filter(|child| is_older[**child]).count())
                    .unwrap_or(u16::MAX)
            }),
            older_threads_shown: shows_older[index],
            older_hidden,
            contains_active_session: holds_active == Some(index),
            folded_status: folded_status.get(&index).copied().unwrap_or_default(),
            ..RowNesting::default()
        };
        nested.push(session);
        sections.push(section);
    }
    (nested, sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::MachineId;

    fn key(session_id: &str) -> SessionKey {
        SessionKey {
            machine: MachineId::Local,
            project_id: "p1".to_string(),
            session_id: session_id.to_string(),
        }
    }

    fn row(session_id: &str, activity: &str, parent: Option<&str>) -> SessionView {
        SessionView {
            row: Arc::new(SessionRow {
                sidebar_session_id: format!("combined-session:p1:{session_id}"),
                key: Some(key(session_id)),
                activity: activity.to_string(),
                is_coordinator: parent.is_none(),
                coordinator_parent: parent.map(key),
                ..SessionRow::default()
            }),
            is_focused: false,
            is_visible: false,
            is_multi_selected: false,
            nesting: RowNesting::default(),
        }
    }

    fn nest(sessions: Vec<SessionView>, collapsed: &[&str]) -> Vec<SessionView> {
        let collapsed = collapsed
            .iter()
            .map(|id| format!("combined-session:p1:{id}"))
            .collect();
        nest_threads(
            sessions,
            false,
            false,
            &HashSet::new(),
            &collapsed,
            &BTreeSet::new(),
            0,
        )
        .0
    }

    /// A folded coordinator carries the dots of the threads its fold hides (a nested thread's
    /// too) and its own; unfolded, it carries none and each thread draws its own.
    #[test]
    fn folded_coordinator_carries_its_hidden_threads_dots() {
        let sessions = || {
            vec![
                row("lead", "idle", None),
                row("working", "working", Some("lead")),
                {
                    let mut done = row("done", "attention", Some("lead"));
                    Arc::make_mut(&mut done.row).is_coordinator = true;
                    done
                },
                {
                    let mut asking = row("asking", "attention", Some("done"));
                    Arc::make_mut(&mut asking.row).pending_question_count = 1;
                    asking
                },
            ]
        };
        let folded = nest(sessions(), &["lead"]);
        let lead = folded
            .iter()
            .find(|session| session.row.sidebar_session_id.ends_with(":lead"))
            .unwrap();
        assert_eq!(
            lead.nesting.folded_status,
            FoldedStatus {
                working: true,
                attention: true,
                background_work: false,
                question: true,
                approval: false,
            }
        );
        // A thread waiting on the user (gxserver's `waiting`, a permission prompt) is the pink
        // needs-you dot, not the stack's blue.
        let mut waiting = sessions();
        Arc::make_mut(&mut waiting[2].row).thread_state = Some("waiting".to_string());
        Arc::make_mut(&mut waiting[3].row).pending_question_count = 0;
        Arc::make_mut(&mut waiting[3].row).activity = "idle".to_string();
        let waiting = nest(waiting, &["lead"]);
        assert!(!waiting[0].nesting.folded_status.attention);
        assert!(waiting[0].nesting.folded_status.question);
        // A thread waiting for approval is the red approval dot instead of the pink one.
        let mut approval = sessions();
        Arc::make_mut(&mut approval[3].row).pending_question_count = 0;
        Arc::make_mut(&mut approval[3].row).awaiting_approval = true;
        let approval = nest(approval, &["lead"]);
        assert!(approval[0].nesting.folded_status.approval);
        assert!(!approval[0].nesting.folded_status.question);

        assert!(folded
            .iter()
            .filter(|session| session.nesting.depth > 0)
            .all(|session| session.nesting.folded));

        let open = nest(sessions(), &[]);
        assert!(open
            .iter()
            .all(|session| !session.nesting.folded_status.any()));
    }

    /// A session waiting for approval counts on every orchestrator above it, its own and a nested
    /// orchestrator's parent alike; the working and waiting counts stay per direct thread.
    #[test]
    fn approval_counts_on_every_orchestrator_above_it() {
        let rows = |approval: bool| {
            let mut sub = row("sub", "working", Some("lead")).row;
            Arc::make_mut(&mut sub).is_coordinator = true;
            Arc::make_mut(&mut sub).thread_state = Some("working".to_string());
            let mut stuck = row("stuck", "attention", Some("sub")).row;
            Arc::make_mut(&mut stuck).thread_state = Some("waiting".to_string());
            Arc::make_mut(&mut stuck).awaiting_approval = approval;
            vec![row("lead", "idle", None).row, sub, stuck]
        };
        let tallies = tally_threads(rows(true).iter(), 0);
        assert_eq!(tallies[&key("lead")].approval, 1);
        assert_eq!(tallies[&key("lead")].working, 1);
        assert_eq!(tallies[&key("sub")].approval, 1);
        assert_eq!(tallies[&key("sub")].waiting, 1);
        let tallies = tally_threads(rows(false).iter(), 0);
        assert_eq!(tallies[&key("lead")].approval, 0);
        assert_eq!(tallies[&key("sub")].approval, 0);
    }

    /// A nested coordinator that is the last thread of its parent but has threads of its own drawn
    /// flat beside it at the depth limit runs its line on to them; its last thread ends the line.
    /// Folded, it ends the line itself.
    #[test]
    fn last_thread_at_depth_limit_runs_its_line_on_to_its_flat_threads() {
        let coordinator = |session_id: &str, parent: &str| {
            let mut session = row(session_id, "working", Some(parent));
            Arc::make_mut(&mut session.row).is_coordinator = true;
            session
        };
        let sessions = || {
            vec![
                row("lead", "idle", None),
                coordinator("l1", "lead"),
                coordinator("l1x", "l1"),
                row("t3", "working", Some("l1x")),
                coordinator("l1xx", "l1x"),
                row("t4", "working", Some("l1xx")),
            ]
        };
        let nesting = |rows: &[SessionView], id: &str| {
            rows.iter()
                .find(|session| session.row.sidebar_session_id.ends_with(&format!(":{id}")))
                .unwrap()
                .nesting
        };

        let open = nest(sessions(), &[]);
        let l1xx = nesting(&open, "l1xx");
        let t4 = nesting(&open, "t4");
        assert_eq!((l1xx.depth, t4.depth), (MAX_DEPTH, MAX_DEPTH));
        assert!(!l1xx.last_child);
        assert!(t4.last_child);
        // Nothing follows l1x's tree, so no line of its level runs past t4 or l1xx's older row.
        assert_eq!(t4.passing_lines & (1 << MAX_DEPTH), 0);
        assert_eq!(l1xx.passing_lines_below() & (1 << MAX_DEPTH), 0);

        let folded = nest(sessions(), &["l1xx"]);
        assert!(nesting(&folded, "l1xx").last_child);
    }
}
