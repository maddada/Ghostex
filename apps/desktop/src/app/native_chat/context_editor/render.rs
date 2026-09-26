use super::super::{appearance::ChatAppearance, transcript::text};
use super::window::ContextEditorWindow;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::Input;
use serde_json::json;

impl ContextEditorWindow {
    pub(super) fn activate_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        command: serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space")
            && self.chat.read(cx).snapshot["contextEditor"]["saving"] != true
        {
            window.prevent_default();
            cx.stop_propagation();
            self.chat.update(cx, |chat, cx| chat.invoke(command, cx));
        }
    }

    fn button(
        &self,
        label: &'static str,
        command: &'static str,
        primary: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let disabled = self.chat.read(cx).snapshot["contextEditor"]["saving"] == true;
        div()
            .id(command)
            .focusable()
            .tab_stop(!disabled)
            .role(gpui::Role::Button)
            .aria_label(label)
            .h(px(32.0 * p.scale))
            .px(px(12.0 * p.scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * p.scale))
            .border_1()
            .border_color(p.border)
            // CDXC:Theming 2026-09-23 DECISION: User: this modal matches the app's other modals, so the primary action is a soft raised wash of the ink rather than a solid white button.
            .when(primary, |item| {
                item.bg(p.foreground.opacity(if p.light { 0.08 } else { 0.12 }))
                    .text_color(p.foreground)
            })
            .when(!disabled, |item| {
                item.hover(|style| style.bg(p.foreground.opacity(if p.light { 0.1 } else { 0.16 })))
            })
            .when(disabled, |item| item.opacity(0.5))
            .when(!disabled, |item| item.chat_cursor_pointer())
            .child(label)
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.activate_key(event, json!({"type":command}), window, cx)
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !disabled {
                    this.chat
                        .update(cx, |chat, cx| chat.invoke(json!({"type":command}), cx));
                }
            }))
            .into_any_element()
    }
}
impl Render for ContextEditorWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.chat.read(cx).snapshot.clone();
        let editor = &snapshot["contextEditor"];
        // CDXC:Theming 2026-09-26 WHY: under window glass this window is frosted
        // (`menu_surface`), so its fills take the glass washes like the suggestions popup;
        // the solid input tone, a lift of the theme colour, showed as a tinted (green) box.
        let p = ChatAppearance::current(&snapshot)
            .on_window_glass(crate::app::helpers::window_glass_active());
        let s = p.scale;
        let mut groups = div()
            .id("context-detail-groups")
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        if editor["groups"]
            .as_array()
            .is_some_and(|groups| groups.is_empty())
        {
            groups = groups.child(
                div()
                    .py(px(24.0 * s))
                    .text_color(p.muted)
                    .child(format!("No rows match “{}”.", text(editor, "query").trim())),
            );
        }
        for group in editor["groups"].as_array().into_iter().flatten() {
            groups = groups.child(
                div()
                    .pt(px(8.0 * s))
                    .pb(px(2.0 * s))
                    .text_size(px(10.0 * s))
                    .text_color(p.muted)
                    .child(text(group, "label").to_uppercase()),
            );
            for row in group["rows"].as_array().into_iter().flatten() {
                groups = groups.child(self.option_row(row, &text(group, "id"), false, &p, cx));
            }
        }
        let mut starred = div().flex().flex_wrap().gap(px(6.0 * s));
        for row in editor["starred"].as_array().into_iter().flatten() {
            starred = starred.child(self.option_row(row, "starred", true, &p, cx));
        }
        let no_stars = editor["starred"]
            .as_array()
            .is_none_or(|rows| rows.is_empty());
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(16.0 * s))
            .p(px(24.0 * s))
            .rounded(px(12.0 * s))
            .border_1()
            .border_color(p.border)
            .bg(p.menu_surface())
            .font_family(p.font.clone())
            .text_size(px(13.0 * s))
            .text_color(p.foreground)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.chat.update(cx, |chat, cx| {
                        chat.invoke(json!({"type":"contextCancel"}), cx)
                    });
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(18.0 * s))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Context details"),
                    )
                    .child(self.icon_button(
                        "context-close".into(),
                        "Close".into(),
                        "titlebar/x.svg",
                        json!({"type":"contextCancel"}),
                        false,
                        &p,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(14.0 * s))
                    .text_color(p.muted)
                    .child(text(editor, "description")),
            )
            .child(
                div()
                    .h(px(32.0 * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.input)
                    .child(
                        Input::new(&self.filter)
                            .cleanable(true)
                            .disabled(editor["saving"] == true)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .placeholder_color(p.muted.opacity(0.6))
                            .w_full()
                            .text_size(px(13.0 * s)),
                    ),
            )
            .child(groups)
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(p.border)
                    .pt(px(12.0 * s))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mb(px(6.0 * s))
                            .text_size(px(11.0 * s))
                            .text_color(p.muted)
                            .child("STATUS LINE")
                            .child(if no_stars {
                                "Star rows above to show them under the chat box."
                            } else {
                                "Drag to arrange."
                            }),
                    )
                    .child(starred),
            )
            .when_some(editor["error"].as_str(), |item, error| {
                item.child(
                    div()
                        .text_color(gpui::rgb(0xef9999))
                        .child(error.to_owned()),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0 * s))
                    .child(self.button("Reset to recommended", "contextReset", false, &p, cx))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0 * s))
                            .child(self.button("Cancel", "contextCancel", false, &p, cx))
                            .child(self.button("Save", "contextSave", true, &p, cx)),
                    ),
            )
    }
}
