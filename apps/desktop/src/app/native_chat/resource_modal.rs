//! The Chat Resources modal: a question form (or chart) the reader clicked in the list, floating
//! over the chat so it can be answered, or its answer read, without scrolling to it. It is a chat
//! menu window (option_menu/pill_menu.rs) like the other floating panels, opened once the core says
//! the modal is open (`resourceModal` on the document, gx-chat-core questions/resource_modal.rs) and
//! closed with Escape or a click outside, which tells the core. The form is the chat card's own
//! (`question_form`), so picking, Other, the notes, Send, For later and Remove are the card's
//! actions on the card's draft: closing it half-filled loses nothing. "Go to chat" scrolls the chat
//! to the form, loading earlier messages first when it is older than the loaded rows.
//!
//! The user's words are on gx-chat-core's questions/resource_modal.rs. The phone shows the same in a
//! sheet (apps/mobile/app/src/chat/native/cards/ResourceSheet.tsx).

use super::side_question::side_button;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use serde_json::json;

/// The modal's id among the pill menus, and its trigger's (chat_pills.rs).
pub(super) const RESOURCE_MODAL_MENU: &str = "resourceModal";
pub(super) const RESOURCE_MODAL_TRIGGER: &str = "chat-resource-modal";

/// Wide enough for a form's choices to read, narrow enough to stay a card over the chat, and never
/// taller than this (unscaled px); a longer form scrolls inside.
pub(super) const RESOURCE_MODAL_WIDTH: f32 = 540.0;
const MAX_HEIGHT: f32 = 560.0;
const HEADER_HEIGHT: f32 = 36.0;
const GAP: f32 = 8.0;

/// A rough height for the modal's content (unscaled px), so the window opens about the size of its
/// form; the form scrolls inside when the estimate is short.
fn estimated_height(modal: &serde_json::Value, form: &serde_json::Value) -> f32 {
    let body = if modal["loading"] == true || !text(modal, "missing").is_empty() {
        90.0
    } else if modal["kind"] == "chart" {
        360.0
    } else {
        let questions: f32 = form["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|question| {
                let choices = question["choices"].as_array().map_or(0, Vec::len) as f32;
                52.0 + choices * 40.0 + if question["text"] == true { 56.0 } else { 80.0 }
            })
            .sum();
        // The card's own head and its footer buttons.
        questions + 120.0
    };
    (HEADER_HEIGHT + GAP + body).clamp(160.0, MAX_HEIGHT)
}

