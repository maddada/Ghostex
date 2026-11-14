//! What a tap on a panel does: Ghostex's built-in actions (open a project, focus a session, start
//! an agent, open a link or a view), checked against the permissions the user granted; an
//! extension's own action, sent to gxserver, which tells the extension; and an editable
//! checklist's edits, which gxserver applies (`/api/extensionPanelAction`).

use ghostex_gx_core::extension_panel::{panel_action_allowed, resolve_panel_project};
use ghostex_gx_core::{MachineId, ProjectKey, SessionKey, SidebarMode};
use ghostex_gx_protocol::extension_panel::{PanelAction, PanelMenuItem};
use gpui::{AppContext as _, Window};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};

use super::PanelPlace;
use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;

/// A user's edit of an editable checklist, which gxserver applies and saves.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExtensionPanelEdit {
    Check { item_id: String, done: bool },
    Add { text: String },
    Rename { item_id: String, text: String },
    Delete { item_id: String },
}

impl ExtensionPanelEdit {
    fn wire(&self) -> Value {
        match self {
            Self::Check { item_id, done } => {
                json!({"op": "check", "itemId": item_id, "done": done})
            }
            Self::Add { text } => json!({"op": "add", "text": text}),
            Self::Rename { item_id, text } => {
                json!({"op": "rename", "itemId": item_id, "text": text})
            }
            Self::Delete { item_id } => json!({"op": "delete", "itemId": item_id}),
        }
    }
}

/// The name editor's id for a checklist item being renamed or added (rename.rs).
pub(super) fn editor_id(place: &PanelPlace, block_id: &str, item_id: Option<&str>) -> String {
    json!({"place": place.wire(), "block": block_id, "item": item_id}).to_string()
}

/// The label of a permission in a refusal, as Settings > Extensions names it.
fn permission_label(permission: &str) -> &'static str {
    match permission {
        "sessions.open" => "Open projects and sessions",
        "agents.start" => "Start agents",
        "projects.read" => "See your projects",
        "sessions.read" => "See your sessions",
        _ => "this permission",
    }
}

