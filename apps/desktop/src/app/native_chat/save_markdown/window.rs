use super::super::{state::NativeChatView, transcript::text};
use gpui::{
    AppContext as _, Context, Entity, Focusable as _, Styled as _, Subscription, WindowBounds,
    WindowOptions, px,
};
use gpui_component::{
    Root,
    input::{InputEvent, InputState},
};
use serde_json::json;

#[derive(Default)]
pub(in crate::app::native_chat) struct SaveMarkdownWindowState {
    pub(in crate::app::native_chat) handle: Option<gpui::WindowHandle<Root>>,
    opening: bool,
    subscription: Option<Subscription>,
}

pub(super) struct SaveMarkdownWindow {
    pub(super) chat: Entity<NativeChatView>,
    pub(super) folder: Entity<InputState>,
    pub(super) name: Entity<InputState>,
    pub(super) scroll: gpui::ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl NativeChatView {
    pub(in crate::app::native_chat) fn sync_save_markdown_window(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.pane_hidden || self.snapshot["saveMarkdown"].is_null() {
            if let Some(handle) = self.save_markdown_window.handle.take() {
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
        if self.save_markdown_window.handle.is_some() || self.save_markdown_window.opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.save_markdown_window.opening = true;
        let pane = self.bounds.get();
        let parent = self.config.parent_native_view;
        let chat = cx.entity();
        cx.defer(move |cx| {
            let result = main.update(cx, |_, window, cx| {
                let origin = window.bounds().origin + pane.origin;
                // Chat Lab's regular macOS titlebar is outside GPUI's content coordinates.
                #[cfg(target_os = "macos")]
                let origin = origin + gpui::point(px(0.0), (window.bounds().size.height - window.viewport_size().height).max(px(0.0)));
                let bounds = gpui::Bounds::new(origin, pane.size);
                (bounds, crate::app::window::popup_frame::display_at(bounds.center(), cx).or_else(|| window.display(cx).map(|display| display.id())))
            }).and_then(|(bounds, display_id)| cx.open_window(WindowOptions {
                kind: crate::app::window::popup_frame::child_window_kind(),
                #[cfg(target_os = "linux")]
                x11_parent: Some(main),
                window_bounds: Some(WindowBounds::Windowed(bounds)), display_id,
                app_id: crate::gpui_platform_window_app_id(), icon: crate::gpui_platform_window_icon(),
                focus: true, show: true, is_resizable: false, is_minimizable: false, is_movable: false, titlebar: None,
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                ..Default::default()
            }, {
                let chat = chat.clone();
                move |window, cx| {
                    super::platform::prepare(window);
                    crate::app::window::attach_gpui_app_modal_window_to_main_window(window, parent);
                    let view = cx.new(|cx| {
                        let mut subscriptions = vec![cx.observe(&chat, |_, _, cx| cx.notify())];
                        let state = &chat.read(cx).snapshot["saveMarkdown"];
                        let folder_value = text(state, "folder");
                        let file_name = text(state, "fileName");
                        let folder = cx.new(|cx| {
                            let mut input = InputState::new(window, cx);
                            input.set_value(folder_value, window, cx);
                            input
                        });
                        let name = cx.new(|cx| {
                            let mut input = InputState::new(window, cx).placeholder("response-name");
                            input.set_value(file_name.clone(), window, cx);
                            input.set_selected_range(0..file_name.len(), cx);
                            input
                        });
                        for (input, command) in [(&folder, "markdownSaveFolder"), (&name, "markdownSaveName")] {
                            subscriptions.push(cx.subscribe(input, move |this: &mut SaveMarkdownWindow, input, event, cx| {
                                let action = match event {
                                    InputEvent::Change => json!({"type":command,"value":input.read(cx).value().to_string()}),
                                    InputEvent::Focus if command == "markdownSaveName" && this.chat.read(cx).snapshot["saveMarkdown"]["suggested"] == true => {
                                        let length = input.read(cx).value().len();
                                        input.update(cx, |input, cx| input.set_selected_range(0..length, cx));
                                        return;
                                    }
                                    InputEvent::PressEnter { .. } => json!({"type":"markdownSaveSubmit"}),
                                    _ => return,
                                };
                                this.chat.update(cx, |chat, cx| chat.invoke(action, cx));
                            }));
                        }
                        super::super::focus::reclaim_keyboard_focus(window);
                        name.focus_handle(cx).focus(window, cx);
                        SaveMarkdownWindow { chat, folder, name, scroll: Default::default(), _subscriptions: subscriptions }
                    });
                    cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
                }
            }));
            chat.update(cx, |chat, cx| {
                chat.save_markdown_window.opening = false;
                match result {
                    Ok(handle) => {
                        chat.save_markdown_window.handle = Some(handle);
                        let weak = cx.weak_entity();
                        chat.save_markdown_window.subscription = Some(cx.on_window_closed(move |cx, id| {
                            let _ = weak.update(cx, |chat, cx| {
                                if chat.save_markdown_window.handle.is_some_and(|handle| handle.window_id() == id) {
                                    chat.save_markdown_window.handle = None;
                                    chat.invoke(json!({"type":"markdownSaveCancel"}), cx);
                                }
                            });
                        }));
                        chat.sync_save_markdown_window(cx);
                    }
                    Err(error) => {
                        chat.error = Some(error.to_string());
                        chat.invoke(json!({"type":"markdownSaveCancel"}), cx);
                    }
                }
            });
        });
    }
}
