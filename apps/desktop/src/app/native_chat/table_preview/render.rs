use super::super::{appearance::ChatAppearance, rich_markdown};
use super::window::{TablePreviewWindow, table_preview_frame};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_component::text::TextView;
use serde_json::json;

impl Render for TablePreviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snapshot, source, area) = {
            let chat = self.chat.read(cx);
            (
                chat.snapshot.clone(),
                chat.table_preview.source.clone().unwrap_or_default(),
                chat.expanded_area(),
            )
        };
        let glass = crate::app::helpers::window_glass_active();
        let p = ChatAppearance::current(&snapshot).on_window_glass(glass);
        let s = p.scale;
        if glass {
            // The main window's content sits at minus this window's frame in its coordinates.
            let frame = table_preview_frame(area);
            crate::app::helpers::sync_overlay_window_glass(
                window,
                gpui::point(-frame.origin.x, -frame.origin.y),
            );
        }
        let link_chat = self.chat.downgrade();
        let close_chat = self.chat.downgrade();
        let [header, table] = table_preview_content(
            source,
            &p,
            &self.scroll,
            move |href, external, cx| {
                let _ = link_chat.update(cx, |chat, cx| {
                    chat.invoke(
                        json!({"type":"openMarkdownLink","href":href,"external":external}),
                        cx,
                    )
                });
            },
            move |cx| {
                let _ = close_chat.update(cx, |chat, cx| chat.close_table_preview(cx));
            },
        );
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
            .child(header)
            .child(table)
    }
}

/// The preview's two rows: the "Table" header with its MD, CSV and close buttons, over the table
/// itself, scrolling. Shared by the chat's preview window and the app's Markdown table popup
/// (window/markdown_table_modal.rs); each wraps them in its own card and handles Escape.
pub(crate) fn table_preview_content(
    source: String,
    p: &ChatAppearance,
    scroll: &gpui::ScrollHandle,
    open_link: impl Fn(&str, bool, &mut gpui::App) + Send + Sync + 'static,
    close: impl Fn(&mut gpui::App) + 'static,
) -> [AnyElement; 2] {
    let s = p.scale;
    let mut style = super::super::markdown_style::text_style(p);
    style.is_dark = !p.light;
    style.highlight_theme = super::super::markdown_style::highlight_theme(p.light);
    // The transcript's adaptive layout with more room: columns share the window's width and wrap,
    // and the table only scrolls sideways when the window is still too narrow for their floors.
    let table = TextView::markdown("chat-table-preview", source.clone())
        .min_w_0()
        .max_w(gpui::relative(1.0))
        .on_link_click(move |href, event, _, cx| {
            // Only a primary click opens a link; the preview has no link menu.
            if !event.standard_click() {
                return;
            }
            open_link(href, event.modifiers().shift, cx);
        })
        .selectable(true)
        .style(style)
        .text_size(px(14.0 * s))
        .line_height(px(22.75 * s))
        .text_color(p.prose);
    let markdown = source.clone();
    let csv = source;
    let mut body = div()
        .id("chat-table-preview-body")
        .size_full()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .px(px(24.0 * s))
        .pb(px(24.0 * s))
        .child(table);
    // A sideways swipe over the page, off the table, is not a vertical scroll.
    body.style().restrict_scroll_to_axis = Some(true);
    let close = rich_markdown::table_action("close-table-preview", "titlebar/x.svg", p, close);
    // The X leads the header on macOS, before the title, and ends it elsewhere (CDXC:AppModal
    // 2026-11-10 in window/round_close_button.rs).
    let (leading_close, trailing_close) =
        if crate::app::window::round_close_button::close_buttons_on_left() {
            (Some(close), None)
        } else {
            (None, Some(close))
        };
    let header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .px(px(24.0 * s))
        .pt(px(20.0 * s))
        .pb(px(12.0 * s))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .children(leading_close)
                .child(
                    div()
                        .text_size(px(18.0 * s))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Table"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0 * s))
                .child(rich_markdown::table_copy_action(
                    "copy-table",
                    "MD",
                    p,
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
                    p,
                    move |cx| {
                        crate::app::helpers::gpui_copy_to_clipboard(
                            ClipboardItem::new_string(rich_markdown::table_csv(&csv)),
                            cx,
                        );
                    },
                ))
                .children(trailing_close),
        );
    let table = div().relative().flex_1().min_h_0().child(body).child(
        Scrollbar::vertical(scroll)
            .id("chat-table-preview-scrollbar")
            .thickness(px(super::super::scrollbar::THICKNESS * s))
            .mode(ScrollbarMode::Scrolling),
    );
    [header.into_any_element(), table.into_any_element()]
}
