//! `TitlebarViewOrderDialog` (settings-modal/tabs/titlebar-view-order-dialog.tsx (deleted 2026-10-01)): the nested
//! "Arrange views" dialog over Settings. Every built-in, extension and custom view in one list,
//! dragged by its grip or moved with the arrows, with its numbered shortcut while it is one of the
//! first nine visible views; Reset order and Done.
//!
//! CDXC:Settings 2026-09-09 DECISION (see the React twin): a button in Settings opens the
//! view-order popup with a backdrop behind it, inside the Settings window.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ReorderOptions, SizedButtonSize, SizedButtonVariant, dialog_background, icon, reorder_handle,
    reorder_options, reorder_order, reorder_row, reorder_scroll_handle, settings_dialog_sheet,
    settings_icon, settings_sized_button, settings_square_button,
};
use super::super::super::palette::SettingsPalette;
use super::ExtensionsTab;
use super::data::{merged_view_order, move_id, titlebar_view_order, view_order_items};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

const LIST: &str = "titlebar-view-order";

impl ExtensionsTab {
    pub(crate) fn open_arrange(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.arrange_open = true;
        self.arrange_focus.focus(window, cx);
        cx.notify();
    }

    fn close_arrange(&mut self, cx: &mut Context<Self>) {
        self.arrange_open = false;
        cx.notify();
    }

    /// `move(from, to)`: saves the moved order followed by the stored ids it does not list.
    fn move_view(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let items = view_order_items(&values, &self.browser.installed);
        if from == to || to >= items.len() {
            return;
        }
        let ids: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
        let order = merged_view_order(move_id(&ids, from, to), &titlebar_view_order(&values));
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting("titlebarViewOrder", order, cx)
        });
    }

    pub(crate) fn render_arrange_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = &p.nested_dialog();
        if !self.arrange_open {
            return None;
        }
        let values = self.store.read(cx).values();
        let items = view_order_items(&values, &self.browser.installed);
        let hotkeys = values.value("hotkeys");
        let visible_ids: Vec<String> = items
            .iter()
            .filter(|item| item.visible)
            .map(|item| item.id.clone())
            .collect();
        let count = items.len();
        // `.settings-titlebar-view-order-list [data-dragging='true'] { opacity: 0.5 }`; the list
        // scrolls itself, so it is the auto-scroll container.
        let scroll = reorder_scroll_handle(self, LIST);
        reorder_options(
            self,
            LIST,
            ReorderOptions {
                lifted_opacity: 0.5,
                activation_distance: 0.0,
                scroll: Some(scroll.clone()),
                fill: true,
            },
        );
        let order = reorder_order(self, LIST, count, cx);
        let row_background = if p.light {
            gpui::rgb(0xffffff)
        } else {
            dialog_background(p)
        };
        let hairline = hsla(p.hairline);
        let mut rows: Vec<AnyElement> = Vec::new();
        for (slot, index) in order.iter().copied().enumerate() {
            let item = &items[index];
            let visible_index = visible_ids.iter().position(|id| *id == item.id);
            let shortcut = visible_index.filter(|index| *index < 9).and_then(|index| {
                hotkeys
                    .get(format!("switchTitlebarView{}", index + 1))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(crate::hotkey_label::terminal_overlay_hotkey_chord_label)
            });
            let grip = div()
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .child(settings_icon(icon::GRIP_VERTICAL, 16.0, p.foreground))
                .into_any_element();
            let handle = reorder_handle(p, LIST, index, item.title.clone(), grip);
            let up = settings_square_button(
                p,
                SharedString::from(format!("view-order-{index}-up")),
                "modals/settings/arrow-up.svg",
                None,
                SizedButtonVariant::Ghost,
                28.0,
                None,
                index == 0,
                None,
                move |page: &mut Self, _window, cx| {
                    page.move_view(index, index.saturating_sub(1), cx)
                },
                cx,
            );
            let down = settings_square_button(
                p,
                SharedString::from(format!("view-order-{index}-down")),
                "modals/settings/arrow-down.svg",
                None,
                SizedButtonVariant::Ghost,
                28.0,
                None,
                index + 1 == count,
                None,
                move |page: &mut Self, _window, cx| page.move_view(index, index + 1, cx),
                cx,
            );
            let row = h_flex()
                .w_full()
                .min_h(px(56.0))
                .px(px(8.0))
                .py(px(8.0))
                .gap(px(8.0))
                .items_center()
                .bg(hsla(row_background))
                .when(slot + 1 < count, |this| {
                    this.border_b_1().border_color(hairline)
                })
                .child(handle)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .text_color(hsla(p.foreground))
                                .child(item.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.57))
                                .text_color(hsla(p.muted))
                                .child(format!(
                                    "{}{}",
                                    item.source,
                                    if item.visible { "" } else { " · Hidden" }
                                )),
                        ),
                )
                .children(shortcut.map(|shortcut| {
                    div()
                        .flex_shrink_0()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .text_color(hsla(p.muted))
                        .child(shortcut)
                }))
                .child(up)
                .child(down)
                .into_any_element();
            rows.push(reorder_row(
                self,
                LIST,
                index,
                slot,
                row,
                |page: &mut Self, from, to, _window, cx| page.move_view(from, to, cx),
                cx,
            ));
        }
        let list = v_flex()
            .id("titlebar-view-order-list")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hairline)
            .children(rows)
            .into_any_element();
        let reset = settings_sized_button(
            p,
            "view-order-reset",
            "Reset order",
            None,
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.update_setting("titlebarViewOrder", serde_json::json!([]), cx)
                });
            },
            cx,
        );
        let done = settings_sized_button(
            p,
            "view-order-done",
            "Done",
            None,
            None,
            SizedButtonVariant::Default,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| page.close_arrange(cx),
            cx,
        );
        let footer = h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .child(reset)
            .child(done)
            .into_any_element();
        let max_height = (f32::from(window.viewport_size().height) * 0.85).min(768.0);
        Some(settings_dialog_sheet(
            p,
            "titlebar-view-order-dialog",
            &self.arrange_focus.clone(),
            "Arrange views",
            Some(
                div()
                    .child("Drag views into order or use the arrows. A view you open lands in this order in the view panel's tabs, and numbered view shortcuts follow those tabs. Hidden views keep their place for when you enable them.")
                    .into_any_element(),
            ),
            544.0,
            Some(max_height),
            20.0,
            16.0,
            false,
            vec![list],
            Some(footer),
            |page: &mut Self, _window, cx| page.close_arrange(cx),
            window,
            cx,
        ))
    }
}
