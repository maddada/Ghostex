//! What a needs-you item offers, drawn once for every place it shows (the dock above the chat box,
//! the Work panel's and Columns' opened cards, cards under replies): its answers as the dock's
//! filled answer buttons, the recommended one starred, and the actions every item repeats (Reply,
//! Open, Show in chat, Dismiss, Snooze, Mention) as quiet icon buttons with a tooltip naming each.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, of an opened Work panel card (a "Yes | No" outlined pill, then outlined Reply, Show in
//! chat, Dismiss, Snooze and Mention buttons) beside the dock's filled answers: "bro these buttons
//! are ugly. please make the ones always repeated just icon buttons with a tooltip and also let's
//! please use the same look for the buttons in the list in the expanded cards and here. the buttons
//! look different between the 2 now". One component for each, so the places cannot drift apart
//! again; the fills are the neutral glass-safe ones (no black on glass).

use super::{appearance::ChatAppearance, state::NativeChatView, tone_card::ToneButton};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

/// An icon button's box, and the icon in it (unscaled).
const ICON_BOX: f32 = 28.0;
const ICON_SIZE: f32 = 14.0;

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

impl NativeChatView {
    /// An ask's answers, the dock's look everywhere: one filled button each, the recommended one
    /// ending in its star, the one picked here carrying a check. `busy`: an answer is being sent,
    /// so the others wait.
    pub(super) fn work_answer_buttons(
        &self,
        key: &str,
        answers: &[Value],
        busy: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                let label = text(answer, "label");
                let picked = answer["picked"] == true;
                let locked = answer["locked"] == true;
                let command = answer["command"].clone();
                self.tone_button(
                    gpui::SharedString::from(format!("work-answer:{key}:{index}")),
                    &label,
                    picked.then_some("titlebar/check.svg"),
                    ToneButton::Secondary,
                    None,
                    (busy || locked) && !picked,
                    p,
                )
                .when(answer["recommended"] == true, |button| {
                    button
                        .aria_label(format!("{label}, Recommended"))
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Recommended").build(window, cx)
                        })
                        .child(
                            svg()
                                .path("titlebar/star-filled.svg")
                                .size(px(10.0 * s))
                                .flex_shrink_0()
                                .text_color(super::ask_card::ask_color(p).opacity(0.75)),
                        )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if !busy && !locked && command.is_object() {
                        this.invoke(command.clone(), cx);
                    }
                }))
                .into_any_element()
            })
            .collect()
    }

    /// One quiet icon button: the icon in the muted colour, brighter with a soft fill on hover, its
    /// name in the tooltip and to a screen reader.
    fn work_icon_button(
        &self,
        id: String,
        icon: &'static str,
        label: &'static str,
        on: bool,
        p: &ChatAppearance,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(gpui::SharedString::from(id))
            .group("work-action")
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .size(px(ICON_BOX * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0 * s))
            .when(on, |this| this.bg(p.foreground.opacity(0.08)))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.08)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(ICON_SIZE * s))
                    .text_color(if on { p.foreground } else { p.muted })
                    .group_hover("work-action", |style| style.text_color(p.foreground)),
            )
    }

    /// The actions an item repeats, as one row of icon buttons: Reply (`actions.reply`, the reply
    /// bubble), Open, Show in chat, Dismiss, Snooze (which opens its three lengths beside it) and
    /// Mention (`mention`, the item's reference for the chat box). Only those it offers show;
    /// `None` when it offers none.
    pub(super) fn work_action_icons(
        &self,
        key: &str,
        actions: &Value,
        mention: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let mut row = div().flex().flex_wrap().items_center().gap(px(2.0 * s));
        let mut any = false;
        let reply = actions["reply"].clone();
        if reply.is_object() {
            any = true;
            row = row.child(
                self.work_icon_button(
                    format!("work-reply:{key}"),
                    "titlebar/arrow-back-up.svg",
                    "Reply",
                    false,
                    p,
                )
                .on_click(cx.listener(
                    move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let at = event.position();
                        let command = json!({
                            "question": reply.clone(),
                            "at": [f32::from(at.x), f32::from(at.y)],
                        });
                        this.open_reply_popover(&command, window, cx);
                    },
                )),
            );
        }
        for (field, icon, label) in [
            ("open", "titlebar/arrow-up-right.svg", "Open"),
            ("showInChat", "titlebar/focus-2.svg", "Show in chat"),
            ("dismiss", "titlebar/x.svg", "Dismiss"),
        ] {
            let command = actions[field].clone();
            if command.is_object() {
                any = true;
                row = row.child(
                    self.work_icon_button(format!("work-{field}:{key}"), icon, label, false, p)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.invoke(command.clone(), cx);
                        })),
                );
            }
        }
        let snooze: Vec<Value> = actions["snooze"].as_array().cloned().unwrap_or_default();
        let snooze_key = format!("work-snooze-open:{key}");
        let snooze_open = self.expanded.contains(&snooze_key);
        if !snooze.is_empty() {
            any = true;
            let toggle = snooze_key.clone();
            row = row.child(
                self.work_icon_button(
                    format!("work-snooze:{key}"),
                    "titlebar/alarm.svg",
                    "Snooze",
                    snooze_open,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if !this.expanded.remove(&toggle) {
                        this.expanded.insert(toggle.clone());
                    }
                    cx.notify();
                })),
            );
        }
        if !mention.is_empty() {
            any = true;
            let mention = mention.to_string();
            row = row.child(
                self.work_icon_button(
                    format!("work-mention:{key}"),
                    "titlebar/message-plus.svg",
                    "Mention",
                    false,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.insert_board_reference(&mention, cx);
                })),
            );
        }
        // Snooze open: its three lengths after the icons, as small answer-look buttons.
        if snooze_open {
            for (index, choice) in snooze.iter().enumerate() {
                let command = choice["command"].clone();
                let close = snooze_key.clone();
                row = row.child(
                    self.tone_button(
                        gpui::SharedString::from(format!("work-snooze:{key}:{index}")),
                        &text(choice, "label"),
                        None,
                        ToneButton::Secondary,
                        None,
                        false,
                        p,
                    )
                    .ml(px(4.0 * s))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.expanded.remove(&close);
                        this.invoke(command.clone(), cx);
                    })),
                );
            }
        }
        any.then(|| row.into_any_element())
    }
}
