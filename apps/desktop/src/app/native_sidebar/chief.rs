//! Chief's row under the Spaces bar: the flower, "Chief", its status dot, and with computers
//! combined a "+N" that unfolds the other computers' Chief rows. A click opens its chat (on first
//! open it asks which agent runs it), and its menu has New Chat, Change Agent, Goal & Instructions,
//! Memory and Disable Extension.
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1734, Q1735, Q1740, Q1743, Q1747): "own row but under spaces bar please (stays static while switching space)", with the Ghostex flower; "ask which agent on first open"; the menu's Close is "Disable Extension", which turns the extension off (hides the row, stops the agent, keeps its chat and memory); with computers combined "this computer's row plus a count", whose "+N" unfolds the other computers' Chief rows the way a coordinator's threads fold.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/chief.rs (which rows), server/src/chief/ (the session and `/api/chief`).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::ElementExt as _;
use serde_json::{Value, json};

use super::actions::NativeSidebarAction;
use super::appearance::SidebarAppearance;
use super::chief_row_view::{
    ChiefFold, ChiefRowColors, chief_block, chief_header_row, chief_other_row, chief_row_shown,
};
use super::model::{NativeSidebarChief, NativeSidebarSnapshot};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// Whether the Chief built-in extension is on in this computer's settings.
pub(crate) fn chief_on() -> bool {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let settings = settings.object();
    ghostex_settings_catalog::built_in_extensions::on_with(
        ghostex_settings_catalog::built_in_extensions::CHIEF,
        |key| settings.get(key).and_then(Value::as_bool),
    )
}

/// gx-core's Chief row as the renderer reads it.
pub(crate) fn native_chief(view: &ghostex_gx_core::ChiefRowView) -> NativeSidebarChief {
    let row = &view.row;
    let key = row.key.as_ref();
    NativeSidebarChief {
        machine_id: view.machine_id.clone(),
        machine_label: view.machine_label.clone(),
        is_local: view.is_local,
        project_id: key.map(|key| key.project_id.clone()).unwrap_or_default(),
        session_id: key.map(|key| key.session_id.clone()).unwrap_or_default(),
        sidebar_session_id: row.sidebar_session_id.clone(),
        activity: row.activity.clone(),
        has_question: row.has_question(),
        awaiting_approval: row.awaiting_approval,
        has_background_work: row.has_background_work,
        failed: row.model_selection_failed || row.program_status_failed || row.stuck,
        lifecycle_state: row.lifecycle_state.clone(),
        is_focused: view.is_focused,
    }
}

/// The status dot a Chief row draws, the same dots and order as a session card's.
fn chief_status(chief: &NativeSidebarChief, scale: f32) -> Option<AnyElement> {
    use super::status::{activity_indicator, approval_indicator, question_indicator};
    if chief.awaiting_approval {
        return Some(approval_indicator(scale));
    }
    if chief.failed {
        return activity_indicator(&chief.activity, false, true, scale);
    }
    if chief.has_question {
        return Some(question_indicator(
            &chief.activity,
            scale,
            titlebar_background(),
        ));
    }
    activity_indicator(&chief.activity, chief.has_background_work, false, scale)
}

fn chief_command(op: &str) -> Value {
    json!({ "type": "chief", "op": op })
}

