//! The voice panel's talk controls: the icon-only hold button beside Mute, the Space keycap in the
//! header, and the strip between the transcript and the buttons that shows Stop talking and the
//! short hints ("Unmute to talk"). Depends only on gpui and gpui-component, so the preview binary
//! (`src/bin/native_modal_demo/voice_panel.rs`) draws the same controls.
//!
//! CDXC:VoiceMode 2026-10-24 DECISION: User, on the push-to-talk page: "i want to not have buttons change when "stop talking" appears while agent is talking. we need to show that as a pill bottom centered below the chat but above the buttons at the bottom that exist now (shouldn't shift things) but this new pill shouldn't cover the chat text also". Stop talking is a pill centred in a strip of its own, always kept between the transcript and the buttons during a call, so it covers no caption and moves no button; the buttons stay Mute, the hold button and Hang up for the whole call. SEE-ALSO: the phone's `apps/mobile/app/src/voice/VoiceStopPill.tsx`.
//! CDXC:VoiceMode 2026-10-24 DECISION: User: "The button on the computer is the tiny icon-only one beside Mute" (the push-to-talk page). The hold button is a 28px icon-only button right after Mute, with a waveform icon so it doesn't read as Mute's microphone; its tooltip names it and its keys.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Hsla, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, px, rgba, svg,
};
use gpui_component::h_flex;

pub(crate) const ICON_HOLD_TO_TALK: &str = "titlebar/hold-to-talk.svg";
pub(crate) const ICON_STOP_TALKING: &str = "titlebar/player-stop.svg";
/// The strip between the transcript and the buttons, kept for the whole call so nothing moves when
/// Stop talking or a hint shows in it.
pub(crate) const STATUS_STRIP_HEIGHT: f32 = 32.0;

/// The colours the controls take from the panel.
#[derive(Clone, Copy)]
pub(crate) struct ControlColors {
    pub(crate) ink: Hsla,
    pub(crate) text: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) border: Hsla,
    pub(crate) background: Hsla,
    /// A neutral button's fill: the frosted dialogs' raised wash under window glass.
    pub(crate) raised: Hsla,
}

/// How the hold button looks, from what holding would do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoldLook {
    /// Holding talks (a push-to-talk call) or unmutes (a muted Regular call).
    Ready,
    /// It is held.
    Held,
    /// Holding does nothing now: a muted push-to-talk call (Mute wins), a Regular call that
    /// already listens, or a call still connecting.
    Unavailable,
}

/// What the strip between the transcript and the buttons shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StripContent {
    Empty,
    /// The agent is speaking: a Stop talking pill.
    StopTalking,
    /// A short note ("Hold to talk, then let go to send", "Unmute to talk").
    Hint(SharedString),
}

fn icon(path: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    svg().size(px(size)).path(path).text_color(color)
}

/// The icon-only hold button. `on_press` runs on the mouse-down; the release is the panel's own
/// window-wide mouse-up, so letting go anywhere ends the hold.
pub(crate) fn hold_button(
    look: HoldLook,
    tooltip: SharedString,
    colors: ControlColors,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let blue: Hsla = rgba(0x2563ebff).into();
    let (fill, hover, foreground): (Hsla, Hsla, Hsla) = match look {
        HoldLook::Ready => (blue, rgba(0x3b82f6ff).into(), gpui::white()),
        HoldLook::Held => (
            rgba(0x38bdf8ff).into(),
            rgba(0x38bdf8ff).into(),
            rgba(0x0b1220ff).into(),
        ),
        HoldLook::Unavailable => (colors.raised, colors.raised, colors.muted.opacity(0.7)),
    };
    let held = look == HoldLook::Held;
    let label = tooltip.clone();
    div()
        .id("voice-panel-hold-to-talk")
        .role(gpui::Role::Button)
        .aria_label(label)
        .size(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        // The panel's other buttons' radius (`RESOURCE_CONTROL_RADIUS`, which this file can't name
        // because the preview binary compiles it too).
        .rounded(px(5.0))
        .bg(fill)
        .cursor_default()
        .when(look == HoldLook::Unavailable, |this| {
            this.border_1().border_color(colors.border)
        })
        .when(held, |this| {
            this.border_2()
                .border_color(rgba(0x38bdf866))
                .shadow(vec![gpui::BoxShadow {
                    color: rgba(0x38bdf840).into(),
                    offset: gpui::point(px(0.0), px(0.0)),
                    blur_radius: px(8.0),
                    spread_radius: px(2.0),
                    inset: false,
                }])
        })
        .hover(move |this| this.bg(hover))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            on_press(window, cx);
        })
        .on_mouse_up(MouseButton::Left, |_: &MouseUpEvent, window, _| {
            window.prevent_default();
        })
        .child(icon(ICON_HOLD_TO_TALK, 15.0, foreground))
        .into_any_element()
}

/// The header's key hint: "Space" while Space would hold Talk, lit while it is held.
pub(crate) fn keycap(text: &'static str, lit: bool, colors: ControlColors) -> AnyElement {
    let blue: Hsla = rgba(0x38bdf8ff).into();
    div()
        .flex_none()
        .h(px(16.0))
        .px(px(5.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .border_1()
        .border_b_2()
        .border_color(if lit {
            blue.opacity(0.7)
        } else {
            colors.border
        })
        .when(lit, |this| this.bg(blue.opacity(0.12)))
        .text_size(px(10.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(if lit { blue } else { colors.muted })
        .child(text)
        .into_any_element()
}

/// The strip between the transcript and the buttons, always `STATUS_STRIP_HEIGHT` tall during a
/// call; Stop talking and the hints show centred in it.
pub(crate) fn status_strip(
    content: StripContent,
    colors: ControlColors,
    on_stop: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let pill = match content {
        StripContent::Empty => None,
        StripContent::StopTalking => Some(
            h_flex()
                .id("voice-panel-stop-talking")
                .role(gpui::Role::Button)
                .aria_label(SharedString::new_static("Stop talking"))
                .h(px(24.0))
                .px(px(10.0))
                .gap(px(5.0))
                .items_center()
                .rounded_full()
                .border_1()
                .border_color(colors.border)
                .bg(colors.background)
                .text_size(px(12.0))
                .text_color(colors.text)
                .cursor_default()
                .hover(move |this| this.bg(colors.ink.opacity(0.10)))
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_mouse_up(MouseButton::Left, move |_: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    on_stop(window, cx);
                })
                .child(icon(ICON_STOP_TALKING, 12.0, colors.text))
                .child("Stop talking")
                .into_any_element(),
        ),
        StripContent::Hint(text) => Some(
            h_flex()
                .h(px(24.0))
                .px(px(10.0))
                .items_center()
                .rounded_full()
                .bg(colors.ink.opacity(0.06))
                .text_size(px(12.0))
                .text_color(colors.text)
                .child(text)
                .into_any_element(),
        ),
    };
    h_flex()
        .h(px(STATUS_STRIP_HEIGHT))
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .children(pill)
        .into_any_element()
}
