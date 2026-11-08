//! The Start (Quick Actions) menu's rows: the project's own Actions, then the Global Actions under
//! a "Global" heading, then Configure. `build_gpui_titlebar_actions_popup_menu` gives it the menu
//! already styled; the native modal demo draws this same file (`start-menu`).

use gpui_component::menu::PopupMenu;

use crate::app::actions::{
    ConfigureGpuiTitlebarActions, RunGpuiTitlebarAction, RunGpuiTitlebarGlobalAction,
};
use crate::app::consts::{TITLEBAR_ICON_SETTINGS, TITLEBAR_POPUP_MENU_ROW_ICON_SIZE};
use crate::app::helpers::{
    GpuiTitlebarAction, titlebar_popup_action_menu_row, titlebar_popup_empty_menu_row,
    titlebar_popup_git_section, titlebar_popup_standard_menu_row,
};

/// CDXC:AgentLauncher 2026-11-08 DECISION:
/// User (issue #501): the Start menu lists the Global Actions (Settings > Actions > Global Actions)
/// below the project's own Actions, under a "Global" heading, each running with one click like
/// `ghostex run-action <id> --scope global`. The heading is left out when there are none, and "No
/// Actions configured" only shows when neither list has any. A Global row never takes the check
/// mark or becomes the Start button's one-click Action: those stay the project's own.
pub(crate) fn titlebar_actions_menu_rows(
    mut menu: PopupMenu,
    actions: &[GpuiTitlebarAction],
    active_command_id: Option<&str>,
    global_actions: &[GpuiTitlebarAction],
) -> PopupMenu {
    if actions.is_empty() && global_actions.is_empty() {
        menu = menu.menu_element_with_disabled(
            Box::new(ConfigureGpuiTitlebarActions),
            true,
            move |_, _| titlebar_popup_empty_menu_row("No Actions configured".to_string()),
        );
    }
    for (action_index, action) in actions.iter().enumerate() {
        let row = action.clone();
        let checked = active_command_id == Some(row.command_id.as_str());
        menu = menu.menu_element_with_check(
            checked,
            Box::new(RunGpuiTitlebarAction {
                action_index: action_index as u64,
            }),
            move |_, _| titlebar_popup_action_menu_row(row.clone()),
        );
    }
    if !global_actions.is_empty() {
        if !actions.is_empty() {
            menu = menu.separator();
        }
        menu = titlebar_popup_git_section(menu, "Global");
        for action in global_actions {
            let row = action.clone();
            menu = menu.menu_element_with_check(
                false,
                Box::new(RunGpuiTitlebarGlobalAction {
                    command_id: row.command_id.clone(),
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
