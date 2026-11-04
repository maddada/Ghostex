//! The display order of a group's rows, and which section heading each row sits under.
//!
//! CDXC:Drafts 2026-11-02 DECISION:
//! User: "please for all empty sessions / draft sessions move those to the draft area and always expand that when a new session goes there (i know draft was supposed to be for sessions that have content that were left, but i want new sessions even if empty, to go there from now on)". Every session nobody has sent a message to yet (gxserver's draft marker), empty or holding unsent text, sits in the DRAFTS section below Pinned and above Sessions from the moment it is created, and leaves it for its usual place once its first message is sent or the agent starts working. A session arriving in Drafts opens the heading (gx_store/draft_arrivals.rs). This supersedes the 2026-09-15 decision that kept new sessions at the top of Sessions for 10 minutes and moved only drafts holding text into Drafts.
//! User (2026-09-15, still in force): pinning a draft moves it into Pinned, retaining its unsent text.
//!
//! Ported from `active-sessions-sort.ts`, `session-drafts.ts` and `session-snooze.ts` in
//! packages/shared and `sidebar-app/project-session-section-model.ts` in packages/core-ui (all
//! deleted 2026-10-01; see git history).
//!
//! SEE-ALSO: apps/mobile/app/src/contract/grouping.ts mirrors the parked ordering.

use std::collections::{HashMap, HashSet};

use super::inputs::{SectionId, SessionSortMode};
use super::view::{SessionRow, SessionView};

/// A new session leads the list for ten minutes.
pub(crate) const NEW_SESSION_PRIORITY_MS: i64 = 10 * 60 * 1_000;

/// `isNewSidebarSession`.
pub(crate) fn is_new_session(row: &SessionRow, now_ms: u64) -> bool {
    row.timing
        .created_ms
        .is_some_and(|created| created + NEW_SESSION_PRIORITY_MS > now_ms as i64)
}

/// `isSidebarSessionSnoozed`, on the parsed wake time alone.
///
/// CDXC:Sessions 2026-09-20 WHY:
/// One function, because three things have to agree about the exact moment a snooze ends: the
/// section a row is drawn in, whether its menu offers Snooze or Unsnooze, and (M5) the action
/// surface. A second copy of `wake_at > now` in the menu builder was the shape that lets the two
/// drift by a tick, which would draw a row in the Snoozed section whose menu already offers the
/// presets. The boundary is strictly greater: gxserver keeps `snoozedUntil` on the row until its
/// own sweep clears it, so a wake time in the past must never hide a session.
pub fn session_is_snoozed(snoozed_until_ms: Option<i64>, now_ms: u64) -> bool {
    snoozed_until_ms.is_some_and(|wake_at| wake_at > now_ms as i64)
}

/// `isSidebarSessionSnoozed` for a drawn row.
pub(crate) fn is_snoozed(row: &SessionRow, now_ms: u64) -> bool {
    session_is_snoozed(row.timing.snoozed_until_ms, now_ms)
}

/// `isSidebarDraftSectionSession`: a never-sent session that is not pinned (CDXC:Drafts 2026-11-02
/// above).
pub(crate) fn is_draft_section_session(row: &SessionRow) -> bool {
    row.is_draft && !row.is_pinned
}

/// Whether a row sits in its project's Working section while Group working sessions is on: its
/// agent is working, and nothing on it is waiting for the user (an unanswered question's pink fill,
/// or a failed model change's red dot). A working row never has the blue attention dot. The rows
/// [`held_out_of_working`] names stay where they are whatever this says.
///
/// CDXC:Sidebar 2026-10-05 DECISION:
/// User: "Add a setting that hides sessions that are working status (should be in filter & view and also in settings, both toggle same thing)", then, replacing hiding: "For the hide working we should have a new section called working in the sidebar and minimize it by default basically unless it has pink or blue dot or it's not working anymore then it goes back to sessions section under the project". One setting, `groupWorkingSessions` (off by default), toggled from the More menu's Sort & Filter page and from Settings > Sidebar. Each project gets a Working heading between Drafts and Sessions, collapsed by default (and, like Drafts, Parked and Snoozed, collapsed again after a restart). Only rows that would otherwise sit in Sessions move there; pinned, draft, parked and snoozed rows keep their own headings. A row returns to Sessions on its own the moment it stops working or starts waiting on the user. The session you have open stays in Sessions while its agent works (decided after a live test showed the focused coordinator folding away into the collapsed heading), and so does every coordinator above it, so opening a thread never hides it inside its working coordinator's block.
pub(crate) fn is_grouped_working(row: &SessionRow) -> bool {
    !row.is_browser
        && row.activity == "working"
        && row.pending_question_count == 0
        && !row.awaiting_approval
        && !row.model_selection_failed
        && !row.program_status_failed
}

