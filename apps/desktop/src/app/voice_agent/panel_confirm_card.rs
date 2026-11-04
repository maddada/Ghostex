//! The voice panel's confirm card: gxserver puts it up when the voice agent is unsure which session
//! the user wants stopped (`rows.rs` reads the frame). One session: its name and state, Stop and
//! Cancel. Several: one row per session with its own Stop, then Cancel. Saying "yes" (or which
//! one) does the same as the buttons. Depends only on gpui, gpui-component and its host's
//! `controls` and `rows`, so the preview binary (`src/bin/native_modal_demo/voice_panel.rs`) draws
//! the same card.
//!
//! CDXC:VoiceMode 2026-10-30 SEE-ALSO: server/src/voice_agent/control.rs (the user's decision: a stop asks first only when the voice agent isn't sure which session is meant, with a card like option B of the mockups).

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Hsla, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, px, rgba,
};
use gpui_component::{h_flex, v_flex};

use super::controls::ControlColors;
use super::rows::ConfirmCard;

/// `on_answer` gets the session of the pressed Stop, or `None` for Cancel.
pub(crate) fn confirm_card(
    card: &ConfirmCard,
    colors: ControlColors,
    on_answer: impl Fn(Option<String>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_answer: Rc<dyn Fn(Option<String>, &mut Window, &mut App)> = Rc::new(on_answer);
    let single = card.choices.len() == 1;
    let stop_button = |at: usize, session: &str| {
        card_button(
            SharedString::from(format!("voice-confirm-{}-stop-{at}", card.id)),
            card.confirm_label.clone(),
            true,
            colors,
            Some(session.to_string()),
            on_answer.clone(),
        )
    };
    let cancel = card_button(
        SharedString::from(format!("voice-confirm-{}-cancel", card.id)),
        card.cancel_label.clone(),
        false,
        colors,
        None,
        on_answer.clone(),
    );
    let detail = |text: &str| {
        div()
            .text_size(px(11.0))
            .text_color(colors.muted)
            .truncate()
            .child(text.to_string())
    };
    let hint = if single {
        format!("Say yes or press {}.", card.confirm_label)
    } else {
        format!("Say which one, or press its {}.", card.confirm_label)
    };
    v_flex()
        .id(SharedString::from(format!("voice-confirm-{}", card.id)))
        .role(gpui::Role::Group)
        .aria_label(SharedString::from(card.title.clone()))
        .p(px(10.0))
        .gap(px(8.0))
        .rounded(px(10.0))
        .bg(colors.ink.opacity(0.05))
        .border_1()
        .border_color(colors.border)
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(colors.text)
                .child(card.title.clone()),
        )
        .map(|this| {
            if single {
                let choice = &card.choices[0];
                return this
                    .when(!choice.detail.is_empty(), |this| {
                        this.child(detail(&choice.detail))
                    })
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .child(stop_button(0, &choice.session))
                            .child(cancel),
                    );
            }
            this.children(card.choices.iter().enumerate().map(|(at, choice)| {
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(colors.text)
                                    .truncate()
                                    .child(choice.title.clone()),
                            )
                            .when(!choice.detail.is_empty(), |this| {
                                this.child(detail(&choice.detail))
                            }),
                    )
                    .child(stop_button(at, &choice.session))
            }))
            .child(h_flex().child(cancel))
        })
        .child(
            div()
                .text_size(px(11.0))
                .italic()
                .text_color(colors.muted)
                .child(hint),
        )
        .into_any_element()
}

fn card_button(
    id: SharedString,
    label: String,
    danger: bool,
    colors: ControlColors,
    session: Option<String>,
    on_answer: Rc<dyn Fn(Option<String>, &mut Window, &mut App)>,
) -> AnyElement {
    let (fill, hover, foreground): (Hsla, Hsla, Hsla) = if danger {
        (
            rgba(0xdc2626ff).into(),
            rgba(0xef4444ff).into(),
            gpui::white(),
        )
    } else {
        (
            colors.ink.opacity(0.07),
            colors.ink.opacity(0.13),
            colors.text,
        )
    };
    let on_click = {
        let on_answer = on_answer.clone();
        let session = session.clone();
        move |_: &MouseUpEvent, window: &mut Window, cx: &mut App| {
            window.prevent_default();
            cx.stop_propagation();
            on_answer(session.clone(), window, cx);
        }
    };
    h_flex()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .h(px(28.0))
        .px(px(12.0))
        .items_center()
        .rounded(px(8.0))
        .bg(fill)
        .text_color(foreground)
        .text_size(px(12.5))
        .cursor_default()
        .hover(move |this| this.bg(hover))
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_mouse_up(MouseButton::Left, on_click)
        .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
            on_answer(session.clone(), window, cx);
        })
        .child(label)
        .into_any_element()
}
