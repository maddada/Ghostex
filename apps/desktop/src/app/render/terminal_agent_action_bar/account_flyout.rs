// The Switch Account flyout of a Claude or Codex session, in the ⋯ menu of the terminal bar.
//
// CDXC:AgentProviders 2026-10-08 DECISION:
// User: "we need to show the 'Switch Account' option in the menu of the terminal view, not just the
// chat view one". The row appears for the sessions the chat's menu offers accounts for (Claude and
// Codex), and its flyout is gx-core's `sessionAccounts` page, the one the sidebar row's Switch
// Account shows: the same read, the same usage lines and the same `select` pick, run by the store
// (`gx_store/sidebar_accounts.rs`). Other agents keep the daemon's switchable-agent rows.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::svg;
use gpui_component::h_flex;
use serde_json::{Value, json};

use super::palette::*;
use super::{
    TERMINAL_AGENT_BAR_MENU_ICON_SIZE, TERMINAL_AGENT_BAR_SWITCH_ACCOUNT_ICON,
    terminal_agent_bar_icon,
};
use crate::app::model::*;
use crate::*;

pub(super) const ACCOUNT_PAGE_WIDTH: f32 = 280.0;
const CHECK_ICON: &str = "titlebar/check.svg";

impl GhostexGpuiApp {
    /// The session's sidebar id when its flyout lists accounts: the same sessions as the chat's
    /// account panel (Claude and Codex), on this computer.
    pub(super) fn terminal_agent_bar_account_target(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<String> {
        if !matches!(
            self.agents_session_chat_transcript_agent(session_id),
            Some("claude" | "codex")
        ) {
            return None;
        }
        let key = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()))?;
        Some(
            ghostex_gx_core::SessionKey::local(key.project_id, key.session_id)
                .to_sidebar_session_id(),
        )
    }

    /// Asks the store for the session's accounts; the answer lands in
    /// `agents_terminal_action_bar_account_page` through `apply_native_sidebar_menu_page`.
    pub(super) fn open_terminal_agent_bar_account_page(
        &mut self,
        sidebar_session_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_terminal_action_bar_account_page = Some((
            sidebar_session_id.clone(),
            vec![json!({"label": "Loading accounts…", "disabled": true})],
        ));
        let command = json!({
            "type": "sessionAccounts",
            "action": "load",
            "sessionId": sidebar_session_id,
        });
        if !self.gx_store_run_sidebar_accounts(&command, cx) {
            self.agents_terminal_action_bar_account_page = Some((
                sidebar_session_id,
                vec![json!({"label": "Accounts are unavailable right now.", "disabled": true})],
            ));
        }
    }

    pub(super) fn render_terminal_agent_bar_account_page(
        &self,
        rows: &[Value],
        place: &super::flyout_host::FlyoutPlace,
        suffix: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let hover = place.hover();
        let hosted_parent = place.hosted_parent();
        let mut flyout = place.frame(
            format!("ghostex-gpui-terminal-agent-bar-account-page-{suffix}"),
            ACCOUNT_PAGE_WIDTH,
        );
        for (index, row) in rows.iter().enumerate() {
            if row["separator"] == true {
                flyout = flyout.child(place.separator());
                continue;
            }
            let label = row["label"].as_str().unwrap_or_default().to_owned();
            let detail = row["detail"].as_str().map(str::to_owned);
            let checked = row["checked"] == true;
            let command = row
                .get("command")
                .cloned()
                .filter(|_| row["disabled"] != true);
            let keep_open = row["keepOpen"] == true;
            let text_color = if row["disabled"] == true && !checked {
                terminal_agent_bar_disabled_icon_color()
            } else {
                terminal_agent_bar_menu_text_color()
            };
            let item = h_flex()
                .id(format!(
                    "ghostex-gpui-terminal-agent-bar-account-row-{index}-{suffix}"
                ))
                .w_full()
                .items_center()
                .gap(px(9.0))
                .px(px(9.0))
                .py(px(6.0))
                .rounded(px(7.0))
                .cursor_default()
                .text_size(px(13.0))
                .text_color(text_color)
                .when_some(command, |this, command| {
                    this.hover(move |this| this.bg(hover)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            let command = command.clone();
                            super::flyout_host::run_in_terminal_bar_window(
                                this,
                                hosted_parent,
                                window,
                                cx,
                                move |this, window, cx| {
                                    if !keep_open {
                                        this.close_terminal_agent_action_bar_menu(cx);
                                    }
                                    this.handle_native_sidebar_action(
                                        &crate::app::native_sidebar::actions::NativeSidebarAction {
                                            command,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            );
                        }),
                    )
                })
                .child(match row["icon"].as_str() {
                    Some("settings") => terminal_agent_bar_icon(
                        "titlebar/settings.svg",
                        TERMINAL_AGENT_BAR_MENU_ICON_SIZE,
                        terminal_agent_bar_icon_color(),
                    ),
                    _ if row["agentIcon"].is_string() => terminal_agent_bar_icon(
                        TERMINAL_AGENT_BAR_SWITCH_ACCOUNT_ICON,
                        TERMINAL_AGENT_BAR_MENU_ICON_SIZE,
                        terminal_agent_bar_icon_color(),
                    ),
                    _ => div()
                        .flex_shrink_0()
                        .size(px(TERMINAL_AGENT_BAR_MENU_ICON_SIZE))
                        .into_any_element(),
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .when_some(detail, |this, detail| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .text_color(terminal_agent_bar_session_id_color())
                            .child(detail),
                    )
                })
                .when(checked, |this| {
                    this.child(
                        svg()
                            .flex_shrink_0()
                            .size(px(TERMINAL_AGENT_BAR_MENU_ICON_SIZE))
                            .path(CHECK_ICON)
                            .text_color(terminal_agent_bar_icon_color()),
                    )
                });
            flyout = flyout.child(item);
        }
        flyout.into_any_element()
    }
}
