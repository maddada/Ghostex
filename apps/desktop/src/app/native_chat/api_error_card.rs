//! The card a failed API call becomes: the tone card with the failure's glyph, a plain sentence
//! with the agent's own words behind Details, and the retry gxserver runs for it. Which rows get it, the titles, the status and the
//! buttons come from the chat core (packages/gx-chat-core/src/transcript/api_errors.rs, the
//! 2026-10-15 decision there); this file only lays it out. The phone draws the same card in
//! apps/mobile/app/src/chat/native/transcript/ApiErrorCard.tsx.

use super::tone_card::{ToneButton, ToneCardParts, ToneLook, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;
use std::time::Duration;

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// "12s", "2m", "1m 30s".
fn countdown(seconds: i64) -> String {
    match (seconds / 60, seconds % 60) {
        (0, seconds) => format!("{seconds}s"),
        (minutes, 0) => format!("{minutes}m"),
        (minutes, seconds) => format!("{minutes}m {seconds}s"),
    }
}

impl NativeChatView {
    /// A failed-call row: the open card when it is the failure the core says nothing has answered
    /// yet, otherwise one quiet line.
    pub(super) fn api_error_row(
        &mut self,
        id: &str,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = self.snapshot["apiErrorCard"].clone();
        if card["messageId"].as_str() == Some(id) {
            return self.api_error_card(&card, p, cx);
        }
        self.api_error_note(&message["apiError"], &message["time"], p)
    }

    /// The card for a failure the agent keeps no record of (Codex shows it only on screen), drawn
    /// above the composer where its screen notice would have been.
    pub(super) fn render_unrecorded_api_error(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = self.snapshot["apiErrorCard"].clone();
        (card.is_object() && card["messageId"].is_null()).then(|| self.api_error_card(&card, p, cx))
    }

    fn api_error_note(&self, error: &Value, time: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        div()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .min_w_0()
            .py(px(2.0 * s))
            .text_size(px(12.0 * s))
            .line_height(px(16.0 * s))
            .text_color(p.muted)
            .child(
                gpui::svg()
                    .path(format!("titlebar/{}.svg", text(error, "icon")))
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(div().flex_shrink_0().child(text(error, "title")))
            .when(time["label"].is_string(), |row| {
                row.child(div().flex_shrink_0().child("\u{b7}"))
                    .child(div().flex_shrink_0().child(text(time, "label")))
            })
            .into_any_element()
    }

    fn api_error_card(
        &self,
        card: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let base = tone_look(card["tone"].as_str().unwrap_or("warning"), p);
        let look = ToneLook {
            hue: base.hue,
            icon: format!("titlebar/{}.svg", text(card, "icon")),
        };
        let status = &card["status"];
        let mut label = text(status, "label");
        if let Some(to) = status["countdownTo"].as_i64() {
            let remaining = (to - now_ms() + 999) / 1000;
            label = if remaining > 0 {
                format!("{label} {}", countdown(remaining))
            } else {
                "Retrying now\u{2026}".to_string()
            };
            self.book_api_error_tick(cx);
        }
        let note = text(status, "note");
        // The retry line sits in the body so Retry now and Stop retrying get the footer row to themselves (Q1118).
        let status_row = (!label.is_empty()).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .min_w_0()
                .text_size(px(12.0 * s))
                .text_color(p.foreground)
                .child(div().truncate().child(label))
                .when(!note.is_empty(), |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.muted)
                            .child(format!("\u{b7} {note}")),
                    )
                })
                .into_any_element()
        });
        let actions = card["actions"].as_array().cloned().unwrap_or_default();
        let buttons = actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let command = action["command"].clone();
                let kind = if index == 0 {
                    ToneButton::Primary
                } else {
                    ToneButton::Secondary
                };
                let icon = (action["id"] == "retryNow").then_some("titlebar/refresh.svg");
                self.tone_button(
                    gpui::ElementId::Name(format!("api-error-{}", text(action, "id")).into()),
                    &text(action, "label"),
                    icon,
                    kind,
                    None,
                    false,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
                .into_any_element()
            })
            .collect::<Vec<_>>();
        div()
            .id("api-error-card")
            .role(gpui::Role::Alert)
            .aria_label(text(card, "title"))
            .w_full()
            .child(
                self.tone_card(
                    ToneCardParts {
                        look,
                        title: text(card, "title"),
                        detail: String::new(),
                        trailing: Vec::new(),
                        body: [div()
                            .text_size(px(12.0 * s))
                            .line_height(px(17.0 * s))
                            .text_color(p.muted)
                            .child(text(card, "body"))
                            .into_any_element()]
                        .into_iter()
                        .chain(status_row)
                        .chain(self.details_toggle(
                            &format!("api-error:{}", text(card, "messageId")),
                            vec![text(card, "detail")],
                            p,
                            cx,
                        ))
                        .collect(),
                        footer_start: Vec::new(),
                        footer_end: buttons,
                    },
                    None,
                    None,
                    p,
                ),
            )
            .into_any_element()
    }

    /// One redraw a second while the countdown runs, never more than one booked at a time.
    fn book_api_error_tick(&self, cx: &mut Context<Self>) {
        if self.api_error_tick.replace(true) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update(cx, |this, cx| {
                this.api_error_tick.set(false);
                cx.notify();
            });
        })
        .detach();
    }
}
