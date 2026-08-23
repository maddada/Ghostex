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

use gpui::Action;
use gpui::px;
use gpui_component::Side;
use gpui_component::menu::PopupMenu;
use gpui_component::scroll::ScrollbarShow;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn build_gpui_open_targets_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let targets = gpui_visible_open_targets_from_current_settings();
        let active_target_index = self.active_open_target_index(&targets);
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_h(px(TITLEBAR_POPUP_MENU_INNER_MAX_HEIGHT))
            .items_padding_bottom(px(0.0))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
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

    pub(crate) fn build_gpui_titlebar_actions_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
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
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_h(px(TITLEBAR_POPUP_MENU_INNER_MAX_HEIGHT))
            .items_padding_bottom(px(0.0))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
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

    pub(crate) fn build_gpui_titlebar_git_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_GIT_WIDTH))
            .max_w(px(TITLEBAR_POPUP_GIT_WIDTH))
            .max_h(px(TITLEBAR_POPUP_MENU_INNER_MAX_HEIGHT))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
            .check_side(Side::Right);

        let Some(state) = self.titlebar_git_menu_state.as_ref() else {
            return menu.menu_element_with_disabled(
                Box::new(CopyGpuiTitlebarGitBranch),
                true,
                move |_, _| titlebar_popup_empty_menu_row("Loading Git state...".to_string()),
            );
        };

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

    #[allow(dead_code)] // no caller: Tips is a titlebar-host CEF panel now; this is the superseded native popup menu
    pub(crate) fn build_gpui_titlebar_tips_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let read_ids = gpui_titlebar_tips_read_ids_from_settings();
        let unread_count = GPUI_NATIVE_TITLEBAR_TIPS
            .len()
            .saturating_sub(read_ids.len());
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_TIPS_WIDTH))
            .max_w(px(TITLEBAR_POPUP_TIPS_WIDTH))
            .max_h(px(TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT))
            .scrollable(true);

        menu = menu.menu_element_with_disabled(
            Box::new(CopyGpuiTitlebarGitBranch),
            true,
            move |_, _| {
                titlebar_popup_reading_header(
                    TITLEBAR_ICON_INFO,
                    "Tips".to_string(),
                    format!("{unread_count} unread"),
                )
            },
        );
        for (action_index, (label, icon_path)) in [
            ("Docs", "titlebar/book.svg"),
            ("Video", "titlebar/sparkles.svg"),
            ("Setup", "titlebar/tool.svg"),
            ("Updates", "titlebar/history.svg"),
        ]
        .into_iter()
        .enumerate()
        {
            menu = menu.menu_element(
                Box::new(RunGpuiTitlebarTipsHeaderAction {
                    action_index: action_index as u64,
                }),
                move |_, _| {
                    titlebar_popup_standard_menu_row(
                        icon_path,
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        label.to_string(),
                        false,
                    )
                },
            );
        }

        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let show_persistence_notice =
            gpui_titlebar_session_persistence_provider_from_settings(settings.object()) == "off";
        let show_debug_notice = settings.debugging_mode();
        if show_persistence_notice || show_debug_notice {
            menu = titlebar_popup_git_section(menu.separator(), "Notices");
            if show_persistence_notice {
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTipsHeaderAction { action_index: 4 }),
                    move |_, _| titlebar_popup_tip_row(
                        "titlebar/bug.svg",
                        "Mobile attach needs persistence".to_string(),
                        "Enable zmx persistence so mobile clients reconnect to durable terminal sessions.".to_string(),
                        false,
                    ),
                );
            }
            if show_debug_notice {
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTipsHeaderAction { action_index: 4 }),
                    move |_, _| titlebar_popup_tip_row(
                        "titlebar/bug.svg",
                        "Debug mode is on".to_string(),
                        "Ghostex is showing debug UI controls and allowing enabled diagnostic scenarios to write routine logs.".to_string(),
                        false,
                    ),
                );
            }
        }

        let unread = GPUI_NATIVE_TITLEBAR_TIPS
            .iter()
            .enumerate()
            .filter(|(_, tip)| !read_ids.contains(tip.id))
            .collect::<Vec<_>>();
        let read = GPUI_NATIVE_TITLEBAR_TIPS
            .iter()
            .enumerate()
            .filter(|(_, tip)| read_ids.contains(tip.id))
            .collect::<Vec<_>>();
        if !unread.is_empty() {
            menu = titlebar_popup_git_section(menu.separator(), "Unread");
            for (tip_index, tip) in unread {
                let tip = *tip;
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTip {
                        tip_index: tip_index as u64,
                    }),
                    move |_, _| {
                        titlebar_popup_tip_row(
                            tip.icon_path,
                            tip.title.to_string(),
                            tip.body.to_string(),
                            true,
                        )
                    },
                );
            }
        }
        if !read.is_empty() {
            menu = titlebar_popup_git_section(menu.separator(), "Read");
            for (tip_index, tip) in read {
                let tip = *tip;
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTip {
                        tip_index: tip_index as u64,
                    }),
                    move |_, _| {
                        titlebar_popup_tip_row(
                            tip.icon_path,
                            tip.title.to_string(),
                            tip.body.to_string(),
                            false,
                        )
                    },
                );
            }
        }
        menu
    }

    #[allow(dead_code)] // no caller: Resources is a titlebar-host CEF panel now; this is the superseded native popup menu
    pub(crate) fn build_gpui_titlebar_resources_popup_menu(
        menu: PopupMenu,
        snapshot: GpuiNativeResourcesSnapshot,
    ) -> PopupMenu {
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_RESOURCES_WIDTH))
            .max_w(px(TITLEBAR_POPUP_RESOURCES_WIDTH))
            .max_h(px(TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT))
            .scrollable(true);
        let total_label = format!(
            "{}  •  {}",
            format_gpui_resource_cpu(snapshot.total_cpu),
            format_gpui_resource_memory(snapshot.total_memory_mb),
        );
        menu = menu.menu_element_with_disabled(
            Box::new(CopyGpuiTitlebarGitBranch),
            true,
            move |_, _| {
                titlebar_popup_reading_header(
                    TITLEBAR_ICON_DEVICE_DESKTOP,
                    "Resources".to_string(),
                    total_label.clone(),
                )
            },
        );
        menu = menu
            .menu_element(Box::new(SleepInactiveSessionsFromTitlebar), move |_, _| {
                titlebar_popup_standard_menu_row(
                    COMMAND_ICON_MOON,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Sleep Inactive Sessions".to_string(),
                    false,
                )
            })
            .menu_element(Box::new(OpenGpuiDaemonSessionsModal), move |_, _| {
                titlebar_popup_standard_menu_row(
                    TITLEBAR_ICON_DEVICE_DESKTOP,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Running Sessions…".to_string(),
                    false,
                )
            })
            .menu_element(Box::new(RestartGpuiGxserverFromTitlebar), move |_, _| {
                titlebar_popup_standard_menu_row(
                    BROWSER_ICON_RELOAD,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Restart gxserver".to_string(),
                    false,
                )
            });

        for (label, rows) in [
            ("Dev Servers", snapshot.server_rows),
            ("Ghostex", snapshot.session_rows),
            ("Code IDE", snapshot.code_rows),
            ("Browser Tabs", snapshot.browser_rows),
            ("Orphaned / Detached", snapshot.orphan_rows),
        ] {
            if rows.is_empty() {
                continue;
            }
            menu = titlebar_popup_git_section(menu.separator(), label);
            for row in rows {
                let action: Box<dyn Action> = if let Some(session_id) = row.session_id.clone() {
                    Box::new(FocusGpuiTitlebarResourceSession { session_id })
                } else if let Some(url) = row.url.clone() {
                    Box::new(OpenGpuiTitlebarResourceUrl { url })
                } else {
                    Box::new(CopyGpuiTitlebarGitBranch)
                };
                let disabled = row.session_id.is_none() && row.url.is_none();
                menu = menu.menu_element_with_disabled(action, disabled, move |_, _| {
                    titlebar_popup_resource_row(row.clone(), disabled)
                });
            }
        }
        menu
    }

}
