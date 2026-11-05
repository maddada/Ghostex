//! The project each background Space opens on, whose page the app keeps awake.
//!
//! CDXC:Spaces 2026-09-30 DECISION:
//! User: "don't sleep stuff that's surfaced in a bg space", because swiping between Spaces must
//! show the web pages instantly. The view a background Space would open on stays awake for as long
//! as that Space exists, past the keep-alive slider and Auto Sleep. This names those projects by
//! asking what `plan_space_switch_restore` would pick on a switch: the Space's newest remembered row
//! it draws, else its first row, else its first project. Membership is asked of a list built with
//! nothing filtering, the way `plan_space_sleep` asks it, so one build covers every Space.
//!
//! SEE-ALSO: apps/desktop/src/app/web_page_sleep.rs (what the app keeps awake from this).

use crate::core::Core;
use crate::keys::ProjectKey;

use super::inputs::{SidebarInputs, SidebarMode};
use super::machine_spaces::section_spaces_enabled;
use super::model::SidebarViewModel;
use super::reveal::{machine_key, unfiltered};
use super::space_sleep::push_project;
use super::spaces::{resolve_selected_space, selection_shows_project, SpaceSelection, SpacesState};
use super::view::GroupView;

/// The workspace project id each Space other than the selected one would open on, once each, in
/// Space order. Empty when Spaces are off or the selected machine has published no Spaces.
pub fn space_landing_project_ids(core: &Core, inputs: &SidebarInputs, now_ms: u64) -> Vec<String> {
    if !section_spaces_enabled(core.presentation(), inputs) {
        return Vec::new();
    }
    let machine = machine_key(&inputs.ui.selected_machine_id);
    let Some(wire) = super::workspaces::window_spaces(core.presentation(), inputs, &machine) else {
        return Vec::new();
    };
    let spaces = SpacesState::from_wire(&wire);
    // The one list files every machine's projects by every machine's Spaces (`merged.rs`).
    let spaces = match inputs.host.merge_machines && machine.is_local() {
        true => super::merged::union_spaces_of(core.presentation(), inputs, spaces),
        false => spaces,
    };
    let section_key = inputs.ui.section_key();
    let selected = resolve_selected_space(
        &spaces,
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .get(&section_key)
            .map(String::as_str),
    );
    let recent = inputs
        .ui
        .collapse
        .recent_sessions_by_space
        .get(&section_key);
    let built = SidebarViewModel::build_from_scratch(
        core,
        &unfiltered(inputs, SidebarMode::Projects),
        now_ms,
    );
    let selections = spaces
        .order
        .iter()
        .filter(|space_id| spaces.spaces.contains_key(*space_id))
        .map(|space_id| SpaceSelection::Space(space_id.clone()))
        .chain(std::iter::once(SpaceSelection::Other));
    let mut project_ids = Vec::new();
    for selection in selections {
        if selection == selected {
            continue;
        }
        let groups: Vec<&GroupView> = built
            .groups
            .iter()
            .filter(|group| {
                let project = group.space_project.as_ref();
                selection_shows_project(
                    &selection,
                    &spaces,
                    project.map(|project| project.project_id.as_str()),
                    project.and_then(|project| project.collection_id.as_deref()),
                    project.and_then(|project| project.parent_project_id.as_deref()),
                )
            })
            .collect();
        let remembered = recent
            .and_then(|by_space| by_space.get(selection.space_id()))
            .map(Vec::as_slice)
            .unwrap_or_default();
        if let Some(group) = landing_group(&groups, remembered) {
            push_project(&mut project_ids, &group.core.group_id);
        }
    }
    project_ids
}

/// The group whose project a switch would open: the one holding the newest remembered row the
/// Space draws, else its first row, else its first group. A row drawn both in its project and in a
/// user-made group resolves to the project, which is what focusing it mounts.
fn landing_group<'a>(groups: &[&'a GroupView], remembered: &[String]) -> Option<&'a GroupView> {
    let holding = |sidebar_session_id: &str| {
        let holders = groups.iter().copied().filter(|group| {
            group
                .core
                .sessions
                .iter()
                .any(|session| session.row.sidebar_session_id == sidebar_session_id)
        });
        let mut first = None;
        for group in holders {
            if ProjectKey::parse_sidebar_group_id(&group.core.group_id).is_some() {
                return Some(group);
            }
            first.get_or_insert(group);
        }
        first
    };
    remembered
        .iter()
        .find_map(|sidebar_session_id| holding(sidebar_session_id))
        .or_else(|| {
            groups
                .iter()
                .flat_map(|group| group.core.sessions.iter())
                .next()
                .and_then(|session| holding(&session.row.sidebar_session_id))
        })
        .or_else(|| groups.first().copied())
}
