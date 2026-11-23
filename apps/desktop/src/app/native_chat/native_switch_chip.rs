//! The "Switch to Native" pill chip in the chat box's pill strip (`Q4768`, `Q4770`, `Q4771`).
//!
//! Shown beside the Where pill on Terminal chats of Antigravity, Claude Code, and Codex for
//! chat-first users until dismissed per agent or Native is enabled for that agent. Clicking mid-reply
//! queues the switch and shifts the chip to a cancelable "Switching when done…" state.
//!
//! CDXC:AgentProviders 2026-11-24 SEE-ALSO:
//! packages/gx-chat-core/src/extras/native_switch.rs (visibility rules),
//! apps/mobile/app/src/chat/native/cards/NativeSwitchPill.tsx (phone counterpart).

use super::{appearance::ChatAppearance, cursor::ChatCursor as _, state::NativeChatView};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

impl NativeChatView {
    pub(super) fn native_switch_offer(&self) -> Option<&Value> {
        let offer = &self.snapshot["nativeSwitch"];
        offer.is_object().then_some(offer)
    }

    pub(super) fn native_switch_chip_shown(&self) -> bool {
        self.native_switch_offer()
            .is_some_and(|offer| offer["showChip"] == true)
    }

    /// Renders the "Switch to Native" (or queued "Switching when done…") pill chip for the strip.
    pub(super) fn native_switch_chip(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let offer = self.native_switch_offer()?;
        if offer["showChip"] != true {
            return None;
        }
        let s = p.scale;
        let family = offer["family"].as_str().unwrap_or_default().to_string();
        let agent_name = offer["agentName"]
            .as_str()
            .unwrap_or("this agent")
            .to_string();
        let queued = offer["queued"] == true;
        let native_default = offer["nativeDefault"] == true;
        let working = self.snapshot["working"] == true;

        let (icon_path, label, tooltip_text, aria_label) = if queued {
            (
                "titlebar/clock.svg",
                "Switching when done…".to_string(),
                format!(
                    "Will reopen {agent_name} in Native mode as soon as this reply finishes. Click × to cancel."
                ),
                format!("Switching {agent_name} to Native when reply finishes"),
            )
        } else {
            (
                "titlebar/bolt.svg",
                "Switch to Native".to_string(),
                format!(
                    "Run chat straight through {agent_name} for faster replies and live approvals."
                ),
                format!("Switch {agent_name} to Native mode"),
            )
        };

        let click_family = family.clone();
        let click_agent_name = agent_name.clone();
        let dismiss_family = family.clone();
        let dismiss_aria = if queued {
            "Cancel Switch to Native".to_string()
        } else {
            format!("Hide Switch to Native for {agent_name}")
        };

        Some(
            div()
                .id("chat-native-switch-chip")
                .role(gpui::Role::Button)
                .aria_label(aria_label)
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(super::chat_pills::PILL_HEIGHT * s))
                .gap(px(5.0 * s))
                .pl(px(8.0 * s))
                .pr(px(5.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(p.foreground.opacity(0.16))
                .bg(p.foreground.opacity(0.08))
                .text_size(px(12.5 * s))
                .line_height(px(19.0 * s))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .text_color(p.foreground)
                .chat_cursor_pointer()
                .hover(|style| {
                    style
                        .bg(p.foreground.opacity(0.13))
                        .border_color(p.foreground.opacity(0.24))
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip_text.clone()).build(window, cx)
                })
                .child(
                    svg()
                        .path(icon_path)
                        .size(px(13.0 * s))
                        .flex_shrink_0()
                        .text_color(p.foreground.opacity(0.9)),
                )
                .child(label)
                .child(
                    div()
                        .id("chat-native-switch-chip-dismiss")
                        .role(gpui::Role::Button)
                        .aria_label(dismiss_aria)
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(18.0 * s))
                        .rounded_full()
                        .text_color(p.foreground.opacity(0.65))
                        .hover(|style| {
                            style
                                .bg(p.foreground.opacity(0.14))
                                .text_color(p.foreground)
                        })
                        .child(
                            svg()
                                .path("titlebar/x.svg")
                                .size(px(11.0 * s))
                                .text_color(p.foreground.opacity(0.65)),
                        )
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if queued {
                                this.host("cancelSwitchToNative", json!({}), cx);
                            } else {
                                this.invoke(
                                    json!({
                                        "type": "dismissNativeSwitchOffer",
                                        "family": dismiss_family,
                                    }),
                                    cx,
                                );
                                this.host(
                                    "dismissNativeSwitchOffer",
                                    json!({ "family": dismiss_family }),
                                    cx,
                                );
                            }
                        })),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if queued {
                        this.host("cancelSwitchToNative", json!({}), cx);
                    } else {
                        this.host(
                            "switchToNative",
                            json!({
                                "family": click_family,
                                "agentName": click_agent_name,
                                "nativeDefault": native_default,
                                "working": working,
                            }),
                            cx,
                        );
                    }
                }))
                .into_any_element(),
        )
    }
}
