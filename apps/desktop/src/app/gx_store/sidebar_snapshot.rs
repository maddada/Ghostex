//! The Rust sidebar list in the shape the renderer draws.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! The renderer reads one list, and this milestone moves where that list comes from without
//! rewriting the thirty files that draw it. Every value the list itself decides (which groups,
//! which rows, their order, sections, titles, tooltips, labels, counts, collapse and focus flags)
//! is the store's view model, and since M4c so are the menus, the hover buttons, the header
//! buttons, the more menu and the agent artwork (`sidebar_menus.rs`).
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! Nothing here is taken from the old projection's publish any more (M4d part 2 step 3). What the
//! view model does not decide now has a Rust owner named at the call site
//! (`gx_store_install_sidebar_list`): the HUD is the runtime facts holder's (composed in
//! gx_store/hud/), the reveal is the newest request held there or from the titlebar, the rename is the store's pending
//! collection rename, the two hotkey labels are the hotkey settings formatted here, and a machine
//! tab's sanitized failure message is this app's own connect state. The daemon revision and the
//! project facts the publish carried beside the store's (`canRemoveProject`, the editor identity,
//! the theme) are gone rather than carried: no renderer file reads one.
//!
//! M4d moved the machine tabs, their connection state and their counts, a group's `isStale` and
//! its remote machine context into the view model, so a remote machine's list is drawn from the
//! store exactly as this computer's is. Since remote focus part 2 step 2 that includes the two
//! marks a remote row draws from focus: the store's core focus owns the remote row, so the carry
//! from the publish (`remote_row_focus`) is gone.

use std::collections::HashMap;
use std::sync::Arc;

use ghostex_gx_core::{
    CollectionView, GroupCore, GroupView, MenuHost, SessionView, SidebarHiddenItems, SidebarMenus,
    SidebarSettings, SidebarView, TagPresentation, colored_agent_logo, menu_to_json,
};
use serde_json::{Map, Value, json};

use crate::app::native_sidebar::model::{
    NativeSidebarCollection, NativeSidebarGroup, NativeSidebarMachine, NativeSidebarOrderItem,
    NativeSidebarRenameRequest, NativeSidebarRevealRequest, NativeSidebarSection,
    NativeSidebarSession, NativeSidebarSnapshot,
};

/// What a cached session element was built from, so a focus change rebuilds two rows rather than
/// two hundred.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// The two sources are held rather than compared by address. This cache is kept between installs,
/// so an entry can outlive the row it was built from, and a freed allocation whose address is
/// handed to the next row would make a stale element compare equal. Holding the two `Arc`s is what
/// makes `Arc::ptr_eq` a real answer: the allocation cannot be freed while the cache holds it, so
/// no other row can ever be given its address.
struct CachedRow {
    row: Arc<ghostex_gx_core::SessionRow>,
    is_focused: bool,
    is_visible: bool,
    is_multi_selected: bool,
    /// The one menu input that moves on its own: a snooze ends without the row changing, and the
    /// hover button turns from Unsnooze back into Snooze.
    is_snoozed: bool,
    timer_label: Option<String>,
    last_interaction_label: Option<String>,
    element: Arc<NativeSidebarSession>,
}

impl CachedRow {
    fn matches(
        &self,
        row: &Arc<ghostex_gx_core::SessionRow>,
        session: &SessionView,
        focus: (bool, bool),
        is_snoozed: bool,
        timer_label: &Option<String>,
        last_interaction_label: &Option<String>,
    ) -> bool {
        Arc::ptr_eq(&self.row, row)
            && self.is_snoozed == is_snoozed
            && (self.is_focused, self.is_visible) == focus
            && self.is_multi_selected == session.is_multi_selected
            && self.timer_label == *timer_label
            && self.last_interaction_label == *last_interaction_label
    }
}

