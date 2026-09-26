//! The menus, hover buttons and header buttons the drawn sidebar list carries.
//!
//! CDXC:ContextMenus 2026-09-20 DECISION:
//! User (2026-09-19): the desktop app stops running product logic in QuickJS. Until this
//! milestone the list took its menus from the old projection's newest publish by row id, which
//! cost a row the projection had not published its whole menu and forced a reinstall of the list
//! on every accepted publish. They are built from the store here instead. A row's context menu is
//! still lazy, exactly as the TypeScript made it: the publish carries one placeholder and the
//! menu is built for the row the user actually opened, which is what keeps two hundred rows off
//! the install path.
//!
//! What the menus still read from outside the store is one list, and it is all HUD (M5): the
//! agents the launcher offers and the Saved Actions pinned to a project header. The HUD is held in
//! `runtime_facts.rs` instead of riding the old projection's publish. The agent the user launched
//! last and the keep-awake duration are client storage, read through `sidebar_ui_storage.rs` behind
//! a one-second cache, because both change while the app runs.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ghostex_gx_core::{HeaderCommand, HoverAction, LauncherAgent, MenuHost, SidebarMenus};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// How long the two client-storage values are reused. Reading them is one indexed row each on an
/// open connection, and an install can run several times a second.
const HOST_STATE_MAX_AGE: Duration = Duration::from_millis(1000);

/// The client-storage half of the menu host, kept between installs.
#[derive(Default)]
pub(super) struct MenuHostCache {
    read_at: Option<Instant>,
    primary_agent_id: Option<String>,
    keep_awake_minutes: Option<i64>,
    /// Bumped whenever a re-read found a different value, so the install gate can see a change
    /// that neither the store nor a publish reports.
    generation: u64,
}

impl MenuHostCache {
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
}

impl GhostexGpuiApp {
    /// Re-reads the two client-storage values at most once a second and returns the generation,
    /// which moves only when one of them really changed.
    pub(super) fn gx_store_menu_host_generation(&mut self) -> u64 {
        let cache = &mut self.gx_store.menu_host;
        if cache
            .read_at
            .is_some_and(|read_at| read_at.elapsed() < HOST_STATE_MAX_AGE)
        {
            return cache.generation;
        }
        cache.read_at = Some(Instant::now());
        if let Ok((primary, keep_awake)) = super::sidebar_ui_storage::read_menu_host_state() {
            if cache.primary_agent_id != primary || cache.keep_awake_minutes != keep_awake {
                cache.primary_agent_id = primary;
                cache.keep_awake_minutes = keep_awake;
                cache.generation += 1;
            }
        }
        cache.generation
    }

    /// Forces the next read to go to storage. Called on the commands that write either value, so
    /// the menu that caused the change redraws with it rather than waiting out the cache.
    pub(crate) fn gx_store_note_menu_host_write(&mut self, command: &Value) {
        let writes = match command["type"].as_str() {
            Some("projectAction") => command["action"] == "agent",
            Some("command") => command["message"]["type"] == "runTitlebarKeepAwakeCommand",
            _ => false,
        };
        if writes {
            self.gx_store.menu_host.read_at = None;
        }
    }

    /// Re-reads the two client-storage values on the sidebar's own one-second tick and installs
    /// the list when one of them moved.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// Both are written by surfaces outside the sidebar (the titlebar's Keep Awake control and the
    /// agent launcher), so neither the store nor a publish reports the change, and without this
    /// the more menu's Keep Awake tick and the project header's agent name would stay wrong until
    /// something unrelated redrew the list. The read is two indexed rows on an open connection and
    /// happens at most once a second, only while the store's list is the one on screen.
    pub(crate) fn gx_store_poll_menu_host(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.gx_store_sidebar_list_ready() {
            return;
        }
        let generation = self.gx_store_menu_host_generation();
        if self.gx_store.sidebar_list.installed_menu_host_generation() != Some(generation) {
            self.gx_store_install_sidebar_list(cx);
        }
    }

