//! The workspace each window shows, saved with its slot, and the window moves the workspace tile's
//! menu asks for: bring forward the window that already shows a workspace, open one in a new
//! window, and open the Workspaces settings page.
//!
//! The slot file also keeps, per workspace, the sessions this window last had open there, which is
//! what a workspace switch restores (`switch_window_workspace`).
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: a window shows one workspace at a time; the tile's menu opens a workspace in a new window,
//! and choosing a workspace another window already shows brings that window forward instead of
//! switching this one. File > New Window opens on the workspace of the window it came from.
//! SEE-ALSO: apps/desktop/src/app/gx_store/workspaces.rs (the menu's commands), packages/gx-core/src/sidebar_view/workspaces.rs (the filter).

use std::fs;
use std::path::PathBuf;

use ghostex_gx_core::{SpaceSwitchFocus, plan_workspace_switch_restore};
use gpui::App;
use serde_json::{Map, Value, json};

use super::open::{WorkspaceWindowStart, open_new_workspace_window_on};
use super::registry::WORKSPACE_WINDOWS;
use super::slots::workspace_window_state_path;
use crate::app::helpers::*;
use crate::*;

fn window_workspace_path(slot: u32) -> PathBuf {
    workspace_window_state_path(
        ghostex_state_root().join("gpui-window-workspace.json"),
        slot,
    )
}

/// How many sessions a window remembers per workspace, newest first.
const MAX_REMEMBERED_WORKSPACE_SESSIONS: usize = 8;