/// A group's menu and header buttons, kept until one of their inputs moves.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// A project menu is about fifteen rows and its header buttons carry the agent launcher, whose
/// items each hold a logo data URL of up to eight kilobytes. Rebuilding both for thirty-five
/// groups on every install was more than half the install's time and a few hundred kilobytes of
/// JSON, for groups whose inputs had not moved. The `GroupCore` is held rather than compared by
/// address, for the same reason `CachedRow` holds its row: an entry outlives the group it was
/// built from, and a freed allocation's address handed to the next group would compare equal.
struct CachedGroupMenus {
    core: Arc<GroupCore>,
    /// The collection a project belongs to decides its Add to Group ticks, and it is not part of
    /// `GroupCore`.
    collection_id: Option<String>,
    menu: Arc<Value>,
    header_actions: Arc<Vec<Value>>,
}

/// A collection's menu, kept until the collection or one of its groups moves.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// The Tag Sessions page builds one batch command per enabled tag over every agent session of the
/// collection, so a collection of sixty sessions and fourteen tags is over eight hundred command
/// objects, and Close All Sessions is a list of every id. Rebuilding that per install was the
/// most expensive thing in the build after the group menus.
struct CachedCollectionMenu {
    /// The groups the collection draws, held so `Arc::ptr_eq` stays a real answer.
    cores: Vec<Arc<GroupCore>>,
    /// Everything of the collection itself the menu reads.
    identity: (String, String, Vec<String>, bool),
    menu: Arc<Value>,
}

/// The more menu, kept until what it reads moves. It is one panel, but it walks the tag list and
/// every drawn group.
struct CachedMoreMenu {
    /// The drawn project group ids, whether any of them is expanded, Show Hidden, and the ticked
    /// filters: everything it reads beyond the shared key.
    identity: (u64, bool, bool, Vec<String>),
    menu: Value,
}

/// What every cached menu was built from besides the group or row it belongs to. A difference in
/// any of it drops the whole cache, which is right: all of them are user actions, not traffic.
#[derive(Clone, PartialEq)]
struct MenuKey {
    settings: SidebarSettings,
    hidden_items: SidebarHiddenItems,
    host: MenuHost,
    /// The tag catalog, the Spaces and the collections, as `SidebarMenus` derived them.
    context: u64,
}

/// Keeps the built rows and menus between publishes.
#[derive(Default)]
pub(super) struct SnapshotCache {
    rows: HashMap<String, CachedRow>,
    groups: HashMap<String, CachedGroupMenus>,
    collections: HashMap<String, CachedCollectionMenu>,
    more_menu: Option<CachedMoreMenu>,
    /// What the cached menus were built from. A settings change does not touch a row or a group
    /// in the view model, so nothing else would invalidate them.
    key: Option<MenuKey>,
    /// Where the newest install's time went, and how much of it the caches saved.
    pub(super) phases: InstallPhases,
}

/// Per-phase microseconds and cache hits of one install, so a rise in `installUs` says which part
/// of the build it came from rather than inviting a guess.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InstallPhases {
    /// Reading the two client-storage values the menus need, behind its own one-second cache.
    /// Measured and written by the caller, because it runs before the build and the build resets
    /// these.
    pub(super) host_us: u64,
    pub(super) key_us: u64,
    /// The rows of every group, summed.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// Assigned here rather than left at zero, which is what it was: nothing ever wrote it, and
    /// `groups_us` is computed by subtracting it, so every install this milestone measured
    /// reported the whole rows-and-groups phase as the GROUP phase and the row phase as free. A
    /// rise in `groupsUs` then read as "a group menu was rebuilt" when the rows underneath it were
    /// the cost. That is the fourth diagnostic in this port to be wrong in the direction of
    /// looking fine.
    pub(super) rows_us: u64,
    pub(super) groups_us: u64,
    pub(super) collections_us: u64,
    pub(super) more_menu_us: u64,
    pub(super) tail_us: u64,
    pub(super) rows_built: u64,
    pub(super) rows_reused: u64,
    pub(super) groups_built: u64,
    pub(super) groups_reused: u64,
    pub(super) collections_built: u64,
    pub(super) collections_reused: u64,
    pub(super) more_menu_built: bool,
    /// The shared key every cached menu hangs off was unchanged, so nothing was dropped wholesale.
    /// False is the expensive case: every row, group and collection menu is rebuilt.
    pub(super) key_reused: bool,
    /// Why a group menu was rebuilt: it had no entry, its `GroupCore` moved (the view model
    /// rebuilt the group), or only the collection it is drawn in moved.
    pub(super) groups_missing: u64,
    pub(super) groups_core_moved: u64,
    pub(super) groups_collection_moved: u64,
}

