//! Side chats in the chat: the menu of the Side chats pill (one row per side chat of this session,
//! each opening it, with Bring back and Delete) and the "Side chat of" bar at the top of a side
//! chat's own chat. Rows, labels and the calls come from the core
//! (packages/gx-chat-core/src/extras/side_chats.rs, where the user's decision lives); the pill is
//! chat_pills.rs.
//!
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/SideChats.tsx (the phone's twin),
//! apps/desktop/src/app/session_chat/host_actions.rs (`openSideChat`, `sideChatDeleted`).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb, svg,
};
use serde_json::{Value, json};

/// The sidebar's working colour, as the Workers menu uses it (coordinator_threads.rs).
const WORKING_COLOR: u32 = 0xc68a06;

/// Fixed heights (unscaled), so the floating menu knows its size before it opens.
const ROW_HEIGHT: f32 = 26.0;
const ROW_GAP: f32 = 2.0;
const LIST_MAX: f32 = 360.0;
const BAR_HEIGHT: f32 = 30.0;

/// The Side chats pill's menu: its header line and its rows.
pub(super) fn side_chats_menu_height(panel: &Value) -> f32 {
    let rows = panel["rows"].as_array().map_or(0, Vec::len);
    let list = (rows as f32 * ROW_HEIGHT + rows.saturating_sub(1) as f32 * ROW_GAP).min(LIST_MAX);
    super::chat_pills::MENU_HEADER_HEIGHT + super::chat_pills::MENU_SECTION_GAP + list
}

/// What a row says after its title: what the side chat waits on, or where it stands.
fn row_status(row: &Value) -> &'static str {
    match text(row, "waiting").as_str() {
        "approval" => "Needs your approval",
        "question" => "Waiting for an answer",
        _ if row["working"] == true => "Working",
        _ if row["unread"] == true => "New reply",
        _ if row["sleeping"] == true => "Sleeping",
        _ => "",
    }
}

/// A small icon button of a row or the bar.
fn icon_button(
    id: gpui::SharedString,
    icon: &'static str,
    label: &'static str,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .size(px(20.0 * s))
        .rounded(px(5.0 * s))
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.foreground.opacity(0.08)))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(svg().path(icon).size(px(13.0 * s)).text_color(p.card_muted))
}

impl NativeChatView {
    /// The Side chats pill's floating menu, drawn in the menu's window with this view's listeners.
    pub(super) fn side_chats_menu_body(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.snapshot["sideChatsPanel"].clone();
        if !panel.is_object() {
            return None;
        }
        let s = p.scale;
        let header = super::chat_pills::pill_menu_header(
            "titlebar/messages.svg",
            "Side chats",
            text(&panel, "meta"),
            super::chat_pills::waiting_color(&text(&panel, "waiting")).map(|color| {
                div()
                    .size(px(8.0 * s))
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(color)
                    .into_any_element()
            }),
            p,
        );
        let mut rows = div()
            .id("chat-side-chats-rows")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(ROW_GAP * s))
            .max_h(px(LIST_MAX * s))
            .overflow_y_scroll();
        for row in panel["rows"].as_array().into_iter().flatten() {
            rows = rows.child(self.side_chat_row(row, p, cx));
        }
        Some(super::chat_pills::pill_menu_column(
            "chat-side-chats",
            "Side chats",
            header,
            Some(rows.into_any_element()),
            None,
            p,
        ))
    }

    fn side_chat_row(&self, row: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let session_id = text(row, "sessionId");
        let waiting = super::chat_pills::waiting_color(&text(row, "waiting"));
        let marker = match (waiting, row["working"] == true) {
            (Some(color), _) => div().size(px(8.0 * s)).rounded_full().bg(color),
            (None, true) => div()
                .size(px(8.0 * s))
                .rounded_full()
                .bg(rgb(WORKING_COLOR)),
            (None, false) => div()
                .size(px(8.0 * s))
                .rounded_full()
                .border(px(1.5 * s))
                .border_color(p.muted.opacity(0.7)),
        };
        let title = text(row, "title");
        let open = json!({
            "type": "openSideChat",
            "projectId": row["projectId"],
            "sessionId": row["sessionId"],
        });
        let bring_back = json!({ "type": "bringBackSideChat", "sessionId": session_id });
        let delete = json!({ "type": "deleteSideChat", "sessionId": session_id });
        div()
            .id(gpui::SharedString::from(format!(
                "chat-side-chat:{session_id}"
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("Open side chat {title}"))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .px(px(4.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(12.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(
                div()
                    .size(px(13.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(marker),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(240.0 * s))
                    .truncate()
                    .text_color(p.foreground)
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.card_muted)
                    .child(row_status(row)),
            )
            .child(
                icon_button(
                    format!("chat-side-chat-bring-back:{session_id}").into(),
                    "titlebar/arrow-back-up.svg",
                    "Bring back to this chat",
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(bring_back.clone(), cx);
                    this.close_pill_menu(cx);
                })),
            )
            .child(
                icon_button(
                    format!("chat-side-chat-delete:{session_id}").into(),
                    "titlebar/trash.svg",
                    "Delete side chat",
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(delete.clone(), cx);
                })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.invoke(open.clone(), cx);
                this.close_pill_menu(cx);
            }))
            .into_any_element()
    }

    /// The bar at the top of a side chat's own chat: whose side chat it is, Bring back and Delete.
    pub(super) fn side_chat_bar(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let bar = &self.snapshot["sideChatOf"];
        if !bar.is_object() {
            return None;
        }
        let s = p.scale;
        let parent = text(bar, "parentTitle");
        Some(
            div()
                .id("chat-side-chat-bar")
                .role(gpui::Role::Group)
                .aria_label(format!("Side chat of {parent}"))
                .flex_shrink_0()
                .h(px(BAR_HEIGHT * s))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .px(px(12.0 * s))
                .border_b(px(s))
                .border_color(p.foreground.opacity(0.08))
                .text_size(px(12.0 * s))
                .child(
                    svg()
                        .path("titlebar/messages.svg")
                        .size(px(13.0 * s))
                        .flex_shrink_0()
                        .text_color(p.card_muted),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(p.card_muted)
                        .child("Side chat of"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(p.foreground)
                        .child(parent),
                )
                .child(
                    icon_button(
                        "chat-side-chat-bar-bring-back".into(),
                        "titlebar/arrow-back-up.svg",
                        "Bring back to the main session",
                        p,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.invoke(json!({ "type": "bringBackSideChat" }), cx)
                    })),
                )
                .child(
                    icon_button(
                        "chat-side-chat-bar-delete".into(),
                        "titlebar/trash.svg",
                        "Delete side chat",
                        p,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.invoke(json!({ "type": "deleteSideChat" }), cx)
                    })),
                )
                .into_any_element(),
        )
    }
}
