//! A project header's context menu, and the one a user-made session group carries.
//!
//! Ported from the TypeScript sidebar page's project menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/project-menu.ts`; see git history).

use crate::sidebar_view::collections::CollectionsState;
use crate::sidebar_view::spaces::SpacesState;
use crate::sidebar_view::view::SessionView;

use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::header::ProjectHeaderRows;
use super::host::MenuOpenTarget;
use super::item::MenuItem;
use super::layout::{assemble_project_menu, entry_row, ProjectMenuSpot};
use super::membership::project_membership_menu;

/// Everything the project menu reads besides the group.
pub struct ProjectMenuInput<'a> {
    pub group: &'a MenuGroup<'a>,
    /// The rows the group draws, after the tag filter.
    pub sessions: &'a [SessionView],
    /// The collection the project belongs to, from the collections document.
    pub collection_id: Option<&'a str>,
    pub collections: &'a CollectionsState,
    pub spaces: Option<&'a SpacesState>,
    /// The project ids the user hid from the list.
    pub hidden_group: bool,
    pub open_targets: &'a [MenuOpenTarget],
    /// This computer's workspaces and the one the project is in; `None` for a remote machine's
    /// project or a daemon without workspaces.
    pub workspace: Option<(&'a ghostex_gx_protocol::SidebarWorkspacesState, &'a str)>,
    /// For a remote machine's project: this computer's workspaces, the machine, the project's id
    /// there and the workspace of this computer it shows in; `None` otherwise.
    pub remote_workspace: Option<RemoteProjectWorkspace<'a>>,
    /// Clone on… (`clone_on.rs`), when some known machine lacks the project.
    pub clone_on: Option<MenuItem>,
    /// The header buttons the menu carries (header.rs), empty for a bot or a sessions group.
    pub header: ProjectHeaderRows,
    /// The worktrees whose sessions this project lists (`sidebar_view/worktree_fold.rs`), as
    /// `(label, group id)`: they have no header of their own, so their Delete Worktree is here.
    pub folded_worktrees: &'a [(String, String)],
}

/// [`ProjectMenuInput::remote_workspace`].
pub struct RemoteProjectWorkspace<'a> {
    pub state: &'a ghostex_gx_protocol::SidebarWorkspacesState,
    pub machine_id: &'a str,
    pub project_id: &'a str,
    pub workspace_id: &'a str,
}

