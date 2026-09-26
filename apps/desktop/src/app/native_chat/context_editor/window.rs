use super::super::state::NativeChatView;
use gpui::{
    AppContext as _, Context, Entity, Styled as _, Subscription, WindowBounds, WindowOptions, px,
};
use gpui_component::{
    Root,
    input::{InputEvent, InputState},
};
use serde_json::json;

#[derive(Default)]
pub(in crate::app::native_chat) struct ContextEditorWindowState {
    pub(in crate::app::native_chat) handle: Option<gpui::WindowHandle<Root>>,
    opening: bool,
    subscription: Option<Subscription>,
}

pub(super) struct ContextEditorWindow {
    pub(super) chat: Entity<NativeChatView>,
    pub(super) filter: Entity<InputState>,
    pub(super) scroll: gpui::ScrollHandle,
    _subscription: Subscription,
    _input_subscription: Subscription,
}

impl NativeChatView {
    pub(in crate::app::native_chat) fn sync_context_editor_window(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.pane_hidden || self.snapshot["contextEditor"].is_null() {
            if let Some(handle) = self.context_editor_window.handle.take() {
                let main = self.main_window.filter(|_| !self.pane_hidden);
                let chat = cx.weak_entity();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    if let Some(main) = main {
                        let _ = main.update(cx, |_, window, cx| {
                            window.activate_window();
                            let _ = chat.update(cx, |chat, cx| {
                                chat.focus_requested = true;
                                chat.ensure_input(window, cx);
                            });
                        });
                    }
                });
            }
            return;
        }
        if self.context_editor_window.handle.is_some() || self.context_editor_window.opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.context_editor_window.opening = true;
        let pane = self.bounds.get();
        let parent = self.child_window_parent(cx);
        let chat = cx.entity();
        let appearance = super::super::appearance::ChatAppearance::current(&self.snapshot);
        cx.defer(move |cx| {
            let result = main.update(cx,|_,window,cx| {
                let frame = context_editor_frame(pane, appearance.scale);
                let bounds = gpui::Bounds::new(window.bounds().origin + frame.origin, frame.size);
                (bounds,crate::app::window::popup_frame::display_at(bounds.center(),cx).or_else(||window.display(cx).map(|display|display.id())))
            }).and_then(|(bounds,display_id)| cx.open_window(WindowOptions {
                kind: crate::app::window::popup_frame::child_window_kind(),
                #[cfg(target_os = "linux")]
                x11_parent: Some(main),
                window_bounds:Some(WindowBounds::Windowed(bounds)),display_id,
                app_id:crate::gpui_platform_window_app_id(),icon:crate::gpui_platform_window_icon(),
                focus:true,show:true,is_resizable:false,is_minimizable:false,is_movable:false,titlebar:None,
                // Under window glass the card's own window blurs what is behind it (the card fills it).
                window_background:if crate::app::helpers::window_glass_active() {
                    gpui::WindowBackgroundAppearance::Blurred
                } else {
                    gpui::WindowBackgroundAppearance::Transparent
                },
                ..Default::default()
            }, {
                let chat=chat.clone();
                move |window,cx| {
                    window.set_background_corner_radius(px(12.0 * appearance.scale));
                    crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                    crate::app::window::attach_gpui_app_modal_window_to_main_window(window,parent);
                    let view=cx.new(|cx| {
                        let subscription=cx.observe(&chat,|_,_,cx|cx.notify());
                        let filter=cx.new(|cx|InputState::new(window,cx).placeholder("Search rows"));
                        let input_subscription=cx.subscribe(&filter,|this: &mut ContextEditorWindow,filter,event,cx| {
                            if matches!(event,InputEvent::Change) {
                                let query=filter.read(cx).value().to_string();
                                this.chat.update(cx,|chat,cx|chat.invoke(json!({"type":"contextQuery","query":query}),cx));
                            }
                        });
                        use gpui::Focusable as _;
                        filter.focus_handle(cx).focus(window,cx);
                        ContextEditorWindow {chat,filter,scroll:Default::default(),_subscription:subscription,_input_subscription:input_subscription}
                    });
                    cx.new(|cx|Root::new(view,window,cx).bg(gpui::transparent_black()))
                }
            }));
            chat.update(cx,|chat,cx| {
                chat.context_editor_window.opening=false;
                match result {
                    Ok(handle)=> {
                        chat.context_editor_window.handle=Some(handle);
                        let weak=cx.weak_entity();
                        chat.context_editor_window.subscription=Some(cx.on_window_closed(move |cx,id| {
                            let _=weak.update(cx,|chat,cx| {
                                if chat.context_editor_window.handle.is_some_and(|handle|handle.window_id()==id) {
                                    chat.context_editor_window.handle=None;
                                    chat.invoke(json!({"type":"contextCancel"}),cx);
                                }
                            });
                        }));
                        chat.sync_context_editor_window(cx);
                    }
                    Err(error)=>{chat.error=Some(error.to_string());chat.invoke(json!({"type":"contextCancel"}),cx);}
                }
            });
        });
    }
}

/// The editor card, centred in the pane and kept inside it.
pub(in crate::app::native_chat) fn context_editor_frame(
    pane: gpui::Bounds<gpui::Pixels>,
    scale: f32,
) -> gpui::Bounds<gpui::Pixels> {
    let width = px(576.0 * scale).min(pane.size.width - px(24.0));
    let height = px(760.0 * scale).min(pane.size.height - px(32.0));
    gpui::Bounds::new(
        pane.origin
            + gpui::point(
                (pane.size.width - width) / 2.0,
                (pane.size.height - height) / 2.0,
            ),
        gpui::size(width, height),
    )
}
