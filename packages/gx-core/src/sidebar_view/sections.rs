//! The section headings of a project's session list, and the compact list rule.
//!
//! SEE-ALSO: packages/core-ui/sidebar-app/project-session-sections.ts and
//! packages/core-ui/project-session-list-toggle.ts.

use std::sync::Arc;

use super::inputs::{SectionCollapse, SectionId};
use super::ordering::section_of;
use super::view::{SectionView, SessionRow, SessionView};

/// What `projectSessionSections` returns beside the sections themselves.
pub(crate) struct SectionLayout {
    pub(crate) sections: Vec<SectionView>,
    pub(crate) show_list_toggle: bool,
    pub(crate) hidden_session_count: usize,
}

/// Builds the headings of one group from its rows in display order.
pub(crate) fn project_session_sections(
    sessions: &[SessionView],
    is_active_group: bool,
    is_project_group: bool,
    collapse: SectionCollapse,
    expanded: bool,
    compact_count: u32,
    enable_parking: bool,
    now_ms: u64,
) -> SectionLayout {
    let section_by_session: Vec<SectionId> = sessions
        .iter()
        .map(|session| section_of(&session.row, enable_parking, now_ms))
        .collect();
    // A group whose only section is Sessions draws no heading for it, so nothing could expand it
    // again: its stored collapse is ignored until another section shows up.
    let lone_sessions = section_by_session
        .iter()
        .all(|section| *section == SectionId::Sessions);
    let is_collapsed = |index: usize| !lone_sessions && collapse.get(section_by_session[index]);
    let countable: Vec<usize> = (0..sessions.len())
        .filter(|index| !is_collapsed(*index))
        .collect();
    let compact_count = compact_count.clamp(1, 50) as usize;
    let visible: Vec<usize> = if !is_project_group || expanded || countable.len() <= compact_count {
        countable.clone()
    } else {
        countable[..compact_count].to_vec()
    };
    // Membership is asked once per row per section, so it is a flag per row rather than a scan.
    let mut is_visible = vec![false; sessions.len()];
    for index in &visible {
        is_visible[*index] = true;
    }
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
                    .filter(|index| {
                        row(index).activity == "attention" && row(index).pending_question_count == 0
                    })
                    .count(),
                background_work_count: members
                    .iter()
                    .filter(|index| row(index).shows_background_work())
                    .count(),
                question_count: members
                    .iter()
                    .filter(|index| row(index).pending_question_count > 0)
                    .count(),
                session_ids: members
                    .iter()
                    .filter(|index| is_visible[**index])
                    .map(|index| row(index).sidebar_session_id.clone())
                    .collect(),
            })
        })
        .collect();
    SectionLayout {
        sections,
        show_list_toggle: is_project_group && countable.len() > compact_count,
        hidden_session_count: countable.len() - visible.len(),
    }
}
