//! The facade a host builds a drawn list's menus through.
//!
//! Holds no state of its own: the tag catalog, the Spaces and the collections are derived from the
//! store when it is made, and every menu is built from the view it was made for, so a menu can
//! never be older than the row it belongs to.

use std::collections::BTreeSet;

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, MachineId, SessionKey};
use crate::sidebar_view::collections::CollectionsState;
use crate::sidebar_view::machine_spaces::spaces_enabled_on;
use crate::sidebar_view::spaces::SpacesState;
use crate::sidebar_view::tags::TagCatalog;
use crate::sidebar_view::{
    CollectionView, GroupView, SessionRow, SessionView, SidebarInputs, SidebarView,
    LOCAL_MACHINE_ID,
};

use super::group::MenuGroup;
use super::host::MenuHost;
use super::hover::HoverAction;
use super::item::MenuItem;
use super::session::SessionActions;
use super::{bulk, collection, header, navigation, project, session};

/// Builds the menus of one drawn list. Holds no state of its own: everything is derived from the
/// store and the list it was made for, so a menu can never be older than the row it belongs to.
pub struct SidebarMenus<'a> {
    view: &'a SidebarView,
    inputs: &'a SidebarInputs,
    host: &'a MenuHost,
    /// The catalog of the machine whose rows are drawn: what a ROW's own tag submenu offers, which
    /// is the `customTags` the TypeScript hands `createNativeSessionActions`.
    catalog: TagCatalog,
    /// Every machine's catalog, this computer's first: what a tag id RESOLVES to for a label or an
    /// icon (`getSessionTagCatalogs`).
    label_catalog: TagCatalog,
    /// This computer's alone: which tag filters the Sort & Filter menu offers at all
    /// (`normalizeSidebarSessionTagListItems(settings, state.customSessionTags)`).
    filter_catalog: TagCatalog,
    spaces: Option<SpacesState>,
    collections: CollectionsState,
    /// This computer's workspaces, while this computer's section is drawn and its daemon has them.
    workspaces: Option<ghostex_gx_protocol::SidebarWorkspacesState>,
    /// The workspace each of this computer's projects is in (worktrees follow their parent).
    project_workspaces: std::collections::BTreeMap<String, String>,
    /// The drawn machine's daemon id (`S…`), the first part of a session's global ref; absent
    /// until a stream frame has named it.
    server_id: Option<String>,
    /// Every loaded machine's daemon id, for a row drawn here that belongs to another computer
    /// (`row_menu_group`).
    server_ids: std::collections::BTreeMap<MachineId, String>,
    /// The drawn orchestrators that have a thread Sleep Inactive Threads would sleep. Left empty
    /// when no orchestrator is drawn, so a list without one never walks the rows for it.
    coordinators_with_inactive_threads: std::collections::HashSet<SessionKey>,
    /// This computer has a Cloud Box, so the Sort & Filter menu offers the Cloud Box filter.
    has_cloud_boxes: bool,
    now_ms: u64,
}

