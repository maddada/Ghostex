//! Voice panel preview: the panel's frame (header, captions, buttons) drawn around the real talk
//! controls from `app/voice_agent/panel_controls.rs` (the hold button, the Space keycap and the
//! Stop talking strip). States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `idle` (a push-to-talk call,
//! Space free; the default), `holding` (Space held), `speaking` (the agent talks: the Stop talking
//! pill), `tap` (the too-short hint), `muted` (a muted push-to-talk call: Mute wins), `regular`
//! (a Regular call that listens: the hold button rests), `confirm` (a stop the agent is unsure
//! about: the confirm card for one session), `confirm-many` (the card for several). Pressing the
//! hold button holds it.
use super::rows;
use super::voice_confirm_card as confirm_card;
use super::voice_panel_controls::{self as controls, ControlColors, HoldLook, StripContent};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Hsla, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, div, px, rgba, svg,
};
use gpui_component::{h_flex, v_flex};

const ICON_MIC: &str = "titlebar/microphone.svg";
const ICON_MIC_OFF: &str = "titlebar/microphone-off.svg";
const ICON_HANG_UP: &str = "titlebar/phone-off.svg";
const ICON_TOOL: &str = "titlebar/tool.svg";

struct VoicePanelDemo {
    state: String,
    held: bool,
    light: bool,
}

impl VoicePanelDemo {
    fn colors(&self) -> ControlColors {
        let ink: Hsla = if self.light {
            gpui::black()
        } else {
            gpui::white()
        };
        ControlColors {
            ink,
            text: ink.opacity(0.90),
            muted: ink.opacity(0.55),
            border: ink.opacity(if self.light { 0.10 } else { 0.12 }),
            raised: ink.opacity(0.07),
            background: if self.light {
                rgba(0xfafafaf7).into()
            } else {
                rgba(0x1e1e21f7).into()
            },
        }
    }
}

fn icon(path: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    svg().size(px(size)).path(path).text_color(color)
}

fn button(
    icon_path: &'static str,
    label: &'static str,
    fill: Hsla,
    text: Hsla,
) -> impl IntoElement {
    h_flex()
        .h(px(28.0))
        .px(px(10.0))
        .gap(px(6.0))
        .items_center()
        .rounded(px(8.0))
        .bg(fill)
        .text_color(text)
        .text_size(px(12.5))
        .child(icon(icon_path, 14.0, text))
        .child(label)
}