/// The rows Group working sessions leaves in their own section even while their agent works, by
/// sidebar row id: the focused session and the coordinators above it in this group
/// (CDXC:Sidebar 2026-10-05 on [`is_grouped_working`]). Empty when nothing in the group is focused.
pub(crate) fn held_out_of_working(sessions: &[SessionView]) -> HashSet<String> {
    let mut held = HashSet::new();
    let Some(focused) = sessions.iter().position(|session| session.is_focused) else {
        return held;
    };
    let by_key: HashMap<_, usize> = sessions
        .iter()
        .enumerate()
        .filter_map(|(index, session)| session.row.key.as_ref().map(|key| (key, index)))
        .collect();
    let mut index = focused;
    // Bounded by the group's size, so a coordinator cycle cannot loop.
    for _ in 0..sessions.len() {
        let row = &sessions[index].row;
        if !held.insert(row.sidebar_session_id.clone()) {
            break;
        }
        match row
            .coordinator_parent
            .as_ref()
            .and_then(|parent| by_key.get(parent))
        {
            Some(parent) => index = *parent,
            None => break,
        }
    }
    held
}

/// `getProjectSessionSection`, plus the Working heading. `group_working` is false for a row
/// [`held_out_of_working`] names.
pub(crate) fn section_of(
    row: &SessionRow,
    enable_parking: bool,
    group_working: bool,
    now_ms: u64,
) -> SectionId {
    if row.is_browser {
        return SectionId::Browser;
    }
    if is_snoozed(row, now_ms) {
        return SectionId::Snoozed;
    }
    if enable_parking && row.is_parked {
        return SectionId::Parked;
    }
    if is_draft_section_session(row) {
        return SectionId::Drafts;
    }
    if row.is_pinned {
        SectionId::Pinned
    } else if group_working && is_grouped_working(row) {
        SectionId::Working
    } else {
        SectionId::Sessions
    }
}

/// The next moment a row's section or order changes on its own.
pub(crate) fn row_deadline_ms(row: &SessionRow, now_ms: u64) -> Option<u64> {
    let now = now_ms as i64;
    [
        row.timing
            .created_ms
            .map(|created| created + NEW_SESSION_PRIORITY_MS),
        row.timing.snoozed_until_ms,
        super::threads::recent_thread_deadline_ms(row),
    ]
    .into_iter()
    .flatten()
    .filter(|deadline| *deadline > now)
    .map(|deadline| deadline as u64)
    .min()
}

/// `createDisplaySessionLayout` for one group: browser rows first, then terminal rows, each split
/// into pinned, drafts, working (while `group_working` is on, except the `held` rows), new, the
/// rest, parked, and snoozed.
pub(crate) fn order_rows_for_display(
    rows: &[std::sync::Arc<SessionRow>],
    sort_mode: SessionSortMode,
    enable_parking: bool,
    group_working: bool,
    held: &HashSet<String>,
    now_ms: u64,
) -> Vec<usize> {
    let mut browser: Vec<usize> = Vec::new();
    let mut terminal: Vec<usize> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.is_browser {
            browser.push(index);
        } else {
            terminal.push(index);
        }
    }
    let sort_by_last_activity = sort_mode == SessionSortMode::LastActivity;
    let mut ordered = order_kind(
        rows,
        &browser,
        sort_by_last_activity,
        enable_parking,
        group_working,
        held,
        now_ms,
    );
    ordered.extend(order_kind(
        rows,
        &terminal,
        sort_by_last_activity,
        enable_parking,
        group_working,
        held,
        now_ms,
    ));
    ordered
}