    /// The facts the menus read that are neither the store nor the settings.
    pub(super) fn gx_store_menu_host(&mut self) -> MenuHost {
        self.gx_store_menu_host_generation();
        // Borrowed, not cloned: the HUD is behind an `Arc` and this runs on every list install
        // (gx_store/runtime_facts.rs).
        let hud = self.gx_store.runtime_facts.hud.as_deref();
        // The selected tab and the connect states are the store's own since M4d, so Add Project on
        // a remote machine is decided by the same list the tabs are drawn from.
        let selected = self.gx_store.sidebar_ui.selected_machine_id().to_string();
        let machine_connected = self
            .gx_store
            .remote
            .tabs()
            .iter()
            .any(|machine| machine.machine_id == selected && machine.is_connected());
        MenuHost {
            // The sidebar page always has the workspace focus bridge; the web app is what does
            // not, and it never reaches this host.
            workspace_focus_bridge: true,
            agents: hud.map(|hud| agents(&hud["agents"])).unwrap_or_default(),
            primary_agent_id: self.gx_store.menu_host.primary_agent_id.clone(),
            global_commands: hud
                .map(|hud| commands(&hud["globalCommands"]))
                .unwrap_or_default(),
            project_commands: hud
                .map(|hud| commands_by_project(&hud["commandsByProject"]))
                .unwrap_or_default(),
            // The Keep Awake period this app is running, which the More menu ticks.
            keep_awake_minutes: self
                .keep_awake_runtime
                .as_ref()
                .map(|runtime| runtime.duration_minutes.minutes() as i64),
            machine_connected,
        }
    }

    /// Answers a `sessionMenu` request from the store rather than sending it to the old runtime.
    /// Returns whether it was answered here; `false` leaves the command on its old path.
    pub(crate) fn gx_store_answer_session_menu(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command["type"] != "sessionMenu" || !self.gx_store_sidebar_list_ready() {
            return false;
        }
        let Some(session_id) = command["sessionId"].as_str().map(str::to_string) else {
            return false;
        };
        let owner_id = command["ownerId"].as_str().unwrap_or_default().to_string();
        let action = command["action"].as_str().and_then(HoverAction::from_id);
        let host = self.gx_store_menu_host();
        let now_ms = super::host::now_ms();
        let items = {
            let list = &self.gx_store.sidebar_list;
            let menus = SidebarMenus::new(
                &self.gx_store.core,
                list.view(),
                &list.last_inputs,
                &host,
                now_ms,
            );
            let items = match action {
                Some(action) => menus.row_hover_submenu(&session_id, action),
                None => menus.row_menu(&session_id),
            };
            items.map(|items| {
                items
                    .iter()
                    .map(ghostex_gx_core::MenuItem::to_json)
                    .collect::<Vec<Value>>()
            })
        };
        // A row the list no longer draws answers with nothing, which closes the panel, exactly as
        // the TypeScript's empty reply did.
        let items = items.unwrap_or_default();
        let owns_panel = self
            .native_sidebar
            .menu
            .as_ref()
            .and_then(|menu| menu.account_panel.as_ref())
            .is_some_and(|(owner, _)| *owner == owner_id);
        if !owns_panel {
            return true;
        }
        if items.is_empty() {
            self.dismiss_native_sidebar_menu(cx);
            return true;
        }
        let index = self
            .native_sidebar
            .menu
            .as_ref()
            .and_then(|menu| menu.account_panel.as_ref())
            .map(|(_, index)| *index)
            .unwrap_or_default();
        if let Some(panel) = self
            .native_sidebar
            .menu
            .as_mut()
            .and_then(|menu| menu.panels.get_mut(index))
        {
            panel.replace_items(items);
        }
        cx.notify();
        true
    }
}

/// `hud.agents`.
fn agents(value: &Value) -> Vec<LauncherAgent> {
    value
        .as_array()
        .map(|agents| {
            agents
                .iter()
                .map(|agent| LauncherAgent {
                    agent_id: text(agent, "agentId"),
                    name: text(agent, "name"),
                    icon: agent
                        .get("icon")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One Saved Action list.
fn commands(value: &Value) -> Vec<HeaderCommand> {
    value
        .as_array()
        .map(|commands| {
            commands
                .iter()
                .map(|command| HeaderCommand {
                    command_id: text(command, "commandId"),
                    name: text(command, "name"),
                    icon: command
                        .get("icon")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    show_on_project_row: command.get("showOnProjectRow").and_then(Value::as_bool)
                        == Some(true),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn commands_by_project(value: &Value) -> BTreeMap<String, Vec<HeaderCommand>> {
    value
        .as_object()
        .map(|projects| {
            projects
                .iter()
                .map(|(project_id, list)| (project_id.clone(), commands(list)))
                .collect()
        })
        .unwrap_or_default()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
