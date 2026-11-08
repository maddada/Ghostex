//! The Start (Quick Actions) menu's rows: the project's own Actions, then the Global Actions under
//! a "Global" heading, then Configure. `build_gpui_titlebar_actions_popup_menu` gives it the menu
//! already styled; the native modal demo draws this same file (`start-menu`).
//! `on_start_menu_picks` is how the window that hosts the menu hears its rows.

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

/// What a Start menu row asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StartMenuPick {
    /// The project's own Action at this position of the menu.
    Project(usize),
    /// A Global Action, by id.
    Global(String),
    Configure,
}

/// Registers every row action of the Start menu on the element of the window that hosts it, and
/// hands each click to `pick`.
///
/// CDXC:AgentLauncher 2026-11-08 WHY: the Start menu draws in its own popup window, whose actions
/// reach the main window only through the listeners that window registers. The Global rows' action
/// was missing from that list, so a click on a Global Action did nothing (issue #501 follow-up).
/// The menu's file now owns the list next to the rows that dispatch it, so a new kind of row cannot
/// be drawn without being heard; the native modal demo registers it the same way.
pub(crate) fn on_start_menu_picks<E: gpui::InteractiveElement>(
    element: E,
    pick: impl Fn(&StartMenuPick, &mut gpui::Window, &mut gpui::App) + 'static,
) -> E {
    let pick = std::rc::Rc::new(pick);
    let (project, global, configure) = (pick.clone(), pick.clone(), pick);
    element
        .on_action(move |action: &RunGpuiTitlebarAction, window, cx| {
            project(
                &StartMenuPick::Project(action.action_index as usize),
                window,
                cx,
            )
        })
        .on_action(move |action: &RunGpuiTitlebarGlobalAction, window, cx| {
            global(
                &StartMenuPick::Global(action.command_id.clone()),
                window,
                cx,
            )
        })
        .on_action(move |_: &ConfigureGpuiTitlebarActions, window, cx| {
            configure(&StartMenuPick::Configure, window, cx)
        })
}
