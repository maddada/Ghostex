//! Chief's row under the Spaces bar: the flower, "Chief", its status dot, and with computers
//! combined a "+N" that unfolds the other computers' Chief rows. A click opens its chat (on first
//! open it asks which agent runs it), and its menu has New Chat, Change Agent, Goal & Instructions,
//! Memory and Disable Extension.
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1734, Q1735, Q1740, Q1743, Q1747): "own row but under spaces bar please (stays static while switching space)", with the Ghostex flower; "ask which agent on first open"; the menu's Close is "Disable Extension", which turns the extension off (hides the row, stops the agent, keeps its chat and memory); with computers combined "this computer's row plus a count", whose "+N" unfolds the other computers' Chief rows the way a coordinator's threads fold.
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1911, Q1910, Q1991, Q1993): the row is now the header of the home project (gx-core `home_headers`, drawn by project_header.rs in the project header's place): its click opens Chief's chat, its chevron folds the home's sessions, its menu is Chief's items over the project's own (Move to Space included). With computers combined the homes are one list under this one header, each other computer's chat with its chip (Q1993, gx-core merged.rs). Resting on the row opens its card beside the sidebar (Q1991, window/main_hub_card.rs). The row under the Spaces bar stays only for a computer with no home project (an older gxserver) and in the Unified sidebar. New Chat and Change Agent ask first (`chiefConfirmNewChat`, window/chief_new_chat_modal.rs).
//! SEE-ALSO: packages/gx-core/src/sidebar_view/chief.rs (which rows), server/src/chief/ (the session and `/api/chief`).

use gpui::AppContext as _;
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

/// gx-core's home header as the renderer reads it.
pub(crate) fn native_home(
    view: &ghostex_gx_core::HomeHeaderView,
) -> super::model::NativeSidebarHome {
    super::model::NativeSidebarHome {
        group_id: view.group_id.clone(),
        machine_id: view.machine_id.clone(),
        machine_label: view.machine_label.clone(),
        chief: view.chief.as_ref().map(native_chief),
    }
}

/// Whether New Chat and Change Agent ask first (`chiefConfirmNewChat`, on by default).
fn confirm_new_chat_on() -> bool {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    settings
        .object()
        .get(CONFIRM_NEW_CHAT_SETTING)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

const CONFIRM_NEW_CHAT_SETTING: &str = "chiefConfirmNewChat";
const SHOW_ON_ALL_SPACES_SETTING: &str = "chiefShowOnAllSpaces";

/// The Chief of Staff card's key for the row under the Spaces bar (a home header's is its group id).
const STATIC_ROW_KEY: &str = "chief-row";

/// The status dot a Chief row draws, the same dots and order as a session card's.
pub(crate) fn chief_status(chief: &NativeSidebarChief, scale: f32) -> Option<AnyElement> {
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

fn chief_command(op: &str, machine_id: &str) -> Value {
    json!({ "type": "chief", "op": op, "machineId": machine_id })
}

impl GhostexGpuiApp {
    /// Chief's row (and the other computers' rows while unfolded), or `None` while the extension
    /// is off.
    pub(crate) fn render_native_sidebar_chief(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        unified: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !chief_row_shown(snapshot.chief_enabled, snapshot.bots_mode) {
            return None;
        }
        // A computer with a home draws its Chief as the home's header in the list; the Unified
        // sidebar, which has no project headers of this kind, keeps the row.
        let homed = |machine_id: &str| {
            !unified
                && snapshot
                    .machines_with_home
                    .iter()
                    .any(|homed| homed == machine_id)
        };
        if homed(snapshot.selected_machine_id.as_str()) {
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
            .filter(|chief| chief.machine_id != selected && !homed(&chief.machine_id))
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
            super::chief_row_view::CHIEF_LABEL,
            scale,
            colors,
            fold,
            focused,
            local.is_none(),
            focused.then(|| super::decorations::selected_outline(appearance)),
            local.and_then(|chief| chief_status(chief, scale)),
        )
        .on_prepaint(super::menus::record_menu_owner(&owner))
        // Resting on the row opens Chief of Staff's card beside the sidebar.
        .on_hover({
            let owner = owner.clone();
            cx.listener(move |app, hovered: &bool, _, cx| {
                let top = owner.get().map(|bounds| bounds.top().as_f32());
                app.hover_main_hub_row(STATIC_ROW_KEY.to_string(), top, *hovered, cx);
            })
        })
        .on_any_mouse_down(cx.listener(|app, _, _, cx| app.dismiss_main_hub_card(cx)))
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
                    command: json!({ "type": "chief", "op": "open" }),
                },
                window,
                cx,
            );
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                let items = app.native_sidebar_chief_menu(None);
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

    /// The pointer entered (`hovered`) or left a Chief of Staff row whose top in the window is `top`:
    /// after the tooltip delay its card opens beside the sidebar (window/main_hub_card.rs on the
    /// desktop, web_host/main_hub_card.rs on the web), and it closes when the pointer leaves.
    pub(crate) fn hover_main_hub_row(
        &mut self,
        key: String,
        top: Option<f32>,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if !hovered {
            if self.native_sidebar.main_hub_card_pending.as_deref() == Some(key.as_str()) {
                self.native_sidebar.main_hub_card_pending = None;
            }
            if self
                .native_sidebar
                .main_hub_card_open
                .as_ref()
                .is_some_and(|(open, _)| *open == key)
            {
                self.close_main_hub_card(cx);
            }
            return;
        }
        let Some(top) = top else {
            return;
        };
        if self.native_sidebar.main_hub_card_pending.as_deref() == Some(key.as_str()) {
            return;
        }
        self.native_sidebar.main_hub_card_pending = Some(key.clone());
        let delay = self.configured_tooltip_delay();
        cx.spawn(async move |app, cx| {
            cx.background_executor().timer(delay).await;
            let _ = app.update(cx, |app, cx| {
                let still_here =
                    app.native_sidebar.main_hub_card_pending.as_deref() == Some(key.as_str());
                if still_here
                    && app.native_sidebar.menu.is_none()
                    && !cx.has_active_drag()
                    && !app.floating_sidebar_shown()
                {
                    app.open_main_hub_card(key, top, cx);
                }
            });
        })
        .detach();
    }

    /// A press on the row: the chat or the menu takes over, so the card goes and stays gone until
    /// the pointer comes back.
    pub(crate) fn dismiss_main_hub_card(&mut self, cx: &mut gpui::Context<Self>) {
        self.native_sidebar.main_hub_card_pending = None;
        if self.native_sidebar.main_hub_card_open.is_some() {
            self.close_main_hub_card(cx);
        }
    }

    /// The "Pick an agent" row at the top of a home group whose computer has no Chief of Staff yet:
    /// a click asks which agent runs it, then its real row takes this place.
    pub(crate) fn render_native_sidebar_chief_placeholder(
        &self,
        group: &super::model::NativeSidebarGroup,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let snapshot = self.native_sidebar.snapshot.as_ref()?;
        let home = snapshot
            .home_headers
            .iter()
            .find(|home| home.group_id == group.group_id)?;
        if home.chief.is_some() || !chief_row_shown(snapshot.chief_enabled, snapshot.bots_mode) {
            return None;
        }
        let scale = appearance.scale;
        let command = chief_command("open", &home.machine_id);
        Some(
            gpui_component::h_flex()
                .id(format!(
                    "native-sidebar-chief-placeholder-{}",
                    home.machine_id
                ))
                .role(gpui::Role::Button)
                .aria_label(format!(
                    "{}: pick the agent it runs on",
                    super::chief_row_view::CHIEF_LABEL
                ))
                .h(px(34.0 * scale))
                .mx(px(3.0 * scale))
                .pl(px(8.0 * scale))
                .pr(px(8.0 * scale))
                .gap(px(8.0 * scale))
                .rounded(px(5.0 * scale))
                .cursor_pointer()
                .hover(|row| row.bg(appearance.session_hover))
                .child(
                    gpui::svg()
                        .path(super::chief_row_view::FLOWER_ICON)
                        .size(px(14.0 * scale))
                        .flex_shrink_0()
                        .text_color(appearance.muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::LIGHT)
                        .child(super::chief_row_view::CHIEF_LABEL),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * scale))
                        .text_color(appearance.muted)
                        .child("Pick an agent"),
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
                .into_any_element(),
        )
    }

    /// Main Hub's own items over the home project's menu: where it shows, whether New Chat asks,
    /// the secret folders, and Start Chief of Staff before its first open.
    ///
    /// CDXC:Chief 2026-11-08 DECISION:
    /// User: "Please don't make Main Hub a thread AND a project header ... Let's just always pin 1 thread to the top of Main Hub called Chief of Staff, and we can have general sessions there too". Main Hub's header is only the home project's: a click folds it, its menu is the project's with Main Hub's settings on top, and its card stays. Chief of Staff is a session row pinned first under it whose own menu has New Chat, Change Agent and its editors (`native_sidebar_session_menu_with_chief`); before the first open a "Pick an agent" row stands in its place (rows.rs). Supersedes the header that opened Chief's chat.
    pub(crate) fn native_sidebar_home_menu(&self, machine_id: &str) -> Value {
        let mut items = Vec::new();
        if chief_on() && self.native_sidebar_chief_of(machine_id).is_none() {
            items.push(json!({ "label": format!("Start {}", super::chief_row_view::CHIEF_LABEL), "icon": "plus", "command": chief_command("open", machine_id) }));
        }
        // The first GBot has no GBots heading to add it from yet (native_sidebar/gbots_rows.rs).
        if machine_id == ghostex_gx_core::LOCAL_MACHINE_ID && super::gbots_rows::gbots_on() {
            items.push(json!({ "label": "New GBot…", "icon": "robot", "command": { "type": "gbots", "op": "newGbot" } }));
        }
        // The settings are this computer's; another computer's home is drawn in its own entry
        // only when this one has none.
        if machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
            let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
            let settings = settings.object();
            let on = |key: &str, default: bool| {
                settings
                    .get(key)
                    .and_then(Value::as_bool)
                    .unwrap_or(default)
            };
            items.push(json!({
                "label": "Show on All Spaces",
                "checked": on(SHOW_ON_ALL_SPACES_SETTING, false),
                "command": { "type": "chief", "op": "toggleSetting", "key": SHOW_ON_ALL_SPACES_SETTING },
            }));
            items.push(json!({
                "label": "Ask Before New Chat",
                "checked": on(CONFIRM_NEW_CHAT_SETTING, true),
                "command": { "type": "chief", "op": "toggleSetting", "key": CONFIRM_NEW_CHAT_SETTING },
            }));
            if cfg!(not(target_arch = "wasm32")) {
                items.push(json!({ "label": "Folders Agents Can't Read…", "icon": "lock", "command": chief_command("homeSettings", machine_id) }));
            }
        }
        json!(items)
    }

    /// A session row's menu, with Chief of Staff's own items first when the row is a Chief.
    pub(crate) fn native_sidebar_session_menu_with_chief(
        &self,
        session: &super::model::NativeSidebarSession,
        menu: &Value,
    ) -> Value {
        if !session.is_chief {
            return menu.clone();
        }
        let machine = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&session.session_id)
            .map(|key| {
                key.machine
                    .remote_id()
                    .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
                    .to_string()
            })
            .unwrap_or_else(|| ghostex_gx_core::LOCAL_MACHINE_ID.to_string());
        let mut items = self
            .native_sidebar_chief_menu(Some(&machine))
            .as_array()
            .cloned()
            .unwrap_or_default();
        items.push(json!({ "separator": true }));
        items.extend(menu.as_array().cloned().unwrap_or_default());
        Value::Array(items)
    }

    /// The row's right-click menu, for `machine_id`'s Chief (the selected computer's when `None`).
    /// Change Agent's agents come from the last `/api/chief` status this app read
    /// (`chief_agents`).
    pub(crate) fn native_sidebar_chief_menu(&self, machine_id: Option<&str>) -> Value {
        let machine = machine_id
            .map(str::to_string)
            .unwrap_or_else(|| self.native_sidebar_selected_machine());
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
                    "command": { "type": "chief", "op": "setAgent", "agentId": agent_id, "machineId": machine },
                })
            })
            .collect();
        let has_chief = self.native_sidebar_chief_of(&machine).is_some();
        let this_computer = machine == ghostex_gx_core::LOCAL_MACHINE_ID;
        let mut items = vec![
            json!({ "label": "New Chat", "icon": "plus", "disabled": !has_chief, "command": chief_command("newChat", &machine) }),
        ];
        if agents.is_empty() {
            items.push(json!({ "label": "Change Agent", "icon": "robot", "disabled": true }));
        } else {
            items.push(json!({ "label": "Change Agent", "icon": "robot", "disabled": !has_chief, "children": agents }));
        }
        items.push(json!({ "label": "Goal & Instructions…", "icon": "target", "disabled": !has_chief, "command": chief_command("goal", &machine) }));
        items.push(json!({ "label": "Memory…", "icon": "brain", "disabled": !has_chief, "command": chief_command("memory", &machine) }));
        // The page has no settings file of its own to turn the extension off in.
        if cfg!(not(target_arch = "wasm32")) && this_computer {
            items.push(json!({ "separator": true }));
            items.push(json!({ "label": "Turn Off Main Hub", "icon": "power", "command": chief_command("disable", &machine) }));
        }
        json!(items)
    }

    /// Runs `{ type: "chief", op, machineId?, ... }` from Chief's row, its home header and their
    /// menus. `machineId` names the computer whose Chief it is (the selected one when absent).
    pub(crate) fn run_native_sidebar_chief_command(
        &mut self,
        command: &Value,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = command
            .get("machineId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.native_sidebar_selected_machine());
        let chief = self.native_sidebar_chief_of(&machine);
        match command
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "open" => match chief {
                Some(chief) => {
                    self.focus_native_sidebar_chief(&chief.sidebar_session_id, cx);
                    self.refresh_native_sidebar_chief_status(&machine, None, cx);
                }
                None => self.refresh_native_sidebar_chief_status(&machine, Some(window), cx),
            },
            "focusOther" => {
                if let Some(id) = command.get("sidebarSessionId").and_then(Value::as_str) {
                    self.focus_native_sidebar_chief(id, cx);
                }
            }
            op @ ("pickAgent" | "setAgent" | "newChat") => {
                let op = if op == "pickAgent" { "open" } else { op };
                let mut params = json!({ "op": op });
                if let Some(agent_id) = command.get("agentId").and_then(Value::as_str) {
                    params["agentId"] = json!(agent_id);
                }
                if op != "open" && confirm_new_chat_on() {
                    self.ask_before_native_sidebar_chief_new_chat(machine, params, cx);
                } else {
                    self.change_native_sidebar_chief(&machine, params, cx);
                }
            }
            action @ ("goal" | "memory") => {
                if let Some(chief) = chief {
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
            "toggleSetting" => {
                if let Some(key @ (SHOW_ON_ALL_SPACES_SETTING | CONFIRM_NEW_CHAT_SETTING)) =
                    command.get("key").and_then(Value::as_str)
                {
                    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
                    let default = key == CONFIRM_NEW_CHAT_SETTING;
                    let current = settings
                        .object()
                        .get(key)
                        .and_then(Value::as_bool)
                        .unwrap_or(default);
                    self.save_native_sidebar_chief_setting(key, !current, cx);
                }
            }
            "homeSettings" => {
                #[cfg(not(target_arch = "wasm32"))]
                self.open_gpui_settings_tab("agents", Some(window), cx);
            }
            _ => {}
        }
    }

    /// The machine tab the sidebar shows (`local` with the computers combined).
    fn native_sidebar_selected_machine(&self) -> String {
        self.native_sidebar
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.selected_machine_id.clone())
            .filter(|machine| !machine.is_empty())
            .unwrap_or_else(|| ghostex_gx_core::LOCAL_MACHINE_ID.to_string())
    }

    /// `machine_id`'s Chief.
    fn native_sidebar_chief_of(&self, machine_id: &str) -> Option<NativeSidebarChief> {
        let snapshot = self.native_sidebar.snapshot.as_ref()?;
        snapshot
            .chiefs
            .iter()
            .chain(
                snapshot
                    .home_headers
                    .iter()
                    .filter_map(|home| home.chief.as_ref()),
            )
            .find(|chief| chief.machine_id == machine_id)
            .cloned()
    }

    /// Where `/api/chief` goes: `None` for this computer, the tunnel of a remote one. `Err` when
    /// that computer is not connected.
    fn native_sidebar_chief_target(
        &self,
        machine_id: &str,
    ) -> Result<Option<crate::app::model::GpuiRemoteGxserverRequestTarget>, ()> {
        if machine_id.is_empty() || machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
            return Ok(None);
        }
        self.gpui_remote_gxserver_request_target(machine_id)
            .map(Some)
            .ok_or(())
    }

    /// The sidebar id of session `project_id`/`session_id` on `machine_id`.
    fn native_sidebar_chief_row_id(machine_id: &str, project_id: &str, session_id: &str) -> String {
        let machine = if machine_id.is_empty() || machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
            ghostex_gx_core::MachineId::Local
        } else {
            ghostex_gx_core::MachineId::Remote(machine_id.to_string())
        };
        ghostex_gx_core::SessionKey {
            machine,
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        }
        .to_sidebar_session_id()
    }

    /// New Chat and Change Agent ask first while `chiefConfirmNewChat` is on: the dialog's
    /// "Don't ask again" turns it off.
    fn ask_before_native_sidebar_chief_new_chat(
        &mut self,
        machine: String,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        use crate::app::model::GpuiAppModalKind;
        use crate::app::window::{
            CHIEF_NEW_CHAT_MODAL_INITIAL_HEIGHT, CHIEF_NEW_CHAT_MODAL_WIDTH,
            ChiefNewChatModalCommand, GpuiChiefNewChatModalWindow,
        };
        let label = super::chief_row_view::CHIEF_LABEL;
        let (title, message, confirm) = if params["op"] == "setAgent" {
            (
                format!("Move {label} to another agent?"),
                format!(
                    "A new chat starts on that agent. This chat stays under {label} as an earlier chat, and its goal, memory and threads carry over."
                ),
                "Change Agent".to_string(),
            )
        } else {
            (
                format!("Start a new chat with {label}?"),
                format!(
                    "This chat stays under {label} as an earlier chat. Its goal, memory and threads carry over to the new one."
                ),
                "New Chat".to_string(),
            )
        };
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.release_native_app_modal_window(GpuiAppModalKind::ChiefNewChat, cx);
            if let ChiefNewChatModalCommand::Start { dont_ask_again } = command {
                if dont_ask_again {
                    app.save_native_sidebar_chief_confirm(false, cx);
                }
                app.change_native_sidebar_chief(&machine, params.clone(), cx);
            }
            cx.notify();
        });
        self.open_native_app_modal(
            GpuiAppModalKind::ChiefNewChat,
            CHIEF_NEW_CHAT_MODAL_WIDTH,
            CHIEF_NEW_CHAT_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiChiefNewChatModalWindow::new(
                        palette, host, title, message, confirm, window, cx,
                    )
                })
            },
            cx,
        );
    }

    /// Saves `chiefConfirmNewChat` the way Settings does.
    fn save_native_sidebar_chief_confirm(&mut self, ask: bool, cx: &mut gpui::Context<Self>) {
        self.save_native_sidebar_chief_setting(CONFIRM_NEW_CHAT_SETTING, ask, cx);
    }

    /// Saves one of Main Hub's switches the way Settings does.
    fn save_native_sidebar_chief_setting(
        &mut self,
        key: &str,
        on: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut patch = serde_json::Map::new();
            patch.insert(key.to_string(), json!(on));
            self.handle_gpui_app_modal_update_settings_patch_message(
                &json!({
                    "patch": patch,
                    "source": "sidebar:chief",
                    "type": "updateSettingsPatch",
                }),
                cx,
            );
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (key, on, cx);
        }
    }

    fn chief_target_or_toast(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Option<crate::app::model::GpuiRemoteGxserverRequestTarget>> {
        match self.native_sidebar_chief_target(machine_id) {
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
        machine_id: &str,
        pick: Option<&mut gpui::Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = machine_id.to_string();
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
        let Some(target) = self.chief_target_or_toast(machine_id, cx) else {
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
                    let row_id = Self::native_sidebar_chief_row_id(&machine, project_id, session_id);
                    app.focus_native_sidebar_chief(&row_id, cx);
                    return;
                }
                let mut items = vec![json!({ "label": "Which agent runs Chief of Staff?", "disabled": true })];
                items.extend(app.native_sidebar.chief_agents.iter().map(|(agent_id, name)| {
                    json!({
                        "label": name,
                        "command": { "type": "chief", "op": "pickAgent", "agentId": agent_id, "machineId": machine },
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
    fn change_native_sidebar_chief(
        &mut self,
        machine_id: &str,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self.chief_target_or_toast(machine_id, cx) else {
            return;
        };
        let machine = machine_id.to_string();
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
                        let row_id =
                            Self::native_sidebar_chief_row_id(&machine, project_id, session_id);
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

    /// Disable Extension (and its Undo): flips `mainHubHidden` in this computer's settings, the way
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
                    "patch": { "mainHubHidden": !enabled },
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