/// `createNativeProjectMenu`.
///
/// CDXC:Bots 2026-09-27 DECISION:
/// User: a bot's menu gains "Open in ›" with the header Open In button's targets, opening the profile folder.
/// It has no project group or Space rows, because bots belong to no Space and Bots lists them flat, so those rows did nothing.
/// Every other project's menu stays as it was.
/// Supersedes the 2026-09-26 decision, which kept the group and Space rows on a bot.
pub fn project_menu(input: &ProjectMenuInput<'_>) -> Vec<MenuItem> {
    let group = input.group;
    let group_id = group.group_id;
    let sleeping = input
        .sessions
        .iter()
        .any(|session| session.row.lifecycle_state == "sleeping");
    let running = input
        .sessions
        .iter()
        .any(|session| session.row.lifecycle_state == "running");
    let Some(project) = group.project else {
        let wake = sleeping && !running;
        let mut menu = vec![MenuItem {
            label: Some("Rename".to_string()),
            icon: Some("pencil".to_string()),
            command: Some(MenuCommand::rename_group(group_id)),
            ..MenuItem::default()
        }];
        if !input.sessions.is_empty() {
            menu.push(MenuItem::row(
                "Full Reload",
                "refresh",
                MenuCommand::command(message::full_reload_group(group_id)),
            ));
        }
        menu.push(
            MenuItem::row(
                if wake { "Wake" } else { "Sleep" },
                if wake { "player-play" } else { "moon" },
                MenuCommand::command(message::set_group_sleeping(group_id, !wake)),
            )
            .with_disabled(if wake { !sleeping } else { !running }),
        );
        menu.push(MenuItem::separator());
        // CDXC:ContextMenus 2026-11-18 DECISION:
        // User: "allow me to close the group without closing the ones inside (call it ungroup)".
        // Ungroup, above Close, removes the group without asking; its sessions stay open and go
        // back to the project's own rows (gx-core workspace_groups/group_commands.rs `plan_ungroup`).
        menu.push(MenuItem::row(
            "Ungroup",
            "folder-open",
            MenuCommand::command(message::ungroup_group(group_id)),
        ));
        menu.push(MenuItem {
            label: Some("Close".to_string()),
            icon: Some("x".to_string()),
            danger: true,
            command: Some(if input.sessions.len() > 1 {
                MenuCommand::confirm_close_group(group_id)
            } else {
                MenuCommand::command(message::close_group(group_id))
            }),
            ..MenuItem::default()
        });
        return menu;
    };

    if project.worktree.is_some() {
        // A worktree's own short menu (unchanged by Q4313): the header buttons, then its rows.
        let mut menu = input.header.in_order();
        if !menu.is_empty() {
            menu.push(MenuItem::separator());
        }
        menu.push(MenuItem::row(
            "Copy Path",
            "copy",
            MenuCommand::command(message::copy_project_path(group_id)),
        ));
        menu.push(MenuItem::row(
            "Open Folder",
            "folder-open",
            MenuCommand::command(message::open_project_in_finder(group_id)),
        ));
        menu.push(MenuItem::row(
            "Rename Worktree",
            "pencil",
            MenuCommand::command(message::prompt_rename_worktree(group_id)),
        ));
        menu.push(MenuItem::separator());
        menu.push(delete_worktree_row(group_id));
        menu.push(
            MenuItem::row(
                "Remove Worktree",
                "x",
                MenuCommand::command(message::remove_worktree_project(group_id)),
            )
            .with_disabled(!group.can_remove_project)
            .with_danger(),
        );
        menu.extend(unified_delete_worktree_menu(input.folded_worktrees));
        return menu;
    }
    let places = ProjectPlaces {
        input,
        project,
        sleeping,
        running,
    };
    assemble_project_menu(ProjectMenuSpot::Sidebar, |place| places.fill(place))
}

/// What each place of the project menu's layout shows for one project right now.
struct ProjectPlaces<'i, 'a> {
    input: &'i ProjectMenuInput<'a>,
    project: &'a crate::sidebar_view::view::ProjectContextView,
    sleeping: bool,
    running: bool,
}

impl ProjectPlaces<'_, '_> {
    fn has_inactive(&self) -> bool {
        self.input.sessions.iter().any(|session| {
            let row = &session.row;
            row.session_kind.as_deref() == Some("terminal")
                && row.lifecycle_state == "running"
                && row.activity != "working"
                && row.activity != "attention"
                && !row.has_background_work
        })
    }

