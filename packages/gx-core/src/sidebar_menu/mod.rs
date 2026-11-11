//! The sidebar's menus, hover buttons and header buttons, as data.
//!
//! CDXC:ContextMenus 2026-09-20 DECISION:
//! User (2026-09-19): the desktop app stops running product logic in QuickJS; one Rust store owns
//! it. The menus were the last part of the sidebar a host still had to take from the TypeScript
//! projection by row id, which meant a row the projection had not published drew with no menu and
//! no hover buttons, and every accepted publish forced the whole list to be reinstalled. They are
//! built here instead, on demand for the row or group the user opened, and nothing is cached that
//! could go stale. This supersedes nothing: the TypeScript builders kept running for the machines
//! this store did not hold yet, until they were deleted.
//!
//! Every builder is a port of one file of the deleted sidebar page, named in its
//! own module comment. The command payloads are unchanged, so a row built here and a row built there
//! reach the same handler.

mod agent_choice;
mod agent_logos;
mod bulk;
mod capabilities;
mod clipboard;
mod clone_on;
mod collection;
mod commands;
mod copy;
mod group;
mod header;
mod host;
mod hover;
mod item;
mod link_menu;
#[cfg(test)]
mod make_coordinator_tests;
mod membership;
mod menus;
mod navigation;
mod project;
mod run_in_box;
mod section;
mod session;
#[cfg(test)]
mod session_notes_tests;
mod text;
mod workspace;

pub use agent_choice::{
    AgentChoicePlace, AgentChoiceScope, AgentChoices, AGENT_CHOICES_STORAGE_KEY,
    AGENT_CHOICE_SCOPE_SETTING_KEY,
};
pub use agent_logos::{agent_logo_icons, agent_logo_svg, colored_agent_logo};
pub use clone_on::CLONE_PROJECT_ON_MACHINE;
pub use commands::MenuCommand;
pub use group::MenuGroup;
pub(crate) use header::account_provider;
pub use header::{
    agent_launcher_items, agent_launcher_items_with_accounts, project_header_actions,
};
pub use host::{HeaderCommand, LauncherAgent, MenuHost, MenuOpenTarget};
pub use hover::{
    hover_slots, hover_strip, unified_default_hover_buttons, unified_hover_buttons, HoverAction,
    HoverSlot, HoverStrip,
};
pub use item::{menu_to_json, MenuItem, MenuSecondary, MenuSplit};
pub use menus::SidebarMenus;
pub use navigation::{
    ADD_PROJECT_ACTION, MENU_PANEL_DEV_SERVERS, MENU_PANEL_EXTENSIONS, MENU_PANEL_HELP,
    MENU_PANEL_MESSAGE_TYPE, MENU_PANEL_RESOURCES, MENU_PANEL_TIPS, UNIFIED_TOGGLE_GROUPS_ACTION,
};
pub use project::unified_delete_worktree_menu;
pub(crate) use run_in_box::{run_in_box_agents_page, run_in_box_locations_page};
pub use session::SessionActions;
pub use workspace::{machine_workspace_menu, workspace_menu, workspace_switch_target};
