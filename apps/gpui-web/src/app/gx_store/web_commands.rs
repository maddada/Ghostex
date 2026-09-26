//! What a sidebar command does in the browser: the desktop's own `sidebar_*.rs` executor files (symlinked into this folder) in the desktop's order, then the page's parts: the sidebar's own state (the core's `SidebarUiStore`, saved to localStorage under the same keys and formats the desktop uses), a row's menu, and focus (the page opens the session in its work area).
use ghostex_gx_core::{Event, HoverAction, Intent, MenuItem, SectionId, SidebarMenus, SidebarUiIntent};
use ghostex_gx_core as sidebar_ui;
use serde_json::Value;

use super::host::now_ms;
use crate::GhostexGpuiApp;

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn section_id(value: &str) -> Option<SectionId> {
    Some(match value {
        "browser" => SectionId::Browser,
        "pinned" => SectionId::Pinned,
        "drafts" => SectionId::Drafts,
        "sessions" => SectionId::Sessions,
        "parked" => SectionId::Parked,
        "snoozed" => SectionId::Snoozed,
        _ => return None,
    })
}

impl GhostexGpuiApp {
    pub(super) fn gx_store_restore_sidebar_ui(&mut self) {
        let Some(storage) = local_storage() else {
            return;
        };
        let read = |key: &str| storage.get_item(key).ok().flatten();
        let mut state = self.gx_store.sidebar_ui.state().clone();
        state.collapse = sidebar_ui::collapse_state_from_storage(
            read(sidebar_ui::COLLAPSE_STORAGE_KEY).as_deref(),
            None,
            None,
        );
        state.hidden_items = sidebar_ui::hidden_items_from_storage(
            read(sidebar_ui::HIDDEN_ITEMS_STORAGE_KEY).as_deref(),
        );
        self.gx_store.sidebar_ui.restore(state);
    }

    fn gx_store_persist_sidebar_ui(&mut self) {
        let owed = self.gx_store.sidebar_ui.take_pending();
        if owed.is_empty() {
            return;
        }
        let Some(storage) = local_storage() else {
            return;
        };
        let state = self.gx_store.sidebar_ui.state();
        let existing = storage.get_item(sidebar_ui::COLLAPSE_STORAGE_KEY).ok().flatten();
        let _ = storage.set_item(
            sidebar_ui::COLLAPSE_STORAGE_KEY,
            &sidebar_ui::collapse_into_storage(&state.collapse, existing.as_deref()),
        );
        let _ = storage.set_item(
            sidebar_ui::HIDDEN_ITEMS_STORAGE_KEY,
            &sidebar_ui::hidden_items_into_storage(&state.hidden_items),
        );
    }

    /// The desktop's `gx_store_apply_sidebar_ui_intent`: moves the sidebar's own state, saves it, and rebuilds the list. Returns whether it moved.
    pub(crate) fn gx_store_apply_sidebar_ui_intent(
        &mut self,
        intent: SidebarUiIntent,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let changed = self.gx_store.sidebar_ui.apply(intent).changed;
        if changed {
            self.gx_store_persist_sidebar_ui();
            self.gx_store_sidebar_state_changed(cx);
        }
        changed
    }

    fn sidebar_ui_intent(&self, command: &Value) -> Option<SidebarUiIntent> {
        let text = |key: &str| command.get(key).and_then(Value::as_str).map(str::to_string);
        match command.get("type").and_then(Value::as_str)? {
            "toggleGroup" => Some(SidebarUiIntent::ToggleGroupCollapsed { group_id: text("groupId")? }),
            "toggleList" => Some(SidebarUiIntent::ToggleSessionListExpanded { storage_id: text("groupId")? }),
            "toggleHoverActions" => Some(SidebarUiIntent::ToggleHoverActions { storage_id: text("groupId")? }),
            "toggleSection" => Some(SidebarUiIntent::ToggleSection {
                storage_id: text("groupId")?,
                section: section_id(command.get("section").and_then(Value::as_str)?)?,
            }),
            "selectSpace" => Some(SidebarUiIntent::SelectSpace { space_id: text("spaceId")? }),
            "toggleTagFilter" => Some(SidebarUiIntent::ToggleTagFilter { tag: text("tag")? }),
            "sidebarAction" if command["action"] == "showHidden" => Some(SidebarUiIntent::ToggleShowHidden),
            "selectSession" if matches!(command["mode"].as_str(), Some("clear" | "focus")) => {
                Some(SidebarUiIntent::SetSelectedSessions { session_ids: Vec::new() })
            }
            _ => None,
        }
    }

