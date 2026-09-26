//! The rewind confirmation, as a native child window.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The deleted React chat's session-chat-rewind-dialog.tsx, which this replaced. Rewinding drives the agent's own terminal dialog, so it
//! is not undoable and not cancellable once the daemon starts typing: both buttons are therefore
//! disabled while the call is in flight instead of offering a Cancel that could leave the terminal
//! half-way through its own picker. The wording, the refusal handling, and the "put the prompt back
//! in the composer" rule live in packages/gx-chat-core (the rewind sheet in `transcript/actions.rs`),
//! ported from the deleted packages/shared/session-chat-controller/native-message-actions.ts.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, WindowBounds, WindowOptions, div, px,
};
use gpui_component::Root;
use serde_json::json;

#[derive(Default)]
pub(in crate::app::native_chat) struct RewindWindowState {
    pub(super) handle: Option<gpui::WindowHandle<Root>>,
    opening: bool,
    subscription: Option<Subscription>,
}

pub(super) struct RewindWindow {
    chat: Entity<NativeChatView>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl NativeChatView {
    pub(in crate::app::native_chat) fn sync_rewind_window(&mut self, cx: &mut Context<Self>) {
        if self.pane_hidden || self.snapshot["rewind"].is_null() {
            if let Some(handle) = self.rewind_window.handle.take() {
                let main = self.main_window.filter(|_| !self.pane_hidden);
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    if let Some(main) = main {
                        let _ = main.update(cx, |_, window, _| window.activate_window());
                    }
                });
            }
            return;
        }
        if self.rewind_window.handle.is_some() || self.rewind_window.opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.rewind_window.opening = true;
        let pane = self.bounds.get();
        let parent = self.child_window_parent(cx);
        let chat = cx.entity();
        cx.defer(move |cx| {
            let result = main
                .update(cx, |_, window, cx| {
                    let origin = window.bounds().origin + pane.origin;
                    // Chat Lab's regular macOS titlebar is outside GPUI's content coordinates.
                    #[cfg(target_os = "macos")]
                    let origin = origin
                        + gpui::point(
                            px(0.0),
                            (window.bounds().size.height - window.viewport_size().height)
                                .max(px(0.0)),
                        );
                    (
                        gpui::Bounds::new(origin, pane.size),
                        window.display(cx).map(|display| display.id()),
                    )
                })
                .and_then(|(bounds, display_id)| {
                    let display_id =
                        crate::app::window::popup_frame::display_at(bounds.center(), cx)
                            .or(display_id);
                    cx.open_window(
                        WindowOptions {
                            kind: crate::app::window::popup_frame::child_window_kind(),
                            #[cfg(target_os = "linux")]
                            x11_parent: Some(main),
                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                            display_id,
                            app_id: crate::gpui_platform_window_app_id(),
                            icon: crate::gpui_platform_window_icon(),
                            focus: true,
                            show: true,
                            is_resizable: false,
                            is_minimizable: false,
                            is_movable: false,
                            titlebar: None,
                            window_background: gpui::WindowBackgroundAppearance::Transparent,
                            ..Default::default()
                        },
                        {
                            let chat = chat.clone();
                            move |window, cx| {
                                super::save_markdown::platform::prepare(window);
                                crate::app::window::attach_gpui_app_modal_window_to_main_window(
                                    window, parent,
                                );
                                let view = cx.new(|cx| {
                                    let subscription = cx.observe(&chat, |_, _, cx| cx.notify());
                                    let focus = cx.focus_handle();
                                    super::focus::reclaim_keyboard_focus(window);
                                    focus.focus(window, cx);
                                    RewindWindow {
                                        chat,
                                        focus,
                                        _subscription: subscription,
                                    }
                                });
                                cx.new(|cx| {
                                    Root::new(view, window, cx).bg(gpui::transparent_black())
                                })
                            }
                        },
                    )
                });
            chat.update(cx, |chat, cx| {
                chat.rewind_window.opening = false;
                match result {
                    Ok(handle) => {
                        chat.rewind_window.handle = Some(handle);
                        let weak = cx.weak_entity();
                        chat.rewind_window.subscription =
                            Some(cx.on_window_closed(move |cx, id| {
                                let _ = weak.update(cx, |chat, cx| {
                                    if chat
                                        .rewind_window
                                        .handle
                                        .is_some_and(|handle| handle.window_id() == id)
                                    {
                                        chat.rewind_window.handle = None;
                                        chat.invoke(json!({"type":"rewindCancel"}), cx);
                                    }
                                });
                            }));
                        chat.sync_rewind_window(cx);
                    }
                    Err(error) => {
                        chat.error = Some(error.to_string());
                        chat.invoke(json!({"type":"rewindCancel"}), cx);
                    }
                }
            });
        });
    }
}

impl RewindWindow {
    fn button(
        &self,
        id: &'static str,
        label: String,
        command: &'static str,
        disabled: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .tab_index(0)
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.0))
            .border_1()
            .border_color(p.border)
            .font_weight(gpui::FontWeight::MEDIUM)
            .when(disabled, |item| item.opacity(0.5).tab_stop(false))
            .when(!disabled, |item| {
                item.chat_cursor_pointer().hover(|style| style.bg(p.input))
            })
            .focus_visible(|style| style.border_color(p.ring))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                if !disabled {
                    this.chat
                        .update(cx, |chat, cx| chat.invoke(json!({"type":command}), cx));
                }
            }))
            .into_any_element()
    }
}

impl Render for RewindWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.chat.read(cx).snapshot.clone();
        let p = ChatAppearance::current(&snapshot);
        let state = &snapshot["rewind"];
        let busy = state["busy"] == true;
        let completed = state["completed"] == true;
        let destructive = gpui::rgb(if p.light { 0xe7000b } else { 0xff6467 });
        let preview = text(state, "preview");
        let error = text(state, "error");
        let card = div()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .w(px(448.0))
            .max_w(gpui::relative(0.92))
            .p(px(24.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_background)
            .text_color(p.card_muted)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .child("Rewind conversation"),
                    )
                    .child(div().line_height(px(20.0)).child(
                        "Restore the conversation to the point before you sent this message?",
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .when(!preview.is_empty(), |column| {
                        column.child(
                            div()
                                .min_w_0()
                                .border_l_2()
                                .border_color(p.border)
                                .pl(px(12.0))
                                .line_height(px(20.0))
                                .text_color(p.muted)
                                .child(preview),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(p.muted)
                            .child(text(state, "description")),
                    )
                    .when(!error.is_empty(), |column| {
                        column.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(destructive)
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.button(
                        "rewind-cancel",
                        text(state, "cancelLabel"),
                        "rewindCancel",
                        busy,
                        &p,
                        cx,
                    ))
                    .child(self.button(
                        "rewind-submit",
                        text(state, "submitLabel"),
                        "rewindSubmit",
                        busy || completed,
                        &p,
                        cx,
                    )),
            );
        div()
            .size_full()
            .track_focus(&self.focus)
            .font_family(p.font.clone())
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::Hsla::from(gpui::rgb(0x000000)).opacity(0.45))
            .text_size(px(14.0))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let command = match event.keystroke.key.as_str() {
                    "escape" => "rewindCancel",
                    "enter" => "rewindSubmit",
                    _ => return,
                };
                this.chat
                    .update(cx, |chat, cx| chat.invoke(json!({"type":command}), cx));
                cx.stop_propagation();
                window.prevent_default();
            }))
            .child(card)
    }
}