impl GhostexGpuiApp {
    /// Runs a tap on `action` from a block of `place`.
    pub(crate) fn run_native_panel_action(
        &mut self,
        place: &PanelPlace,
        block_id: &str,
        item_id: Option<&str>,
        action: &PanelAction,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !panel_action_allowed(action, &place.granted_permissions) {
            let needed = action.required_permission().unwrap_or_default();
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Extension not allowed",
                &format!(
                    "{} needs “{}”. Allow it in Settings > Extensions.",
                    place.extension_id,
                    permission_label(needed)
                ),
                cx,
            );
            return;
        }
        match action {
            PanelAction::OpenProject(reference) => {
                self.open_native_panel_project(place, reference, window, cx)
            }
            PanelAction::FocusSession(session) => {
                self.focus_native_panel_session(place, session, window, cx)
            }
            PanelAction::NewAgent(new_agent) => {
                let Some(project_id) = self.native_panel_project_id(&new_agent.project) else {
                    self.native_panel_missing_project(&new_agent.project, cx);
                    return;
                };
                let agent_id = new_agent
                    .agent
                    .clone()
                    .or_else(crate::app::gx_store::read_primary_agent_launcher_id)
                    .unwrap_or_else(|| "claude".to_string());
                self.gx_store.create.pending_first_input_draft = new_agent.draft.clone();
                let group_id = ProjectKey {
                    machine: MachineId::Local,
                    project_id,
                }
                .to_sidebar_group_id();
                self.gx_store_request_agent_launch(&agent_id, Some(&group_id), None, cx);
            }
            PanelAction::OpenUrl(url) => cx.open_url(url),
            PanelAction::OpenView(view) => self.open_native_panel_view(view, window, cx),
            PanelAction::Extension(target) => {
                let mut params = place.wire();
                params["blockId"] = json!(block_id);
                params["action"] = json!(target.name());
                if let Some(item_id) = item_id {
                    params["itemId"] = json!(item_id);
                }
                if let ghostex_gx_protocol::extension_panel::ExtensionActionTarget::Detailed {
                    data: Some(data),
                    ..
                } = target
                {
                    params["data"] = data.clone();
                }
                self.send_native_panel_rpc(params, cx);
            }
        }
    }

    /// Sends a checklist edit; gxserver applies it, and the new panel arrives with the next
    /// `extensionPanelsChanged`.
    pub(crate) fn send_native_panel_edit(
        &mut self,
        place: &PanelPlace,
        block_id: &str,
        edit: ExtensionPanelEdit,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params = place.wire();
        params["blockId"] = json!(block_id);
        params["edit"] = edit.wire();
        self.send_native_panel_rpc(params, cx);
    }

    fn send_native_panel_rpc(&mut self, params: Value, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |app, cx| {
            let answer = gx_rpc(None, "/api/extensionPanelAction", params).await;
            if let Err(error) = answer {
                let _ = app.update(cx, |app, cx| {
                    app.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Panel didn't update",
                        &error.message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// The local project a panel names, by id or path.
    fn native_panel_project_id(&self, reference: &str) -> Option<String> {
        let loaded = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&MachineId::Local)?;
        resolve_panel_project(
            reference,
            loaded
                .projects()
                .iter()
                .map(|project| (project.project_id.as_str(), project.path.as_deref())),
            native_panel_home_dir().as_deref(),
        )
    }

    fn native_panel_missing_project(&mut self, reference: &str, cx: &mut gpui::Context<Self>) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Project not found",
            &format!("{reference} isn't a project on this computer. Add it with Add Project."),
            cx,
        );
    }

    /// Opens a project: its most recent session, shown in the Projects list.
    pub(crate) fn open_native_panel_project(
        &mut self,
        place: &PanelPlace,
        reference: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project_id) = self.native_panel_project_id(reference) else {
            self.native_panel_missing_project(reference, cx);
            return;
        };
        let latest = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&MachineId::Local)
            .and_then(|loaded| {
                loaded
                    .server_sessions()
                    .filter(|session| session.project_id == project_id)
                    .max_by(|left, right| left.last_active_at.cmp(&right.last_active_at))
                    .map(|session| session.session_id.clone())
            });
        self.leave_native_extension_mode(place, cx);
        if let Some(session_id) = latest {
            self.focus_native_panel_session(
                place,
                &format!("{project_id}:{session_id}"),
                window,
                cx,
            );
        }
    }

    /// Focuses a session (`<projectId>:<sessionId>`) as a click on its row does.
    pub(crate) fn focus_native_panel_session(
        &mut self,
        place: &PanelPlace,
        session: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((project_id, session_id)) = session.split_once(':') else {
            return;
        };
        let sidebar_session_id = SessionKey {
            machine: MachineId::Local,
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        }
        .to_sidebar_session_id();
        self.leave_native_extension_mode(place, cx);
        self.close_sidebar_extension_view(window, cx);
        self.dispatch_native_sidebar_ui(
            json!({"type": "selectSession", "sessionId": sidebar_session_id, "mode": "focus"}),
            cx,
        );
        let _ = self.react_to_native_sidebar_session_click(&sidebar_session_id, cx);
        self.reveal_floating_sessions(cx);
    }

    /// A tap in the sidebar's extension mode that opens a project or a session shows the
    /// Projects list again, where that row now is. A side panel leaves the sidebar alone.
    fn leave_native_extension_mode(&mut self, place: &PanelPlace, cx: &mut gpui::Context<Self>) {
        if place.surface == "sidebar"
            && self.gx_store_sidebar_mode().extension_id() == Some(place.extension_id.as_str())
        {
            self.dispatch_native_sidebar_ui(
                json!({"type": "setSidebarMode", "mode": SidebarMode::Projects.as_wire()}),
                cx,
            );
        }
    }

    #[cfg(not(ghostex_web))]
    fn open_native_panel_view(
        &mut self,
        view: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mode = crate::app::model::TitlebarMode::from_slug(view).or_else(|| {
            crate::ExtensionId::new(view).map(crate::app::model::TitlebarMode::Extension)
        });
        match mode.filter(|mode| self.titlebar_mode_available(*mode)) {
            Some(mode) => {
                self.open_view_tab(mode, window, cx);
            }
            None => self.dispatch_gpui_app_modal_toast(
                "warning",
                "View not available",
                &format!("{view} isn't a view Ghostex can open here."),
                cx,
            ),
        }
    }

    #[cfg(ghostex_web)]
    fn open_native_panel_view(
        &mut self,
        view: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_app_modal_toast(
            "info",
            "Open it on the computer",
            &format!("The {view} view opens in the Ghostex app."),
            cx,
        );
    }

    /// The right-click menu of an item: its own menu, then Rename and Delete for an editable
    /// checklist's item. Every row is a sidebar menu command (`extensionPanel`).
    pub(crate) fn native_panel_menu(
        place: &PanelPlace,
        block_id: &str,
        item_id: Option<&str>,
        menu: Option<&[PanelMenuItem]>,
        editable: bool,
    ) -> Value {
        let mut items: Vec<Value> = menu
            .unwrap_or_default()
            .iter()
            .map(|item| {
                let allowed = panel_action_allowed(&item.action, &place.granted_permissions);
                json!({
                    "label": item.title,
                    "icon": item.icon.clone().unwrap_or_default(),
                    "disabled": !allowed,
                    "command": {
                        "type": "extensionPanel",
                        "op": "action",
                        "place": place.wire_with_grants(),
                        "blockId": block_id,
                        "itemId": item_id,
                        "action": item.action,
                    },
                })
            })
            .map(|mut item| {
                if item["icon"] == "" {
                    item.as_object_mut().map(|item| item.remove("icon"));
                }
                item
            })
            .collect();
        if editable && let Some(item_id) = item_id {
            if !items.is_empty() {
                items.push(json!({"separator": true}));
            }
            for (label, icon, op) in [
                ("Rename", "pencil", "rename"),
                ("Delete", "trash", "delete"),
            ] {
                items.push(json!({
                    "label": label,
                    "icon": icon,
                    "command": {
                        "type": "extensionPanel",
                        "op": op,
                        "place": place.wire_with_grants(),
                        "blockId": block_id,
                        "itemId": item_id,
                    },
                }));
            }
        }
        Value::Array(items)
    }

    /// A sidebar menu row of a panel (`native_panel_menu`), routed here by actions.rs.
    pub(crate) fn run_native_sidebar_extension_panel_command(
        &mut self,
        command: &Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(place) = PanelPlace::from_wire(&command["place"]) else {
            return;
        };
        let block_id = command["blockId"].as_str().unwrap_or_default().to_string();
        let item_id = command["itemId"].as_str().map(str::to_string);
        match command["op"].as_str() {
            Some("action") => {
                if let Ok(action) = serde_json::from_value::<PanelAction>(command["action"].clone())
                {
                    self.run_native_panel_action(
                        &place,
                        &block_id,
                        item_id.as_deref(),
                        &action,
                        window,
                        cx,
                    );
                }
            }
            Some("delete") => {
                if let Some(item_id) = item_id {
                    self.send_native_panel_edit(
                        &place,
                        &block_id,
                        ExtensionPanelEdit::Delete { item_id },
                        cx,
                    );
                }
            }
            Some("rename") => {
                let text = command["text"].as_str().map(str::to_string).or_else(|| {
                    self.native_panel_item_text(&place, &block_id, item_id.as_deref()?)
                });
                self.begin_native_panel_item_editor(
                    &place,
                    &block_id,
                    item_id.as_deref(),
                    text.as_deref().unwrap_or_default(),
                    window,
                    cx,
                );
            }
            _ => {}
        }
    }

    /// The text of a checklist item as the panel on screen has it.
    fn native_panel_item_text(
        &self,
        place: &PanelPlace,
        block_id: &str,
        item_id: &str,
    ) -> Option<String> {
        let panels = self.native_extension_panels()?;
        let entry = panels.0.get(&place.extension_id)?;
        let document = match (place.surface, &place.session) {
            ("sidebar", _) => &entry.sidebar.as_ref()?.document,
            (_, Some(session)) => entry
                .side_panel
                .as_ref()?
                .sessions
                .get(session)
                .map(|panel| &panel.document)
                .unwrap_or(&entry.side_panel.as_ref()?.document),
            _ => &entry.side_panel.as_ref()?.document,
        };
        let block = document.blocks.iter().find(|block| block.id == block_id)?;
        match &block.body {
            ghostex_gx_protocol::extension_panel::PanelBlockBody::Checklist { items, .. } => items
                .iter()
                .find(|item| item.id == item_id)
                .map(|item| item.text.clone()),
            _ => None,
        }
    }

    /// Opens the inline field that renames a checklist item, or adds one (`item_id` `None`).
    pub(crate) fn begin_native_panel_item_editor(
        &mut self,
        place: &PanelPlace,
        block_id: &str,
        item_id: Option<&str>,
        initial: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let input = cx.new(|cx| {
            let input = InputState::new(window, cx).default_value(initial.to_string());
            if item_id.is_none() {
                input.placeholder("Add an item")
            } else {
                input
            }
        });
        let subscription = cx.subscribe_in(&input, window, |app, _, event, _, cx| match event {
            InputEvent::Focus => {
                app.drop_pending_browser_keyboard_handoff();
                app.reclaim_gpui_root_for_chrome_input_focus();
            }
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                app.finish_native_sidebar_rename(true, cx);
            }
            _ => {}
        });
        self.native_sidebar.name_editor =
            Some(crate::app::native_sidebar::rename::SidebarNameEditor::new(
                if item_id.is_some() {
                    "extensionItem"
                } else {
                    "extensionAdd"
                },
                editor_id(place, block_id, item_id),
                input.clone(),
                subscription,
            ));
        let length = initial.len();
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(0..length, cx);
        });
        cx.notify();
    }

    /// Saves what the inline field holds (rename.rs calls this for the panel editors).
    pub(crate) fn finish_native_panel_item_editor(
        &mut self,
        kind: &str,
        id: &str,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(editor) = serde_json::from_str::<Value>(id) else {
            return;
        };
        let Some(place) = PanelPlace::from_wire(&editor["place"]) else {
            return;
        };
        let block_id = editor["block"].as_str().unwrap_or_default().to_string();
        let edit = match (kind, editor["item"].as_str()) {
            ("extensionItem", Some(item_id)) => ExtensionPanelEdit::Rename {
                item_id: item_id.to_string(),
                text: text.to_string(),
            },
            ("extensionAdd", _) => ExtensionPanelEdit::Add {
                text: text.to_string(),
            },
            _ => return,
        };
        self.send_native_panel_edit(&place, &block_id, edit, cx);
    }
}

#[cfg(not(ghostex_web))]
fn native_panel_home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .filter(|home| !home.is_empty())
}

#[cfg(ghostex_web)]
fn native_panel_home_dir() -> Option<String> {
    None
}
