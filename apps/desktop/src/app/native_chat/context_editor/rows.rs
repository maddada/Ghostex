use super::super::{appearance::ChatAppearance, transcript::text};
use super::window::ContextEditorWindow;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use gpui_component::{Disableable as _, Sizable as _, switch::Switch};
use serde_json::{Value, json};

#[derive(Clone)]
struct ContextRowDrag {
    owner: gpui::EntityId,
    group: String,
    id: String,
    label: String,
    appearance: ChatAppearance,
}
impl Render for ContextRowDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        crate::app::helpers::drag_ghost(self.label.clone(), None, None, self.appearance.scale)
    }
}
impl ContextEditorWindow {
    pub(super) fn icon_button(
        &self,
        id: String,
        label: String,
        icon: &'static str,
        command: Value,
        active: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let disabled = self.chat.read(cx).snapshot["contextEditor"]["saving"] == true;
        let tooltip = label.clone();
        let key_command = command.clone();
        div()
            .id(id)
            .focusable()
            .tab_stop(!disabled)
            .role(gpui::Role::Button)
            .aria_label(label)
            .size(px(24.0 * p.scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * p.scale))
            .when(!disabled, |item| {
                item.chat_cursor_pointer().hover(|style| style.bg(p.border))
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(13.0 * p.scale))
                    .text_color(if active {
                        gpui::rgb(0xfcd34d).into()
                    } else {
                        p.muted
                    }),
            )
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.activate_key(event, key_command.clone(), window, cx)
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !disabled {
                    this.chat
                        .update(cx, |chat, cx| chat.invoke(command.clone(), cx));
                }
            }))
            .into_any_element()
    }
    pub(super) fn option_row(
        &self,
        item: &Value,
        group: &str,
        chip: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = text(item, "id");
        let label = text(item, "label");
        let owner = cx.entity_id();
        let group = group.to_owned();
        let source = ContextRowDrag {
            owner,
            group: group.clone(),
            id: id.clone(),
            label: label.clone(),
            appearance: p.clone(),
        };
        let target = id.clone();
        let target_group = group.clone();
        let disabled = self.chat.read(cx).snapshot["contextEditor"]["saving"] == true;
        let mut row=div().id(format!("context-{group}-{id}")).flex().items_center().gap(px(8.0*p.scale)).rounded(px(6.0*p.scale))
            .px(px(4.0*p.scale)).py(px(6.0*p.scale)).hover(|style|style.bg(p.border.opacity(0.4)))
            .when(chip,|item|item.border_1().border_color(p.border).py(px(2.0*p.scale)))
            .when(!disabled,|item|item.on_drop(cx.listener(move |this,drag:&ContextRowDrag,_,cx| {
                if drag.owner==owner && drag.group==target_group {
                    this.chat.update(cx,|chat,cx|chat.invoke(json!({"type":"contextReorder","group":target_group,"from":drag.id,"to":target}),cx));
                }
            })))
            .child(div().id(format!("context-grip-{group}-{id}")).role(gpui::Role::Button).aria_label(format!("Reorder {label}"))
                .w(px(14.0*p.scale)).flex_shrink_0().text_color(p.muted).child("⠿")
                .when(!disabled,|item|item.cursor_grab().on_drag(source,|drag,_,_,cx|cx.new(|_|drag.clone()))));
        if chip {
            row = row
                .child(div().text_size(px(11.0 * p.scale)).child(label.clone()))
                .child(self.icon_button(
                    format!("remove-star-{id}"),
                    format!("Unstar {label}"),
                    "titlebar/x.svg",
                    json!({"type":"contextStar","id":id}),
                    false,
                    p,
                    cx,
                ));
        } else {
            let sample = item["sample"].as_str().unwrap_or("\u{2014}").to_owned();
            row = row
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(12.0 * p.scale))
                                .text_ellipsis()
                                .child(label.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0 * p.scale))
                                .text_color(p.muted)
                                .text_ellipsis()
                                .child(text(item, "description")),
                        ),
                )
                .child(
                    div()
                        .max_w(px(150.0 * p.scale))
                        .text_size(px(11.0 * p.scale))
                        .text_color(p.muted)
                        .text_ellipsis()
                        .child(sample),
                )
                .child(self.icon_button(
                    format!("star-{id}"),
                    format!(
                        "{} {label}",
                        if item["starred"] == true {
                            "Unstar"
                        } else {
                            "Star"
                        }
                    ),
                    "titlebar/star-filled.svg",
                    json!({"type":"contextStar","id":id}),
                    item["starred"] == true,
                    p,
                    cx,
                ));
            let chat = self.chat.clone();
            let key_id = id.clone();
            let key_shown = item["shown"] != true;
            row = row.child(
                div()
                    .id(format!("shown-keyboard-{id}"))
                    .focusable()
                    .tab_stop(!disabled)
                    .role(gpui::Role::Switch)
                    .aria_label(format!("Show {label}"))
                    .aria_toggled(if item["shown"] == true {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .on_key_down(cx.listener(move |this, event, window, cx| {
                        this.activate_key(
                            event,
                            json!({"type":"contextShown","id":key_id,"shown":key_shown}),
                            window,
                            cx,
                        )
                    }))
                    .child(
                        Switch::new(format!("shown-{id}"))
                            .small()
                            .checked(item["shown"] == true)
                            .disabled(disabled)
                            .on_click(move |shown, _, cx| {
                                chat.update(cx, |chat, cx| {
                                    chat.invoke(
                                        json!({"type":"contextShown","id":id,"shown":shown}),
                                        cx,
                                    )
                                })
                            }),
                    ),
            );
        }
        row.into_any_element()
    }
}
