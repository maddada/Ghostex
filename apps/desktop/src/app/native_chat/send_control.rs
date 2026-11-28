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

    /// Visual-only Send/Stop button matching `render_send_control`, used by the non-interactive
    /// compact-toolbar cross-fade overlay during composer collapse/expand transitions.
    pub(super) fn render_send_control_visual(&self, p: &ChatAppearance) -> AnyElement {
        let has_draft = !self.draft.trim().is_empty()
            || self.attachments_on_their_way()
            || self.draft_coordinator_mode;
        let stop = (self.snapshot["working"] == true || self.stop_cooldown_task.is_some())
            && !has_draft
            && !self.collab_guest();
        let disabled = if stop {
            self.stop_cooldown_task.is_some()
        } else {
            !has_draft || !self.composer_ready
        };
        let blocked = !stop && self.snapshot["sendBlockedReason"].is_string();
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
        div()
            .size(px(24.0 * p.scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * p.scale))
            .bg(fill)
            .when(clear_stop, |this| this.border_1().border_color(p.border))
            .when(disabled || blocked, |this| this.opacity(0.5))
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

    pub(super) fn render_send_control(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        // A picture still being saved can be sent on its own: it goes the moment it is in.
        let has_draft = !self.draft.trim().is_empty()
            || self.attachments_on_their_way()
            || self.draft_coordinator_mode;
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
        // With words to send, pointing at Send (or right-clicking it) opens its popup, which names
        // every way to send and their keys (send_popup.rs); otherwise a plain tooltip names Stop,
        // or Send and Enter.
        // CDXC:SessionChat 2026-11-28 SEE-ALSO: the popup replaced this tooltip's gesture rows
        // (Queue, Compact & Send, Interrupt and send now); the user's words are in send_popup.rs.
        // The 2026-11-27 decision to name Tab for Queue, not Hold, is kept there.
        let popup_ready = !stop && has_draft;
        let chord = super::keyboard::chat_chord_label;
        let (label, keys): (gpui::SharedString, String) = if stop {
            ("Stop the agent".into(), String::new())
        } else {
            ("Send".into(), chord("enter"))
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
        let button = div()
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
            .when(!popup_ready, |this| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone())
                        .keys(keys.clone())
                        .build(window, cx)
                })
            })
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if !hovered {
                    this.send_hold_task = None;
                }
                this.send_button_hovered(*hovered && popup_ready, window, cx);
            }))
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
                    cx.listener(move |this, _: &gpui::MouseDownEvent, _, cx| {
                        this.send_hold_task = None;
                        this.toggle_send_popup_pinned(cx);
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
            .when(!stop && self.send_popup_open(), |this| {
                this.child(self.render_send_popup(p, cx))
            })
            // Where Send was painted, which its tip lines up with (send_tip.rs).
            .child(
                gpui::canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset(px(0.0)),
            )
            .into_any_element();
        // The one-time tip's dot sits on Send's corner, never on Stop; an open tip stays open
        // when Send turns into Stop under it (send_tip.rs).
        let show_dot = !stop && super::send_tip::send_tip_pending();
        if !show_dot && !self.send_tip_open {
            return button;
        }
        self.wrap_send_with_tip(button, show_dot, p, cx)
    }
}
