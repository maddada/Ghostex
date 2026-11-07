//! Ghostex Collab above the chat box (`collab` in the chat core's document,
//! `extras/collab.rs`): the owner's "Message from <guest>" cards with Decline / Edit / Send, a
//! guest's own messages waiting for the owner, and who is typing. The desktop and the web build
//! (owner and guest pages alike) draw it from this file.
//!
//! SEE-ALSO: packages/gx-chat-core/src/extras/collab.rs, server/src/server/collab_http.rs.

use super::tone_card::{ToneButton, ToneCardParts, ToneLook, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::{Value, json};

impl NativeChatView {
    /// This page is a Ghostex Collab guest's: only the shared chat, nothing the owner answers.
    pub(crate) fn collab_guest(&self) -> bool {
        self.snapshot["collab"]["role"] == "guest"
    }

    fn resolve_collab_message(&mut self, id: &str, action: &str, cx: &mut Context<Self>) {
        self.invoke(
            json!({"type": "resolveCollabMessage", "heldId": id, "action": action}),
            cx,
        );
    }

    /// The guest's line under their chat box ("Message as Alper. Mohamad approves each
    /// message."), or `None` off a guest's page.
    pub(super) fn collab_guest_note(&self) -> Option<String> {
        self.snapshot["collab"]["guestNote"]
            .as_str()
            .filter(|note| !note.is_empty())
            .map(str::to_string)
    }

    /// The cards, waiting rows and typing line, or `None` while the chat is not shared.
    pub(super) fn render_collab(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let collab = self
            .snapshot
            .get("collab")
            .filter(|c| c["shared"] == true)?
            .clone();
        let s = p.scale;
        let mut column = div().flex().flex_col().gap(px(8.0 * s)).w_full();
        let mut any = false;
        for card in collab["cards"].as_array().into_iter().flatten() {
            let pictures = self.collab_pictures(card, p, cx);
            column = column.child(self.collab_card(card, pictures, p, cx));
            any = true;
        }
        for row in collab["waiting"].as_array().into_iter().flatten() {
            let pictures = self.collab_pictures(row, p, cx);
            column = column.child(self.collab_waiting_row(row, pictures, p, cx));
            any = true;
        }
        if let Some(typing) = collab["typing"].as_str().filter(|line| !line.is_empty()) {
            column = column.child(
                div()
                    .id("collab-typing")
                    .role(gpui::Role::Status)
                    .aria_label(typing.to_string())
                    .px(px(4.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .child(format!("{typing}\u{2026}")),
            );
            any = true;
        }
        any.then(|| column.into_any_element())
    }

    /// A held message's pictures, as the chat draws a user turn's own.
    fn collab_pictures(
        &mut self,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let images = row["images"]
            .as_array()
            .filter(|images| !images.is_empty())?;
        self.user_image_thumbnails(
            &json!({"id": format!("collab:{}", text(row, "id")), "images": images}),
            p,
            cx,
        )
    }

    fn collab_card(
        &self,
        card: &Value,
        pictures: Option<AnyElement>,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(card, "id");
        let busy = card["busy"] == true;
        let look = ToneLook {
            icon: "titlebar/users.svg".to_string(),
            ..tone_look("info", p)
        };
        let button =
            |key: &'static str, label: &'static str, kind: ToneButton, cx: &mut Context<Self>| {
                let held = id.clone();
                self.tone_button(
                    gpui::SharedString::from(format!("collab-{key}:{id}")),
                    label,
                    None,
                    kind,
                    None,
                    busy,
                    p,
                )
                .when(!busy, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.resolve_collab_message(&held, key, cx)
                    }))
                })
                .into_any_element()
            };
        let decline = button("decline", "Decline", ToneButton::Secondary, cx);
        let edit = button("edit", "Edit", ToneButton::Secondary, cx);
        let send = button("send", "Send", ToneButton::Primary, cx);
        self.tone_card(
            ToneCardParts {
                look,
                title: text(card, "title"),
                detail: String::new(),
                trailing: Vec::new(),
                body: vec![
                    div().children(pictures).into_any_element(),
                    // A guest's answers to a form or page show as the answers card the
                    // transcript draws for them, not as the message that carries them.
                    div()
                        .w_full()
                        .min_w_0()
                        .when(card["answers"].is_object(), |slot| {
                            slot.child(self.page_answers_card(
                                &card["answers"],
                                &Value::Null,
                                p,
                                cx,
                            ))
                        })
                        .into_any_element(),
                    div()
                        .w_full()
                        .min_w_0()
                        .text_size(px(13.5 * s))
                        .text_color(p.foreground)
                        .whitespace_normal()
                        .child(text(card, "text"))
                        .into_any_element(),
                ],
                footer_start: Vec::new(),
                footer_note: None,
                footer_end: vec![decline, edit, send],
            },
            None,
            None,
            p,
        )
    }

    fn collab_waiting_row(
        &self,
        row: &Value,
        pictures: Option<AnyElement>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let words = text(row, "text");
        div()
            .id(gpui::SharedString::from(format!(
                "collab-waiting:{}",
                text(row, "id")
            )))
            .role(gpui::Role::Article)
            .aria_label(format!("{}: {}", text(row, "label"), text(row, "text")))
            .w_full()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(3.0 * s))
            .child(
                div()
                    .text_size(px(11.5 * s))
                    .text_color(p.muted)
                    .child(text(row, "label")),
            )
            .children(pictures)
            // Answers to a form or page wait as their answers card (gx-chat-core extras/collab.rs).
            .when(row["answers"].is_object(), |column| {
                column.child(div().w_full().child(self.page_answers_card(
                    &row["answers"],
                    &Value::Null,
                    p,
                    cx,
                )))
            })
            .when(!words.is_empty(), |column| {
                column.child(
                    div()
                        .max_w(relative_width())
                        .px(px(12.0 * s))
                        .py(px(8.0 * s))
                        .rounded(px(12.0 * s))
                        .border_1()
                        .border_dashed()
                        .border_color(p.muted.opacity(0.45))
                        .text_size(px(13.5 * s))
                        .text_color(p.foreground.opacity(0.85))
                        .whitespace_normal()
                        .child(words.clone()),
                )
            })
            .into_any_element()
    }
}

fn relative_width() -> gpui::Length {
    gpui::relative(0.85).into()
}