fn order_kind(
    rows: &[std::sync::Arc<SessionRow>],
    indices: &[usize],
    sort_by_last_activity: bool,
    enable_parking: bool,
    group_working: bool,
    held: &HashSet<String>,
    now_ms: u64,
) -> Vec<usize> {
    let mut pinned: Vec<usize> = Vec::new();
    let mut drafts: Vec<usize> = Vec::new();
    let mut working: Vec<usize> = Vec::new();
    let mut new_sessions: Vec<usize> = Vec::new();
    let mut other: Vec<usize> = Vec::new();
    let mut parked: Vec<usize> = Vec::new();
    let mut snoozed: Vec<usize> = Vec::new();
    for index in indices {
        let row = &rows[*index];
        if is_snoozed(row, now_ms) {
            snoozed.push(*index);
        } else if enable_parking && row.is_parked {
            parked.push(*index);
        } else if !row.is_browser && is_draft_section_session(row) {
            drafts.push(*index);
        } else if row.is_pinned {
            pinned.push(*index);
        } else if group_working
            && is_grouped_working(row)
            && !held.contains(&row.sidebar_session_id)
        {
            working.push(*index);
        } else if !row.is_browser && is_new_session(row, now_ms) {
            new_sessions.push(*index);
        } else {
            other.push(*index);
        }
    }
    // Newest first by creation time; an unparsable time reads as 0, and equal times keep the
    // existing order.
    let by_created_desc = |left: &usize, right: &usize| {
        let created = |index: &usize| rows[*index].timing.created_ms.unwrap_or(0);
        created(right).cmp(&created(left))
    };
    drafts.sort_by(by_created_desc);
    new_sessions.sort_by(by_created_desc);
    if sort_by_last_activity {
        sort_by_activity(rows, &mut working);
        sort_by_activity(rows, &mut other);
    }
    sort_parked_by_last_activity(rows, &mut parked);

    let mut ordered = pinned;
    ordered.extend(drafts);
    ordered.extend(working);
    ordered.extend(new_sessions);
    ordered.extend(other);
    ordered.extend(parked);
    ordered.extend(snoozed);
    ordered
}

/// `getSessionActivitySortPriority`.
fn activity_priority(row: &SessionRow) -> u8 {
    match row.activity.as_str() {
        "attention" => 2,
        "working" if is_meaningful_working_stint(row) => 1,
        _ => 0,
    }
}

/// `isMeaningfulWorkingStint`: a working row earns its priority once the activity clock has caught
/// up with the current stint. A row without both stamps keeps the priority at once.
fn is_meaningful_working_stint(row: &SessionRow) -> bool {
    match (
        row.timing.working_started_ms,
        row.timing.last_interaction_ms,
    ) {
        (Some(started), Some(recency)) => recency >= started,
        // A row without both stamps, or with one that does not parse, keeps the legacy priority.
        _ => true,
    }
}

/// `getSessionActivitySortTime`.
fn activity_sort_time(row: &SessionRow, priority: u8) -> i64 {
    if priority == 1 {
        if let Some(started) = row.timing.working_started_ms {
            return started;
        }
    }
    row.timing.last_interaction_ms.unwrap_or(0)
}

/// `sortSessionIdsByLastActivity`: attention first, then a meaningful working stint, then recency,
/// with the existing order as the tie-break.
fn sort_by_activity(rows: &[std::sync::Arc<SessionRow>], indices: &mut [usize]) {
    let mut keyed: Vec<(u8, i64, usize, usize)> = indices
        .iter()
        .enumerate()
        .map(|(position, index)| {
            let row = &rows[*index];
            let priority = activity_priority(row);
            (
                priority,
                activity_sort_time(row, priority),
                position,
                *index,
            )
        })
        .collect();
    keyed.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(left.2.cmp(&right.2))
    });
    for (slot, entry) in keyed.into_iter().enumerate() {
        indices[slot] = entry.3;
    }
}

/// `sortParkedSessionIdsByLastActivity`: latest active first, ties by sidebar session id.
///
/// CDXC:Sessions 2026-09-12 DECISION:
/// User: every sidebar and session list on GPUI, mobile and web shows parked sessions from latest active to oldest, regardless of the active-session sort mode.
/// Parked rows use the activity timestamp alone, without attention or working priority.
///
/// CDXC:StateSync 2026-09-20 SEE-ALSO:
/// packages/shared/active-sessions-sort.ts (deleted 2026-10-01) broke the tie with `localeCompare`; see the note in projects.rs on why byte order was the same order in the desktop's QuickJS and is not in V8.
fn sort_parked_by_last_activity(rows: &[std::sync::Arc<SessionRow>], indices: &mut [usize]) {
    indices.sort_by(|left, right| {
        let time = |index: &usize| rows[*index].timing.last_interaction_ms.unwrap_or(0);
        time(right).cmp(&time(left)).then_with(|| {
            rows[*left]
                .sidebar_session_id
                .cmp(&rows[*right].sidebar_session_id)
        })
    });
}
