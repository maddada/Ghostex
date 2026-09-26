use super::{state::NativeChatView, transcript::text};
use gpui::{Context, Window};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NativeChatAction {
    pub(crate) command: Value,
}

/// Toggle keys in `menu_toggle.rs` for the menus this file opens.
pub(super) const MORE_ACTIONS_TRIGGER: &str = "chat-more-actions";
const SEND_ACTIONS_TRIGGER: &str = "chat-send-actions";
const SWITCH_ACCOUNT_TRIGGER: &str = "chat-notice-switch-account";

/// More actions row icons, ported from `HOST_ACTION_ICONS` and the Sleep moon in the React chat's
/// `session-chat-composer-actions.tsx`.
fn host_action_icon(id: &str) -> Option<&'static str> {
    Some(match id {
        "splitSessionRight" => "titlebar/layout-columns.svg",
        "closeAfterDone" => "titlebar/clock.svg",
        "delayedActions" => "titlebar/clock-check.svg",
        "exportTranscript" => "titlebar/file-export.svg",
        "fork" => "titlebar/git-branch.svg",
        "fullReload" => "titlebar/refresh.svg",
        "rename" => "titlebar/pencil.svg",
        "sleep" => "titlebar/moon.svg",
        "switchAccount" => "titlebar/switch-horizontal.svg",
        _ => return None,
    })
}