impl<'a> SidebarMenus<'a> {
    pub fn new(
        core: &Core,
        view: &'a SidebarView,
        inputs: &'a SidebarInputs,
        host: &'a MenuHost,
        now_ms: u64,
    ) -> Self {
        let machine = if view.selected_machine_id == LOCAL_MACHINE_ID {
            MachineId::Local
        } else {
            MachineId::Remote(view.selected_machine_id.clone())
        };
        let store = core.presentation();
        let side_state = store.machine(&machine).map(|entry| entry.side_state());
        Self {
            view,
            inputs,
            host,
            catalog: TagCatalog::from_state(
                side_state.and_then(|side| side.custom_session_tags.as_ref()),
            ),
            label_catalog: TagCatalog::merged(
                std::iter::once(MachineId::Local)
                    .chain(
                        store
                            .machines()
                            .map(|(machine, _)| machine.clone())
                            .filter(|machine| !machine.is_local()),
                    )
                    .map(|machine| {
                        store
                            .machine(&machine)
                            .and_then(|entry| entry.side_state().custom_session_tags.as_ref())
                    })
                    .collect::<Vec<_>>(),
            ),
            filter_catalog: TagCatalog::from_state(
                store
                    .machine(&MachineId::Local)
                    .and_then(|entry| entry.side_state().custom_session_tags.as_ref()),
            ),
            // The Spaces submenu follows the same switch the drawn list does, the drawn machine's:
            // with Spaces off the sidebar has none to offer.
            // Only the window's workspace's Spaces (`sidebar_view/workspaces.rs`).
            spaces: spaces_enabled_on(store, inputs, &machine)
                .then(|| crate::sidebar_view::window_spaces(store, inputs, &machine))
                .flatten()
                .map(|spaces| SpacesState::from_wire(&spaces)),
            workspaces: crate::sidebar_view::window_workspace(store, inputs, &machine)
                .map(|workspace| workspace.state.clone()),
            project_workspaces: match (
                crate::sidebar_view::window_workspace(store, inputs, &machine),
                store.loaded(&machine),
            ) {
                (Some(workspace), Some(loaded)) => loaded
                    .projects()
                    .iter()
                    .map(|project| {
                        (
                            project.project_id.clone(),
                            crate::sidebar_view::project_workspace_id(
                                workspace.state,
                                loaded,
                                project,
                            )
                            .to_string(),
                        )
                    })
                    .collect(),
                _ => std::collections::BTreeMap::new(),
            },
            collections: match side_state.and_then(|side| side.project_collections.as_ref()) {
                Some(state) => CollectionsState::from_wire(state),
                None => inputs
                    .host
                    .stored_project_collections
                    .as_ref()
                    .map(CollectionsState::from_local_json)
                    .unwrap_or_default(),
            },
            server_id: store
                .loaded(&machine)
                .map(|loaded| loaded.server_id.clone())
                .filter(|server_id| !server_id.is_empty()),
            server_ids: store
                .machines()
                .filter_map(|(machine, _)| {
                    let server_id = &store.loaded(machine)?.server_id;
                    (!server_id.is_empty()).then(|| (machine.clone(), server_id.clone()))
                })
                .collect(),
            coordinators_with_inactive_threads: if view.groups.iter().any(|group| {
                group
                    .core
                    .sessions
                    .iter()
                    .any(|session| session.row.is_coordinator)
            }) {
                crate::sidebar_actions::coordinators_with_inactive_threads(core, &machine)
            } else {
                Default::default()
            },
            has_cloud_boxes: !core.cloud_boxes().is_empty(),
            now_ms,
        }
    }

