// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the open-targets, actions, git, tips, and resources PopupMenu builders.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::px;
use gpui_component::Side;
use gpui_component::menu::{PopupMenu, PopupMenuAppearance};
use gpui_component::scroll::ScrollbarShow;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::*;

/// CDXC:ContextMenus 2026-09-16 DECISION:
/// User: GPUI context menus use the same style as the shared React sidebar menu.
/// Panel corners are 8px, rows 6px, panel padding 6px, and horizontal row padding 10px, with neutral theme-aware colors.
/// User: remove GPUI menu shadows because they are cut off by the popup window.
/// SEE-ALSO: packages/components/ui/app-menu-panel.css, app/consts.rs menu geometry, app/context_menu.rs label sizing.
pub(crate) fn titlebar_popup_menu_with_scroll_behavior(
    menu: PopupMenu,
    width: f32,
    max_height: f32,
    scrollable: bool,
) -> PopupMenu {
    let menu = menu
        .appearance(PopupMenuAppearance {
            shadow: false,
            panel_radius: px(8.0),
            item_radius: px(6.0),
            padding: px(6.0),
            item_padding_x: px(10.0),
            item_height: px(TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT),
            separator_margin: px(6.0),
            background: popup_window_surface(titlebar_popup_menu_background()),
            foreground: titlebar_popup_menu_foreground(),
            border: titlebar_popup_menu_border_color(),
            hover: titlebar_popup_menu_hover_color(),
        })
        .min_w(px(width))
        .max_w(px(width))
        .max_h(px(max_height))
        .scrollable(scrollable);
    if scrollable {
        menu.scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Hover)
    } else {
        menu
    }
}

impl GhostexGpuiApp {
    pub(crate) fn build_gpui_open_targets_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let targets = gpui_visible_open_targets_from_current_settings();
        let active_target_index = self.active_open_target_index(&targets);
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);
        for (target_index, target) in targets.iter().enumerate() {
            let label = target.label.clone();
            let (icon_path, icon_size) = titlebar_open_target_icon_for_id(&target.id);
            menu = menu.menu_element_with_check(
                Some(target_index) == active_target_index,
                Box::new(OpenGpuiWorkspaceInTarget {
                    target_index: target_index as u64,
                }),
                move |_, _| {
                    titlebar_popup_standard_menu_row(icon_path, icon_size, label.clone(), false)
                },
            );
        }
        if !targets.is_empty() {
            menu = menu.separator();
        }
        menu.menu_element(Box::new(OpenGpuiOpenTargetsModal), move |_, _| {
            titlebar_popup_standard_menu_row(
                TITLEBAR_ICON_SETTINGS,
                TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                "Configure".to_string(),
                false,
            )
        })
    }

    pub(crate) fn build_gpui_titlebar_actions_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let actions = self.visible_gpui_titlebar_actions();
        let active_command_id = self
            .active_action_command_id
            .as_deref()
            .and_then(|active_id| {
                actions
                    .iter()
                    .find(|action| action.command_id == active_id && action.is_configured())
            })
            .or_else(|| actions.iter().find(|action| action.is_configured()))
            .map(|action| action.command_id.clone());
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);

        if actions.is_empty() {
            menu = menu.menu_element_with_disabled(
                Box::new(ConfigureGpuiTitlebarActions),
                true,
                move |_, _| titlebar_popup_empty_menu_row("No Actions configured".to_string()),
            );
        } else {
            for (action_index, action) in actions.iter().enumerate() {
                let row = action.clone();
                let checked = active_command_id.as_deref() == Some(row.command_id.as_str());
                menu = menu.menu_element_with_check(
                    checked,
                    Box::new(RunGpuiTitlebarAction {
                        action_index: action_index as u64,
                    }),
                    move |_, _| titlebar_popup_action_menu_row(row.clone()),
                );
            }
        }

        menu.separator()
            .menu_element(Box::new(ConfigureGpuiTitlebarActions), move |_, _| {
                titlebar_popup_standard_menu_row(
                    TITLEBAR_ICON_SETTINGS,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Configure".to_string(),
                    false,
                )
            })
    }

    pub(crate) fn build_gpui_titlebar_git_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);

        let Some(state) = self
            .titlebar_git_menu_state
            .as_ref()
            .filter(|state| !state.is_busy)
        else {
            return menu.menu_element_with_disabled(
                Box::new(CopyGpuiTitlebarGitBranch),
                true,
                move |_, _| titlebar_popup_empty_menu_row("Loading Git state...".to_string()),
            );
        };
        if !state.is_repo {
            return menu.menu_element_with_disabled(
                Box::new(CopyGpuiTitlebarGitBranch),
                true,
                move |_, _| titlebar_popup_empty_menu_row("Not a Git repository".to_string()),
            );
        }

        menu = titlebar_popup_git_section(menu, "Status");

        let branch_value = state
            .branch
            .clone()
            .unwrap_or_else(|| "(detached HEAD)".to_string());
        let branch_disabled = !state.is_repo;
        menu = menu.menu_element_with_disabled(
            Box::new(CopyGpuiTitlebarGitBranch),
            branch_disabled,
            move |_, _| titlebar_popup_git_branch_menu_row(branch_value.clone(), branch_disabled),
        );

        menu = menu.menu_element(Box::new(OpenGpuiTitlebarGitCommitScreen), {
            let additions = state.additions;
            let deletions = state.deletions;
            move |_, _| titlebar_popup_git_changes_menu_row(additions, deletions)
        });

        let commits_disabled =
            state.sync_remote_disabled || (state.ahead_count == 0 && state.behind_count == 0);
        menu = menu.menu_element_with_disabled(
            Box::new(RunGpuiTitlebarGitRemoteSync),
            commits_disabled,
            {
                let ahead_count = state.ahead_count;
                let behind_count = state.behind_count;
                move |_, _| {
                    titlebar_popup_git_commits_menu_row(ahead_count, behind_count, commits_disabled)
                }
            },
        );

        menu = titlebar_popup_git_section(menu.separator(), "Actions");
        for (row_index, row) in state.rows.iter().enumerate() {
            let row = row.clone();
            menu = menu.menu_element_with_disabled(
                Box::new(RunGpuiTitlebarGitMenuAction {
                    row_index: row_index as u64,
                }),
                row.disabled,
                move |_, _| titlebar_popup_git_action_menu_row(row.clone()),
            );
        }
        menu
    }
}