impl NativeChatView {
    /// Send's own right-click menu.
    ///
    /// CDXC:SessionChat 2026-09-18 WHY:
    /// It is the chat's own popup, not the app shell's native menu, because
    /// React drew it as one more `ghostex-session-chat-popup`
    /// (`session-chat-composer.tsx`).
    pub(crate) fn show_send_actions(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_menu_toggled_shut(SEND_ACTIONS_TRIGGER, cx) {
            return;
        }
        let enabled = self.composer_ready && !self.pending_send && !self.draft.trim().is_empty();
        let can_queue = enabled && self.snapshot["queue"]["capabilities"]["canQueue"] == true;
        let rows = vec![json!({
            "label":"Compact & Send",
            "detail":crate::hotkey_label::terminal_overlay_hotkey_chord_label("alt+enter"),
            "disabled":!can_queue,
            "command":{"type":"submit","mode":"compact"},
        })];
        self.show_chat_menu_at(rows, position, 240.0, window, cx);
    }
    pub(crate) fn show_actions(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_menu_toggled_shut(MORE_ACTIONS_TRIGGER, cx) {
            return;
        }
        let appearance = super::appearance::ChatAppearance::current(&self.snapshot);
        let mut rows = Vec::new();
        if self.snapshot["composerOverflow"]["optionsOverflowed"] == true {
            rows.push(json!({"heading":true,"label":"Model settings"}));
            if self.snapshot["optionLabels"]["showOptions"] == true
                && !self.snapshot["modelMenu"].is_object()
            {
                rows.push(json!({
                "label": self.snapshot["optionLabels"]["optionsTitle"].as_str().unwrap_or("Options"),
                "detail":self.snapshot["optionLabels"]["options"],
                "disabled":self.snapshot["optionLabels"]["options"].as_str().is_none_or(str::is_empty),
                "openOnHover":false,
                "children":self.snapshot["optionMenus"]["options"],
            }));
            }
            if self.snapshot["contextMeter"].is_object() {
                rows.push(json!({"label":"Context window","detail":self.snapshot["contextMeter"]["percentage"],"openOnHover":false,"children":self.context_menu_rows()}));
            }
            rows.push(json!({"separator":true}));
        }
        rows.push(json!({"heading":true,"label":"Chat"}));
        let verbose_icon = if appearance.verbose {
            "titlebar/eye-filled.svg"
        } else {
            "titlebar/eye-off.svg"
        };
        rows.push(json!({"label":"Verbose mode","iconPath":verbose_icon,"checked":appearance.verbose,"command":{"type":"setVerbose","enabled":!appearance.verbose}}));
        rows.push(json!({"label":"Simple mode","iconPath":"titlebar/leaf.svg","checked":appearance.simple,"command":{"type":"host","action":"setSimpleMode","enabled":!appearance.simple}}));
        if self.composer_control_overflowed("summary") {
            let summary = self.snapshot["summaryMode"] == true;
            let summary_icon = if summary {
                "titlebar/list-check.svg"
            } else {
                "titlebar/list-details.svg"
            };
            rows.push(json!({"label":"Summary mode","iconPath":summary_icon,"checked":summary,"command":{"type":"toggleSummary"}}));
        }
        let host_row = |action: &Value| {
            json!({
                "label":action["label"],"hotkeyAction":action["hotkey"],
                "iconPath":host_action_icon(action["id"].as_str().unwrap_or_default()),
                "command":{"type":"host","action":action["id"]},
            })
        };
        let actions: Vec<_> = self.snapshot["hostActions"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        for action in actions.iter().filter(|action| {
            matches!(
                text(action, "id").as_str(),
                "delayedActions" | "closeAfterDone" | "splitSessionRight"
            )
        }) {
            rows.push(host_row(action));
        }
        for (id, action, label, icon) in super::toolbar::COMPOSER_CONTROLS {
            // An overflowed control folds into this menu only when the host gave it a handler.
            if id != "summary"
                && self.composer_control_overflowed(id)
                && self.composer_control_available(id)
            {
                let (label, icon) = if id == "maximize" && self.maximized_window.is_some() {
                    ("Exit maximize", "titlebar/minimize.svg")
                } else {
                    (label, icon)
                };
                let mut row = json!({"label":label,"iconPath":icon,"command":{"type":"composerHost","action":action}});
                if id == "note" {
                    row["checked"] = self.snapshot["note"]["open"].clone();
                }
                rows.push(row);
            }
        }
        let agent_actions: Vec<_> = actions
            .iter()
            .filter(|action| action["group"] == "agent")
            .collect();
        if !agent_actions.is_empty() {
            rows.push(json!({"separator":true}));
            rows.push(json!({"heading":true,"label":"Agent"}));
            for action in agent_actions {
                if action["id"] == "switchAccount" {
                    /*
                    CDXC:AgentProviders 2026-09-18 DECISION:
                    User: Switch Account in GPUI chat works like React chat. Claude and Codex
                    sessions open the Accounts & limits panel (SessionAccountsPanel); other
                    agents keep the daemon's switchable-agent rows. Either submenu opens on
                    click only, per the 2026-09-12 decision below.
                    */
                    // CDXC:AgentProviders 2026-09-06 DECISION: User requested the Claude and Codex account controls under More actions > Switch Account as a submenu, replacing the standalone composer Accounts button and the old account row.
                    // CDXC:AgentProviders 2026-09-12 DECISION: User: opening Switch Account under the chat's More actions requires a click; hovering must not open either version of the account submenu.
                    if self.snapshot["accountPanel"].is_object() {
                        rows.push(json!({"label":"Switch Account","iconPath":host_action_icon("switchAccount"),"openOnHover":false,"children":[{"accounts":self.snapshot["accountPanel"]}]}));
                        continue;
                    }
                    let accounts: Vec<_> = self.snapshot["switchableAgents"].as_array().into_iter().flatten().map(|account| json!({
                        "label":account["name"],"icon":account["icon"],
                        "command":{"type":"host","action":"switchAccount","agentId":account["agentId"]},
                    })).collect();
                    if !accounts.is_empty() {
                        rows.push(json!({"label":"Switch Account","iconPath":host_action_icon("switchAccount"),"openOnHover":false,"children":accounts}));
                    }
                } else {
                    rows.push(host_row(action));
                }
            }
        }
        let other: Vec<_> = actions
            .iter()
            .filter(|action| {
                action["group"] != "agent"
                    && !matches!(
                        text(action, "id").as_str(),
                        "delayedActions" | "closeAfterDone" | "splitSessionRight"
                    )
            })
            .collect();
        if !other.is_empty() {
            rows.push(json!({"separator":true}));
            rows.extend(other.into_iter().map(|action| host_row(action)));
        }
        self.show_chat_menu(
            rows,
            gpui::Bounds::new(position, gpui::size(gpui::px(0.0), gpui::px(0.0))),
            240.0,
            window,
            cx,
        );
    }

    /// The notice card's Switch account button: the same panel as More actions > Switch Account.
    pub(crate) fn show_account_panel(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot["accountPanel"].is_object() {
            if self.chat_menu_toggled_shut(SWITCH_ACCOUNT_TRIGGER, cx) {
                return;
            }
            self.show_chat_menu(
                vec![json!({"accounts":self.snapshot["accountPanel"]})],
                gpui::Bounds::new(position, gpui::size(gpui::px(0.0), gpui::px(0.0))),
                super::option_menu::ACCOUNT_PANEL_WIDTH,
                window,
                cx,
            );
        }
    }

    pub(crate) fn handle_action(
        &mut self,
        action: &NativeChatAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.command["type"] == "submit" {
            self.submit(
                action.command["mode"].as_str().unwrap_or("send"),
                window,
                cx,
            );
        } else if action.command["type"] == "composerHost" {
            self.perform_composer_action(
                action.command["action"].as_str().unwrap_or_default(),
                window.mouse_position(),
                window,
                cx,
            );
        } else if action.command["type"] == "openAccountsSettings" {
            cx.emit(super::state::NativeChatEvent::Host(
                json!({"type":"open","modal":"settings","initialTab":"accounts"}),
            ));
        } else if action.command["type"] == "copyText" {
            // Copy Path, Copy URL and the transcript's Copy, with the feedback every other chat copy gives.
            if let Some(text) = action.command["text"].as_str() {
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(text.to_owned()),
                    cx,
                );
            }
        } else if action.command["type"] == "appendToDraft" {
            self.append_to_draft(action.command["text"].as_str().unwrap_or_default(), cx);
        } else if action.command["type"] == "host" {
            self.host(
                action.command["action"].as_str().unwrap_or_default(),
                action.command.clone(),
                cx,
            );
        } else {
            self.invoke(action.command.clone(), cx);
        }
    }
}
