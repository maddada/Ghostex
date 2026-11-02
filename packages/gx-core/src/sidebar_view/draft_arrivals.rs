//! Which sessions just arrived in Drafts, so the sidebar can open the heading for them and scroll
//! the new row into view.
//!
//! CDXC:Drafts 2026-11-02 DECISION:
//! User: "i just hit cmd + shift + o and created a new sessino but the sessions area was collapsed in the sidebar. please for all empty sessions / draft sessions move those to the draft area and always expand that when a new session goes there". A session arriving in Drafts, however it was made (New Agent Session, the New Thread picker, New session, the CLI), opens its Drafts heading in the Project sidebar and the Drafts shelf in the Unified sidebar if they were collapsed, and once it is the open session its row is scrolled into view. Only an arrival opens it: a user who collapses Drafts afterwards keeps it collapsed until the next new session arrives.
//!
//! CDXC:Drafts 2026-11-02 WHY:
//! "Arrived" means a draft row the previous list did not hold that was created in the last two minutes and after the tracker first saw a list. The first list after a launch only records what is there, and a draft created before it never counts (the first ready list can still be empty while the rows load), so a restart or a page load never opens Drafts; the creation clock also keeps a row that only came into view (another Space or machine tab selected, a draft unpinned or woken from a snooze) from counting as new. The open session is the creating client's own; a session another client created opens the heading but is not scrolled to or selected. The focus can land a moment after the row does, so an arrival waits a few seconds for it.

use std::collections::HashSet;

use crate::sidebar_ui::SidebarUiIntent;

use super::inputs::{SectionId, SidebarUiState};
use super::ordering::section_of;
use super::view::SidebarView;

/// How recently a draft must have been created to count as an arrival.
pub const DRAFT_ARRIVAL_WINDOW_MS: i64 = 2 * 60 * 1_000;

/// How much older than the first list a draft may be and still count, for the clock difference
/// between gxserver, which stamps the creation, and this client.
pub const DRAFT_CLOCK_SLACK_MS: i64 = 5_000;

/// How long an arrival waits for its session to become the open one before it is scrolled to.
pub const DRAFT_FOCUS_WAIT_MS: u64 = 15_000;

/// A row in Drafts, as the arrival tracker reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftRow {
    pub sidebar_session_id: String,
    /// The id the row's group keeps its headings under; empty for the Unified sidebar's one shelf.
    pub storage_id: String,
    pub created_ms: Option<i64>,
}

/// A draft that just arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftArrival {
    pub sidebar_session_id: String,
    pub storage_id: String,
}

/// Remembers the drafts of the previous list, and the arrivals still waiting to be opened.
#[derive(Clone, Debug, Default)]
pub struct DraftArrivals {
    /// `None` until the first list has been seen.
    known: Option<HashSet<String>>,
    /// When the first list was seen.
    started_ms: i64,
    awaiting_focus: Vec<(String, u64)>,
}

impl DraftArrivals {
    /// Reads the drafts of the list as it stands now and returns those that just arrived.
    pub fn observe(&mut self, drafts: &[DraftRow], now_ms: u64) -> Vec<DraftArrival> {
        let current: HashSet<String> = drafts
            .iter()
            .map(|draft| draft.sidebar_session_id.clone())
            .collect();
        let Some(known) = self.known.replace(current) else {
            self.started_ms = now_ms as i64;
            return Vec::new();
        };
        let started_ms = self.started_ms;
        let arrivals: Vec<DraftArrival> = drafts
            .iter()
            .filter(|draft| !known.contains(&draft.sidebar_session_id))
            .filter(|draft| {
                draft.created_ms.is_some_and(|created| {
                    created + DRAFT_ARRIVAL_WINDOW_MS > now_ms as i64
                        && created + DRAFT_CLOCK_SLACK_MS >= started_ms
                })
            })
            .map(|draft| DraftArrival {
                sidebar_session_id: draft.sidebar_session_id.clone(),
                storage_id: draft.storage_id.clone(),
            })
            .collect();
        self.awaiting_focus
            .retain(|(_, deadline)| *deadline > now_ms);
        for arrival in &arrivals {
            self.awaiting_focus
                .retain(|(id, _)| *id != arrival.sidebar_session_id);
            self.awaiting_focus.push((
                arrival.sidebar_session_id.clone(),
                now_ms + DRAFT_FOCUS_WAIT_MS,
            ));
        }
        arrivals
    }

    /// The arrival to scroll to: the open session, when it is one still waiting. Each arrival is
    /// answered once.
    pub fn take_focused(&mut self, focused: Option<&str>, now_ms: u64) -> Option<String> {
        let focused = focused?;
        let index = self
            .awaiting_focus
            .iter()
            .position(|(id, deadline)| id == focused && *deadline > now_ms)?;
        Some(self.awaiting_focus.remove(index).0)
    }

