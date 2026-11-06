//! A project header's context menu, and the one a user-made session group carries.
//!
//! Ported from the TypeScript sidebar page's project menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/project-menu.ts`; see git history).

use crate::sidebar_view::collections::CollectionsState;
use crate::sidebar_view::spaces::SpacesState;
use crate::sidebar_view::view::SessionView;

use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::host::MenuOpenTarget;
use super::item::MenuItem;
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

    let mut menu = vec![
        MenuItem::row(
            "Copy Path",
            "copy",
            MenuCommand::command(message::copy_project_path(group_id)),
        ),
        MenuItem::row(
            "Open Folder",
            "folder-open",
            MenuCommand::command(message::open_project_in_finder(group_id)),
        ),
    ];
    if project.bot_profile.is_some() && !group.is_remote && !input.open_targets.is_empty() {
        menu.push(MenuItem::submenu(
            "Open in",
            "external-link",
            input
                .open_targets
                .iter()
                .map(|target| {
                    MenuItem::row(
                        &target.label,
                        &target.icon,
                        MenuCommand::command(message::open_project_in_target(
                            group_id,
                            &target.target_id,
                            &target.label,
                        )),
                    )
                })
                .collect(),
        ));
    }
    if project.worktree.is_some() {
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
        return menu;
    }
    if let Some(remote_url) = group.git_remote_origin_url.filter(|url| !url.is_empty()) {
        menu.push(MenuItem::row(
            "Copy Remote URL",
            "link",
            MenuCommand::command(message::copy_project_remote_url(remote_url)),
        ));
    }
    if project.bot_profile.is_none() {
        menu.extend(project_membership_menu(
            group_id,
            Some(project.project_id.as_str()),
            input.collection_id,
            input.collections,
            input.spaces,
        ));
    }
    // CDXC:Workspaces 2026-10-09 WHY:
    // Move to workspace belongs to the Workspaces built-in extension. A daemon with it off publishes no workspaces document, so `input.workspace` is `None` and the row does not show, on the desktop and the web build alike.
    // CDXC:WorkMode 2026-10-24 DECISION: User: Issues Workflow has "No switch", so the Work Mode row that sat here is gone (supersedes the 2026-10-09 "right-click the project → Work mode" decision).
    if let Some((workspaces, project_workspace_id)) = input
        .workspace
        .filter(|_| project.bot_profile.is_none() && !group.is_remote && !group.is_stale)
    {
        menu.push(super::workspace::move_to_workspace_menu(
            workspaces,
            project.project_id.as_str(),
            project_workspace_id,
        ));
    }
    menu.push(MenuItem::separator());
    // CDXC:ContextMenus 2026-09-29 DECISION:
    // User: a project's menu does not have both a sessions group row and Add to Group > New Group; only
    // the second stays. A session still starts a sessions group with its own Move to New Sessions
    // Group (Q1312, 2026-11-05: the project menu stays without one).
    menu.push(MenuItem::row(
        if input.hidden_group { "Unhide" } else { "Hide" },
        "eye-off",
        MenuCommand::project_membership(group_id, "hide", None),
    ));
    let all_sleeping = sleeping && !running;
    let has_inactive = input.sessions.iter().any(|session| {
        let row = &session.row;
        row.session_kind.as_deref() == Some("terminal")
            && row.lifecycle_state == "running"
            && row.activity != "working"
            && row.activity != "attention"
            && !row.has_background_work
    });
    menu.push(
        MenuItem::row(
            if all_sleeping {
                "Wake"
            } else {
                "Sleep Inactive"
            },
            if all_sleeping { "player-play" } else { "moon" },
            MenuCommand::command(if all_sleeping {
                message::wake_project_sleeping_sessions(group_id)
            } else {
                message::sleep_inactive_project_sessions(group_id)
            }),
        )
        .with_disabled(!all_sleeping && !has_inactive),
    );
    if !input.sessions.is_empty() {
        menu.push(MenuItem::row(
            "Full Reload",
            "refresh",
            MenuCommand::command(message::full_reload_project_sessions(group_id)),
        ));
    }
    menu.push(MenuItem::separator());
    menu.push(
        MenuItem::row(
            "Close Inactive",
            "x",
            MenuCommand::command(message::close_inactive_project_sessions(group_id)),
        )
        .with_disabled(!has_inactive)
        .with_danger(),
    );
    menu.push(
        MenuItem::row(
            "Close Project",
            "x",
            MenuCommand::command(message::close_project(group_id)),
        )
        .with_disabled(!group.can_remove_project && !group.is_remote)
        .with_danger(),
    );
    menu
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