impl NativeChatView {
    /// Opens the floating window once the core says the modal is open, and not again while a close
    /// the reader made is on its way to the core.
    pub(super) fn sync_resource_modal_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.snapshot["resourceModal"].is_object();
        if !open {
            self.resource_modal_closing = false;
            return;
        }
        let shown = self
            .option_menu
            .as_ref()
            .is_some_and(|menu| menu.read(cx).pill() == Some(RESOURCE_MODAL_TRIGGER));
        if shown || self.resource_modal_pending || self.resource_modal_closing {
            return;
        }
        self.resource_modal_pending = true;
        cx.defer_in(window, |this, window, cx| {
            this.resource_modal_pending = false;
            this.open_resource_modal_menu(window, cx);
        });
    }

    /// Centred over the chat box, as wide as a form needs.
    fn open_resource_modal_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.composer_bounds.get();
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let Some(height) = self.pill_menu_height(RESOURCE_MODAL_MENU) else {
            return;
        };
        if composer.size.width <= px(0.0) {
            return;
        }
        let width = px(RESOURCE_MODAL_WIDTH * scale);
        let centre = composer.origin.x + composer.size.width / 2.0;
        let trigger = gpui::Bounds::new(
            gpui::point(centre - width / 2.0, composer.origin.y),
            gpui::size(width, px(0.0)),
        );
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": RESOURCE_MODAL_MENU, "height": height}})],
            trigger,
            RESOURCE_MODAL_WIDTH,
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(RESOURCE_MODAL_TRIGGER));
        }
    }

    /// The modal's window went away (Escape, a click outside, or the core closed it): the core
    /// forgets it, and the modal's text boxes go with the window. The picks stay in the card's
    /// draft, so a half-filled answer is not lost.
    pub(super) fn resource_modal_closed(&mut self, cx: &mut Context<Self>) {
        self.modal_form_inputs.clear();
        if self.snapshot["resourceModal"].is_object() {
            self.resource_modal_closing = true;
            self.invoke(json!({"type": "closeResource"}), cx);
        }
    }

    /// The modal's content height (unscaled), or `None` once the core closed it.
    pub(super) fn resource_modal_height(&self) -> Option<f32> {
        let modal = &self.snapshot["resourceModal"];
        if !modal.is_object() {
            return None;
        }
        Some(estimated_height(
            modal,
            &self.snapshot["questionForms"][text(modal, "id")],
        ))
    }

    /// The modal: its title and the Go to chat button, then the form (or chart) under it.
    pub(super) fn resource_modal_body(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let modal = self.snapshot["resourceModal"].clone();
        if !modal.is_object() {
            return None;
        }
        let s = p.scale;
        let id = text(&modal, "id");
        let kind = text(&modal, "kind");
        let title = text(&modal, "title");
        let missing = text(&modal, "missing");
        let going = modal["going"] == true;
        let loading = modal["loading"] == true;
        let chat = cx.weak_entity();
        let go_id = id.clone();
        let go_to_chat = side_button(
            "chat-resource-go",
            Some("titlebar/arrow-up.svg"),
            if going { "Loading…" } else { "Go to chat" },
            None,
            going || loading,
            Some("Scroll the chat to this"),
            p,
            move |_, _, app| {
                let _ = chat.update(app, |this, cx| {
                    this.invoke(json!({"type": "goToResource", "id": go_id}), cx)
                });
            },
        );
        let header = div()
            .flex_shrink_0()
            .h(px(HEADER_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(4.0 * s))
            .child(
                svg()
                    .path(if kind == "chart" {
                        "titlebar/chart-bar.svg"
                    } else {
                        "titlebar/list-check.svg"
                    })
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(if title.is_empty() {
                        "Resource".to_string()
                    } else {
                        title
                    }),
            )
            .child(go_to_chat);
        let content = if loading {
            self.modal_note("Reading it from the chat…", p)
        } else if !missing.is_empty() {
            self.modal_note(&missing, p)
        } else if kind == "chart" {
            let source = text(&modal, "source");
            let fence = format!("```visual\n{source}\n```");
            self.visual_card(format!("resource-modal-chart:{id}"), &fence, p, cx)
        } else {
            self.sync_modal_form_inputs(&id, window, cx);
            self.form_in_modal.set(true);
            let card = self.question_form(&id, "", p, cx);
            self.form_in_modal.set(false);
            card
        };
        Some(
            div()
                .id("chat-resource-modal")
                .role(gpui::Role::Dialog)
                .aria_label("Resource")
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(GAP * s))
                .child(header)
                .child(
                    div()
                        .id("chat-resource-modal-content")
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .overflow_y_scroll()
                        .child(content),
                )
                .into_any_element(),
        )
    }

    fn modal_note(&self, note: &str, p: &ChatAppearance) -> AnyElement {
        div()
            .px(px(4.0 * p.scale))
            .py(px(8.0 * p.scale))
            .text_size(px(12.5 * p.scale))
            .text_color(p.card_muted)
            .child(note.to_string())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_modal_is_sized_to_its_form_and_never_taller_than_the_cap() {
        let loading = json!({"loading": true});
        assert!(estimated_height(&loading, &json!(null)) < 200.0);
        let short = json!({"questions": [{"choices": [{}, {}]}]});
        let long = json!({"questions": (0..12).map(|_| json!({"choices": [{}, {}, {}]})).collect::<Vec<_>>()});
        let short_h = estimated_height(&json!({"kind": "form"}), &short);
        assert!(short_h > 200.0 && short_h < MAX_HEIGHT, "{short_h}");
        assert_eq!(
            estimated_height(&json!({"kind": "form"}), &long),
            MAX_HEIGHT
        );
        assert!(estimated_height(&json!({"kind": "chart"}), &json!(null)) <= MAX_HEIGHT);
    }
}
