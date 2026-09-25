//! The Spaces and Add to Group submenus a project or a collection carries.
//!
//! Ported from the TypeScript sidebar page's membership menus (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/membership.ts`; see git history).

use crate::sidebar_view::collections::CollectionsState;
use crate::sidebar_view::spaces::SpacesState;

use super::commands::MenuCommand;
use super::item::MenuItem;

/// Which member a Spaces submenu is being built for.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SpaceMember<'a> {
    Collection(&'a str),
    Project(&'a str),
}

impl<'a> SpaceMember<'a> {
    fn command(self, space_id: Option<&str>) -> MenuCommand {
        match self {
            SpaceMember::Collection(collection_id) => {
                MenuCommand::space_membership(space_id, Some(collection_id), None)
            }
            SpaceMember::Project(project_id) => {
                MenuCommand::space_membership(space_id, None, Some(project_id))
            }
        }
    }

    fn is_member_of(self, spaces: &SpacesState, space_id: &str) -> bool {
        let Some(space) = spaces.spaces.get(space_id) else {
            return false;
        };
        match self {
            SpaceMember::Collection(collection_id) => space
                .member_collection_ids
                .iter()
                .any(|member| member == collection_id),
            SpaceMember::Project(project_id) => space
                .member_project_ids
                .iter()
                .any(|member| member == project_id),
        }
    }
}

/// `createNativeSpaceMembershipMenu`: the Spaces page, or nothing when Spaces are off.
pub(crate) fn space_membership_menu(
    spaces: Option<&SpacesState>,
    member: SpaceMember<'_>,
) -> Option<MenuItem> {
    let spaces = spaces?;
    let mut children: Vec<MenuItem> = spaces
        .order
        .iter()
        .filter_map(|space_id| {
            let space = spaces.spaces.get(space_id)?;
            Some(MenuItem {
                label: Some(space.name.clone()),
                icon: Some(space.icon.clone()),
                checked: member.is_member_of(spaces, space_id),
                command: Some(member.command(Some(space_id))),
                ..MenuItem::default()
            })
        })
        .collect();
    children.push(MenuItem::separator());
    children.push(MenuItem::row("New Space", "plus", member.command(None)));
    Some(MenuItem::submenu("Spaces", "stack", children).with_page())
}

/// `createNativeProjectMembershipMenu`: Add to Group, plus the Spaces page for a project that is
/// not in a collection (a collection owns its projects' Space membership).
pub(crate) fn project_membership_menu(
    group_id: &str,
    project_id: Option<&str>,
    current_collection_id: Option<&str>,
    collections: &CollectionsState,
    spaces: Option<&SpacesState>,
) -> Vec<MenuItem> {
    let Some(project_id) = project_id else {
        return Vec::new();
    };
    let mut children = vec![MenuItem::row(
        "New Project Group",
        "plus",
        MenuCommand::project_membership(group_id, "createCollection", None),
    )];
    for collection in &collections.collections {
        children.push(MenuItem {
            label: Some(collection.title.clone()),
            checked: current_collection_id == Some(collection.collection_id.as_str()),
            command: Some(MenuCommand::project_membership(
                group_id,
                "moveCollection",
                Some(&collection.collection_id),
            )),
            ..MenuItem::default()
        });
    }
    if current_collection_id.is_some() {
        children.push(MenuItem::separator());
        children.push(MenuItem::row(
            "Remove from Group",
            "x",
            MenuCommand::project_membership(group_id, "moveCollection", None),
        ));
    }
    let mut items = vec![MenuItem::submenu("Add to Group", "plus", children).with_page()];
    if current_collection_id.is_none() {
        if let Some(menu) = space_membership_menu(spaces, SpaceMember::Project(project_id)) {
            items.push(menu);
        }
    }
    items
}