impl Render for VoicePanelDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let state = self.state.as_str();
        let held = self.held || state == "holding";
        let muted = state == "muted";
        let regular = state == "regular";
        let (dot, label): (Hsla, &str) = if held {
            (rgba(0x38bdf8ff).into(), "Hearing you")
        } else {
            match state {
                "speaking" => (rgba(0xa78bfaff).into(), "Speaking"),
                "muted" => (colors.muted, "Muted"),
                "regular" => (rgba(0x4ade80ff).into(), "Listening"),
                _ => (rgba(0x4ade80ff).into(), "Hold to talk"),
            }
        };
        let space_key = if held {
            Some(true)
        } else if muted || regular {
            None
        } else {
            Some(false)
        };
        let look = if held {
            HoldLook::Held
        } else if muted || regular {
            HoldLook::Unavailable
        } else {
            HoldLook::Ready
        };
        let tooltip: SharedString = match state {
            "muted" => "Unmute to talk (or hold Space)".into(),
            "regular" => "Hold to talk while muted (or hold Space)".into(),
            _ => "Hold to talk (or hold Space)".into(),
        };
        let strip = match state {
            "speaking" => StripContent::StopTalking,
            "tap" => StripContent::Hint("Hold to talk, then let go to send".into()),
            "muted" => StripContent::Hint("Unmute to talk".into()),
            _ => StripContent::Empty,
        };
        let mut lines: Vec<gpui::AnyElement> = vec![
            div()
                .text_color(colors.muted)
                .child("You: What is the Find port doing?")
                .into_any_element(),
            h_flex()
                .gap(px(6.0))
                .text_size(px(12.0))
                .text_color(colors.muted)
                .child(icon(ICON_TOOL, 12.0, colors.muted))
                .child("Read \u{201c}Port the Find screen to Rust\u{201d}")
                .into_any_element(),
            div()
                .text_color(colors.text)
                .child("It finished: ranking passes and the phone screens are next. Want me to start them?")
                .into_any_element(),
        ];
        if held {
            lines.push(
                div()
                    .text_color(colors.text)
                    .child("Yes, start the phone screens and use the same\u{258d}")
                    .into_any_element(),
            );
        }
        if state == "speaking" {
            lines.push(
                div()
                    .text_color(colors.muted)
                    .child("You: Yes, start them.")
                    .into_any_element(),
            );
            lines.push(
                div()
                    .text_color(colors.text)
                    .child("Done. I asked it to build the phone screens on the same matcher, and it is starting with the list view now.")
                    .into_any_element(),
            );
        }
        if let Some(card) = demo_card(state) {
            lines.push(
                div()
                    .text_color(colors.muted)
                    .child("You: Stop the triage one.")
                    .into_any_element(),
            );
            lines.push(
                h_flex()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted)
                    .child(icon(ICON_TOOL, 12.0, colors.muted))
                    .child("Checking which session to stop")
                    .into_any_element(),
            );
            lines.push(confirm_card::confirm_card(
                &card,
                colors,
                |session, _, _| eprintln!("confirm answered: {session:?}"),
            ));
        }
        let panel = cx.entity().downgrade();
        let hold = controls::hold_button(look, tooltip, colors, move |_, cx| {
            let _ = panel.update(cx, |this, cx| {
                this.held = !this.held;
                cx.notify();
            });
        });
        v_flex()
            .size_full()
            .overflow_hidden()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(12.5))
            .child(
                h_flex()
                    .h(px(36.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .gap(px(8.0))
                    .items_center()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(div().size(px(9.0)).rounded_full().bg(dot))
                    .child(
                        h_flex()
                            .flex_1()
                            .gap(px(6.0))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(colors.text)
                                    .child(label),
                            )
                            .children(space_key.map(|lit| controls::keycap("Space", lit, colors))),
                    )
                    .child(div().text_color(colors.muted).child("Grok · Eve"))
                    .child(div().text_color(colors.muted).child("1×")),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .px(px(12.0))
                    .py(px(10.0))
                    .gap(px(8.0))
                    .children(lines),
            )
            .child(controls::status_strip(strip, colors, |_, _| {
                eprintln!("stop talking")
            }))
            .child(
                h_flex()
                    .h(px(44.0))
                    .flex_shrink_0()
                    .px(px(10.0))
                    .gap(px(6.0))
                    .items_center()
                    .border_t_1()
                    .border_color(colors.border)
                    .child(button(
                        if muted { ICON_MIC_OFF } else { ICON_MIC },
                        if muted { "Unmute" } else { "Mute" },
                        colors.ink.opacity(0.07),
                        colors.text,
                    ))
                    .child(hold)
                    .child(div().flex_1())
                    .child(button(
                        ICON_HANG_UP,
                        "Hang up",
                        rgba(0xdc2626ff).into(),
                        gpui::white(),
                    )),
            )
            .when(self.light, |this| this.text_color(gpui::black()))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = if demo.state.is_empty() {
        "idle".to_string()
    } else {
        demo.state.clone()
    };
    let light = super::env("GHOSTEX_NATIVE_MODAL_DEMO_THEME") == "light";
    let _ = super::open_modal_window(
        340.0,
        360.0,
        move |_, cx| {
            cx.new(|_| VoicePanelDemo {
                state,
                held: false,
                light,
            })
        },
        cx,
    );
}

/// The `confirm` frame gxserver sends for the demo's states, read the way the panel reads it.
fn demo_card(state: &str) -> Option<rows::ConfirmCard> {
    let choice = |session: &str, title: &str| serde_json::json!({"session": session, "title": title, "detail": "Ghostex · working"});
    let frame = match state {
        "confirm" => {
            serde_json::json!({"type": "confirm", "id": "c1", "title": "Stop Discord triage?",
            "choices": [choice("P1:S1", "Discord triage")], "confirmLabel": "Stop", "cancelLabel": "Cancel"})
        }
        "confirm-many" => {
            serde_json::json!({"type": "confirm", "id": "c1", "title": "Stop which session?",
            "choices": [choice("P1:S1", "Discord triage"), choice("P1:S2", "Sidebar triage fix")],
            "confirmLabel": "Stop", "cancelLabel": "Cancel"})
        }
        _ => return None,
    };
    rows::confirm_card(&frame)
}
