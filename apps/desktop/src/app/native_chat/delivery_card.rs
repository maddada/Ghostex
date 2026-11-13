//! The card under a message that has not reached the agent: the chat core's `deliveryCards` entry
//! for it (packages/gx-chat-core/src/composer/delivery_card.rs), drawn in the Dictate card's
//! shape: the level's icon, the title and ⋯ on one line, one sentence, the main actions as filled
//! buttons on one line. Only the icon carries the level's colour.
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User, of the failed send's loose grey line with bare Retry and Remove links: "these look so out of place here". Q2941 B: the Dictate card right under the bubble, keeping every action ("please don't lose options", with a 3 dots menu when there are too many); Q2944: the level shows only in the icon's colour. Supersedes the 2026-11-05 note shape (`delivery_note`) for a plain message's own delivery.
//! SEE-ALSO: native_chat/dictation.rs (the card's shape), apps/mobile/app/src/chat/native/transcript/DeliveryCard.tsx (the phone's card).

use super::appearance::ChatAppearance;
use super::cursor::ChatCursor as _;
use super::state::NativeChatView;
use super::tone_card::{ToneButton, tone_look};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, relative, svg,
};
use serde_json::{Value, json};

/// The trigger id the ⋯ menu toggles shut on a second press.
const DELIVERY_MENU_TRIGGER: &str = "delivery-card-menu";

/// The icon file a card's button or menu row names.
fn icon_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "refresh" => "titlebar/refresh.svg",
        "terminal-2" => "titlebar/terminal-2.svg",
        "trash" => "titlebar/trash.svg",
        "pencil" => "titlebar/pencil.svg",
        _ => return None,
    })
}

/// What a button or menu row sends to the core.
fn item_command(item: &Value) -> Value {
    item["command"].clone()
}

impl NativeChatView {
    /// The card for one message's delivery, at the message's side and at most its width.
    pub(super) fn delivery_card(
        &self,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let prompt_id = card["promptId"].as_str().unwrap_or_default().to_string();
        let level = card["level"].as_str().unwrap_or("working");
        let (icon, hue) = match level {
            "failed" => ("titlebar/alert-circle.svg", tone_look("error", p).hue),
            "needs" => ("titlebar/clock.svg", tone_look("wait", p).hue),
            // The spinner beside the bubble already turns; the card's own mark stays still.
            _ => ("titlebar/clock.svg", None),
        };
        let title = card["title"].as_str().unwrap_or_default().to_string();
        let sentence = card["sentence"].as_str().unwrap_or_default().to_string();
        let menu: Vec<Value> = card["menu"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                let mut row = json!({"label": item["label"], "command": item_command(item)});
                if let Some(path) = item["icon"].as_str().and_then(icon_path) {
                    row["iconPath"] = json!(path);
                }
                row
            })
            .collect();
        let buttons: Vec<AnyElement> = card["buttons"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(at, item)| {
                let label = item["label"].as_str().unwrap_or_default();
                let kind = if item["primary"] == true {
                    ToneButton::Primary
                } else {
                    ToneButton::Secondary
                };
                let command = item_command(item);
                let icon = item["icon"].as_str().and_then(icon_path);
                self.tone_button(
                    gpui::SharedString::from(format!("delivery-card:{prompt_id}:{at}")),
                    label,
                    icon,
                    kind,
                    None,
                    false,
                    p,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.handle_action(
                        &super::actions::NativeChatAction {
                            command: command.clone(),
                        },
                        window,
                        cx,
                    )
                }))
                .into_any_element()
            })
            .collect();
        let more = (!menu.is_empty()).then(|| {
            div()
                .id(gpui::SharedString::from(format!(
                    "delivery-card-menu:{prompt_id}"
                )))
                .role(gpui::Role::Button)
                .aria_label("More actions")
                .flex_shrink_0()
                .size(px(24.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0 * s))
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.08)))
                .child(
                    svg()
                        .path("titlebar/dots.svg")
                        .size(px(14.0 * s))
                        .text_color(p.card_muted),
                )
                .on_click(
                    cx.listener(move |chat, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if chat.chat_menu_toggled_shut(DELIVERY_MENU_TRIGGER, cx) {
                            return;
                        }
                        let at = event.position();
                        let in_main = chat
                            .open_main_window(cx)
                            .is_none_or(|main| main == window.window_handle());
                        if in_main {
                            chat.show_chat_menu_at(menu.clone(), at, 200.0, window, cx);
                        } else {
                            chat.show_chat_menu_in_window_at(menu.clone(), at, 200.0, window, cx);
                        }
                    }),
                )
        });
        div()
            .id(gpui::SharedString::from(format!(
                "delivery-card:{prompt_id}"
            )))
            .role(gpui::Role::Status)
            .aria_label(format!("{title}. {sentence}"))
            .w(px(380.0 * s))
            .max_w(relative(0.8))
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
            .pl(px(12.0 * s))
            .pr(px(8.0 * s))
            .pt(px(8.0 * s))
            .pb(px(10.0 * s))
            .rounded(px(14.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .min_h(px(24.0 * s))
                    .child(
                        svg()
                            .path(icon)
                            .flex_shrink_0()
                            .size(px(14.0 * s))
                            .text_color(hue.unwrap_or(p.muted)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.0 * s))
                            .line_height(px(18.0 * s))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .child(title),
                    )
                    .children(more),
            )
            .child(
                div()
                    .pr(px(4.0 * s))
                    .text_size(px(12.5 * s))
                    .line_height(px(17.0 * s))
                    .text_color(p.card_muted)
                    .child(sentence),
            )
            .when(!buttons.is_empty(), |card| {
                card.child(
                    div()
                        .mt(px(4.0 * s))
                        .pr(px(4.0 * s))
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(6.0 * s))
                        .children(buttons),
                )
            })
            .into_any_element()
    }
}