impl GhostexGpuiApp {
    /// Chief's row (and the other computers' rows while unfolded), or `None` while the extension
    /// is off.
    pub(crate) fn render_native_sidebar_chief(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !chief_row_shown(snapshot.chief_enabled, snapshot.bots_mode) {
            return None;
        }
        let scale = appearance.scale;
        // The selected computer's Chief is the main row (this computer's with computers combined,
        // the tab's own with Separate by machine); the others fold under it.
        let selected = snapshot.selected_machine_id.as_str();
        let local = snapshot
            .chiefs
            .iter()
            .find(|chief| chief.machine_id == selected);
        let others: Vec<&NativeSidebarChief> = snapshot
            .chiefs
            .iter()
            .filter(|chief| chief.machine_id != selected)
            .collect();
        let expanded = self.native_sidebar.chief_others_expanded && !others.is_empty();
        let fold = match (others.is_empty(), expanded) {
            (true, _) => ChiefFold::Single,
            (false, false) => ChiefFold::Folded,
            (false, true) => ChiefFold::Unfolded,
        };
        let owner = self.native_sidebar.chief_row_bounds.clone();
        let menu_owner = owner.clone();
        let colors = ChiefRowColors {
            muted: appearance.muted,
            hover: appearance.session_hover,
            selected_fill: appearance.selected,
        };
        let focused = local.is_some_and(|chief| chief.is_focused);
        let main = chief_header_row(
            scale,
            colors,
            fold,
            focused,
            local.is_none(),
            focused.then(|| super::decorations::selected_outline(appearance)),
            local.and_then(|chief| chief_status(chief, scale)),
        )
        .on_prepaint(super::menus::record_menu_owner(&owner))
        // The fold chevron is its own hit area: a click anywhere else on the row opens the chat.
        .when(fold != ChiefFold::Single, |row| {
            row.child(
                div()
                    .id("native-sidebar-chief-fold")
                    .role(gpui::Role::Button)
                    .aria_label(if expanded {
                        "Fold Chief of Staff on the other computers"
                    } else {
                        "Show Chief of Staff on the other computers"
                    })
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .w(px(20.0 * scale))
                    .on_click(cx.listener(|app, _, _, cx| {
                        cx.stop_propagation();
                        app.native_sidebar.chief_others_expanded =
                            !app.native_sidebar.chief_others_expanded;
                        cx.notify();
                    })),
            )
        })
        .on_click(cx.listener(|app, _, window, cx| {
            cx.stop_propagation();
            app.handle_native_sidebar_action(
                &NativeSidebarAction {
                    command: chief_command("open"),
                },
                window,
                cx,
            );
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                let items = app.native_sidebar_chief_menu();
                Self::show_native_sidebar_menu_hung(
                    &items,
                    menu_owner.get(),
                    None,
                    Some(("chief".to_owned(), Some(event.position))),
                    event.position,
                    scale,
                    window,
                    cx,
                );
            }),
        );
        let other_rows = others.into_iter().filter(|_| expanded).map(|chief| {
            let command = json!({
                "type": "chief",
                "op": "focusOther",
                "sidebarSessionId": chief.sidebar_session_id,
            });
            chief_other_row(
                format!("native-sidebar-chief-{}", chief.machine_id),
                scale,
                colors,
                &chief.machine_label,
                chief.is_focused,
                chief
                    .is_focused
                    .then(|| super::decorations::selected_outline(appearance)),
                chief_status(chief, scale),
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                app.handle_native_sidebar_action(
                    &NativeSidebarAction {
                        command: command.clone(),
                    },
                    window,
                    cx,
                );
            }))
        });
        Some(
            chief_block(scale)
                .child(main)
                .children(other_rows)
                .into_any_element(),
        )
    }

    /// The row's right-click menu. Change Agent's agents come from the last `/api/chief` status
    /// this app read (`chief_agents`).
    fn native_sidebar_chief_menu(&self) -> Value {
        let current = self
            .native_sidebar
            .chief_agent_id
            .clone()
            .unwrap_or_default();
        let agents: Vec<Value> = self
            .native_sidebar
            .chief_agents
            .iter()
            .map(|(agent_id, name)| {
                json!({
                    "label": name,
                    "checked": *agent_id == current,
                    "command": { "type": "chief", "op": "setAgent", "agentId": agent_id },
                })
            })
            .collect();
        let has_chief = self.native_sidebar_main_chief().is_some();
        let this_computer = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| {
                snapshot.selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID
            });
        let mut items = vec![
            json!({ "label": "New Chat", "icon": "plus", "disabled": !has_chief, "command": chief_command("newChat") }),
        ];
        if agents.is_empty() {
            items.push(json!({ "label": "Change Agent", "icon": "robot", "disabled": true }));
        } else {
            items.push(json!({ "label": "Change Agent", "icon": "robot", "disabled": !has_chief, "children": agents }));
        }
        items.push(json!({ "label": "Goal & Instructions…", "icon": "target", "disabled": !has_chief, "command": chief_command("goal") }));
        items.push(json!({ "label": "Memory…", "icon": "brain", "disabled": !has_chief, "command": chief_command("memory") }));
        // The page has no settings file of its own to turn the extension off in.
        if cfg!(not(target_arch = "wasm32")) && this_computer {
            items.push(json!({ "separator": true }));
            items.push(json!({ "label": "Disable Extension", "icon": "power", "command": chief_command("disable") }));
        }
        json!(items)
    }

    /// Runs `{ type: "chief", op, ... }` from Chief's row and menu.
    pub(crate) fn run_native_sidebar_chief_command(
        &mut self,
        command: &Value,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let local = self.native_sidebar_main_chief();
        match command
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "open" => match local {
                Some(chief) => {
                    self.focus_native_sidebar_chief(&chief.sidebar_session_id, cx);
                    self.refresh_native_sidebar_chief_status(None, cx);
                }
                None => self.refresh_native_sidebar_chief_status(Some(window), cx),
            },
            "focusOther" => {
                if let Some(id) = command.get("sidebarSessionId").and_then(Value::as_str) {
                    self.focus_native_sidebar_chief(id, cx);
                }
            }
            "pickAgent" | "setAgent" | "newChat" => {
                let op = match command["op"].as_str() {
                    Some("pickAgent") => "open",
                    Some(op) => op,
                    None => return,
                };
                let mut params = json!({ "op": op });
                if let Some(agent_id) = command.get("agentId").and_then(Value::as_str) {
                    params["agentId"] = json!(agent_id);
                }
                self.change_native_sidebar_chief(params, cx);
            }
            action @ ("goal" | "memory") => {
                if let Some(chief) = local {
                    self.dispatch_native_sidebar_command(
                        json!({
                            "type": "coordinatorSessionAction",
                            "sessionId": chief.sidebar_session_id,
                            "action": action,
                        }),
                        cx,
                    );
                }
            }
            "disable" => self.set_native_sidebar_chief_enabled(false, cx),
            _ => {}
        }
    }

    /// The selected computer's Chief.
    fn native_sidebar_main_chief(&self) -> Option<NativeSidebarChief> {
        let snapshot = self.native_sidebar.snapshot.as_ref()?;
        snapshot
            .chiefs
            .iter()
            .find(|chief| chief.machine_id == snapshot.selected_machine_id)
            .cloned()
    }

    /// Where `/api/chief` goes: `None` for this computer, the tunnel of the selected remote one.
    /// `Err` when that computer is not connected.
    fn native_sidebar_chief_target(
        &self,
    ) -> Result<Option<crate::app::model::GpuiRemoteGxserverRequestTarget>, ()> {
        let machine_id = self
            .native_sidebar
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.selected_machine_id.clone())
            .unwrap_or_default();
        if machine_id.is_empty() || machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
            return Ok(None);
        }
        self.gpui_remote_gxserver_request_target(&machine_id)
            .map(Some)
            .ok_or(())
    }

    /// The sidebar id of session `project_id`/`session_id` on the selected computer.
    fn native_sidebar_chief_row_id(&self, project_id: &str, session_id: &str) -> String {
        let machine = match self.native_sidebar.snapshot.as_ref() {
            Some(snapshot) if snapshot.selected_machine_id != ghostex_gx_core::LOCAL_MACHINE_ID => {
                ghostex_gx_core::MachineId::Remote(snapshot.selected_machine_id.clone())
            }
            _ => ghostex_gx_core::MachineId::Local,
        };
        ghostex_gx_core::SessionKey {
            machine,
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        }
        .to_sidebar_session_id()
    }

    fn chief_target_or_toast(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Option<crate::app::model::GpuiRemoteGxserverRequestTarget>> {
        match self.native_sidebar_chief_target() {
            Ok(target) => Some(target),
            Err(()) => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "That computer is not connected",
                    "",
                    cx,
                );
                None
            }
        }
    }

    /// Opens a Chief's chat like a click on its session card would.
    fn focus_native_sidebar_chief(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_native_sidebar_ui(
            json!({ "type": "selectSession", "sessionId": sidebar_session_id, "mode": "focus" }),
            cx,
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = self.react_to_native_sidebar_session_click(sidebar_session_id, cx);
        }
    }

    /// Reads `/api/chief` status: the agents Chief can run on (for Change Agent) and its current
    /// agent. With `pick`, Chief has no session yet and the agent picker opens from its row.
    fn refresh_native_sidebar_chief_status(
        &mut self,
        pick: Option<&mut gpui::Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        let remember = |app: &mut Self, status: &Value| {
            app.native_sidebar.chief_agents = status["agents"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| {
                            let id = row["agentId"].as_str()?;
                            let name = row["name"].as_str().unwrap_or(id);
                            Some((id.to_string(), name.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            app.native_sidebar.chief_agent_id =
                status["chief"]["agentId"].as_str().map(str::to_string);
        };
        let Some(target) = self.chief_target_or_toast(cx) else {
            return;
        };
        let Some(window) = pick else {
            cx.spawn(async move |this, cx| {
                let status =
                    crate::app::gx_store::gx_rpc(target, "/api/chief", json!({ "op": "status" }))
                        .await;
                if let Ok(status) = status {
                    let _ = this.update(cx, |app, _| remember(app, &status));
                }
            })
            .detach();
            return;
        };
        let owner = self.native_sidebar.chief_row_bounds.clone();
        cx.spawn_in(window, async move |this, cx| {
            let status =
                crate::app::gx_store::gx_rpc(target, "/api/chief", json!({ "op": "status" })).await;
            let _ = this.update_in(cx, |app, window, cx| {
                let status = match status {
                    Ok(status) => status,
                    Err(error) => {
                        app.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Chief of Staff isn't available",
                            &error.message,
                            cx,
                        );
                        return;
                    }
                };
                remember(app, &status);
                // Opened from another window or the phone meanwhile: just open it.
                if let (Some(project_id), Some(session_id)) = (
                    status["chief"]["projectId"].as_str(),
                    status["chief"]["sessionId"].as_str(),
                ) {
                    let row_id = app.native_sidebar_chief_row_id(project_id, session_id);
                    app.focus_native_sidebar_chief(&row_id, cx);
                    return;
                }
                let mut items = vec![json!({ "label": "Which agent runs Chief of Staff?", "disabled": true })];
                items.extend(app.native_sidebar.chief_agents.iter().map(|(agent_id, name)| {
                    json!({
                        "label": name,
                        "command": { "type": "chief", "op": "pickAgent", "agentId": agent_id },
                    })
                }));
                if items.len() == 1 {
                    app.dispatch_gpui_app_modal_toast(
                        "warning",
                        "No agent can run Chief of Staff",
                        "Set up Claude, Codex or another coordinator agent in Settings > Agents first.",
                        cx,
                    );
                    return;
                }
                let position = owner
                    .get()
                    .map(|bounds| bounds.bottom_left())
                    .unwrap_or_default();
                let scale = app
                    .native_sidebar
                    .snapshot
                    .as_ref()
                    .map(|snapshot| super::appearance::SidebarAppearance::from_hud(&snapshot.hud, window).scale)
                    .unwrap_or(1.0);
                Self::show_native_sidebar_menu_hung(
                    &json!(items),
                    owner.get(),
                    None,
                    Some(("chief-pick".to_owned(), None)),
                    position,
                    scale,
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    /// `open` (first open with the picked agent), `newChat` or `setAgent`, then opens the chat.
    fn change_native_sidebar_chief(&mut self, params: Value, cx: &mut gpui::Context<Self>) {
        let Some(target) = self.chief_target_or_toast(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let answer = crate::app::gx_store::gx_rpc(target, "/api/chief", params).await;
            let _ = this.update(cx, |app, cx| match answer {
                Ok(result) => {
                    let chief = &result["chief"];
                    if let Some(agent_id) = chief["agentId"].as_str() {
                        app.native_sidebar.chief_agent_id = Some(agent_id.to_string());
                    }
                    if let (Some(project_id), Some(session_id)) =
                        (chief["projectId"].as_str(), chief["sessionId"].as_str())
                    {
                        let row_id = app.native_sidebar_chief_row_id(project_id, session_id);
                        app.focus_native_sidebar_chief(&row_id, cx);
                    }
                }
                Err(error) => app.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Chief of Staff didn't start",
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }

    /// Disable Extension (and its Undo): flips `chiefHidden` in this computer's settings, the way
    /// the extension's switch in Settings > Extensions does.
    pub(crate) fn set_native_sidebar_chief_enabled(
        &mut self,
        enabled: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.handle_gpui_app_modal_update_settings_patch_message(
                &json!({
                    "patch": { "chiefHidden": !enabled },
                    "source": "sidebar:chief",
                    "type": "updateSettingsPatch",
                }),
                cx,
            );
            if !enabled {
                self.show_chief_disabled_toast(cx);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (enabled, cx);
        }
    }
}
