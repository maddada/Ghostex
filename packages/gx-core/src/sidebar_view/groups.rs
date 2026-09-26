//! One drawn group: its rows in display order, its sections, its counts, and its header tooltip.
//!
//! Ported from the deleted sidebar page's `project-sections.ts` and `createSidebarGroups` (the focus
//! and browser-row overrides) in the deleted `gxserver-runtime/sidebar-groups.ts`.
//!
//! SEE-ALSO: packages/core-ui/group-session-summary.ts.

use std::sync::Arc;

use super::inputs::{SidebarSettings, SidebarUiState};
use super::ordering::{order_rows_for_display, row_deadline_ms};
use super::sections::project_session_sections;
use super::tags::matches_tag_filters;
use super::view::{
    GroupCore, GroupSummary, ProjectContextView, RemoteMachineView, SessionRow, SessionView,
    WorktreeView,
};

/// Where a group's rows come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GroupKind {
    /// The Chats collection of a machine. Never drawn by the native sidebar; its rows still count
    /// towards the machine tab and the Other view.
    ///
    /// CDXC:Automations 2026-09-21 WHY:
    /// This is also where the All Automations overview row would be, and why no sidebar draws one.
    /// `withQuickAutomationsOverviewGroup` splices a synthetic row into the CHATS group of the
    /// published projection, and `createNativeSidebarSnapshot` drops every chat collection before
    /// it builds the list (the deleted sidebar page's `model.ts`, `group.isChatCollection` in the group loop),
    /// so the desktop sidebar has never shown it and neither does this list. What the More menu's
    /// All Automations really does is `openAutomationsPage`, which changes the active project and
    /// opens the Automate workarea; the row is a side effect nobody sees here. Do not add one: the
    /// sidebar has never drawn it, so a row here would be a new difference, not a fix.
    Chats,
    Project,
    /// A user-made session group inside a project.
    Subgroup,
}

/// The project facts a project group draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectContextInput {
    pub(crate) project_id: String,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) icon_data_url: Option<String>,
    pub(crate) discovered_icon_data_url: Option<String>,
    pub(crate) worktree: Option<WorktreeView>,
    pub(crate) diff_stats: super::inputs::ProjectDiffStats,
    /// How many worktree projects name this project as their parent.
    pub(crate) worktree_count: usize,
    pub(crate) git_remote_origin_url: Option<String>,
}

/// Who is focused right now, in the vocabulary the rows compare against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusKey {
    pub(crate) active_group_id: Option<String>,
    pub(crate) active_project_id: Option<String>,
    /// The raw session id, as the old runtime's focus payload carries it.
    pub(crate) focused_session_id: Option<String>,
    pub(crate) visible_session_ids: Vec<String>,
}

/// One row of a group, with the number the cache minted for it. A row keeps its number while it
/// is reused, so a group can tell whether its rows moved by comparing numbers rather than the
/// addresses behind them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowRef {
    pub(crate) id: u64,
    pub(crate) row: Arc<SessionRow>,
}

/// One group before its rows are ordered and filtered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupPlan {
    pub(crate) group_id: String,
    pub(crate) storage_id: String,
    pub(crate) title: String,
    pub(crate) kind: GroupKind,
    /// The project's own rows in the daemon's order, then the group's members.
    pub(crate) rows: Vec<RowRef>,
    pub(crate) project: Option<Arc<ProjectContextInput>>,
    /// The remote machine this group belongs to, with the raw project id in that machine's daemon.
    pub(crate) remote_machine: Option<RemoteMachineView>,
    /// The machine's stream is down while its rows are still held.
    pub(crate) is_stale: bool,
}

/// A built group: what is drawn, plus what the top level needs from every group, drawn or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupBuild {
    pub(crate) core: Arc<GroupCore>,
    /// Every row of the group, before the tag filter; the machine and Space counts read these.
    pub(crate) store_rows: Vec<SessionView>,
    /// The tag filter is on and left the group empty, so it is not drawn.
    pub(crate) tag_filtered_out: bool,
    /// The next moment a row moves section or place on its own.
    pub(crate) deadline_ms: Option<u64>,
}

impl GroupBuild {
    pub(crate) fn contains_focused_session(&self) -> bool {
        self.store_rows.iter().any(|session| session.is_focused)
    }
}

