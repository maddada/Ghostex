use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::json;
use std::time::Duration;

impl NativeChatView {
    fn begin_send_hold(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.send_hold_task = None;
        self.send_hold_fired = false;
        if self.draft.trim().is_empty()
            || self.pending_send
            || self.snapshot["queue"]["capabilities"]["canQueue"] != true
        {
            return;
        }
        let Some(delay) = self.snapshot["interaction"]["queueLongPressMs"].as_u64() else {
            return;
        };
        self.send_hold_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.send_hold_fired = true;
                this.submit("queue", window, cx);
            });
        }));
    }

    /// Pressing a blocked Send: the reason is a native toast, never a read-only composer.
    ///
    /// CDXC:SessionChat 2026-09-18 SEE-ALSO:
    /// The request comes from the core (`send_blocked_toast_request` in
    /// packages/gx-chat-core/src/composer/policy.rs), the toast React raised over the app-modal
    /// bridge (`session-chat-send-blocked-toast.tsx`, the user's 2026-09-03 decision).
    pub(super) fn report_send_blocked(&mut self, reason: &str, cx: &mut Context<Self>) {
        let request = self.runtime.as_ref().and_then(|runtime| {
            runtime.query(
                "sendBlockedToast",
                vec![serde_json::Value::String(reason.to_owned())],
                std::time::Duration::from_millis(60),
            )
        });
        if let Some(request) = request.filter(serde_json::Value::is_object) {
            cx.emit(super::state::NativeChatEvent::Host(request));
        }
    }

    fn stop_from_button(&mut self, cx: &mut Context<Self>) {
        if self.stop_cooldown_task.is_some() {
            return;
        }
        let Some(delay) = self.snapshot["interaction"]["stopButtonCooldownMs"].as_u64() else {
            return;
        };
        self.stop_cooldown_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.stop_cooldown_task = None;
                cx.notify();
            });
        }));
        self.invoke(json!({"type":"interrupt"}), cx);
    }

    pub(super) fn render_send_control(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let has_draft = !self.draft.trim().is_empty();
        let stop =
            (self.snapshot["working"] == true || self.stop_cooldown_task.is_some()) && !has_draft;
        let disabled = if stop {
            self.stop_cooldown_task.is_some()
        } else {
            !has_draft
                || !self.composer_ready
                || self.pending_send
                || self.snapshot["pendingAttachments"].as_u64().unwrap_or(0) > 0
        };
        let blocked = !stop && self.snapshot["sendBlockedReason"].is_string();
        // As React's Send label did: the gesture hints appear only while the draft can queue.
        let label: gpui::SharedString = if stop {
            "Stop the agent".into()
        } else if has_draft && self.snapshot["queue"]["capabilities"]["canQueue"] == true {
            format!(
                "Send (hold to queue, {}-click or right-click for Compact & Send)",
                crate::hotkey_label::terminal_overlay_hotkey_chord_label("alt")
            )
            .into()
        } else {
            "Send".into()
        };
        let fill = gpui::rgb(if stop {
            if p.light { 0xf6b5b5 } else { 0x171717 }
        } else if p.light {
            0x7db8fb
        } else {
            0xe5e5e5
        });
        let ink = gpui::rgb(if stop {
            if p.light { 0x7a2929 } else { 0xfcfcfc }
        } else if p.light {
            0xffffff
        } else {
            0x171717
        });
        div()
            .id("chat-send")
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .size(px(24.0 * p.scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * p.scale))
            .bg(fill)
            .when(disabled || blocked, |this| this.opacity(0.5))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
            })
            .when(!disabled, |this| {
                this.chat_cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if !stop {
                                this.begin_send_hold(window, cx);
                            }
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, _| this.send_hold_task = None),
                    )
                    .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                        if !hovered {
                            this.send_hold_task = None;
                        }
                    }))
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            this.send_hold_task = None;
                            if std::mem::take(&mut this.send_hold_fired) {
                                return;
                            }
                            if stop {
                                this.stop_from_button(cx);
                            } else if event.modifiers().alt {
                                // Option-click is Compact & Send, the same as Option+Enter.
                                this.submit("compact", window, cx);
                            } else {
                                this.send(false, window, cx);
                            }
                        }),
                    )
            })
            .when(!stop, |this| {
                this.on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        this.send_hold_task = None;
                        this.show_send_actions(event.position, window, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .child(
                gpui::svg()
                    .path(if stop {
                        "titlebar/player-stop.svg"
                    } else {
                        "titlebar/arrow-up.svg"
                    })
                    .size(px(12.0 * p.scale))
                    .text_color(ink),
            )
            .into_any_element()
    }
}
