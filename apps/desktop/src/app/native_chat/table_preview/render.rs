use super::super::{appearance::ChatAppearance, rich_markdown};
use super::window::{TablePreviewWindow, table_preview_frame};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_component::text::TextView;
use serde_json::json;

impl Render for TablePreviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snapshot, source, pane) = {
            let chat = self.chat.read(cx);
            (
                chat.snapshot.clone(),
                chat.table_preview.source.clone().unwrap_or_default(),
                chat.bounds.get(),
            )
        };
        let glass = crate::app::helpers::window_glass_active();
        let p = ChatAppearance::current(&snapshot).on_window_glass(glass);
        let s = p.scale;
        if glass {
            // The main window's content sits at minus this window's frame in its coordinates.
            let frame = table_preview_frame(pane, s);
            crate::app::helpers::sync_overlay_window_glass(
                window,
                gpui::point(-frame.origin.x, -frame.origin.y),
            );
        }
        let mut style = super::super::markdown_style::text_style(&p);
        style.is_dark = !p.light;
        style.highlight_theme = super::super::markdown_style::highlight_theme(p.light);
        // Room is what the preview is for: every cell wraps inside a column as wide as the
        // component allows, and the table only scrolls sideways when the window is still too narrow.
        style.table_cell_max_width = Some(px(480.0 * s));
        style.table_wrap_cells = true;
        let link_chat = self.chat.downgrade();
        let table = TextView::markdown("chat-table-preview", source.clone())
            .min_w_0()
            .max_w(gpui::relative(1.0))
            .on_link_click(move |href, event, _, cx| {
                // Only a primary click opens a link; the preview has no link menu.
                if !event.standard_click() {
                    return;
                }
                let _ = link_chat.update(cx, |chat, cx| {
                    chat.invoke(
                        json!({"type":"openMarkdownLink","href":href,"external":event.modifiers().shift}),
                        cx,
                    )
                });
            })
            .selectable(true)
            .style(style)
            .text_size(px(14.0 * s))
            .line_height(px(22.75 * s))
            .text_color(p.prose);
        let markdown = source.clone();
        let csv = source;
        let close_chat = self.chat.downgrade();
        let mut body = div()
            .id("chat-table-preview-body")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(24.0 * s))
            .pb(px(24.0 * s))
            .child(table);
        // A sideways swipe over the page, off the table, is not a vertical scroll.
        body.style().restrict_scroll_to_axis = Some(true);
        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.0 * s))
            .border_1()
            .border_color(p.border)
            .map(|card| {
                if glass {
                    card.bg(crate::app::helpers::sidebar_glass_tint())
                } else {
                    card.bg(p.background)
                }
            })
            .font_family(p.font.clone())
            .text_color(p.foreground)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.chat
                        .update(cx, |chat, cx| chat.close_table_preview(cx));
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px(px(24.0 * s))
                    .pt(px(20.0 * s))
                    .pb(px(12.0 * s))
                    .child(
                        div()
                            .text_size(px(18.0 * s))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Table"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0 * s))
                            .child(rich_markdown::table_copy_action(
                                "copy-table",
                                "MD",
                                &p,
                                move |cx| {
                                    crate::app::helpers::gpui_copy_to_clipboard(
                                        ClipboardItem::new_string(markdown.clone()),
                                        cx,
                                    );
                                },
                            ))
                            .child(rich_markdown::table_copy_action(
                                "copy-table-csv",
                                "CSV",
                                &p,
                                move |cx| {
                                    crate::app::helpers::gpui_copy_to_clipboard(
                                        ClipboardItem::new_string(rich_markdown::table_csv(&csv)),
                                        cx,
                                    );
                                },
                            ))
                            .child(rich_markdown::table_action(
                                "close-table-preview",
                                "titlebar/x.svg",
                                &p,
                                move |cx| {
                                    let _ = close_chat
                                        .update(cx, |chat, cx| chat.close_table_preview(cx));
                                },
                            )),
                    ),
            )
            .child(
                div().relative().flex_1().min_h_0().child(body).child(
                    Scrollbar::vertical(&self.scroll)
                        .id("chat-table-preview-scrollbar")
                        .thickness(px(super::super::scrollbar::THICKNESS * s))
                        .mode(ScrollbarMode::Scrolling),
                ),
            )
    }
}