    fn fill(&self, place: &str) -> Vec<MenuItem> {
        let input = self.input;
        let group = input.group;
        let group_id = group.group_id;
        let project = self.project;
        let bot = project.bot_profile.is_some();
        if let Some(rows) = input.header.place(place) {
            return rows;
        }
        let command = MenuCommand::command;
        match place {
            // CDXC:Projects 2026-11-19 WHY: a project had no way to change its name but remove + add, which lost its session records. Rename Project edits the name in place on the header (native_sidebar/rename.rs) and changes the name only. A bot's name comes from its profile and a stale machine's project can't be reached, so neither gets the tile; a worktree has Rename Worktree.
            "renameProject" if !bot && !group.is_stale => vec![entry_row(
                "renameProject",
                false,
                MenuCommand::rename_project(group_id),
            )],
            "copyPath" => vec![entry_row(
                "copyPath",
                false,
                command(message::copy_project_path(group_id)),
            )],
            "copyRemoteUrl" => group
                .git_remote_origin_url
                .filter(|url| !url.is_empty())
                .map(|url| {
                    entry_row(
                        "copyRemoteUrl",
                        false,
                        command(message::copy_project_remote_url(url)),
                    )
                })
                .into_iter()
                .collect(),
            "openFolder" => vec![entry_row(
                "openFolder",
                false,
                command(message::open_project_in_finder(group_id)),
            )],
            "openIn" if bot && !group.is_remote && !input.open_targets.is_empty() => {
                vec![MenuItem::submenu(
                    "Open in",
                    "external-link",
                    input
                        .open_targets
                        .iter()
                        .map(|target| {
                            MenuItem::row(
                                &target.label,
                                &target.icon,
                                command(message::open_project_in_target(
                                    group_id,
                                    &target.target_id,
                                    &target.label,
                                )),
                            )
                        })
                        .collect(),
                )]
            }
            "cloneOn" if !bot => input.clone_on.iter().cloned().collect(),
            // Project Group, then Spaces for a project in no group (a group owns its projects'
            // Space membership).
            "projectGroup" | "spaces" if !bot => {
                let mut rows = project_membership_menu(
                    group_id,
                    Some(project.project_id.as_str()),
                    input.collection_id,
                    input.collections,
                    input.spaces,
                )
                .into_iter();
                let row = if place == "projectGroup" {
                    rows.next()
                } else {
                    rows.nth(1)
                };
                row.into_iter().collect()
            }
            "moveToWorkspace" if !bot && !group.is_stale => self.move_to_workspace(),
            // CDXC:ContextMenus 2026-09-29 DECISION:
            // User: a project's menu does not have both a sessions group row and Add to Group > New Group; only
            // the second stays. A session still starts a sessions group with its own Move to New
            // Group (Q1312, 2026-11-05: the project menu stays without one).
            "hide" => vec![entry_row(
                "hide",
                input.hidden_group,
                MenuCommand::project_membership(group_id, "hide", None),
            )],
            "sleepInactive" => {
                let all_sleeping = self.sleeping && !self.running;
                vec![entry_row(
                    "sleepInactive",
                    all_sleeping,
                    command(if all_sleeping {
                        message::wake_project_sleeping_sessions(group_id)
                    } else {
                        message::sleep_inactive_project_sessions(group_id)
                    }),
                )
                .with_disabled(!all_sleeping && !self.has_inactive())]
            }
            // The session menu's Full Reload, without its key: the key reloads one session.
            "fullReload" if !input.sessions.is_empty() => vec![MenuItem {
                hotkey: None,
                ..entry_row(
                    "fullReload",
                    false,
                    command(message::full_reload_project_sessions(group_id)),
                )
            }],
            // CDXC:Autopilot 2026-11-07 DECISION:
            // User: "we need to be able to right click on a project and in advanced we disable it for it (also applies for that project's worktrees)". The row shows while Autopilot is on and goes by project (gxserver sends `autopilot` on the project only then), ticked while Autopilot covers the project; a worktree follows its main project, so its own menu has no row. Projects of a connected computer are left out: their Autopilot is that computer's. Since Q4313 (2026-11-20) it is More › Autopilot.
            "autopilot" if !group.is_remote && !bot => project
                .autopilot
                .map(|inside| {
                    entry_row(
                        "autopilot",
                        false,
                        command(message::set_project_autopilot(
                            project.project_id.as_str(),
                            !inside,
                        )),
                    )
                    .with_checked(inside)
                })
                .into_iter()
                .collect(),
            "closeInactive" => vec![entry_row(
                "closeInactive",
                false,
                command(message::close_inactive_project_sessions(group_id)),
            )
            .with_disabled(!self.has_inactive())],
            "closeProject" => vec![entry_row(
                "closeProject",
                false,
                command(message::close_project(group_id)),
            )
            .with_disabled(!group.can_remove_project && !group.is_remote)],
            "deleteWorktree" => unified_delete_worktree_menu(input.folded_worktrees),
            _ => Vec::new(),
        }
    }

    fn move_to_workspace(&self) -> Vec<MenuItem> {
        let input = self.input;
        let group = input.group;
        // CDXC:Workspaces 2026-10-09 WHY:
        // Move to Workspace belongs to the Workspaces built-in extension. A daemon with it off publishes no workspaces document, so `input.workspace` is `None` and the row does not show, on the desktop and the web build alike.
        // CDXC:WorkMode 2026-10-24 DECISION: User: Issues Workflow has "No switch", so the Work Mode row that sat here is gone (supersedes the 2026-10-09 "right-click the project → Work mode" decision).
        if let Some((workspaces, project_workspace_id)) =
            input.workspace.filter(|_| !group.is_remote)
        {
            return vec![super::workspace::move_to_workspace_menu(
                workspaces,
                self.project.project_id.as_str(),
                project_workspace_id,
            )];
        }
        // A remote machine's project moves on this computer only (CDXC:Workspaces 2026-11-19 in
        // server/src/workspaces/store.rs `assign_remote_project_workspace_in`).
        input
            .remote_workspace
            .as_ref()
            .filter(|remote| group.is_remote && remote.state.workspaces.len() > 1)
            .map(|remote| {
                super::workspace::move_remote_project_to_workspace_menu(
                    remote.state,
                    remote.machine_id,
                    remote.project_id,
                    remote.workspace_id,
                )
            })
            .into_iter()
            .collect()
    }
}

