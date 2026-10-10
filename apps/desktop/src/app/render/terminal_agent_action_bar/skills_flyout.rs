// The Skills flyout of the terminal bar's ⋯ menu: the Ghostex skills the session's agent has
// installed, each typing its invocation into the agent's input (never Enter), then Configure /
// Install more (Settings > Integrations > Agent skills). The rule, and the user's decision, live in
// gx-chat-core's `composer/ghostex_skills.rs`, which the chat's More actions menu reads too; the rows
// come from gxserver's `readSessionChatSkills`, the read the chat's `$` picker makes.

use std::time::Duration;

use gpui::AnyElement;
use gpui::BoxShadow;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Rgba;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::v_flex;
use serde_json::{Value, json};

use super::palette::*;
use super::{
    TERMINAL_AGENT_BAR_ACCOUNT_SUBMENU_GAP, TERMINAL_AGENT_BAR_MENU_ICON_SIZE,
    TerminalAgentBarSurface, terminal_agent_bar_icon,
};
use crate::app::helpers::gpui_gxserver_rpc_result;
use crate::app::model::*;
use crate::*;

pub(super) const SKILLS_FLYOUT_WIDTH: f32 = 300.0;
const SETTINGS_ICON: &str = "titlebar/settings.svg";

impl GhostexGpuiApp {
    /// Opens or shuts the flyout; opening asks gxserver for the session's skills.
    pub(super) fn toggle_terminal_agent_bar_skills(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_terminal_action_bar_account_submenu_open = false;
        self.agents_terminal_action_bar_account_page = None;
        if self.agents_terminal_action_bar_skills.take().is_some() {
            cx.notify();
            return;
        }
        // Like Switch Account, the list is read for sessions on this computer.
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()))
        else {
            self.agents_terminal_action_bar_skills = Some(vec![
                json!({"label": "Skills are listed for sessions on this computer."}),
            ]);
            cx.notify();
            return;
        };
        self.agents_terminal_action_bar_skills = Some(vec![json!({"label": "Loading skills…"})]);
        cx.notify();
        let params = json!({"projectId": key.project_id, "sessionId": key.session_id});
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/readSessionChatSkills",
                        &params,
                        Duration::from_secs(20),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.agents_terminal_action_bar_menu_session != Some(session_id)
                    || this.agents_terminal_action_bar_skills.is_none()
                {
                    return;
                }
                this.agents_terminal_action_bar_skills = Some(terminal_skill_rows(result));
                cx.notify();
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_terminal_agent_bar_skills_flyout(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        menu_width: f32,
        opens_left: bool,
        rows: &[Value],
        suffix: &str,
        flyout_bounds: &super::TerminalBarPopupBounds,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let offset = px(menu_width + TERMINAL_AGENT_BAR_ACCOUNT_SUBMENU_GAP);
        let mut flyout = div()
            .id(format!("ghostex-gpui-terminal-agent-bar-skills-{suffix}"))
            .absolute()
            .when(opens_left, |this| this.right(offset))
            .when(!opens_left, |this| this.left(offset))
            .bottom_0()
            .w(px(SKILLS_FLYOUT_WIDTH))
            .font_family(crate::ui_fonts::UI_FONT)
            .flex()
            .flex_col()
            .p(px(5.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(terminal_agent_bar_menu_border())
            .bg(terminal_agent_bar_menu_background())
            .shadow(vec![
                BoxShadow::new(
                    px(0.0),
                    px(10.0),
                    Rgba {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.45,
                    }
                    .into(),
                )
                .blur_radius(px(22.0)),
            ])
            .occlude()
            .child(super::terminal_agent_bar_popup_bounds_probe(flyout_bounds));
        for (index, row) in rows.iter().enumerate() {
            let label = row["label"].as_str().unwrap_or_default().to_owned();
            let Some(invocation) = row["invocation"].as_str().map(str::to_owned) else {
                // A status line: loading, nothing installed, or the read failed.
                flyout = flyout.child(
                    div()
                        .px(px(9.0))
                        .py(px(6.0))
                        .text_size(px(13.0))
                        .text_color(terminal_agent_bar_disabled_icon_color())
                        .child(label),
                );
                continue;
            };
            let description = row["description"].as_str().map(str::to_owned);
            flyout = flyout.child(
                v_flex()
                    .id(format!(
                        "ghostex-gpui-terminal-agent-bar-skill-{index}-{suffix}"
                    ))
                    .w_full()
                    .px(px(9.0))
                    .py(px(6.0))
                    .rounded(px(7.0))
                    .cursor_default()
                    .hover(|this| this.bg(terminal_agent_bar_hover_background()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.type_terminal_agent_bar_skill(
                                surface,
                                session_id,
                                &invocation,
                                window,
                                cx,
                            );
                        }),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(terminal_agent_bar_menu_text_color())
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(label),
                    )
                    .when_some(description, |this, description| {
                        this.child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .text_color(terminal_agent_bar_session_id_color())
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(description),
                        )
                    }),
            );
        }
        flyout
            .child(super::terminal_agent_bar_menu_separator())
            .child(
                h_flex()
                    .id(format!(
                        "ghostex-gpui-terminal-agent-bar-skills-configure-{suffix}"
                    ))
                    .w_full()
                    .items_center()
                    .gap(px(9.0))
                    .px(px(9.0))
                    .py(px(6.0))
                    .rounded(px(7.0))
                    .cursor_default()
                    .text_size(px(13.0))
                    .text_color(terminal_agent_bar_menu_text_color())
                    .hover(|this| this.bg(terminal_agent_bar_hover_background()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.close_terminal_agent_action_bar_menu(cx);
                            this.open_gpui_settings_agent_skills(Some(window), cx);
                        }),
                    )
                    .child(terminal_agent_bar_icon(
                        SETTINGS_ICON,
                        TERMINAL_AGENT_BAR_MENU_ICON_SIZE,
                        terminal_agent_bar_icon_color(),
                    ))
                    .child("Configure / Install more"),
            )
            .into_any_element()
    }

    /// Types the skill into the agent's input the way dictation does (`dictation.rs`), with a
    /// trailing space and no Enter, and gives the terminal the keyboard.
    fn type_terminal_agent_bar_skill(
        &mut self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        invocation: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.close_terminal_agent_action_bar_menu(cx);
        let TerminalAgentBarSurface::AgentsPane(pane_id) = surface;
        self.focus_agents_terminal_mount_slot(
            AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            },
            window,
            cx,
        );
        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        let typed = format!("{invocation} ");
        view.update(cx, |view, cx| view.paste_text(&typed, cx));
        cx.notify();
    }

    /// Settings > Integrations, scrolled to Agent skills, where Ghostex's skills are installed.
    pub(crate) fn open_gpui_settings_agent_skills(
        &mut self,
        window: Option<&mut gpui::Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = json!({
            "initialSection": "agentSkills",
            "initialTab": "integrations",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, window, cx);
    }
}

/// The flyout's rows from the skill read: gx-chat-core's Ghostex skill rows, or one status line.
fn terminal_skill_rows(result: Result<Value, String>) -> Vec<Value> {
    let Ok(result) = result else {
        return vec![json!({"label": "Skills could not be loaded."})];
    };
    let skills: Vec<ghostex_gx_chat_core::composer::trigger::Skill> = result
        .get("skills")
        .and_then(|skills| serde_json::from_value(skills.clone()).ok())
        .unwrap_or_default();
    let rows = ghostex_gx_chat_core::composer::ghostex_skills::ghostex_skill_rows(&skills);
    if rows.is_empty() {
        return vec![json!({"label": "No Ghostex skills installed"})];
    }
    rows.into_iter()
        .filter_map(|skill| {
            Some(json!({
                "label": skill.name,
                "description": skill.description,
                "invocation": skill.invocation?,
            }))
        })
        .collect()
}