impl SnapshotCache {
    /// Rebuilds the rows whose drawn time reads differently now, keeping everything else.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// A clock wake changes at most a handful of label strings on rows the installed list already
    /// holds; it changes no order, no section, no menu and no group. Servicing one through
    /// `snapshot_from_view` rebuilt the whole list, and the live run measured ninety-three of a
    /// hundred and twenty-eight installs coming from a wake, which is a fifth of a second of main
    /// thread per minute spent on relative times on an idle sidebar. A row is a copy of its
    /// element with two strings replaced, and the list it sits in is otherwise shared.
    ///
    /// Returns nothing when no drawn time moved, which is what tells the caller to install
    /// nothing at all.
    pub(super) fn relabelled_rows(
        &mut self,
        view: &SidebarView,
        now_ms: u64,
    ) -> Vec<(String, Arc<NativeSidebarSession>)> {
        let mut moved: Vec<(String, Arc<NativeSidebarSession>)> = Vec::new();
        for session in view
            .groups
            .iter()
            .flat_map(|group| group.core.sessions.iter())
        {
            let row = &session.row;
            let timer_label = row.timer_label(now_ms);
            let last_interaction_label = row.last_interaction_label(now_ms);
            let Some(cached) = self.rows.get_mut(&row.sidebar_session_id) else {
                continue;
            };
            if cached.timer_label == timer_label
                && cached.last_interaction_label == last_interaction_label
            {
                continue;
            }
            let mut next = (*cached.element).clone();
            insert_optional(&mut next.details, "timerLabel", timer_label.clone());
            insert_optional(
                &mut next.details,
                "lastInteractionLabel",
                last_interaction_label.clone(),
            );
            let next = Arc::new(next);
            cached.timer_label = timer_label;
            cached.last_interaction_label = last_interaction_label;
            cached.element = next.clone();
            moved.push((row.sidebar_session_id.clone(), next));
        }
        moved
    }
}

/// Everything one install needs besides the view and the cache.
pub(super) struct SnapshotInput<'a> {
    pub(super) menus: &'a SidebarMenus<'a>,
    /// The sidebar HUD, from the runtime facts holder (composed in gx_store/hud/).
    pub(super) hud: &'a std::sync::Arc<Value>,
    /// The collection whose inline rename the renderer opens next, and the row it scrolls to.
    pub(super) rename_request: Option<NativeSidebarRenameRequest>,
    pub(super) reveal_request: Option<NativeSidebarRevealRequest>,
    /// The two hotkey labels the empty state and the Commands row draw.
    pub(super) search_shortcut: Option<String>,
    pub(super) commands_shortcut: Option<String>,
    pub(super) settings: &'a SidebarSettings,
    pub(super) hidden_items: &'a SidebarHiddenItems,
    pub(super) host: &'a MenuHost,
    /// What the more menu reads beyond the shared key.
    pub(super) collapsed_groups: &'a std::collections::BTreeSet<String>,
    pub(super) show_hidden: bool,
    pub(super) selected_tag_filters: &'a [String],
    /// The clock the time labels are formatted against.
    pub(super) now_ms: u64,
}

