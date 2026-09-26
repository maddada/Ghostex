use super::keyboard::ComposerInputActions as _;
use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{
    AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Render, Styled as _, Subscription, Window, WindowBounds,
    WindowOptions, div, px,
};
use gpui_component::Root;

/// CDXC:SessionChat 2026-09-16 DECISION:
/// User: never show the working indicator, task list, or subagents above the maximized composer. This supersedes the maximized status placement from 2026-09-12; otherwise the working indicator stays above all component cards. `render_composer` leaves the working strip, the agent tasks and the agent fleet out while `maximized_window` is set.
struct MaximizedComposer {
    chat: Entity<NativeChatView>,
    _subscription: Subscription,
}

impl Render for MaximizedComposer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.chat.update(cx, |chat, cx| {
            chat.ensure_input(window, cx);
            /*
            CDXC:SessionChat 2026-09-26 DECISION:
            User, of the maximized composer under window glass (an opaque theme-coloured box on a near-black backdrop): "please make it look better (should match the look of the composer in the gpui app please". This window is transparent and sits exactly over the chat pane, which paints nothing but its glass while the composer is maximized (render.rs), so the composer takes the pane's glass appearance and is the same frosted card as the inline one. The scrim is only a faint dim under glass: the 55% black it keeps off glass turned the empty glass pane into a dark slab, and there is no conversation behind it left to hold back.
            */
            let glass = crate::app::helpers::window_glass_active_for(chat.main_window);
            let p = ChatAppearance::current(&chat.snapshot).on_window_glass(glass);
            let scrim = match (glass, p.light) {
                (false, _) => gpui::black().opacity(0.55),
                (true, false) => gpui::black().opacity(0.16),
                (true, true) => gpui::black().opacity(0.05),
            };
            let composer = chat.render_composer(&p, window, cx);
            let available = window.viewport_size();
            let width = (available.width.as_f32() - 48.0 * p.scale)
                .min(896.0 * p.scale)
                .max(1.0);
            let height = (available.height.as_f32() - 48.0 * p.scale)
                .min(720.0 * p.scale)
                .max(1.0);
            div()
                .size_full()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(scrim)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|chat, _, _, cx| chat.close_maximized(cx)),
                )
                .font_family(p.font)
                .text_color(p.primary)
                .text_size(px(14.0 * p.scale))
                .line_height(px(24.0 * p.scale))
                .on_action(cx.listener(NativeChatView::handle_action))
                // The maximized composer scales with the pane, so it answers the zoom chords too.
                .capture_action(cx.listener(NativeChatView::chat_zoom_in_action))
                .capture_action(cx.listener(NativeChatView::chat_zoom_out_action))
                .capture_action(cx.listener(NativeChatView::chat_zoom_reset_action))
                .capture_key_down(cx.listener(NativeChatView::composer_key_down))
                .composer_input_actions(cx)
                .capture_key_up(cx.listener(|chat, _, _, _| chat.composer_held_key = None))
                .capture_action(cx.listener(NativeChatView::paste_attachments))
                .capture_action(cx.listener(NativeChatView::composer_copy))
                .capture_action(cx.listener(NativeChatView::composer_cut))
                .on_drop(cx.listener(|chat, paths: &gpui::ExternalPaths, _, cx| {
                    let paths = paths
                        .0
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    chat.invoke(serde_json::json!({"type":"attachPaths","paths":paths}), cx);
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .w(px(width))
                        .h(px(height))
                        .min_h_0()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(composer),
                )
        })
    }
}

impl NativeChatView {
    pub(crate) fn toggle_maximized(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.maximized_wanted {
            self.close_maximized(cx);
            return;
        }
        self.maximized_wanted = true;
        self.invoke(
            serde_json::json!({"type":"composerExpand","editor":true}),
            cx,
        );
        self.sync_maximized_window(cx);
    }

