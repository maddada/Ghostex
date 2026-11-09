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
        // A picture still being saved can be sent on its own: it goes the moment it is in.
        let has_draft = !self.draft.trim().is_empty() || self.attachments_on_their_way();
        // A Ghostex Collab guest never stops the owner's agent (gxserver refuses it too).
        let stop = (self.snapshot["working"] == true || self.stop_cooldown_task.is_some())
            && !has_draft
            && !self.collab_guest();
        let disabled = if stop {
            self.stop_cooldown_task.is_some()
        } else {
            !has_draft || !self.composer_ready
        };
        let blocked = !stop && self.snapshot["sendBlockedReason"].is_string();
        // As React's Send label did: the gesture hints appear only while the draft can queue.
        // The tooltip is a mini menu: one row per gesture, its keys on the right (CDXC:Tooltips 2026-10-15).
        use gpui_component::tooltip::TooltipRow;
        let chord = super::keyboard::chat_chord_label;
        let (label, keys, rows): (gpui::SharedString, String, Vec<TooltipRow>) = if stop {
            ("Stop the agent".into(), String::new(), Vec::new())
        } else if has_draft && self.snapshot["working"] == true {
            (
                "Send".into(),
                chord("enter"),
                vec![
                    TooltipRow::new("Interrupt and send now").keys(chord("ctrl+enter")),
                    TooltipRow::new("More options").hint("Right-click"),
                ],
            )
        } else if has_draft && self.snapshot["queue"]["capabilities"]["canQueue"] == true {
            (
                "Send".into(),
                chord("enter"),
                vec![
                    TooltipRow::new("Queue").hint("Hold"),
                    TooltipRow::new("Compact & Send")
                        .hint(format!("{}-click or right-click", chord("alt"))),
                ],
            )
        } else {
            ("Send".into(), chord("enter"), Vec::new())
        };
        // Dark Stop is a hairline-outlined, transparent button like its toolbar neighbours: the old
        // solid #171717 square read as a black hole in the frosted composer.
        let clear_stop = stop && !p.light;
        let fill: gpui::Hsla = if clear_stop {
            gpui::transparent_black()
        } else {
            gpui::rgb(if stop {
                0xf6b5b5
            } else if p.light {
                0x7db8fb
            } else {
                0xe5e5e5
            })
            .into()
        };
        let ink = gpui::rgb(if stop {
            if p.light { 0x7a2929 } else { 0xfcfcfc }
        } else if p.light {
            0xffffff
        } else {
            0x171717
        });
        let measured = self.composer_button_bounds.send.clone();
        div()
            .id("chat-send")
            .relative()
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .size(px(24.0 * p.scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * p.scale))
            .bg(fill)
            .when(clear_stop, |this| {
                let hover = p.border;
                this.border_1()
                    .border_color(p.border)
                    .hover(move |style| style.bg(hover))
            })
            .when(disabled || blocked, |this| this.opacity(0.5))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label.clone())
                    .keys(keys.clone())
                    .rows(rows.clone())
                    .build(window, cx)
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
            // Send's right-click menu lines up with this button's right edge.
            .child(
                gpui::canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset(px(0.0)),
            )
            .into_any_element()
    }
}