/// Builds the list the renderer draws from the view model and the menus.
pub(super) fn snapshot_from_view(
    view: &SidebarView,
    input: &SnapshotInput<'_>,
    cache: &mut SnapshotCache,
) -> NativeSidebarSnapshot {
    let SnapshotInput { menus, now_ms, .. } = *input;

    let started = web_time::Instant::now();
    cache.phases = InstallPhases::default();
    // Compared before it is built, so a hit does not clone the settings, the hidden items and the
    // host's agent and command lists on every install just to throw them away.
    let context = menus.context_fingerprint();
    let same_key = cache.key.as_ref().is_some_and(|key| {
        key.context == context
            && key.settings == *input.settings
            && key.hidden_items == *input.hidden_items
            && key.host == *input.host
    });
    cache.phases.key_reused = same_key;
    if !same_key {
        cache.key = Some(MenuKey {
            settings: input.settings.clone(),
            hidden_items: input.hidden_items.clone(),
            host: input.host.clone(),
            context,
        });
        cache.rows.clear();
        cache.groups.clear();
        cache.collections.clear();
        cache.more_menu = None;
    }
    cache.phases.key_us = started.elapsed().as_micros() as u64;
    let phase = web_time::Instant::now();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let groups: Vec<NativeSidebarGroup> = view
        .groups
        .iter()
        .map(|group| {
            let rows_started = web_time::Instant::now();
            let sessions = group
                .core
                .sessions
                .iter()
                .map(|session| {
                    used.insert(session.row.sidebar_session_id.clone());
                    let focus = (session.is_focused, session.is_visible);
                    session_element(group, session, focus, menus, cache, now_ms)
                })
                .collect();
            let rows_us = rows_started.elapsed().as_micros() as u64;
            let built = native_group(group, menus, sessions, cache);
            cache.phases.rows_us += rows_us;
            built
        })
        .collect();
    cache
        .rows
        .retain(|session_id, _| used.contains(session_id.as_str()));
    let drawn: std::collections::HashSet<&str> = view
        .groups
        .iter()
        .map(|group| group.core.group_id.as_str())
        .collect();
    cache
        .groups
        .retain(|group_id, _| drawn.contains(group_id.as_str()));
    cache.phases.groups_us =
        (phase.elapsed().as_micros() as u64).saturating_sub(cache.phases.rows_us);
    let phase = web_time::Instant::now();
    let collections: Vec<NativeSidebarCollection> = view
        .collections
        .iter()
        .map(|collection| native_collection(collection, view, menus, cache))
        .collect();
    let drawn_collections: std::collections::HashSet<&str> = view
        .collections
        .iter()
        .map(|collection| collection.collection_id.as_str())
        .collect();
    cache
        .collections
        .retain(|collection_id, _| drawn_collections.contains(collection_id.as_str()));
    cache.phases.collections_us = phase.elapsed().as_micros() as u64;
    let phase = web_time::Instant::now();
    let more_menu = more_menu(view, input, menus, cache);
    cache.phases.more_menu_us = phase.elapsed().as_micros() as u64;
    let phase = web_time::Instant::now();

    let snapshot = NativeSidebarSnapshot {
        scroll_scope: view.scroll_scope.clone(),
        rename_request: input.rename_request.clone(),
        reveal_request: input.reveal_request.clone(),
        empty_state: json!({
            "loading": view.empty_state.loading,
            "error": view.empty_state.error,
            "canAddProject": view.empty_state.can_add_project,
            "copy": view.empty_state.copy,
        }),
        hud: std::sync::Arc::clone(input.hud),
        groups,
        selected_machine_id: view.selected_machine_id.clone(),
        machines: view
            .machines
            .iter()
            .map(|machine| NativeSidebarMachine {
                working_count: machine.working_count,
                attention_count: machine.attention_count,
                background_work_count: machine.background_work_count,
                id: machine.id.clone(),
                label: machine.label.clone(),
                state: machine.state.clone(),
                // The sanitized failure summary the header shows under a machine that could not
                // connect: this app's own, from the connect transition that produced it
                // (os_integration/toast_and_status_dispatch.rs), carried into the tabs by
                // `remote_machine_tabs`.
                message: machine.message.clone(),
            })
            .collect(),
        spaces: view
            .spaces
            .iter()
            .map(
                |space| crate::app::native_sidebar::model::NativeSidebarSpace {
                    id: space.id.clone(),
                    name: space.name.clone(),
                    icon: space.icon.clone(),
                    color: space.color.clone(),
                    selected: space.selected,
                    contains_active_session: space.contains_active_session,
                    working_count: space.working_count,
                    attention_count: space.attention_count,
                    background_work_count: space.background_work_count,
                },
            )
            .collect(),
        spaces_enabled: view.spaces_enabled,
        collections,
        order: view
            .order
            .iter()
            .map(|item| NativeSidebarOrderItem {
                kind: match item.kind {
                    ghostex_gx_core::OrderKind::Project => "project".to_string(),
                    ghostex_gx_core::OrderKind::Collection => "collection".to_string(),
                },
                id: item.id.clone(),
            })
            .collect(),
        more_menu,
        // The two hotkey labels are the hotkey settings formatted for the current platform, which
        // the list re-reads whenever the settings file's content hash moves.
        search_shortcut: input.search_shortcut.clone(),
        commands_shortcut: input.commands_shortcut.clone(),
    };
    cache.phases.tail_us = phase.elapsed().as_micros() as u64;
    snapshot
}

