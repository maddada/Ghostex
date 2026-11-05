//! One drawn group: its rows in display order, its sections, its counts, and its header tooltip.
//!
//! Ported from the deleted sidebar page's `project-sections.ts` and `createSidebarGroups` (the focus
//! and browser-row overrides) in the deleted `gxserver-runtime/sidebar-groups.ts`.
//!
//! SEE-ALSO: packages/core-ui/group-session-summary.ts (deleted 2026-10-01).

use std::collections::HashMap;
use std::sync::Arc;

use crate::keys::SessionKey;

use super::inputs::{SectionId, SidebarSettings, SidebarUiState};
use super::ordering::{held_out_of_working, order_rows_for_display, row_deadline_ms};
use super::sections::{project_session_sections, SectionLayout};
use super::tags::matches_session_filters;
use super::threads::nest_threads;
use super::view::{
    GroupCore, GroupSummary, NeedsYou, ProjectContextView, RemoteMachineView, SessionRow,
    SessionView, ThreadTally, WorktreeView,
};

/// Where a group's rows come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GroupKind {
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
    pub(crate) git_work_tree: bool,
    pub(crate) bot_profile: Option<String>,
    pub(crate) bot_gateway_running: bool,
    pub(crate) bot_runs_today: u64,
    pub(crate) work_mode: bool,
    pub(crate) work_linear: bool,
    pub(crate) work_linear_create: bool,
    pub(crate) work_pull_requests: bool,
    pub(crate) work_github: bool,
    pub(crate) work_tracker_github: bool,
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
    thread_tallies: &HashMap<SessionKey, ThreadTally>,
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
                nesting: Default::default(),
                is_multi_selected: ui
                    .selected_session_ids
                    .iter()
                    .any(|selected| *selected == row.sidebar_session_id),
                row: row.clone(),
            }
        })
        .collect();

    let GroupLayout {
        mut sessions,
        layout,
        tag_filtered_out,
        ..
    } = lay_out_group_rows(
        &store_rows,
        &plan.storage_id,
        is_active,
        ui,
        settings,
        now_ms,
    );
    for session in &mut sessions {
        if let Some(tally) = session
            .row
            .key
            .as_ref()
            .filter(|_| session.row.is_coordinator)
            .and_then(|key| thread_tallies.get(key))
        {
            session.nesting.threads = *tally;
        }
    }
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
            git_work_tree: project.git_work_tree,
            bot_profile: project.bot_profile.clone(),
            bot_gateway_running: project.bot_gateway_running,
            work_mode: project.work_mode,
            work_linear: project.work_linear,
            work_linear_create: project.work_linear_create,
            work_pull_requests: project.work_pull_requests,
            work_github: project.work_github,
            work_tracker_github: project.work_tracker_github,
        }),
        summary,
        collapsed: ui.collapse.collapsed_groups.contains(&plan.group_id),
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

/// What [`lay_out_group_rows`] returns.
pub(crate) struct GroupLayout {
    /// The rows in display order, after the tag filter, threads under their coordinators.
    pub(crate) sessions: Vec<SessionView>,
    /// The section each of those rows sits under.
    pub(crate) section_by_session: Vec<SectionId>,
    pub(crate) layout: SectionLayout,
    /// The tag filter is on and left the group empty.
    pub(crate) tag_filtered_out: bool,
}

/// A group's rows (in the group's own order) as the list draws them: sorted into display order, tag
/// filtered, threads nested under their coordinators, and split into sections. Also says whether the tag filter left the group empty.
///
/// CDXC:Sidebar 2026-10-01 WHY:
/// The list and a drag's drop line both call this, so the line is drawn where the row really lands
/// (`SidebarViewModel::preview_session_drop`). A line computed from the hovered row alone was wrong
/// wherever the layout rules move a row on their own: Last Activity order, new sessions leading,
/// threads following their coordinator, drafts and parked rows.
pub(crate) fn lay_out_group_rows(
    store_rows: &[SessionView],
    storage_id: &str,
    is_active: bool,
    ui: &SidebarUiState,
    settings: &SidebarSettings,
    now_ms: u64,
) -> GroupLayout {
    let rows: Vec<Arc<SessionRow>> = store_rows
        .iter()
        .map(|session| session.row.clone())
        .collect();
    let held = if settings.group_working_sessions {
        held_out_of_working(store_rows)
    } else {
        Default::default()
    };
    let ordered = order_rows_for_display(
        &rows,
        settings.sort_mode,
        settings.enable_session_parking,
        settings.group_working_sessions,
        &held,
        now_ms,
    );
    let sessions: Vec<SessionView> = ordered
        .into_iter()
        .map(|index| store_rows[index].clone())
        .filter(|session| {
            matches_session_filters(
                session.row.effective_tag.as_deref(),
                session.row.cloud_box.is_some(),
                &ui.selected_tag_filters,
            )
        })
        .collect();
    let tag_filtered_out = !ui.selected_tag_filters.is_empty() && sessions.is_empty();
    let (sessions, section_by_session) = nest_threads(
        sessions,
        settings.enable_session_parking,
        settings.group_working_sessions,
        &held,
        &ui.collapse.collapsed_coordinators,
        &ui.collapse.expanded_coordinator_older_threads,
        now_ms,
    );
    let section_collapse = ui
        .collapse
        .section_collapse
        .get(storage_id)
        .copied()
        .unwrap_or_default();
    let layout =
        project_session_sections(&sessions, &section_by_session, is_active, section_collapse);
    GroupLayout {
        sessions,
        section_by_session,
        layout,
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
        match row.needs_you() {
            Some(NeedsYou::Approval) => summary.approval_count += 1,
            Some(NeedsYou::Question) => summary.question_count += 1,
            Some(NeedsYou::Attention) => summary.attention_count += 1,
            None => {}
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
/// counts. A bot's has no git or worktree line, because a bot is not a repo, and names its
/// gateway state, which its row's dot shows.
fn project_title_tooltip(
    plan: &GroupPlan,
    project: &ProjectContextInput,
    session_count: usize,
) -> String {
    if project.bot_profile.is_some() {
        let gateway = if project.bot_gateway_running {
            "running"
        } else {
            "stopped"
        };
        return format!(
            "{}\nHermes bot · gateway {gateway}\n{}\n{session_count} {}",
            plan.title,
            project.path,
            count_label(session_count as i64, "session"),
        );
    }
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

#[cfg(test)]
mod summary_tests {
    use std::sync::Arc;

    use super::group_summary;
    use crate::sidebar_view::view::{RowNesting, SessionRow, SessionView};

    fn view(row: SessionRow) -> SessionView {
        SessionView {
            row: Arc::new(row),
            is_focused: false,
            is_visible: false,
            is_multi_selected: false,
            nesting: RowNesting::default(),
        }
    }

    /// A header counts a row waiting for approval in its own red count only, even when it also has
    /// a question or sits in attention; the pink and blue counts keep the rest.
    #[test]
    fn approval_rows_count_red_only() {
        let summary = group_summary(&[
            view(SessionRow {
                activity: "attention".to_string(),
                pending_question_count: 1,
                awaiting_approval: true,
                ..SessionRow::default()
            }),
            view(SessionRow {
                activity: "attention".to_string(),
                pending_question_count: 1,
                ..SessionRow::default()
            }),
            view(SessionRow {
                activity: "attention".to_string(),
                ..SessionRow::default()
            }),
        ]);
        assert_eq!(
            (
                summary.approval_count,
                summary.question_count,
                summary.attention_count
            ),
            (1, 1, 1)
        );
    }
}
