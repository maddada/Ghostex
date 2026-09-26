//! The display order of a group's rows, and which section heading each row sits under.
//!
//! SEE-ALSO: packages/shared/active-sessions-sort.ts, packages/shared/session-drafts.ts,
//! packages/shared/session-snooze.ts, packages/core-ui/sidebar-app/project-session-section-model.ts.

use super::inputs::{SectionId, SessionSortMode};
use super::view::SessionRow;

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

/// `isSidebarDraftSectionSession`.
pub(crate) fn is_draft_section_session(row: &SessionRow, now_ms: u64) -> bool {
    row.is_draft && !row.is_pinned && row.has_composer_draft && !is_new_session(row, now_ms)
}

/// `getProjectSessionSection`.
pub(crate) fn section_of(row: &SessionRow, enable_parking: bool, now_ms: u64) -> SectionId {
    if row.is_browser {
        return SectionId::Browser;
    }
    if is_snoozed(row, now_ms) {
        return SectionId::Snoozed;
    }
    if enable_parking && row.is_parked {
        return SectionId::Parked;
    }
    if is_draft_section_session(row, now_ms) {
        return SectionId::Drafts;
    }
    if row.is_pinned {
        SectionId::Pinned
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
    ]
    .into_iter()
    .flatten()
    .filter(|deadline| *deadline > now)
    .map(|deadline| deadline as u64)
    .min()
}

/// `createDisplaySessionLayout` for one group: browser rows first, then terminal rows, each split
/// into pinned, drafts, new, the rest, parked, and snoozed.
pub(crate) fn order_rows_for_display(
    rows: &[std::sync::Arc<SessionRow>],
    sort_mode: SessionSortMode,
    enable_parking: bool,
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
        now_ms,
    );
    ordered.extend(order_kind(
        rows,
        &terminal,
        sort_by_last_activity,
        enable_parking,
        now_ms,
    ));
    ordered
}

fn order_kind(
    rows: &[std::sync::Arc<SessionRow>],
    indices: &[usize],
    sort_by_last_activity: bool,
    enable_parking: bool,
    now_ms: u64,
) -> Vec<usize> {
    let mut pinned: Vec<usize> = Vec::new();
    let mut drafts: Vec<usize> = Vec::new();
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
        } else if !row.is_browser && is_draft_section_session(row, now_ms) {
            drafts.push(*index);
        } else if row.is_pinned {
            pinned.push(*index);
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
        sort_by_activity(rows, &mut other);
    }
    sort_parked_by_last_activity(rows, &mut parked);

    let mut ordered = pinned;
    ordered.extend(drafts);
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
/// CDXC:StateSync 2026-09-20 SEE-ALSO:
/// packages/shared/active-sessions-sort.ts breaks the tie with `localeCompare`; see the note in projects.rs on why byte order was the same order in the desktop's QuickJS and is not in V8.
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