    /// A sidebar command, through the desktop's own store handlers in the desktop's order (`native_sidebar/actions.rs` `dispatch_native_sidebar_ui`), each on the envelope it expects: a gxserver message wrapped as `{type: "command", message}`, a renderer command at the top level. Left out are the handlers for what a page does not have: remote machines, the accounts pages, Keep Awake, Space Sleep's views.
    pub(crate) fn web_run_sidebar_command(&mut self, command: Value, cx: &mut gpui::Context<Self>) {
        if !self.gx_store_sidebar_list_ready() {
            return;
        }
        if self.web_answer_session_menu(&command, cx)
            || self.gx_store_run_sidebar_git(&command, cx)
            || self.gx_store_run_sidebar_action(&command, cx)
            || self.gx_store_run_sidebar_lifecycle(&command, cx)
            || self.gx_store_run_sidebar_close(&command, cx)
            || self.gx_store_run_close_after_done(&command, cx)
            || self.gx_store_run_session_edit_command(&command, cx)
            || self.gx_store_run_group_sleep(&command, cx)
            || self.gx_store_run_sidebar_fork(&command, cx)
            || self.gx_store_run_sidebar_flags(&command, cx)
            || self.gx_store_run_sidebar_modal(&command, cx)
            || self.gx_store_run_sidebar_open(&command, cx)
            || self.gx_store_run_sidebar_snooze_action(&command, cx)
            || self.gx_store_run_sidebar_snooze(&command, cx)
            || self.gx_store_run_sidebar_reload(&command, cx)
            || self.gx_store_run_sidebar_reload_set(&command, cx)
            || self.gx_store_run_sidebar_split(&command, cx)
            || self.gx_store_run_sidebar_session_move(&command, cx)
            || self.gx_store_run_sidebar_order_write(&command, cx)
            || self.gx_store_run_project_move(&command, cx)
            || self.gx_store_run_collection_menu_edit(&command, cx)
            || self.gx_store_run_sidebar_batch(&command, cx)
            || self.gx_store_run_sidebar_bulk(&command, cx)
            || self.gx_store_run_sidebar_create(&command, cx)
        {
            return;
        }
        // The drawing code wraps runtime-bound messages as `{type: "command", message}`.
        let command = match command.get("type").and_then(Value::as_str) {
            Some("command") => command["message"].clone(),
            _ => command,
        };
        if let Some(intent) = self.sidebar_ui_intent(&command) {
            self.gx_store_apply_sidebar_ui_intent(intent, cx);
            // The sidebar's own state is the whole answer; a clear-selection click also focuses, below.
            if command["type"] != "selectSession" {
                return;
            }
        }
        if command["type"] == "selectSession" && command["mode"] == "focus" {
            if let Some(row_id) = command["sessionId"].as_str() {
                self.web_focus_row(row_id, cx);
            }
            return;
        }
        log::info!("sidebar command not handled on web: {}", command["type"]);
    }

    /// `?session=<projectId>:<sessionId>` opens that session once the list has it, and `&surface=terminal` shows its terminal: a deep link, and what the screenshot driver uses.
    pub(crate) fn web_open_linked_session(&mut self, cx: &mut gpui::Context<Self>) {
        if self.open_session.is_some() || self.linked_session_opened {
            return;
        }
        let Some(search) = web_sys::window().and_then(|window| window.location().search().ok()) else {
            return;
        };
        let parameter = |name: &str| {
            search.trim_start_matches('?').split('&').find_map(|pair| {
                pair.strip_prefix(name)?.strip_prefix('=').map(str::to_string)
            })
        };
        let Some((project_id, session_id)) =
            parameter("session").and_then(|value| value.split_once(':').map(|(p, s)| (p.to_string(), s.to_string())))
        else {
            return;
        };
        // By store key rather than by drawn row: a compact session list hides most of a project's rows.
        if !self.gx_store.sidebar_view().ready {
            return;
        }
        self.linked_session_opened = true;
        self.web_open_session(
            ghostex_gx_core::SessionKey { machine: ghostex_gx_core::MachineId::Local, project_id, session_id },
            cx,
        );
        if parameter("surface").as_deref() == Some("terminal") {
            self.web_show_terminal(true, cx);
        }
    }

    fn web_focus_row(&mut self, row_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(session) = self.gx_store.session_key_for_row(row_id) else {
            return;
        };
        self.web_open_session(session, cx);
    }

    /// Opens `session` in the work area on the surface asked for: its terminal, or its chat.
    pub(crate) fn web_open_session_in_work_area(
        &mut self,
        session: ghostex_gx_core::SessionKey,
        terminal: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.web_open_session(session, cx);
        if terminal != self.show_terminal {
            self.web_show_terminal(terminal, cx);
        }
    }

    fn web_open_session(&mut self, session: ghostex_gx_core::SessionKey, cx: &mut gpui::Context<Self>) {
        self.open_session = Some(session.clone());
        self.ensure_native_chat(&session, cx);
        if self.show_terminal {
            self.ensure_terminal(&session, cx);
        }
        self.gx_store_handle(Event::Intent(Intent::FocusSession { session, visible: None }), cx);
        // The Git state follows the active project, as the desktop's workspace reconcile does.
        let active = self.gpui_app_modal_active_project_id();
        if active != self.web_host.git_active_project {
            self.web_host.git_active_project = active;
            self.gx_store_git_active_project_changed(cx);
        }
        cx.notify();
    }

    fn web_answer_session_menu(&mut self, command: &Value, cx: &mut gpui::Context<Self>) -> bool {
        if command["type"] != "sessionMenu" {
            return false;
        }
        let Some(session_id) = command["sessionId"].as_str() else {
            return true;
        };
        let owner_id = command["ownerId"].as_str().unwrap_or_default();
        let action = command["action"].as_str().and_then(HoverAction::from_id);
        let store = &self.gx_store;
        let menus = SidebarMenus::new(
            &store.core,
            store.sidebar_list.view(),
            &store.sidebar_list.last_inputs,
            &store.menu_host,
            now_ms(),
        );
        let items = match action {
            Some(action) => menus.row_hover_submenu(session_id, action),
            None => menus.row_menu(session_id),
        }
        .map(|items| items.iter().map(MenuItem::to_json).collect::<Vec<Value>>())
        .unwrap_or_default();
        let panel_index = self
            .native_sidebar
            .menu
            .as_ref()
            .and_then(|menu| menu.account_panel.as_ref())
            .filter(|(owner, _)| owner == owner_id)
            .map(|(_, index)| *index);
        let Some(index) = panel_index else {
            return true;
        };
        if items.is_empty() {
            self.dismiss_native_sidebar_menu(cx);
        } else if let Some(panel) =
            self.native_sidebar.menu.as_mut().and_then(|menu| menu.panels.get_mut(index))
        {
            panel.replace_items(items);
        }
        cx.notify();
        true
    }
}