    /// Forgets everything, so the next list is only recorded (a switch between the two sidebars).
    pub fn reset(&mut self) {
        self.known = None;
        self.awaiting_focus.clear();
    }

    /// Whether the next list will only be recorded.
    pub fn is_reset(&self) -> bool {
        self.known.is_none()
    }
}

/// Every row the Project sidebar's list puts under a Drafts heading.
pub fn project_draft_rows(view: &SidebarView, enable_parking: bool, now_ms: u64) -> Vec<DraftRow> {
    view.groups
        .iter()
        .flat_map(|group| {
            group.core.sessions.iter().filter_map(move |session| {
                (section_of(&session.row, enable_parking, false, now_ms) == SectionId::Drafts).then(
                    || DraftRow {
                        sidebar_session_id: session.row.sidebar_session_id.clone(),
                        storage_id: group.core.storage_id.clone(),
                        created_ms: session.row.timing.created_ms,
                    },
                )
            })
        })
        .collect()
}

/// The changes that open the Drafts heading of every group an arrival landed in, where it is
/// collapsed.
pub fn open_draft_headings(arrivals: &[DraftArrival], ui: &SidebarUiState) -> Vec<SidebarUiIntent> {
    let mut opened: Vec<&str> = Vec::new();
    arrivals
        .iter()
        .filter_map(|arrival| {
            let storage_id = arrival.storage_id.as_str();
            if opened.contains(&storage_id) {
                return None;
            }
            opened.push(storage_id);
            let collapsed = ui
                .collapse
                .section_collapse
                .get(storage_id)
                .copied()
                .unwrap_or_default()
                .get(SectionId::Drafts);
            collapsed.then(|| SidebarUiIntent::ToggleSection {
                storage_id: storage_id.to_string(),
                section: SectionId::Drafts,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::sidebar_view::inputs::SectionCollapse;
    use crate::sidebar_view::ordering::order_rows_for_display;
    use crate::sidebar_view::view::SessionRow;
    use crate::sidebar_view::SessionSortMode;

    const NOW: u64 = 10_000_000;

    fn draft(id: &str, storage: &str, created: i64) -> DraftRow {
        DraftRow {
            sidebar_session_id: id.to_string(),
            storage_id: storage.to_string(),
            created_ms: Some(created),
        }
    }

    fn row(id: &str, is_draft: bool, text: bool, created: i64) -> SessionRow {
        let mut row = SessionRow {
            sidebar_session_id: id.to_string(),
            is_draft,
            has_composer_draft: text,
            ..SessionRow::default()
        };
        row.timing.created_ms = Some(created);
        row
    }

    #[test]
    fn a_new_empty_session_lands_in_drafts_at_once() {
        // Created a second ago, nothing typed: Drafts, not the top of Sessions.
        let empty = row("empty", true, false, NOW as i64 - 1_000);
        assert_eq!(section_of(&empty, false, false, NOW), SectionId::Drafts);
        // An old draft holding text is still a draft.
        let typed = row("typed", true, true, 0);
        assert_eq!(section_of(&typed, false, false, NOW), SectionId::Drafts);
    }

    #[test]
    fn a_sent_session_leaves_drafts_and_a_pinned_draft_stays_pinned() {
        let sent = row("sent", false, false, NOW as i64 - 1_000);
        assert_eq!(section_of(&sent, false, false, NOW), SectionId::Sessions);
        let mut pinned = row("pinned", true, false, NOW as i64 - 1_000);
        pinned.is_pinned = true;
        assert_eq!(section_of(&pinned, false, false, NOW), SectionId::Pinned);
        let mut parked = row("parked", true, false, NOW as i64 - 1_000);
        parked.is_parked = true;
        assert_eq!(section_of(&parked, true, false, NOW), SectionId::Parked);
    }

    #[test]
    fn drafts_are_ordered_newest_first_above_sessions() {
        let rows: Vec<Arc<SessionRow>> = vec![
            Arc::new(row("sent", false, false, 50)),
            Arc::new(row("older", true, false, 100)),
            Arc::new(row("newer", true, true, 200)),
        ];
        let order = order_rows_for_display(
            &rows,
            SessionSortMode::LastActivity,
            false,
            false,
            &HashSet::new(),
            NOW,
        );
        let ids: Vec<&str> = order
            .iter()
            .map(|index| rows[*index].sidebar_session_id.as_str())
            .collect();
        assert_eq!(ids, ["newer", "older", "sent"]);
    }

    #[test]
    fn the_first_list_is_only_recorded() {
        let mut arrivals = DraftArrivals::default();
        let recent = NOW as i64 - 1_000;
        assert!(arrivals.observe(&[draft("a", "p", recent)], NOW).is_empty());
        assert!(!arrivals.is_reset());
    }

    #[test]
    fn a_draft_the_previous_list_lacked_arrives_once() {
        let mut arrivals = DraftArrivals::default();
        let recent = NOW as i64 - 1_000;
        arrivals.observe(&[draft("a", "p", recent)], NOW);
        let both = [draft("a", "p", recent), draft("b", "q", recent)];
        assert_eq!(
            arrivals.observe(&both, NOW),
            [DraftArrival {
                sidebar_session_id: "b".to_string(),
                storage_id: "q".to_string(),
            }]
        );
        // The next list holds it already.
        assert!(arrivals.observe(&both, NOW + 10).is_empty());
    }

    #[test]
    fn a_draft_made_before_the_first_list_is_not_an_arrival() {
        // The first ready list is empty while the rows load; the draft made before it then lands.
        let mut arrivals = DraftArrivals::default();
        arrivals.observe(&[], NOW);
        let before = NOW as i64 - DRAFT_CLOCK_SLACK_MS - 1;
        assert!(arrivals
            .observe(&[draft("a", "p", before)], NOW + 500)
            .is_empty());
    }

    #[test]
    fn an_old_draft_that_only_came_into_view_is_not_an_arrival() {
        let mut arrivals = DraftArrivals::default();
        arrivals.observe(&[], NOW);
        let old = NOW as i64 - DRAFT_ARRIVAL_WINDOW_MS - 1;
        assert!(arrivals.observe(&[draft("a", "p", old)], NOW).is_empty());
    }

    #[test]
    fn an_arrival_is_scrolled_to_once_it_is_the_open_session() {
        let mut arrivals = DraftArrivals::default();
        arrivals.observe(&[], NOW);
        arrivals.observe(&[draft("a", "p", NOW as i64)], NOW);
        // Another session is open: nothing yet.
        assert_eq!(arrivals.take_focused(Some("other"), NOW + 100), None);
        assert_eq!(
            arrivals.take_focused(Some("a"), NOW + 200).as_deref(),
            Some("a")
        );
        assert_eq!(arrivals.take_focused(Some("a"), NOW + 300), None);
    }

    #[test]
    fn an_arrival_that_never_opens_stops_waiting() {
        let mut arrivals = DraftArrivals::default();
        arrivals.observe(&[], NOW);
        arrivals.observe(&[draft("a", "p", NOW as i64)], NOW);
        assert_eq!(
            arrivals.take_focused(Some("a"), NOW + DRAFT_FOCUS_WAIT_MS),
            None
        );
    }

    #[test]
    fn an_arrival_opens_a_collapsed_drafts_heading_once_per_group() {
        let mut ui = SidebarUiState::default();
        // Drafts starts collapsed in every group; "open" was expanded by the user.
        let mut expanded = SectionCollapse::default();
        expanded.set(SectionId::Drafts, false);
        ui.collapse
            .section_collapse
            .insert("open".to_string(), expanded);
        let arrivals = [
            DraftArrival {
                sidebar_session_id: "a".to_string(),
                storage_id: "p".to_string(),
            },
            DraftArrival {
                sidebar_session_id: "b".to_string(),
                storage_id: "p".to_string(),
            },
            DraftArrival {
                sidebar_session_id: "c".to_string(),
                storage_id: "open".to_string(),
            },
        ];
        assert_eq!(
            open_draft_headings(&arrivals, &ui),
            [SidebarUiIntent::ToggleSection {
                storage_id: "p".to_string(),
                section: SectionId::Drafts,
            }]
        );
    }

    #[test]
    fn collapsing_drafts_after_an_arrival_sticks_until_the_next_one() {
        let mut arrivals = DraftArrivals::default();
        let recent = NOW as i64 - 1_000;
        arrivals.observe(&[], NOW);
        let first = arrivals.observe(&[draft("a", "p", recent)], NOW);
        // The heading is collapsed again by the user: the same list opens nothing.
        let ui = SidebarUiState::default();
        assert_eq!(open_draft_headings(&first, &ui).len(), 1);
        let again = arrivals.observe(&[draft("a", "p", recent)], NOW + 1_000);
        assert!(open_draft_headings(&again, &ui).is_empty());
        // The next new session opens it again.
        let next = arrivals.observe(
            &[draft("a", "p", recent), draft("b", "p", recent)],
            NOW + 2_000,
        );
        assert_eq!(open_draft_headings(&next, &ui).len(), 1);
    }
}
