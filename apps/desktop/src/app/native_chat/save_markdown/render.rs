use super::super::{appearance::ChatAppearance, transcript::text};
use super::window::SaveMarkdownWindow;
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::Input;
use serde_json::json;

impl SaveMarkdownWindow {
    fn button(
        &self,
        label: &'static str,
        command: &'static str,
        disabled: bool,
        saving: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(command)
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .h(px(32.0))
            .pl(px(12.0))
            .pr(px(if command == "markdownSaveSubmit" {
                10.0
            } else {
                12.0
            }))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(p.border)
            .font_weight(gpui::FontWeight::MEDIUM)
            .when(disabled, |item| item.opacity(0.5).tab_stop(false))
            .when(!disabled, |item| {
                item.chat_cursor_pointer().hover(|style| style.bg(p.input))
            })
            .focus_visible(|style| style.border_color(p.ring))
            .when(saving, |item| {
                item.child(
                    gpui_component::Icon::new(gpui_component::IconName::Loader)
                        .size(px(16.0))
                        .with_throttled_animation(
                            "save-spinner",
                            std::time::Duration::from_secs(1),
                            |icon, progress| {
                                icon.transform(gpui::Transformation::rotate(gpui::percentage(
                                    progress,
                                )))
                            },
                        ),
                )
            })
            .child(label)
            .when(command == "markdownSaveSubmit", |item| {
                item.child(
                    gpui::svg()
                        .path("titlebar/chevron-right.svg")
                        .size(px(16.0))
                        .text_color(p.primary),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if !disabled {
                    this.chat
                        .update(cx, |chat, cx| chat.invoke(json!({"type":command}), cx));
                }
            }))
            .into_any_element()
    }
}
impl Render for SaveMarkdownWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.chat.read(cx).snapshot.clone();
        let state = &snapshot["saveMarkdown"];
        let p = ChatAppearance::current(&snapshot);
        for (input, key) in [(&self.folder, "folder"), (&self.name, "fileName")] {
            let value = text(state, key);
            if input.read(cx).value().as_str() != value {
                let select = key == "fileName"
                    && state["suggested"] == true
                    && input.focus_handle(cx).is_focused(window);
                input.update(cx, |input, cx| {
                    input.set_value(value.clone(), window, cx);
                    if select {
                        input.set_selected_range(0..value.len(), cx);
                    }
                });
            }
        }
        let saving = state["saving"] == true;
        let unavailable = state["loading"] == true || !state["listingError"].is_null();
        let folder_error = state["folderError"]
            .as_str()
            .or(state["listingError"].as_str());
        let name_error = state["fileNameError"].as_str();
        let pane_width = self.chat.read(cx).bounds.get().size.width;
        let input_size = if pane_width < px(768.0) { 16.0 } else { 14.0 };
        let destructive = gpui::rgb(if p.light { 0xe7000b } else { 0xff6467 });
        let label = |value: &'static str, invalid: bool| {
            div()
                .line_height(px(19.25))
                .font_weight(gpui::FontWeight::MEDIUM)
                .when(invalid, |item| item.text_color(destructive))
                .child(value)
        };
        let error = |value: &str| {
            div()
                .text_color(destructive)
                .line_height(px(20.0))
                .child(value.to_owned())
        };
        let mut prose = p.clone();
        prose.scale = 1.0;
        let description_style = super::super::markdown_style::text_style(&prose);
        let input = |entity: &gpui::Entity<gpui_component::input::InputState>, invalid: bool| {
            let focused = entity.focus_handle(cx).is_focused(window);
            let color = if invalid { destructive.into() } else { p.ring };
            Input::new(entity)
                .min_h(px(36.0))
                .max_h(px(36.0))
                .text_size(px(input_size))
                .disabled(saving)
                .rounded(px(0.0))
                .bg(p.input)
                .focus_bordered(false)
                .border_color(if invalid || focused {
                    color
                } else {
                    p.input_border
                })
                .text_color(if invalid {
                    destructive.into()
                } else {
                    p.foreground
                })
                .shadow(if invalid || focused {
                    vec![gpui::BoxShadow {
                        color: color.opacity(0.2),
                        offset: gpui::point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(3.0),
                        inset: false,
                    }]
                } else {
                    vec![]
                })
        };
        let header = div()
            .flex().flex_col().gap(px(6.0)).flex_shrink_0()
            .child(div().text_size(px(16.0)).line_height(px(16.0))
                .font_weight(gpui::FontWeight::MEDIUM).child("Save to Markdown"))
            .child(gpui_component::text::TextView::markdown(
                "save-markdown-description",
                "Save this final response in the project Docs folder. Its full path will be copied after saving.",
            ).selectable(false).style(description_style).text_size(px(14.0)).line_height(px(20.0)).text_color(p.muted));
        let folder = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(label("Folder", folder_error.is_some()))
            .child(
                input(&self.folder, folder_error.is_some())
                    .aria_label("Folder")
                    .prefix(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(p.muted)
                            .child("…/docs/"),
                    ),
            )
            .child(
                div()
                    .line_height(px(21.0))
                    .text_color(p.muted)
                    .child("Use / to create nested folders."),
            )
            .when_some(folder_error, |item, value| item.child(error(value)));
        let name = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(label("File name", name_error.is_some()))
            .child(
                input(&self.name, name_error.is_some())
                    .aria_label("File name")
                    .suffix(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(p.muted)
                            .child(".md"),
                    ),
            )
            .when_some(name_error, |item, value| item.child(error(value)));
        let fields = div()
            .id("save-markdown-fields")
            .mx(px(-4.0))
            .px(px(4.0))
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(28.0))
                    .child(folder)
                    .child(name),
            );
        let footer = div()
            .flex()
            .flex_shrink_0()
            .justify_end()
            .gap(px(8.0))
            .when(pane_width < px(640.0), |item| item.flex_col_reverse())
            .child(self.button("Cancel", "markdownSaveCancel", saving, false, &p, cx))
            .child(self.button(
                "Save to md",
                "markdownSaveSubmit",
                saving || unavailable,
                saving || unavailable,
                &p,
                cx,
            ));
        let dialog = div()
            .id("save-markdown-dialog")
            .w_full()
            .max_w(px(448.0))
            .max_h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(24.0))
            .gap(px(24.0))
            .rounded(px(14.0))
            .bg(if p.light {
                p.background
            } else {
                gpui::rgb(0x191919).into()
            })
            .shadow(vec![
                gpui::BoxShadow {
                    color: gpui::black().opacity(0.1),
                    offset: gpui::point(px(0.0), px(20.0)),
                    blur_radius: px(25.0),
                    spread_radius: px(-5.0),
                    inset: false,
                },
                gpui::BoxShadow {
                    color: gpui::black().opacity(0.1),
                    offset: gpui::point(px(0.0), px(8.0)),
                    blur_radius: px(10.0),
                    spread_radius: px(-6.0),
                    inset: false,
                },
            ])
            .font_family("DM Sans")
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(p.foreground)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.chat.update(cx, |chat, cx| {
                        chat.invoke(json!({"type":"markdownSaveCancel"}), cx)
                    });
                }
            }))
            .child(header)
            .child(fields)
            .child(footer)
            .on_click(|_, _, cx| cx.stop_propagation());
        div()
            .id("save-markdown-backdrop")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .bg(gpui::black().opacity(0.65))
            .capture_any_mouse_down(|_, window, _| {
                super::super::focus::reclaim_keyboard_focus(window);
            })
            .child(dialog)
            .on_click(cx.listener(|this, _, _, cx| {
                this.chat.update(cx, |chat, cx| {
                    chat.invoke(json!({"type":"markdownSaveCancel"}), cx)
                })
            }))
    }
}
