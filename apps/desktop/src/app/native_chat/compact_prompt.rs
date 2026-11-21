//! The Ask-on-Enter compact-before-sending card above the chat box.
//!
//! CDXC:SessionChat 2026-11-22 DECISION:
//! User (Q4396, "ask on enter"): when pressing Enter or clicking Send on a long chat whose prompt
//! cache has expired, hold the first Send and show a card above the chat box explaining that
//! compacting first saves usage, with:
//! - Primary: "Compact, then send" (Option + Enter / Alt + Enter)
//! - Secondary: "Send now" (pressing Enter again)
//! - Two "Don't ask again" options: "This chat" (this session only) and "Always" (saves
//!   `offerCompactBeforeSending: false` in Settings).
//!
//! Eligibility and card state live in `packages/gx-chat-core/src/composer/compact_prompt.rs` so
//! desktop, web and phone share the exact rule.

use std::time::Duration;

use super::tone_card::{ToneButton, ToneCardParts, ToneLook, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

/// How long after the card opens a second Enter is ignored as a held-key repeat.
const SECOND_SEND_AFTER: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct CompactPromptGuard {
    /// When the prompt was opened by Enter/Send, to ignore held-Enter repeats.
    opened_at: Option<web_time::Instant>,
}

impl NativeChatView {
    /// Whether the Ask-on-Enter compact-before-sending card is currently open.
    pub(super) fn compact_prompt_open(&self) -> bool {
        self.snapshot["compactBeforeSend"]["open"] == true
    }

    /// Holds a plain Send when the chat qualifies for compact-before-sending and the card is not
    /// open yet; a second Send after `SECOND_SEND_AFTER` (or an explicit button click) goes
    /// through.
    pub(super) fn hold_compact_before_send(
        &mut self,
        mode: &str,
        interrupt: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if mode != "send" || interrupt {
            self.compact_prompt.opened_at = None;
            return false;
        }
        if self.draft.trim_start().starts_with('/') {
            return false;
        }
        if !self.snapshot["compactBeforeSend"].is_object() {
            self.compact_prompt.opened_at = None;
            return false;
        }
        if self.compact_prompt_open() {
            let too_soon = self
                .compact_prompt
                .opened_at
                .is_some_and(|at| at.elapsed() < SECOND_SEND_AFTER);
            if too_soon {
                return true;
            }
            self.compact_prompt.opened_at = None;
            return false;
        }
        self.compact_prompt.opened_at = Some(web_time::Instant::now());
        self.invoke(json!({"type": "compactPromptOpen"}), cx);
        true
    }

    /// Closes an open compact-before-sending card (Escape or the card's X).
    pub(super) fn dismiss_compact_prompt(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.compact_prompt_open() {
            return false;
        }
        self.compact_prompt.opened_at = None;
        self.invoke(json!({"type": "compactPromptClose"}), cx);
        true
    }

    /// Renders the Ask-on-Enter compact-before-sending card above the chat box when open.
    pub(super) fn render_compact_prompt(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let offer = &self.snapshot["compactBeforeSend"];
        if !offer.is_object() || offer["open"] != true {
            return None;
        }
        let s = p.scale;
        let title = {
            let value = text(offer, "title");
            if value.is_empty() {
                "Compact this chat first?".to_string()
            } else {
                value
            }
        };
        let tokens = text(offer, "tokensLabel");
        let idle = text(offer, "idleLabel");
        let subtitle = match (!tokens.is_empty(), !idle.is_empty()) {
            (true, true) => format!("{tokens} tokens · idle {idle}"),
            (true, false) => format!("{tokens} tokens · cache expired"),
            (false, true) => format!("Idle {idle} · cache expired"),
            (false, false) => "Prompt cache expired".to_string(),
        };
        let body_copy = {
            let value = text(offer, "detail");
            if value.is_empty() {
                "This chat is long and its prompt cache has expired. Compacting first shrinks it before sending, which saves usage.".to_string()
            } else {
                value
            }
        };
        let compact_hint = if super::keyboard::chat_mac_keys() {
            "⌥↵"
        } else {
            "Alt+↵"
        };
        let look = ToneLook {
            hue: tone_look("info", p).hue,
            icon: "titlebar/arrows-diagonal-minimize.svg".to_string(),
        };
        let dismiss_option = |id: &'static str,
                              label: &'static str,
                              scope: &'static str,
                              tip: &'static str| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(format!("Don't ask again: {label}"))
                .chat_cursor_pointer()
                .px(px(6.0 * s))
                .py(px(3.0 * s))
                .rounded(px(6.0 * s))
                .text_size(px(12.5 * s))
                .line_height(px(16.0 * s))
                .text_color(p.muted)
                .hover(|style| {
                    style
                        .text_color(p.foreground)
                        .bg(p.foreground.opacity(0.08))
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tip).build(window, cx)
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.compact_prompt.opened_at = None;
                    this.invoke(
                        json!({"type": "compactPromptDismiss", "scope": scope}),
                        cx,
                    );
                }))
                .into_any_element()
        };
        let dont_ask_controls = div()
            .id("compact-prompt-dont-ask")
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(2.0 * s))
            .child(
                div()
                    .pr(px(2.0 * s))
                    .text_size(px(12.5 * s))
                    .line_height(px(16.0 * s))
                    .text_color(p.muted)
                    .child("Don't ask again:"),
            )
            .child(dismiss_option(
                "compact-prompt-dismiss-session",
                "This chat",
                "session",
                "Stop asking in this chat",
            ))
            .child(
                div()
                    .text_size(px(12.0 * s))
                    .text_color(p.muted.opacity(0.5))
                    .child("·"),
            )
            .child(dismiss_option(
                "compact-prompt-dismiss-always",
                "Always",
                "always",
                "Turn off in Settings > Agents > Claude Code",
            ))
            .into_any_element();

        let send_now = self
            .tone_button(
                "compact-prompt-send-now",
                "Send now",
                None,
                ToneButton::Secondary,
                Some("↵".to_string()),
                false,
                p,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.compact_prompt.opened_at = None;
                this.submit("send", window, cx);
            }))
            .into_any_element();

        let compact_then_send = self
            .tone_button(
                "compact-prompt-compact-send",
                "Compact, then send",
                Some("titlebar/arrows-diagonal-minimize.svg"),
                ToneButton::Primary,
                Some(compact_hint.to_string()),
                false,
                p,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.compact_prompt.opened_at = None;
                this.submit("compact", window, cx);
            }))
            .into_any_element();

        let close = self
            .tone_icon_button(
                "compact-prompt-close",
                "titlebar/chat-card-x.svg",
                "Close",
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.dismiss_compact_prompt(cx);
            }))
            .into_any_element();

        Some(self.tone_card(
            ToneCardParts {
                look,
                title,
                detail: subtitle,
                trailing: vec![close],
                body: vec![
                    div()
                        .id("compact-prompt-body")
                        .line_height(px(19.6 * s))
                        .text_color(p.card_muted)
                        .child(body_copy)
                        .into_any_element(),
                ],
                footer_start: vec![dont_ask_controls],
                footer_note: None,
                footer_end: vec![send_now, compact_then_send],
            },
            None,
            None,
            p,
        ))
    }
}
