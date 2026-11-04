//! The section headings of a project's session list.
//!
//! CDXC:Projects 2026-10-13 DECISION:
//! User: "remove this feature, no more show less / show more." Every project always shows its full session list: there is no Compact mode, no "Show all (N)" row, no Compact/Full row in the project menu and no Compact Session Rows setting. Collapsible headings (Pinned, Drafts, Sessions, Parked, ...) stay. This supersedes the 2026-09-12 Compact/Full decisions (and the 2026-05-16 Show more / Show less model before them); a saved set of Full projects is ignored.
//!
//! Ported from `project-session-sections.ts` in packages/core-ui (deleted 2026-10-01; see git
//! history).

use std::sync::Arc;

use super::inputs::{SectionCollapse, SectionId};
use super::view::{NeedsYou, SectionView, SessionRow, SessionView};

/// What `projectSessionSections` returns.
pub(crate) struct SectionLayout {
    pub(crate) sections: Vec<SectionView>,
}

/// Builds the headings of one group from its rows in display order and the section each row sits
/// in (`section_of`, except that a coordinator's threads sit in its section).
pub(crate) fn project_session_sections(
    sessions: &[SessionView],
    section_by_session: &[SectionId],
    is_active_group: bool,
    collapse: SectionCollapse,
) -> SectionLayout {
    // A group whose only section is Sessions draws no heading for it, so nothing could expand it
    // again: its stored collapse is ignored until another section shows up.
    let lone_sessions = section_by_session
        .iter()
        .all(|section| *section == SectionId::Sessions);
    // A thread under a folded coordinator is hidden like a row under a closed heading.
    let is_collapsed = |index: usize| {
        sessions[index].nesting.folded
            || (!lone_sessions && collapse.get(section_by_session[index]))
    };
    // Membership is asked once per row per section, so it is a flag per row rather than a scan.
    let is_visible: Vec<bool> = (0..sessions.len())
        .map(|index| !is_collapsed(index))
        .collect();
    let sections = SectionId::ORDER
        .into_iter()
        .filter_map(|id| {
            let members: Vec<usize> = (0..sessions.len())
                .filter(|index| section_by_session[*index] == id)
                .collect();
            if members.is_empty() {
                return None;
            }
            let row = |index: &usize| -> &Arc<SessionRow> { &sessions[*index].row };
            Some(SectionView {
                id,
                collapsed: !lone_sessions && collapse.get(id),
                count: members.len(),
                contains_active_session: is_active_group
                    && members.iter().any(|index| sessions[*index].is_focused),
                working_count: members
                    .iter()
                    .filter(|index| row(index).activity == "working")
                    .count(),
                attention_count: members
                    .iter()
                    .filter(|index| row(index).needs_you() == Some(NeedsYou::Attention))
                    .count(),
                background_work_count: members
                    .iter()
                    .filter(|index| row(index).shows_background_work())
                    .count(),
                question_count: members
                    .iter()
                    .filter(|index| row(index).needs_you() == Some(NeedsYou::Question))
                    .count(),
                approval_count: members
                    .iter()
                    .filter(|index| row(index).needs_you() == Some(NeedsYou::Approval))
                    .count(),
                session_ids: members
                    .iter()
                    .filter(|index| is_visible[**index])
                    .map(|index| row(index).sidebar_session_id.clone())
                    .collect(),
                member_ids: if id == SectionId::Parked {
                    members
                        .iter()
                        .map(|index| row(index).sidebar_session_id.clone())
                        .collect()
                } else {
                    Vec::new()
                },
            })
        })
        .collect();
    SectionLayout { sections }
}