    /// One number over everything a menu reads from the store besides the row or group it is
    /// built for: the custom tag catalog, the Spaces and the collections. A host that caches a
    /// built menu compares this to know when the cached one went stale, because none of the three
    /// touches the `GroupCore` the menu belongs to.
    pub fn context_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.catalog.hash(&mut hasher);
        self.label_catalog.hash(&mut hasher);
        self.filter_catalog.hash(&mut hasher);
        self.spaces.hash(&mut hasher);
        self.collections.hash(&mut hasher);
        self.workspaces.hash(&mut hasher);
        self.project_workspaces.hash(&mut hasher);
        self.server_id.hash(&mut hasher);
        self.server_ids.hash(&mut hasher);
        hasher.finish()
    }

    /// The group facts every menu of a group and of its rows reads.
    pub fn menu_group(&self, group: &'a GroupView) -> MenuGroup<'a> {
        let core = &group.core;
        MenuGroup {
            group_id: core.group_id.as_str(),
            storage_id: core.storage_id.as_str(),
            title: core.title.as_str(),
            is_remote: core.remote_machine.is_some(),
            remote_machine_name: core
                .remote_machine
                .as_ref()
                .map(|remote| remote.machine_name.as_str()),
            is_stale: core.is_stale,
            // A project group and a user-made group can take a session into a new group.
            //
            // CDXC:ContextMenus 2026-09-20 WHY:
            // Asked of the project row rather than of the group id, which is what
            // `createGpuiRemotePresentationSidebarGroups` does (`project !== undefined`). The id
            // test alone was written for this computer, where a project group always has a row;
            // a remote group whose project the machine did not publish is the case it was never
            // asked about, and a group-id test would offer the item on a group with nothing to put
            // in it.
            //
            // A bot's sessions stay under the bot: Bots lists them flat, with no groups (the bot
            // row's own menu has no New Group, see sidebar_menu/project.rs), so a group made from
            // one would leave Bots for Projects > Other.
            can_create_session_group: core
                .project_context
                .as_ref()
                .is_some_and(|project| project.bot_profile.is_none())
                || is_user_made_group(core.group_id.as_str()),
            // The projection never sets it, so the Focus item never appears.
            can_focus_mode: false,
            project: core.project_context.as_ref(),
            // The projection marks every project group removable; only a remote machine's are not.
            can_remove_project: true,
            git_remote_origin_url: core
                .project_context
                .as_ref()
                .and_then(|project| project.git_remote_origin_url.as_deref()),
            server_id: self.server_id.clone(),
            agent_place: super::AgentChoicePlace::for_group(self.view, group),
        }
    }

    /// The group facts the header's creating actions read: for an entry the one-list sidebar
    /// merged, the copy a session of it last started in (`GroupCore::action_target`), so New
    /// Terminal, Create Agent and the ⋯ menu's Add Worktree, History and the rest run there. The
    /// rest of the menu (fold, hide, Spaces, collections) stays the entry's own.
    pub fn action_menu_group(&self, group: &'a GroupView) -> MenuGroup<'a> {
        let mut menu_group = self.menu_group(group);
        if let Some(target) = &group.core.action_target {
            menu_group.group_id = target.group_id.as_str();
            menu_group.is_remote = target.remote_machine.is_some();
            menu_group.remote_machine_name = target
                .remote_machine
                .as_ref()
                .map(|remote| remote.machine_name.as_str());
            menu_group.project = Some(&target.project_context);
            menu_group.git_remote_origin_url =
                target.project_context.git_remote_origin_url.as_deref();
        }
        menu_group
    }

    /// The group facts a row's own menus read. Another machine's session in an entry the
    /// one-list sidebar merged (`SessionRow::machine_name`) is that machine's, so its menus offer
    /// what a remote row offers, and no session group of this entry to make or join.
    fn row_menu_group(&self, group: &'a GroupView, row: &'a SessionRow) -> MenuGroup<'a> {
        let mut menu_group = self.menu_group(group);
        // CDXC:ContextMenus 2026-11-12 WHY: a row of another computer drawn in this list (the one
        // list of every computer, a thread under a coordinator here) took the drawn machine's
        // daemon id, so its Copy ID, Copy Link, Copy Details and a drop on a chat box named this
        // computer (`S90:P715t:G39v8` for a session on the Windows computer `S8g`). The ref names
        // the row's own computer; one not loaded yet has no ref, so those rows wait for it.
        if let Some(key) = row.key.as_ref() {
            menu_group.server_id = self.server_ids.get(&key.machine).cloned();
        }
        if let Some(machine_name) = row.machine_name.as_deref() {
            menu_group.is_remote = true;
            menu_group.remote_machine_name = Some(machine_name);
            menu_group.can_create_session_group = false;
        }
        // A worktree session its parent project lists (`sidebar_view/worktree_fold.rs`) is the
        // worktree project's: its menus act there, and it joins no session group of the parent.
        if let Some(folded) = row.worktree.as_ref().and_then(|worktree| {
            group
                .core
                .folded_worktrees
                .iter()
                .find(|folded| folded.group_id == worktree.group_id)
        }) {
            menu_group.group_id = folded.group_id.as_str();
            menu_group.project = Some(&folded.project_context);
            menu_group.git_remote_origin_url =
                folded.project_context.git_remote_origin_url.as_deref();
            menu_group.can_create_session_group = false;
        }
        menu_group
    }

    /// The hover buttons and the placeholder menu a row publishes.
    pub fn row_actions(&self, group: &GroupView, session: &SessionView) -> SessionActions {
        let menu_group = self.row_menu_group(group, &session.row);
        session::session_hover_actions(&self.session_input(&menu_group, &session.row, &[], None))
    }

    /// The whole context menu of one row: the bulk menu when the row is part of a multi-selection,
    /// its own menu otherwise. `None` when the list no longer draws the row.
    pub fn row_menu(&self, sidebar_session_id: &str) -> Option<Vec<MenuItem>> {
        let (group, session) = self.find_row(sidebar_session_id)?;
        if self
            .inputs
            .ui
            .selected_session_ids
            .iter()
            .any(|selected| selected == sidebar_session_id)
        {
            if let Some(menu) = self.bulk_menu() {
                return Some(menu);
            }
        }
        let below = self.rows_below(group, &session.row);
        let below: Vec<&SessionRow> = below.iter().map(|session| &*session.row).collect();
        let menu_group = self.row_menu_group(group, &session.row);
        let group_moves = self.group_moves(group, &session.row);
        let mut menu = session::session_menu(&self.session_input(
            &menu_group,
            &session.row,
            &below,
            group_moves.as_ref(),
        ));
        // A worktree session its parent project lists carries its worktree's Rename and Delete
        // Worktree, since the worktree has no header here (`sidebar_view/worktree_fold.rs`).
        if let Some(worktree) = &session.row.worktree {
            menu.extend(super::project::folded_worktree_rows(
                worktree.group_id.as_str(),
            ));
        }
        Some(menu)
    }

    /// What the row's Copy ID copies (`[<session title>](<Global Ref>)`), which a session row
    /// dropped on a chat's composer types too. `None` when the list does not draw the row or its
    /// machine's daemon id is not known yet.
    pub fn row_mention(&self, sidebar_session_id: &str) -> Option<String> {
        let (group, session) = self.find_row(sidebar_session_id)?;
        let menu_group = self.row_menu_group(group, &session.row);
        super::clipboard::session_mention_text(&session.row, &menu_group.details())
    }

    /// A card's menu in the Unified sidebar: the row's own menu, ending with Delete Worktree
    /// when the session is in a worktree project, since the Unified sidebar draws no header of
    /// the worktree's own to carry it (`unified_delete_worktree_menu`). A multi-selection's bulk
    /// menu stays as it is.
    pub fn unified_row_menu(&self, sidebar_session_id: &str) -> Option<Vec<MenuItem>> {
        let mut menu = self.row_menu(sidebar_session_id)?;
        let (group, _) = self.find_row(sidebar_session_id)?;
        let bulk = self
            .inputs
            .ui
            .selected_session_ids
            .iter()
            .any(|selected| selected == sidebar_session_id)
            && self.bulk_menu().is_some();
        let worktree = group
            .core
            .project_context
            .as_ref()
            .is_some_and(|project| project.worktree.is_some());
        if worktree && !bulk {
            menu.extend(super::project::unified_card_delete_worktree(
                group.core.group_id.as_str(),
            ));
        }
        Some(menu)
    }

    /// The submenu behind one hover button of one row.
    pub fn row_hover_submenu(
        &self,
        sidebar_session_id: &str,
        action: HoverAction,
    ) -> Option<Vec<MenuItem>> {
        let (group, session) = self.find_row(sidebar_session_id)?;
        let menu_group = self.row_menu_group(group, &session.row);
        Some(session::session_hover_submenu(
            &self.session_input(&menu_group, &session.row, &[], None),
            action,
        ))
    }

    /// The submenu of one of a row's actions whatever the hover strip shows
    /// (`session::session_action_submenu`). `None` when the list does not draw the row.
    pub fn row_action_submenu(
        &self,
        sidebar_session_id: &str,
        action: HoverAction,
    ) -> Option<Vec<MenuItem>> {
        let (group, session) = self.find_row(sidebar_session_id)?;
        let menu_group = self.row_menu_group(group, &session.row);
        Some(session::session_action_submenu(
            &self.session_input(&menu_group, &session.row, &[], None),
            action,
        ))
    }

    /// The menu a multi-selection carries, or `None` below two selected rows.
    pub fn bulk_menu(&self) -> Option<Vec<MenuItem>> {
        let selected: Vec<&SessionRow> = self
            .inputs
            .ui
            .selected_session_ids
            .iter()
            .filter_map(|session_id| self.find_row(session_id).map(|(_, session)| &*session.row))
            .collect();
        bulk::bulk_menu(&bulk::BulkMenuInput {
            selected: &selected,
            settings: &self.inputs.settings,
            catalog: &self.catalog,
        })
    }

    /// A section heading's context menu, or `None` for a heading that has none (only Parked does).
    pub fn section_menu(
        &self,
        group: &GroupView,
        section: &crate::sidebar_view::SectionView,
    ) -> Option<Vec<MenuItem>> {
        super::section::section_menu(group, section)
    }

    /// A project header's context menu: the buttons the header no longer draws, then the menu proper.
    pub fn project_menu(&self, group: &GroupView) -> Vec<MenuItem> {
        let menu_group = self.menu_group(group);
        let mut menu = header::project_header_menu_rows(
            &self.action_menu_group(group),
            &self.inputs.settings,
            self.host,
        );
        if !menu.is_empty() {
            menu.push(MenuItem::separator());
        }
        menu.extend(project::project_menu(&project::ProjectMenuInput {
            group: &menu_group,
            sessions: &group.core.sessions,
            collection_id: group.collection_id.as_deref(),
            collections: &self.collections,
            spaces: self.spaces.as_ref(),
            hidden_group: self
                .inputs
                .ui
                .hidden_items
                .group_ids
                .iter()
                .any(|group_id| *group_id == group.core.group_id),
            open_targets: &self.host.open_targets,
            clone_on: super::clone_on::clone_on_menu(group, self.inputs),
            workspace: self.workspaces.as_ref().and_then(|workspaces| {
                let project_id = group.core.project_context.as_ref()?.project_id.as_str();
                Some((
                    workspaces,
                    self.project_workspaces.get(project_id)?.as_str(),
                ))
            }),
        }));
        // The worktrees whose sessions this project lists (`sidebar_view/worktree_fold.rs`) have
        // no header of their own, so their Delete Worktree is here, as on a Unified repository.
        let worktrees: Vec<(String, String)> = group
            .core
            .folded_worktrees
            .iter()
            .map(|folded| {
                let label = match &folded.remote_machine {
                    Some(remote) => format!("{} · {}", folded.label(), remote.machine_name),
                    None => folded.label().to_string(),
                };
                (label, folded.group_id.clone())
            })
            .collect();
        menu.extend(project::unified_delete_worktree_menu(&worktrees));
        menu
    }

    /// A project header's buttons: the ⋯ menu, the last-used agent and the agent picker.
    ///
    /// CDXC:Sidebar 2026-10-08 DECISION:
    /// User (for the phone, applied to the desktop header too): keep only the agent picker and New agent on the project row and move the other buttons into the 3-dots menu. The ⋯ button opens the same menu as a right click.
    pub fn header_actions(&self, group: &GroupView) -> Vec<MenuItem> {
        let menu_group = self.action_menu_group(group);
        let mut actions =
            header::project_header_actions(&menu_group, &self.inputs.settings, self.host);
        if !header::project_header_menu_rows(&menu_group, &self.inputs.settings, self.host)
            .is_empty()
        {
            actions.insert(
                0,
                MenuItem {
                    label: Some("More".to_string()),
                    icon: Some("dots".to_string()),
                    children: Some(self.project_menu(group)),
                    ..MenuItem::default()
                },
            );
        }
        actions
    }

    /// A collection's context menu.
    pub fn collection_menu(&self, collection: &CollectionView) -> Vec<MenuItem> {
        let sessions: Vec<&SessionView> =
            crate::sidebar_view::collection_groups(&self.view.groups, &collection.group_ids)
                .flat_map(|group| group.core.sessions.iter())
                .collect();
        collection::collection_menu(&collection::CollectionMenuInput {
            collection_id: collection.collection_id.as_str(),
            color: collection.color.as_str(),
            sessions: &sessions,
            hidden: self
                .inputs
                .ui
                .hidden_items
                .collection_keys
                .contains(&collection.storage_id),
            tag_list_items: &self.inputs.settings.tag_list_items,
            catalog: &self.catalog,
            spaces: self.spaces.as_ref(),
        })
    }

    /// The sidebar's own "more" menu.
    pub fn more_menu(&self) -> Vec<MenuItem> {
        let drawn = crate::sidebar_view::toggle_all_project_ids(
            self.view.groups.iter().map(|group| &group.core.group_id),
        );
        navigation::more_menu(&navigation::MoreMenuInput {
            ui: &self.inputs.ui,
            settings: &self.inputs.settings,
            catalog: &self.label_catalog,
            filter_catalog: &self.filter_catalog,
            host: self.host,
            drawn_project_group_ids: &drawn,
            // Asked of the registry, not the drawn rows: another ticked filter may be hiding them.
            cloud_box_rows: self.has_cloud_boxes,
        })
    }

    fn session_input<'b>(
        &'b self,
        group: &'b MenuGroup<'b>,
        row: &'b SessionRow,
        below: &'b [&'b SessionRow],
        group_moves: Option<&'b session::SessionGroupMoves>,
    ) -> session::SessionMenuInput<'b> {
        session::SessionMenuInput {
            row,
            group,
            settings: &self.inputs.settings,
            catalog: &self.catalog,
            below,
            has_inactive_threads: row.is_coordinator
                && row
                    .key
                    .as_ref()
                    .is_some_and(|key| self.coordinators_with_inactive_threads.contains(key)),
            now_ms: self.now_ms,
            group_moves,
        }
    }

    /// Where Move to Group can send this row: the user-made groups of its project that the list
    /// draws, and the project's own list. `None` for a row outside a project and its user-made
    /// groups (a bot's sessions).
    fn group_moves(
        &self,
        current: &GroupView,
        row: &SessionRow,
    ) -> Option<session::SessionGroupMoves> {
        let project = row.key.as_ref()?.project_key();
        let project_list_id = project.to_sidebar_group_id();
        let current_id = current.core.group_id.as_str();
        let of_project = |group_id: &str| {
            parse_workspace_subgroup_id(group_id).is_some_and(|(owner, _)| owner == project)
        };
        if current_id != project_list_id && !of_project(current_id) {
            return None;
        }
        let groups = self
            .view
            .groups
            .iter()
            .filter(|group| of_project(&group.core.group_id))
            .map(|group| session::SessionGroupTarget {
                group_id: group.core.group_id.clone(),
                title: group.core.title.clone(),
                is_current: group.core.group_id == current_id,
            })
            .collect();
        Some(session::SessionGroupMoves {
            project_list_id,
            groups,
        })
    }

    fn find_row(&self, sidebar_session_id: &str) -> Option<(&'a GroupView, &'a SessionView)> {
        self.view.groups.iter().find_map(|group| {
            group
                .core
                .sessions
                .iter()
                .find(|session| session.row.sidebar_session_id == sidebar_session_id)
                .map(|session| (group, session))
        })
    }

    /// The rows drawn under this one in its group, which is what Sleep Below and Close Below
    /// target. A row inside a collapsed section is not drawn and is not below anything.
    fn rows_below(&self, group: &'a GroupView, row: &SessionRow) -> Vec<&'a SessionView> {
        let visible: BTreeSet<&str> = group
            .core
            .sections
            .iter()
            .filter(|section| !section.collapsed)
            .flat_map(|section| section.session_ids.iter().map(String::as_str))
            .collect();
        let drawn: Vec<&SessionView> = group
            .core
            .sessions
            .iter()
            .filter(|session| visible.contains(session.row.sidebar_session_id.as_str()))
            .collect();
        match drawn
            .iter()
            .position(|session| session.row.sidebar_session_id == row.sidebar_session_id)
        {
            Some(index) => drawn[index + 1..].to_vec(),
            None => Vec::new(),
        }
    }
}

/// Whether a sidebar group id is a user-made session group, which has no project row of its own
/// and can still take a session into a new group.
fn is_user_made_group(group_id: &str) -> bool {
    group_id.starts_with("gpui-wsg:")
}