/// The sidebar's own menu, kept until one of the four things it reads beyond the shared key moves.
fn more_menu(
    view: &SidebarView,
    input: &SnapshotInput<'_>,
    menus: &SidebarMenus<'_>,
    cache: &mut SnapshotCache,
) -> Value {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut any_expanded = false;
    for group in &view.groups {
        if group.core.group_id == ghostex_gx_core::CHATS_GROUP_ID {
            continue;
        }
        group.core.group_id.hash(&mut hasher);
        any_expanded |= !input.collapsed_groups.contains(&group.core.group_id);
    }
    let identity = (
        hasher.finish(),
        any_expanded,
        input.show_hidden,
        input.selected_tag_filters.to_vec(),
    );
    if let Some(cached) = &cache.more_menu {
        if cached.identity == identity {
            return cached.menu.clone();
        }
    }
    let menu = menu_to_json(&menus.more_menu());
    cache.phases.more_menu_built = true;
    cache.more_menu = Some(CachedMoreMenu {
        identity,
        menu: menu.clone(),
    });
    menu
}

fn native_group(
    group: &GroupView,
    menus: &SidebarMenus<'_>,
    sessions: Vec<Arc<NativeSidebarSession>>,
    cache: &mut SnapshotCache,
) -> NativeSidebarGroup {
    let core = &group.core;
    let (menu, header_actions) = group_menus(group, menus, cache);
    NativeSidebarGroup {
        title_tooltip: core.title_tooltip.clone(),
        group_id: core.group_id.clone(),
        storage_id: core.storage_id.clone(),
        summary: json!({
            "workingCount": core.summary.working_count,
            "attentionCount": core.summary.attention_count,
            "backgroundWorkCount": core.summary.background_work_count,
            "awakeCount": core.summary.awake_count,
        }),
        collapsed: core.collapsed,
        expanded: core.expanded,
        hidden_session_count: core.hidden_session_count,
        show_list_toggle: core.show_list_toggle,
        hover_actions_expanded: core.hover_actions_expanded,
        menu,
        header_actions,
        sections: core
            .sections
            .iter()
            .map(|section| NativeSidebarSection {
                id: section.id.as_str().to_string(),
                collapsed: section.collapsed,
                count: section.count,
                contains_active_session: section.contains_active_session,
                working_count: section.working_count,
                attention_count: section.attention_count,
                background_work_count: section.background_work_count,
                question_count: section.question_count,
                session_ids: section.session_ids.clone(),
            })
            .collect(),
        title: core.title.clone(),
        is_active: core.is_active,
        is_stale: core.is_stale,
        project_context: project_context(group),
        remote_machine_context: core.remote_machine.as_ref().map(|remote| {
            let mut context = json!({
                "machineId": remote.machine_id,
                "machineName": remote.machine_name,
            });
            if let Some(project_id) = &remote.project_id {
                context["projectId"] = Value::String(project_id.clone());
            }
            context
        }),
        sessions,
    }
}