/// `menu` without the Rename Project tile, for a project menu shown away from the project's
/// header (the Unified sidebar names a row by its repository, and an extension panel draws no
/// header), where the name field it opens has nowhere to show.
pub fn without_project_rename(menu: &mut Vec<MenuItem>) {
    let is_rename = |item: &MenuItem| {
        item.command
            .as_ref()
            .is_some_and(MenuCommand::is_rename_project)
    };
    menu.retain(|item| !is_rename(item));
    for item in menu.iter_mut() {
        if let Some(tiles) = item.quick.as_mut() {
            tiles.retain(|tile| !is_rename(tile));
        }
    }
    menu.retain(|item| item.quick.as_ref().is_none_or(|tiles| !tiles.is_empty()));
}

/// The Delete Worktree row: it opens the Delete Worktree dialog for the worktree project
/// `group_id`, whose checks and gxserver call are the same wherever the row sits.
fn delete_worktree_row(group_id: &str) -> MenuItem {
    MenuItem::row(
        "Delete Worktree",
        "trash",
        MenuCommand::command(message::prompt_delete_worktree(group_id)),
    )
    .with_danger()
}

/// What the Unified sidebar adds to a repository's menu (its group header with Group by Project,
/// and its row in the filter menu) for the repository's worktrees: a Delete Worktree submenu with
/// one row per worktree, `(label, group id)`. Empty when the repository has none.
///
/// CDXC:Sidebar 2026-11-06 DECISION:
/// User (Elliott on Discord): the Unified sidebar with Group by Project had no way to delete a worktree, because a repository's header opens its main checkout's Project sidebar menu and its worktrees sit inside that header with no entry of their own. The header and the filter row now end with "Delete Worktree ›" listing that repository's worktrees on every machine, and a card of a worktree's session carries "Delete Worktree" too. Every row opens the Project sidebar's own Delete Worktree dialog (`promptDeleteWorktreeForGroup`), so the uncommitted-changes and running-session checks and the gxserver delete are the one path.
pub fn unified_delete_worktree_menu(worktrees: &[(String, String)]) -> Vec<MenuItem> {
    if worktrees.is_empty() {
        return Vec::new();
    }
    vec![
        MenuItem::separator(),
        MenuItem::submenu(
            "Delete Worktree",
            "trash",
            worktrees
                .iter()
                .map(|(label, group_id)| {
                    // An open panel refreshes a row's label from the row with the same command
                    // in the Project sidebar's menus (native_sidebar/menu_state.rs), which is the
                    // worktree's own "Delete Worktree"; the tag keeps this row's name. The
                    // handler reads only `groupId`.
                    let mut command = message::prompt_delete_worktree(group_id);
                    command["menu"] = "unifiedRepository".into();
                    MenuItem::row(label, "git-branch", MenuCommand::command(command)).with_danger()
                })
                .collect(),
        ),
    ]
}

/// What a worktree session its parent project lists adds to its menu in the Project sidebar
/// (`sidebar_view/worktree_fold.rs`): the worktree header's Rename Worktree and Delete Worktree,
/// since the worktree has no header there.
pub(crate) fn folded_worktree_rows(group_id: &str) -> [MenuItem; 3] {
    [
        MenuItem::separator(),
        MenuItem::row(
            "Rename Worktree",
            "pencil",
            MenuCommand::command(message::prompt_rename_worktree(group_id)),
        ),
        delete_worktree_row(group_id),
    ]
}

/// The Delete Worktree rows a worktree session's card adds to its menu in the Unified sidebar.
pub(crate) fn unified_card_delete_worktree(group_id: &str) -> [MenuItem; 2] {
    [MenuItem::separator(), delete_worktree_row(group_id)]
}