fn read_window_workspace_state(slot: u32) -> Map<String, Value> {
    fs::read_to_string(window_workspace_path(slot))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_window_workspace_state(slot: u32, state: &Map<String, Value>) {
    let path = window_workspace_path(slot);
    if state.is_empty() {
        let _ = fs::remove_file(path);
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, Value::Object(state.clone()).to_string());
}

/// The workspace the slot's window showed when the app last quit; `None` = the default one.
pub(crate) fn saved_window_workspace_id(slot: u32) -> Option<String> {
    read_window_workspace_state(slot)
        .get("workspaceId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|id| !id.trim().is_empty())
}

/// The sessions (sidebar session ids) the slot's window last had open in `workspace_id`, newest
/// first.
fn remembered_workspace_sessions(slot: u32, workspace_id: &str) -> Vec<String> {
    read_window_workspace_state(slot)
        .get("recentSessionsByWorkspace")
        .and_then(|by_workspace| by_workspace.get(workspace_id))
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn remember_workspace_session(slot: u32, workspace_id: &str, sidebar_session_id: &str) {
    let mut state = read_window_workspace_state(slot);
    let by_workspace = state
        .entry("recentSessionsByWorkspace")
        .or_insert_with(|| json!({}));
    if !by_workspace.is_object() {
        *by_workspace = json!({});
    }
    let mut ids: Vec<String> = by_workspace
        .get(workspace_id)
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter(|id| *id != sidebar_session_id)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    ids.insert(0, sidebar_session_id.to_string());
    ids.truncate(MAX_REMEMBERED_WORKSPACE_SESSIONS);
    by_workspace[workspace_id] = json!(ids);
    write_window_workspace_state(slot, &state);
}

pub(super) fn discard_window_workspace(slot: u32) {
    let _ = fs::remove_file(window_workspace_path(slot));
}

impl GhostexGpuiApp {
    /// Called once the window's app exists: the workspace it starts on.
    pub(crate) fn restore_window_workspace(&mut self, start: &WorkspaceWindowStart) {
        let workspace_id = match start {
            WorkspaceWindowStart::Restore { slot, .. } => saved_window_workspace_id(*slot),
            WorkspaceWindowStart::New {
                workspace_id,
                active_project_id,
                ..
            } => {
                // A window opened on another workspace than the one it came from starts the way
                // switching to it does, once its list is built (workspace_landing.rs).
                self.window_workspace_landing.pending = active_project_id.is_none();
                workspace_id.clone()
            }
        };
        self.gx_store_init_window_workspace(workspace_id);
        self.persist_window_workspace_id();
    }

    /// Saves the window's workspace with its slot.
    pub(crate) fn persist_window_workspace_id(&self) {
        let slot = self.workspace_window_slot;
        let mut state = read_window_workspace_state(slot);
        match self.gx_store_window_workspace_id() {
            Some(workspace_id) => {
                state.insert("workspaceId".to_string(), json!(workspace_id));
            }
            None => {
                state.remove("workspaceId");
            }
        }
        write_window_workspace_state(slot, &state);
    }

    /// The workspace tile's "switch to": remembers the session this window has open under the
    /// workspace it leaves, shows the new one, opens the session the window last had open there
    /// (else its first project with no session, else nothing), and puts to sleep the views that
    /// projects outside it still keep.
    ///
    /// CDXC:Workspaces 2026-10-09 DECISION:
    /// User: switching a window's workspace also switches what the window shows: select the
    /// session that window last had open in that workspace (or none), and close the views that
    /// belong to projects outside it, the way switching Spaces restores the last session. The
    /// restore is the Space switch's (gx-core `plan_workspace_switch_restore`, `focusSession` with
    /// `keepView`); with nothing remembered the window lands on the workspace's first project with
    /// no session selected (`focusGroup`), and an empty workspace empties the work area. Unlike a
    /// Space switch it runs whatever `sidebarSpaceSwitchBehavior` says: keeping the old session on
    /// screen would show another workspace's session beside a list that no longer holds it. The
    /// views are put to sleep the way Sleep Space sleeps them, so their tabs stay in place.
    pub(crate) fn switch_window_workspace(
        &mut self,
        workspace_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.enter_window_workspace(&workspace_id, cx);
        self.land_on_window_workspace(&workspace_id, cx);
    }

    /// The first half of a switch: remembers the session this window has open under the workspace
    /// it leaves and shows `workspace_id`, without choosing what to show there. A focus routed here
    /// from outside the workspace (session_routing.rs) chooses that itself.
    pub(super) fn enter_window_workspace(
        &mut self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let slot = self.workspace_window_slot;
        if let (Some(outgoing), Some(focused)) = (
            self.gx_store_resolved_window_workspace_id(),
            self.gx_store
                .core
                .focus()
                .focused_session
                .as_ref()
                .map(ghostex_gx_core::SessionKey::to_sidebar_session_id),
        ) {
            remember_workspace_session(slot, &outgoing, &focused);
        }
        self.gx_store_set_window_workspace(Some(workspace_id.to_string()), cx);
    }

    /// Shows what the window last had open in `workspace_id`, the workspace it now shows: the
    /// switch's restore, also run when a window opens on a workspace and whenever the window finds
    /// itself showing a project outside it (workspace_landing.rs).
    pub(super) fn land_on_window_workspace(
        &mut self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let recent = remembered_workspace_sessions(self.workspace_window_slot, workspace_id);
        let view = self.gx_store.sidebar_list.view();
        let plan = plan_workspace_switch_restore(view, &recent).or_else(|| {
            view.groups.first().map(|group| SpaceSwitchFocus::Group {
                group_id: group.core.group_id.clone(),
            })
        });
        match plan {
            Some(SpaceSwitchFocus::Session { sidebar_session_id }) => {
                self.dispatch_native_sidebar_command(
                    json!({
                        "type": "focusSession",
                        "sessionId": sidebar_session_id,
                        "keepView": true,
                    }),
                    cx,
                );
            }
            Some(SpaceSwitchFocus::Group { group_id }) => {
                self.dispatch_native_sidebar_command(
                    json!({ "type": "focusGroup", "groupId": group_id }),
                    cx,
                );
            }
            None => {
                self.swap_agents_workspace_to_project_id(None, cx);
            }
        }
        self.sleep_views_outside_window_workspace(cx);
        cx.notify();
    }

    /// Puts to sleep the views of every project of this computer that the window's workspace does
    /// not hold: the open views of the project on screen, if it is one of them, and the browser
    /// pages and held-awake view the others keep while parked.
    pub(super) fn sleep_views_outside_window_workspace(&mut self, cx: &mut gpui::Context<Self>) {
        let mut project_ids: Vec<String> = self
            .project_view_states_by_project
            .keys()
            .chain(self.parked_browser_runtimes_by_project.keys())
            .chain(self.agents_workspace_project_id.as_ref())
            .cloned()
            .collect();
        project_ids.sort();
        project_ids.dedup();
        let outside: Vec<String> = project_ids
            .into_iter()
            .filter(|project_id| self.gx_store_project_outside_window_workspace(project_id))
            .collect();
        for project_id in &outside {
            self.sleep_project_views(project_id, cx);
        }
    }

    /// Brings forward another window that shows `workspace_id`; `false` when none does.
    pub(crate) fn focus_window_showing_workspace(
        &self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.other_window_showing_workspace(workspace_id, cx)
            .is_some()
    }

    /// The workspaces the other open windows show.
    pub(crate) fn workspaces_shown_in_other_windows(&self, cx: &gpui::Context<Self>) -> Vec<String> {
        let own = cx.entity_id();
        let apps: Vec<gpui::WeakEntity<GhostexGpuiApp>> = WORKSPACE_WINDOWS.with(|windows| {
            windows
                .borrow()
                .iter()
                .filter(|entry| entry.app.entity_id() != own && !entry.closing)
                .map(|entry| entry.app.clone())
                .collect()
        });
        apps.into_iter()
            .filter_map(|app| app.upgrade())
            .filter_map(|app| app.read(cx).gx_store_resolved_window_workspace_id())
            .collect()
    }

    /// Another window that shows `workspace_id`, brought forward; `None` when none does.
    pub(crate) fn other_window_showing_workspace(
        &self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::WeakEntity<GhostexGpuiApp>> {
        let (handle, app) = self.find_other_window_showing_workspace(workspace_id, cx)?;
        cx.defer(move |cx: &mut App| {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        });
        Some(app)
    }

    /// Another window that shows `workspace_id`, left where it is.
    pub(super) fn find_other_window_showing_workspace(
        &self,
        workspace_id: &str,
        cx: &gpui::Context<Self>,
    ) -> Option<(gpui::AnyWindowHandle, gpui::WeakEntity<GhostexGpuiApp>)> {
        let own = cx.entity_id();
        let windows: Vec<(gpui::AnyWindowHandle, gpui::WeakEntity<GhostexGpuiApp>)> =
            WORKSPACE_WINDOWS.with(|windows| {
                windows
                    .borrow()
                    .iter()
                    .filter(|entry| entry.app.entity_id() != own && !entry.closing)
                    .map(|entry| (entry.handle, entry.app.clone()))
                    .collect()
            });
        let target = windows.into_iter().find(|(_, app)| {
            app.upgrade().is_some_and(|app| {
                app.read(cx)
                    .gx_store_resolved_window_workspace_id()
                    .as_deref()
                    == Some(workspace_id)
            })
        });
        target
    }

    /// A new window on `workspace_id`, cascaded from this one.
    pub(crate) fn open_workspace_in_new_window(
        &self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let own = cx.entity_id();
        let source = WORKSPACE_WINDOWS.with(|windows| {
            windows
                .borrow()
                .iter()
                .find(|entry| entry.app.entity_id() == own)
                .map(|entry| (entry.handle, entry.app.clone()))
        });
        let workspace_id = workspace_id.to_string();
        cx.defer(move |cx: &mut App| open_new_workspace_window_on(source, Some(workspace_id), cx));
    }

    /// Settings on its Workspaces page.
    pub(crate) fn open_workspace_settings_page(&mut self, cx: &mut gpui::Context<Self>) {
        self.open_gpui_settings_tab_from_new_thread_picker("workspaces", cx);
    }
}