/// One group's menu and header buttons, kept until the group or one of the menu inputs moves.
fn group_menus(
    group: &GroupView,
    menus: &SidebarMenus<'_>,
    cache: &mut SnapshotCache,
) -> (Arc<Value>, Arc<Vec<Value>>) {
    let core = &group.core;
    match cache.groups.get(&core.group_id) {
        None => cache.phases.groups_missing += 1,
        Some(cached) => {
            if Arc::ptr_eq(&cached.core, core) && cached.collection_id == group.collection_id {
                cache.phases.groups_reused += 1;
                return (cached.menu.clone(), cached.header_actions.clone());
            }
            if Arc::ptr_eq(&cached.core, core) {
                cache.phases.groups_collection_moved += 1;
            } else {
                cache.phases.groups_core_moved += 1;
            }
        }
    }
    cache.phases.groups_built += 1;
    let menu = Arc::new(menu_to_json(&menus.project_menu(group)));
    let header_actions = Arc::new(
        menus
            .header_actions(group)
            .iter()
            .map(ghostex_gx_core::MenuItem::to_json)
            .collect::<Vec<Value>>(),
    );
    cache.groups.insert(
        core.group_id.clone(),
        CachedGroupMenus {
            core: core.clone(),
            collection_id: group.collection_id.clone(),
            menu: menu.clone(),
            header_actions: header_actions.clone(),
        },
    );
    (menu, header_actions)
}

/// The project facts a header draws, all of them the view model's.
///
/// CDXC:Sidebar 2026-09-21 WHY:
/// This used to start from the old projection's own object for the same group, so the store's
/// values were written OVER the publish's `canRemoveProject`, its editor identity and its theme.
/// Every renderer file that reads a project context reads one of the keys written here
/// (`native_sidebar/rows.rs`, `project_status.rs`, `project_header.rs`,
/// `sidebar_agent_launch_placeholder.rs`), so the merge only kept keys nothing draws alive.
fn project_context(group: &GroupView) -> Option<Value> {
    let context = group.core.project_context.as_ref()?;
    let mut object = Map::new();
    object.insert("path".to_string(), Value::String(context.path.clone()));
    insert_optional(&mut object, "iconDataUrl", context.icon_data_url.clone());
    insert_optional(
        &mut object,
        "discoveredIconDataUrl",
        context.discovered_icon_data_url.clone(),
    );
    if let Some(worktree) = &context.worktree {
        object.insert(
            "worktree".to_string(),
            json!({
                "branch": worktree.branch,
                "name": worktree.name,
                "parentProjectId": worktree.parent_project_id,
                "parentProjectName": worktree.parent_project_name,
                "parentProjectPath": worktree.parent_project_path,
            }),
        );
    }
    object.insert(
        "editor".to_string(),
        json!({
            "projectId": context.project_id,
            "diffStats": {
                "additions": context.diff_stats.additions,
                "deletions": context.diff_stats.deletions,
                "files": context.diff_stats.files,
                "isLoading": context.diff_stats.is_loading,
                "isRepo": context.diff_stats.is_repo,
            },
        }),
    );
    Some(Value::Object(object))
}

fn native_collection(
    collection: &CollectionView,
    view: &SidebarView,
    menus: &SidebarMenus<'_>,
    cache: &mut SnapshotCache,
) -> NativeSidebarCollection {
    NativeSidebarCollection {
        awake_count: collection.awake_count as u64,
        collection_id: collection.collection_id.clone(),
        title: collection.title.clone(),
        color: collection.color.clone(),
        group_ids: collection.group_ids.clone(),
        collapsed: collection.collapsed,
        contains_active_session: collection.contains_active_session,
        working_count: collection.working_count,
        attention_count: collection.attention_count,
        background_work_count: collection.background_work_count,
        menu: collection_menu(collection, view, menus, cache),
    }
}