/// Builds one group.
pub(crate) fn build_group(
    plan: &GroupPlan,
    focus: &FocusKey,
    ui: &SidebarUiState,
    settings: &SidebarSettings,
    now_ms: u64,
) -> GroupBuild {
    let rows: Vec<Arc<SessionRow>> = plan.rows.iter().map(|row| row.row.clone()).collect();
    let is_active = focus.active_group_id.as_deref() == Some(plan.group_id.as_str());
    let project_is_active = plan.project.as_ref().is_some_and(|project| {
        focus.active_project_id.as_deref() == Some(project.project_id.as_str())
    });
    // A focused browser tab takes the focus mark away from every session row of the group.
    let browser_owns_focus = is_active
        && rows
            .iter()
            .any(|row| row.is_browser && project_is_active && row.browser_is_active);
    let store_rows: Vec<SessionView> = rows
        .iter()
        .map(|row| {
            let (is_focused, is_visible) = if row.is_browser {
                (
                    is_active && project_is_active && row.browser_is_active,
                    is_active && project_is_active && row.browser_is_visible,
                )
            } else {
                let raw = row.key.as_ref().map(|key| key.session_id.as_str());
                (
                    is_active
                        && !browser_owns_focus
                        && raw.is_some()
                        && raw == focus.focused_session_id.as_deref(),
                    is_active
                        && raw.is_some_and(|raw| {
                            focus
                                .visible_session_ids
                                .iter()
                                .any(|visible| visible == raw)
                        }),
                )
            };
            SessionView {
                is_focused,
                is_visible,
                is_multi_selected: ui
                    .selected_session_ids
                    .iter()
                    .any(|selected| *selected == row.sidebar_session_id),
                row: row.clone(),
            }
        })
        .collect();

    let ordered = order_rows_for_display(
        &rows,
        settings.sort_mode,
        settings.enable_session_parking,
        now_ms,
    );
    let sessions: Vec<SessionView> = ordered
        .into_iter()
        .map(|index| store_rows[index].clone())
        .filter(|session| {
            matches_tag_filters(
                session.row.effective_tag.as_deref(),
                &ui.selected_tag_filters,
            )
        })
        .collect();
    let tag_filtered_out = !ui.selected_tag_filters.is_empty() && sessions.is_empty();

    let is_project_group = plan.project.is_some();
    let section_collapse = ui
        .collapse
        .section_collapse
        .get(&plan.storage_id)
        .copied()
        .unwrap_or_default();
    let expanded = ui
        .collapse
        .expanded_session_lists
        .contains(&plan.storage_id);
    let layout = project_session_sections(
        &sessions,
        is_active,
        is_project_group,
        section_collapse,
        expanded,
        settings.project_session_list_collapsed_count,
        settings.enable_session_parking,
        now_ms,
    );
    let summary = group_summary(&sessions);
    let core = GroupCore {
        group_id: plan.group_id.clone(),
        storage_id: plan.storage_id.clone(),
        title: plan.title.clone(),
        title_tooltip: plan
            .project
            .as_ref()
            .map(|project| project_title_tooltip(plan, project, sessions.len())),
        is_active,
        project_context: plan.project.as_ref().map(|project| ProjectContextView {
            project_id: project.project_id.clone(),
            path: project.path.clone(),
            icon_data_url: project.icon_data_url.clone(),
            discovered_icon_data_url: project.discovered_icon_data_url.clone(),
            diff_stats: project.diff_stats,
            worktree: project.worktree.clone(),
            git_remote_origin_url: project.git_remote_origin_url.clone(),
        }),
        summary,
        collapsed: ui.collapse.collapsed_groups.contains(&plan.group_id),
        expanded,
        hidden_session_count: layout.hidden_session_count,
        show_list_toggle: layout.show_list_toggle,
        hover_actions_expanded: ui
            .collapse
            .expanded_hover_actions
            .contains(&plan.storage_id),
        sections: layout.sections,
        sessions,
        remote_machine: plan.remote_machine.clone(),
        is_stale: plan.is_stale,
    };
    GroupBuild {
        deadline_ms: rows
            .iter()
            .filter_map(|row| row_deadline_ms(row, now_ms))
            .min(),
        core: Arc::new(core),
        store_rows,
        tag_filtered_out,
    }
}

/// `getGroupSessionSummary` plus `getAwakeTerminalAndBrowserCount`.
pub(crate) fn group_summary(sessions: &[SessionView]) -> GroupSummary {
    let mut summary = GroupSummary::default();
    for session in sessions {
        let row = &session.row;
        if row.activity == "working" {
            summary.working_count += 1;
        }
        if row.activity == "attention" || row.pending_question_count > 0 {
            summary.attention_count += 1;
        }
        if row.shows_background_work() {
            summary.background_work_count += 1;
        }
        let is_terminal_or_browser = row.is_browser
            || row.session_kind.as_deref() == Some("terminal")
            || row.session_kind.as_deref() == Some("browser");
        if row.lifecycle_state == "running" && is_terminal_or_browser {
            summary.awake_count += 1;
        }
    }
    summary
}

/// The project header tooltip: title, kind, path, git numbers, and the session and worktree
/// counts.
fn project_title_tooltip(
    plan: &GroupPlan,
    project: &ProjectContextInput,
    session_count: usize,
) -> String {
    let kind = if project.worktree.is_some() {
        "Worktree project"
    } else {
        "Repository project"
    };
    format!(
        "{}\n{kind}\n{}\n{}\n{session_count} {} · {} {}",
        plan.title,
        project.path,
        format_project_tooltip_git_stats(&project.diff_stats),
        count_label(session_count as i64, "session"),
        project.worktree_count,
        count_label(project.worktree_count as i64, "worktree"),
    )
}

/// `formatProjectTooltipGitStats`.
fn format_project_tooltip_git_stats(stats: &super::inputs::ProjectDiffStats) -> String {
    if stats.is_loading {
        return "Git: loading changes".to_string();
    }
    if !stats.is_repo {
        return "Git: not a repository".to_string();
    }
    let files = stats.files.max(0);
    let changed_lines = stats.additions.max(0) + stats.deletions.max(0);
    format!(
        "{files} {} changed  +{}  -{} {}",
        count_label(files, "file"),
        format_line_count(stats.additions),
        format_line_count(stats.deletions),
        count_label(changed_lines, "line"),
    )
}

/// `formatProjectEditorLineCount`.
fn format_line_count(lines: i64) -> String {
    lines.clamp(0, 9999).to_string()
}

/// `formatCountLabel`.
fn count_label(count: i64, singular: &str) -> String {
    if count.abs() == 1 {
        singular.to_string()
    } else {
        format!("{singular}s")
    }
}
