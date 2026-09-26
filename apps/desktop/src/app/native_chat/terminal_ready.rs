//! The composer footer's Terminal View button doubles as the agent CLI's
//! readiness light. It comes from the session-scoped terminal-tail read the
//! `composerNotReady` card uses; the read happens only on hover or focus, never
//! on a background timer.
//!
//! Only a measured verdict tints the glyph: `unknown`, an unreadable screen and
//! the time before the first hover keep the inherited footer color, because the
//! daemon fails open on `unknown` and a red button would accuse a session that
//! sends fine. Colors match the React chat's
//! `.ghostex-chat-footer-control[data-terminal-ready]` (packages/core-ui/styles/chat.css until
//! 2026-09-25).

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-24 DECISION: The user asked for the Terminal View button's tooltip to say only "Switch to Terminal"; the old "Agent CLI Preview" tooltip with the readiness reason and screen tail is commented out rather than deleted.
    pub(super) fn render_terminal_view_button(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tail = &self.snapshot["terminalTail"];
        let tint = match tail["readiness"].as_str() {
            Some("ready") => Some(gpui::rgb(0xa6e3b1)),
            Some("notReady") => Some(gpui::rgb(0xf0a3a3)),
            _ => None,
        };
        // let preview = text(tail, "preview");
        // let reason = text(tail, "reason");
        // let tooltip = [
        //     Some("Agent CLI Preview".to_string()),
        //     Some("Click to Switch to Terminal View".to_string()),
        //     Some(reason).filter(|reason| !reason.is_empty()),
        //     Some(preview).filter(|preview| !preview.is_empty()),
        // ]
        // .into_iter()
        // .flatten()
        // .collect::<Vec<_>>()
        // .join("\n\n");
        div()
            .id("terminalView")
            .role(gpui::Role::Button)
            .aria_label("Terminal View")
            .chat_cursor_pointer()
            .size(px(28.0 * p.scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .hover(|style| style.bg(p.border))
            .child(
                gpui::svg()
                    .path("titlebar/terminal-2.svg")
                    .size(px(16.0 * p.scale))
                    .text_color(tint.map_or(p.primary, |color| color.into())),
            )
            // .tooltip(move |window, cx| {
            //     gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            // })
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Switch to Terminal").build(window, cx)
            })
            // Hovering still reads the terminal tail so the glyph's readiness tint stays current.
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    this.invoke(json!({"type":"terminalTailHover"}), cx);
                }
            }))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.perform_composer_action("terminalView", event.position(), window, cx)
                }),
            )
            .into_any_element()
    }
}