/// One collection's menu, kept until the collection itself or any of the groups whose rows it acts
/// on moves.
fn collection_menu(
    collection: &CollectionView,
    view: &SidebarView,
    menus: &SidebarMenus<'_>,
    cache: &mut SnapshotCache,
) -> Arc<Value> {
    let cores: Vec<Arc<GroupCore>> = collection
        .group_ids
        .iter()
        .filter_map(|group_id| view.group(group_id))
        .map(|group| group.core.clone())
        .collect();
    let identity = (
        collection.collection_id.clone(),
        collection.color.clone(),
        collection.group_ids.clone(),
        cache.key.as_ref().is_some_and(|key| {
            key.hidden_items
                .collection_keys
                .contains(&collection.storage_id)
        }),
    );
    if let Some(cached) = cache.collections.get(&collection.collection_id) {
        if cached.identity == identity
            && cached.cores.len() == cores.len()
            && cached
                .cores
                .iter()
                .zip(&cores)
                .all(|(held, core)| Arc::ptr_eq(held, core))
        {
            cache.phases.collections_reused += 1;
            return cached.menu.clone();
        }
    }
    cache.phases.collections_built += 1;
    let menu = Arc::new(menu_to_json(&menus.collection_menu(collection)));
    cache.collections.insert(
        collection.collection_id.clone(),
        CachedCollectionMenu {
            cores,
            identity,
            menu: menu.clone(),
        },
    );
    menu
}

/// Which row of a remote machine is focused, and which are on screen.
///
/// CDXC:FocusRouting 2026-09-20 WHY:
fn session_element(
    group: &GroupView,
    session: &SessionView,
    focus: (bool, bool),
    menus: &SidebarMenus<'_>,
    cache: &mut SnapshotCache,
    now_ms: u64,
) -> Arc<NativeSidebarSession> {
    let (is_focused, is_visible) = focus;
    let row = &session.row;
    let timer_label = row.timer_label(now_ms);
    let last_interaction_label = row.last_interaction_label(now_ms);
    let is_snoozed = row
        .timing
        .snoozed_until_ms
        .is_some_and(|wake_at| wake_at > now_ms as i64);
    if let Some(cached) = cache.rows.get(&row.sidebar_session_id) {
        if cached.matches(
            row,
            session,
            focus,
            is_snoozed,
            &timer_label,
            &last_interaction_label,
        ) {
            cache.phases.rows_reused += 1;
            return cached.element.clone();
        }
    }
    cache.phases.rows_built += 1;
    let element = Arc::new(build_session(
        group,
        session,
        focus,
        menus,
        timer_label.clone(),
        last_interaction_label.clone(),
    ));
    cache.rows.insert(
        row.sidebar_session_id.clone(),
        CachedRow {
            row: row.clone(),
            is_focused,
            is_visible,
            is_multi_selected: session.is_multi_selected,
            is_snoozed,
            timer_label,
            last_interaction_label,
            element: element.clone(),
        },
    );
    element
}

