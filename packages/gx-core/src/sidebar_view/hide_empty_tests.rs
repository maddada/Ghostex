//! `sidebarHideProjectsWithoutSessions` and what an empty list says while a tag filter is on.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::assemble::{assemble, AssembleInput};
use super::collections::CollectionsState;
use super::groups::{GroupBuild, GroupKind, GroupPlan, ProjectContextInput};
use super::inputs::{SidebarHostInputs, SidebarSettings, SidebarUiState};
use super::projects::ProjectMeta;
use super::view::{GroupCore, GroupSummary, SessionRow, SessionView, SidebarView};

fn plan(project: &str) -> GroupPlan {
    GroupPlan {
        group_id: format!("project:{project}"),
        storage_id: project.to_string(),
        title: project.to_string(),
        kind: GroupKind::Project,
        rows: Vec::new(),
        project: Some(Arc::new(ProjectContextInput::default())),
        remote_machine: None,
        is_stale: false,
    }
}

fn build(project: &str, sessions: usize, is_active: bool, tag_filtered_out: bool) -> GroupBuild {
    let rows: Vec<SessionView> = (0..sessions)
        .map(|index| SessionView {
            row: Arc::new(SessionRow {
                sidebar_session_id: format!("{project}:{index}"),
                ..SessionRow::default()
            }),
            is_focused: false,
            is_visible: false,
            is_multi_selected: false,
            nesting: Default::default(),
        })
        .collect();
    GroupBuild {
        core: Arc::new(GroupCore {
            group_id: format!("project:{project}"),
            storage_id: project.to_string(),
            title: project.to_string(),
            title_tooltip: None,
            is_active,
            project_context: None,
            summary: GroupSummary::default(),
            collapsed: false,
            hover_actions_expanded: false,
            sections: Vec::new(),
            sessions: if tag_filtered_out {
                Vec::new()
            } else {
                rows.clone()
            },
            remote_machine: None,
            is_stale: false,
        }),
        store_rows: rows,
        tag_filtered_out,
        deadline_ms: None,
    }
}

fn view_of(
    groups: &[(&str, usize, bool, bool)],
    settings: &SidebarSettings,
    ui: &SidebarUiState,
    labels: &[&str],
) -> SidebarView {
    let plans: Vec<GroupPlan> = groups.iter().map(|group| plan(group.0)).collect();
    let builds: BTreeMap<String, GroupBuild> = groups
        .iter()
        .map(|&(project, sessions, active, filtered)| {
            (
                format!("project:{project}"),
                build(project, sessions, active, filtered),
            )
        })
        .collect();
    let meta = ProjectMeta {
        project_order: groups.iter().map(|group| group.0.to_string()).collect(),
        project_settings_count: groups.len(),
        ..ProjectMeta::default()
    };
    let host = SidebarHostInputs::default();
    let summaries = BTreeMap::new();
    let any = || true;
    assemble(AssembleInput {
        plans: &plans,
        builds: &builds,
        meta: &meta,
        ui,
        settings,
        host: &host,
        spaces: None,
        spaces_enabled: false,
        collections: CollectionsState::default(),
        local_machine_loaded: true,
        tag_filter_labels: labels.iter().map(|label| label.to_string()).collect(),
        machine_summaries: &summaries,
        any_machine_draws_a_project: &any,
        now_ms: 1,
    })
}

fn drawn(view: &SidebarView) -> Vec<&str> {
    view.groups
        .iter()
        .map(|group| group.core.storage_id.as_str())
        .collect()
}

const PROJECTS: [(&str, usize, bool, bool); 4] = [
    ("used", 2, false, false),
    ("empty", 0, false, false),
    ("open-but-empty", 0, true, false),
    ("also-empty", 0, false, false),
];

#[test]
fn off_by_default_every_project_draws_and_no_row_offers_anything() {
    let view = view_of(
        &PROJECTS,
        &SidebarSettings::default(),
        &SidebarUiState::default(),
        &[],
    );
    assert_eq!(drawn(&view).len(), 4);
    assert_eq!(view.show_all_projects_row, None);
}

#[test]
fn on_a_project_with_no_sessions_leaves_but_the_open_one_stays() {
    let settings = SidebarSettings {
        hide_projects_without_sessions: true,
        ..SidebarSettings::default()
    };
    let view = view_of(&PROJECTS, &settings, &SidebarUiState::default(), &[]);
    assert_eq!(drawn(&view), ["used", "open-but-empty"]);
    let row = view.show_all_projects_row.expect("the row");
    assert_eq!(row.empty_projects, 2);
    assert!(!row.showing);
}

#[test]
fn the_show_all_row_brings_them_back_in_their_own_places_and_stays_as_the_way_back() {
    let settings = SidebarSettings {
        hide_projects_without_sessions: true,
        ..SidebarSettings::default()
    };
    let ui = SidebarUiState {
        show_all_projects: true,
        ..SidebarUiState::default()
    };
    let view = view_of(&PROJECTS, &settings, &ui, &[]);
    assert_eq!(
        drawn(&view),
        ["used", "empty", "open-but-empty", "also-empty"]
    );
    assert!(view.show_all_projects_row.expect("the row").showing);
}

#[test]
fn every_project_empty_says_so_rather_than_no_projects() {
    let settings = SidebarSettings {
        hide_projects_without_sessions: true,
        ..SidebarSettings::default()
    };
    let view = view_of(
        &[("a", 0, false, false), ("b", 0, false, false)],
        &settings,
        &SidebarUiState::default(),
        &[],
    );
    assert!(view.groups.is_empty());
    assert_eq!(view.empty_state.copy, "No projects with sessions.");
    assert!(view.show_all_projects_row.is_some());
}

#[test]
fn a_tag_filter_that_matches_nothing_says_how_to_tag_and_offers_to_clear() {
    let ui = SidebarUiState {
        selected_tag_filters: vec!["blocked".to_string(), "custom-1".to_string()],
        ..SidebarUiState::default()
    };
    let view = view_of(
        &[("a", 2, false, true), ("b", 1, false, true)],
        &SidebarSettings::default(),
        &ui,
        &["Blocked", "On Hold"],
    );
    assert!(view.groups.is_empty());
    assert_eq!(
        view.empty_state.copy,
        "No sessions tagged Blocked or On Hold."
    );
    assert_eq!(view.empty_state.detail, "Right-click a session > Tag As");
    assert_eq!(view.empty_state.clear_tag_filters, ui.selected_tag_filters);
}

#[test]
fn a_project_whose_sessions_the_tag_filter_hid_still_counts_as_having_sessions() {
    // Hide is about projects with no sessions, not about the filter of the moment.
    let settings = SidebarSettings {
        hide_projects_without_sessions: true,
        ..SidebarSettings::default()
    };
    let ui = SidebarUiState {
        selected_tag_filters: vec!["blocked".to_string()],
        ..SidebarUiState::default()
    };
    let view = view_of(
        &[("tagged", 1, false, false), ("other", 1, false, true)],
        &settings,
        &ui,
        &["Blocked"],
    );
    assert_eq!(drawn(&view), ["tagged"]);
    assert_eq!(view.show_all_projects_row, None);
}