    /// CDXC:SessionChat 2026-09-24 DECISION:
    /// User: the chat's modals must not stay on screen over the session shown next, and they come back sized to the pane when the user returns to their own session. Maximize is therefore a per-session state: its window closes with the pane and reopens on the pane's current frame.
    pub(in crate::app::native_chat) fn sync_maximized_window(&mut self, cx: &mut Context<Self>) {
        if !self.maximized_wanted || self.pane_hidden {
            self.take_down_maximized_window(!self.pane_hidden, cx);
            return;
        }
        if self.maximized_window.is_some() || self.maximized_opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.maximized_opening = true;
        let pane = self.bounds.get();
        let parent_native_view = self.config.parent_native_view;
        let chat = cx.entity();
        #[cfg(target_os = "linux")]
        let owner = main;
        // WHY: the Maximize click arrives while the main window is mid-update, so reading its
        // frame has to wait for the deferred pass; updating it re-entrantly fails and nothing opens.
        cx.defer(move |cx| {
            let Ok((bounds, display_id)) = main.update(cx, |_, window, cx| {
                (
                    gpui::Bounds::new(
                        super::child_window::content_bounds(window).origin + pane.origin,
                        pane.size,
                    ),
                    window.display(cx).map(|display| display.id()),
                )
            }) else {
                chat.update(cx, |this, _| this.maximized_opening = false);
                return;
            };
            let display_id =
                crate::app::window::popup_frame::display_at(bounds.center(), cx).or(display_id);
            let result = cx.open_window(
                WindowOptions {
                    kind: crate::app::window::popup_frame::child_window_kind(),
                    #[cfg(target_os = "linux")]
                    x11_parent: Some(owner),
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
                        crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                        crate::app::window::attach_gpui_app_modal_window_to_main_window(
                            window,
                            parent_native_view,
                        );
                        let view = cx.new(|cx| {
                            let subscription = cx.observe(&chat, |_, _, cx| cx.notify());
                            MaximizedComposer {
                                chat,
                                _subscription: subscription,
                            }
                        });
                        cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
                    }
                },
            );
            chat.update(cx, |this, cx| {
                this.maximized_opening = false;
                match result {
                    // The pane left the screen while the window was opening.
                    Ok(handle) if !this.maximized_wanted || this.pane_hidden => {
                        cx.defer(move |cx| {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        });
                    }
                    Ok(handle) => {
                        this.maximized_window = Some(handle);
                        let chat = cx.weak_entity();
                        this.window_subscription = Some(cx.on_window_closed(move |cx, id| {
                            let _ = chat.update(cx, |chat, cx| {
                                if chat
                                    .maximized_window
                                    .is_some_and(|handle| handle.window_id() == id)
                                {
                                    chat.close_maximized(cx);
                                }
                            });
                        }));
                        this.focus_requested = true;
                        cx.notify();
                    }
                    Err(error) => {
                        this.error = Some(error.to_string());
                        cx.notify();
                    }
                }
            });
        });
    }

    pub(crate) fn close_maximized(&mut self, cx: &mut Context<Self>) {
        self.maximized_wanted = false;
        self.take_down_maximized_window(true, cx);
    }

    /// Removes the window itself. The composer takes the keyboard back only when the pane is still
    /// on screen; a pane that is going away would be taking focus from the session shown next.
    fn take_down_maximized_window(&mut self, focus_composer: bool, cx: &mut Context<Self>) {
        let Some(handle) = self.maximized_window.take() else {
            return;
        };
        let main_window = self.main_window.filter(|_| focus_composer);
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            if let Some(main_window) = main_window {
                let _ = main_window.update(cx, |_, window, cx| {
                    window.activate_window();
                    let _ = chat.update(cx, |chat, cx| {
                        chat.ensure_input(window, cx);
                        if let Some(input) = &chat.input {
                            input.read(cx).focus_handle(cx).focus(window, cx);
                        }
                        cx.notify();
                    });
                });
            }
        });
        cx.notify();
    }
}