/// One drawn row: every key the view model owns, plus the hover buttons and the placeholder menu
/// the store builds for it.
fn build_session(
    group: &GroupView,
    session: &SessionView,
    focus: (bool, bool),
    menus: &SidebarMenus<'_>,
    timer_label: Option<String>,
    last_interaction_label: Option<String>,
) -> NativeSidebarSession {
    let (is_focused, is_visible) = focus;
    let row = &session.row;
    let mut details = Map::new();
    let actions = menus.row_actions(group, session);
    details.insert("menu".to_string(), menu_to_json(&actions.menu));
    details.insert(
        "hoverBefore".to_string(),
        menu_to_json(&actions.hover_before),
    );
    details.insert("hoverAfter".to_string(), menu_to_json(&actions.hover_after));
    details.insert(
        "hoverChevron".to_string(),
        Value::Bool(actions.hover_chevron),
    );
    insert_optional(
        &mut details,
        "agentLogoDataUrl",
        row.agent_icon
            .as_deref()
            .and_then(colored_agent_logo)
            .map(str::to_string),
    );
    details.insert(
        "titleTooltip".to_string(),
        Value::String(row.title_tooltip.clone()),
    );
    details.insert(
        "pendingQuestionCount".to_string(),
        Value::from(row.pending_question_count),
    );
    details.insert(
        "isMultiSelected".to_string(),
        Value::Bool(session.is_multi_selected),
    );
    details.insert("isFavorite".to_string(), Value::Bool(row.is_favorite));
    // Read by the direct-focus path to decide whether a row has a transcript to open.
    insert_optional(
        &mut details,
        "agentSessionId",
        row.menu_facts.agent_session_id.clone(),
    );
    insert_optional(&mut details, "sessionTag", row.session_tag.clone());
    insert_optional(&mut details, "effectiveTag", row.effective_tag.clone());
    details.insert(
        "tagPresentation".to_string(),
        tag_presentation(row.tag_presentation.as_ref()),
    );
    details.insert(
        "queuedPromptFailedCount".to_string(),
        Value::from(row.queued_prompt_failed_count.unwrap_or(0)),
    );
    insert_optional(&mut details, "timerLabel", timer_label);
    insert_optional(&mut details, "lastInteractionLabel", last_interaction_label);
    let delayed = row.delayed_send.as_ref();
    insert_optional(
        &mut details,
        "delayedSendDeadlineAt",
        delayed.and_then(|delayed| delayed.deadline_at.clone()),
    );
    insert_optional(
        &mut details,
        "delayedSendRemainingLabel",
        delayed.and_then(|delayed| delayed.remaining_label.clone()),
    );
    insert_optional(
        &mut details,
        "delayedSendRemainingMs",
        delayed
            .and_then(|delayed| delayed.remaining_ms)
            .map(Value::from),
    );
    details.insert(
        "sendWhenAllProjectSessionsStopActive".to_string(),
        Value::Bool(
            delayed.is_some_and(|delayed| delayed.send_when_all_project_sessions_stop_active),
        ),
    );
    details.insert(
        "sendWhenAgentStopsActive".to_string(),
        Value::Bool(delayed.is_some_and(|delayed| delayed.send_when_agent_stops_active)),
    );
    let close = row.close_after_done.as_ref();
    details.insert(
        "closeAfterDone".to_string(),
        Value::Bool(close.is_some_and(|close| close.armed)),
    );
    insert_optional(
        &mut details,
        "closeAfterDoneDeadlineAt",
        close.and_then(|close| close.deadline_at.clone()),
    );
    insert_optional(
        &mut details,
        "closeAfterDoneRemainingLabel",
        close.and_then(|close| close.remaining_label.clone()),
    );
    insert_optional(
        &mut details,
        "closeAfterDoneRemainingMs",
        close.and_then(|close| close.remaining_ms).map(Value::from),
    );
    details.insert(
        "isGeneratingFirstPromptTitle".to_string(),
        Value::Bool(row.is_generating_first_prompt_title),
    );
    NativeSidebarSession {
        session_id: row.sidebar_session_id.clone(),
        display_title: Some(row.display_title.clone()),
        alias: row.alias.clone(),
        activity: row.activity.clone(),
        has_background_work: row.has_background_work,
        agent_icon: row.agent_icon.clone(),
        kind: row.is_browser.then(|| "browser".to_string()),
        session_kind: row.session_kind.clone(),
        is_focused,
        is_visible,
        is_pinned: row.is_pinned,
        is_draft: row.is_draft,
        last_interaction_at: row.last_interaction_at.clone(),
        lifecycle_state: Some(row.lifecycle_state.clone()),
        session_note: row.session_note.clone(),
        favicon_data_url: row.favicon_data_url.clone(),
        has_composer_draft: row.has_composer_draft,
        queued_prompt_count: row.queued_prompt_count.unwrap_or(0),
        details,
    }
}

fn tag_presentation(presentation: Option<&TagPresentation>) -> Value {
    match presentation {
        Some(presentation) => json!({
            "icon": presentation.icon,
            "iconColor": presentation.icon_color,
        }),
        None => Value::Null,
    }
}

fn insert_optional(object: &mut Map<String, Value>, key: &str, value: Option<impl Into<Value>>) {
    match value {
        Some(value) => {
            object.insert(key.to_string(), value.into());
        }
        None => {
            object.remove(key);
        }
    }
}
