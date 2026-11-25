//! The header Commit pill's summary of the active project's Git state: its text, arrows and
//! tooltip. The desktop's Commit split button (action_buttons.rs) and the web build's Commit
//! button (apps/gpui-web/src/shell.rs) both draw it, so the two read the same.

use gpui::AnyElement;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;

use crate::GhostexGpuiApp;

/// CDXC:Titlebar 2026-11-16 DECISION:
/// User: "instead of showing Commit we should instead show the +123 -213 there for that session's
/// worktree basically and also show up or down arrow to the left of them when there's something
/// that we didn't push or something we should pull". The numbers come from the active project's
/// git read the Commit menu already draws from (`titlebar_git_menu_state`: `git diff --numstat
/// HEAD`, untracked files listed, ahead/behind from the local upstream refs), so no new poll and no
/// fetch. With nothing to show the button keeps the word "Commit", so it is never empty.
/// Then: the same colours and look as the sidebar's numbers, and only while the sidebar is not
/// already drawing them for that project (with Separate by machine off it draws none); when it
/// is, the button stays the plain "Commit".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkareaHeaderGitSummary {
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
    pub(crate) ahead: u64,
    pub(crate) behind: u64,
}

impl WorkareaHeaderGitSummary {
    pub(crate) fn from_state(state: &crate::app::model::GpuiTitlebarGitMenuState) -> Self {
        if !state.is_repo {
            return Self::default();
        }
        Self {
            additions: state.additions,
            deletions: state.deletions,
            ahead: state.ahead_count,
            behind: state.behind_count,
        }
    }

    pub(crate) fn has_diff(self) -> bool {
        self.additions > 0 || self.deletions > 0
    }

    pub(crate) fn is_empty(self) -> bool {
        !self.has_diff() && self.ahead == 0 && self.behind == 0
    }

    /// "123 lines added, 213 removed · 2 commits to push · 1 to pull"
    pub(crate) fn tooltip(self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if self.has_diff() {
            parts.push(format!(
                "{} {} added, {} removed",
                self.additions,
                if self.additions == 1 { "line" } else { "lines" },
                self.deletions
            ));
        }
        if self.ahead > 0 {
            parts.push(format!(
                "{} {} to push",
                self.ahead,
                if self.ahead == 1 { "commit" } else { "commits" }
            ));
        }
        if self.behind > 0 {
            parts.push(format!("{} to pull", self.behind));
        }
        Some(parts.join(" \u{00b7} "))
    }

    /// The pill shows an up, down or both arrow: there are commits to push or pull.
    pub(crate) fn shows_arrows(self) -> bool {
        self.ahead > 0 || self.behind > 0
    }

    /// CDXC:Titlebar 2026-11-25 DECISION:
    /// User (Q4869, "do all 3 please. also I don't want the icon on the left to be shown (the
    /// commit icon) when there's arrows + numbers"): the pill gets narrower. Every number reads
    /// short (7.3k from 1000, 13k from 10000), which ends the old 9999 cap. With lines changed,
    /// commits to push or pull show as one arrow without a count (↑ push, ↓ pull, ↕ both) before
    /// "+N -N"; with no lines changed the arrows keep their counts ("↑3 ↓12"). The commit icon is
    /// left out whenever an arrow shows (`shows_arrows`). The tooltip keeps every number in full.
    /// Line numbers use the sidebar project header's own colours and opacity (`project_status.rs`).
    pub(crate) fn render(self) -> Option<AnyElement> {
        use crate::app::native_sidebar::project_status::{
            PROJECT_DIFF_OPACITY, compact_count, project_diff_additions_color,
            project_diff_deletions_color,
        };
        if self.is_empty() {
            return None;
        }
        let arrows: Vec<String> = if self.has_diff() {
            let arrow = match (self.ahead > 0, self.behind > 0) {
                (true, true) => Some("\u{2195}"),
                (true, false) => Some("\u{2191}"),
                (false, true) => Some("\u{2193}"),
                (false, false) => None,
            };
            arrow.into_iter().map(str::to_string).collect()
        } else {
            [("\u{2191}", self.ahead), ("\u{2193}", self.behind)]
                .into_iter()
                .filter(|(_, count)| *count > 0)
                .map(|(arrow, count)| format!("{arrow}{}", compact_count(count)))
                .collect()
        };
        Some(
            h_flex()
                .items_center()
                .gap(px(8.0))
                .children(arrows.into_iter().map(|arrow| div().child(arrow)))
                .when(self.has_diff(), |this| {
                    this.child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .opacity(PROJECT_DIFF_OPACITY)
                            .child(
                                div()
                                    .text_color(project_diff_additions_color())
                                    .child(format!("+{}", compact_count(self.additions))),
                            )
                            .child(
                                div()
                                    .text_color(project_diff_deletions_color())
                                    .child(format!("-{}", compact_count(self.deletions))),
                            ),
                    )
                })
                .into_any_element(),
        )
    }
}

impl GhostexGpuiApp {
    /// What the header's Commit pill shows for the active project; empty (the plain "Commit")
    /// while the sidebar already draws the project's numbers.
    pub(crate) fn workarea_header_git_summary(&self) -> WorkareaHeaderGitSummary {
        self.titlebar_git_menu_state
            .as_ref()
            .filter(|_| !self.sidebar_shows_active_project_diff_stats())
            .map(WorkareaHeaderGitSummary::from_state)
            .unwrap_or_default()
    }

    /// The Project sidebar is drawing the active project's "+N -N" on its header now, so the
    /// Commit button would only repeat it. Mirrors where the sidebar draws a project header (the
    /// classic list, not Unified or the Bots list) and `project_header_shows_diff_stats`.
    fn sidebar_shows_active_project_diff_stats(&self) -> bool {
        if self.native_sidebar.unified.is_some() {
            return false;
        }
        let Some(snapshot) = self.native_sidebar.snapshot.as_deref() else {
            return false;
        };
        if snapshot.bots_mode {
            return false;
        }
        let Some(active) = self.git_active_scoped_project_id() else {
            return false;
        };
        snapshot.groups.iter().any(|group| {
            group.owner_group_id.is_none()
                && self
                    .git_scope_for_group(&group.group_id)
                    .is_ok_and(|scope| scope.scoped_id() == active)
                && crate::app::native_sidebar::project_status::project_header_shows_diff_stats(
                    group,
                    &snapshot.hud,
                )
        })
    }
}
