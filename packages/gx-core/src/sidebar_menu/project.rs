//! A project header's context menu, and the one a user-made session group carries.
//!
//! Ported from the TypeScript sidebar page's project menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/project-menu.ts`; see git history).

use crate::sidebar_view::collections::CollectionsState;
use crate::sidebar_view::spaces::SpacesState;
use crate::sidebar_view::view::SessionView;

use super::commands::{MenuCommand, message};
use super::group::MenuGroup;
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
}

/// `createNativeProjectMenu`.
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
    if project.worktree.is_some() {
        menu.push(MenuItem::row(
            "Rename Worktree",
            "pencil",
            MenuCommand::command(message::prompt_rename_worktree(group_id)),
        ));
        menu.push(MenuItem::separator());
        menu.push(
            MenuItem::row(
                "Delete Worktree",
                "trash",
                MenuCommand::command(message::prompt_delete_worktree(group_id)),
            )
            .with_danger(),
        );
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
    menu.extend(project_membership_menu(
        group_id,
        Some(project.project_id.as_str()),
        input.collection_id,
        input.collections,
        input.spaces,
    ));
    menu.push(MenuItem::separator());
    if group.can_create_session_group {
        menu.push(MenuItem::row(
            "New Group",
            "plus",
            MenuCommand::command(message::create_group(group_id)),
        ));
    }
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
