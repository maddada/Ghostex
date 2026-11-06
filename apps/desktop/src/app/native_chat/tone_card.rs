//! The tone card the notices and approvals above the composer are drawn in: a faint wash of the
//! card's tone, a tinted icon tile beside a title and a muted detail line, the body, and a footer
//! with one primary action and neutral filled secondary buttons.
//!
//! The tone comes from the chat core (`tone` on `terminalNotice`, always `approval` for an
//! approval card); its hue, glyph and the shell's measurements are the shared
//! `packages/gx-chat-core/visual/notice-tone.json`, which the phone's `ToneCard.tsx` mirrors.
//! The decision is on `notice_tone` in `packages/gx-chat-core/src/questions/notice_tone.rs`.

use super::disclosure_motion::{FoldFrame, measured, motion_clip};
use super::{
    appearance::{ChatAppearance, ChatButtonFills, ChatFilledButton as _},
    state::NativeChatView,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
struct ToneEntry {
    icon: String,
    dark: Option<String>,
    light: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Shell {
    radius: f32,
    pad_x: f32,
    pad_y: f32,
    tile: f32,
    tile_radius: f32,
    tile_icon: f32,
    header_gap: f32,
    title_size: f32,
    title_line_height: f32,
    detail_size: f32,
    detail_line_height: f32,
    wash_dark: f32,
    wash_light: f32,
    tile_fill: f32,
    tile_ring: f32,
    footer_shade_dark: f32,
    footer_shade_light: f32,
    button_height: f32,
    button_radius: f32,
    button_text_size: f32,
}

#[derive(Deserialize)]
struct Visual {
    tones: HashMap<String, ToneEntry>,
    shell: Shell,
}

static VISUAL: LazyLock<Visual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/notice-tone.json"
    ))
    .expect("shared notice tone table")
});

fn hex(color: &str) -> Option<Hsla> {
    u32::from_str_radix(color.trim_start_matches('#'), 16)
        .ok()
        .map(|value| rgb(value).into())
}

/// A tone's hue (`None` for the neutral terminal tone) and glyph.
#[derive(Clone)]
pub(super) struct ToneLook {
    pub(super) hue: Option<Hsla>,
    pub(super) icon: String,
}

pub(super) fn tone_look(tone: &str, p: &ChatAppearance) -> ToneLook {
    let entry = VISUAL
        .tones
        .get(tone)
        .unwrap_or_else(|| &VISUAL.tones["info"]);
    let hue = if p.light {
        entry.light.as_deref()
    } else {
        entry.dark.as_deref()
    }
    .and_then(hex);
    ToneLook {
        hue,
        icon: format!("titlebar/{}.svg", entry.icon),
    }
}

/// How much a footer button stands out. There is no ghost or outlined footer button: every
/// labelled one is filled (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum ToneButton {
    Primary,
    Secondary,
}

/// The card's parts, laid out by [`NativeChatView::tone_card`].
pub(super) struct ToneCardParts {
    pub(super) look: ToneLook,
    pub(super) title: String,
    pub(super) detail: String,
    /// Drawn at the header's right end (a dismiss button, the fold chevron).
    pub(super) trailing: Vec<AnyElement>,
    pub(super) body: Vec<AnyElement>,
    /// Footer buttons on the left (Copy), then the ones pushed to the right.
    pub(super) footer_start: Vec<AnyElement>,
    /// Muted text after `footer_start` that takes the room the buttons leave, wrapping there, in
    /// place of the gap that pushes `footer_end` right (the held-messages card's way out).
    pub(super) footer_note: Option<String>,
    pub(super) footer_end: Vec<AnyElement>,
}

/// How a tone card's body opens and closes (a notice whose choices fold).
pub(super) struct ToneBodyMotion<'a> {
    pub(super) key: &'a str,
    pub(super) frame: Option<FoldFrame>,
    /// The card keeps its collapsed choices while shut, so the motion starts from their height.
    pub(super) shut: bool,
}

impl NativeChatView {
    /// The tone card. `header` wraps the header row when the card's header is pressable (it gets
    /// the row back and returns it with its id, hover and click attached).
    ///
    /// CDXC:SessionChat 2026-09-23 DECISION:
    /// User: "when i hover over this kind of card, you're making the only middle part change color, I want all of it to change color", and the same for "all very similar components in the chat view". A status card's pressable header reaches out to the card's own edges, so its hover lights the whole card while the card is folded to that header, and the card's full width above its body when it is open, never a band inside the padding.
    pub(super) fn tone_card(
        &self,
        parts: ToneCardParts,
        motion: Option<ToneBodyMotion>,
        header: Option<Box<dyn FnOnce(Div) -> Stateful<Div>>>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let v = &VISUAL.shell;
        let radius = v.radius * s;
        let inner_radius = px((radius - 1.0).max(0.0));
        let has_body = !parts.body.is_empty();
        let has_footer = !parts.footer_start.is_empty()
            || parts.footer_note.is_some()
            || !parts.footer_end.is_empty();
        let title_column = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            // A notice's title can be a whole sentence, so it wraps rather than losing its end.
            .child(
                div()
                    .text_size(px(v.title_size * s))
                    .line_height(px(v.title_line_height * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(parts.title),
            )
            .when(!parts.detail.is_empty(), |column| {
                column.child(
                    div()
                        .truncate()
                        .text_size(px(v.detail_size * s))
                        .line_height(px(v.detail_line_height * s))
                        .text_color(p.muted)
                        .child(parts.detail),
                )
            });
        let row = div()
            .flex()
            .items_center()
            .gap(px(v.header_gap * s))
            .px(px(v.pad_x * s))
            .py(px(v.pad_y * s))
            .child(tone_tile(&parts.look, p))
            .child(title_column)
            .children(parts.trailing);
        let header = match header {
            Some(press) => press(row)
                .hover(|style| style.bg(super::cards::card_hover_fill(p)))
                .map(|this| {
                    if has_body || has_footer {
                        this.rounded_t(inner_radius)
                    } else {
                        this.rounded(inner_radius)
                    }
                })
                .into_any_element(),
            None => row.into_any_element(),
        };
        let body = has_body.then(|| {
            let column = div()
                .flex()
                .flex_col()
                .gap(px(12.0 * s))
                .px(px(v.pad_x * s))
                .pb(px(v.pad_x * s))
                .text_size(px(14.0 * s))
                .line_height(px(21.0 * s))
                .children(parts.body)
                .into_any_element();
            match motion {
                Some(ToneBodyMotion {
                    key,
                    frame: Some(frame),
                    ..
                }) => motion_clip(
                    self.disclosure_height(key),
                    frame,
                    self.disclosure_floor(key).get(),
                    0.0,
                    column,
                ),
                Some(ToneBodyMotion {
                    key, shut: true, ..
                }) => measured(self.disclosure_floor(key), column),
                _ => column,
            }
        });
        let wash = match parts.look.hue {
            Some(hue) => hue.opacity(if p.light { v.wash_light } else { v.wash_dark }),
            None => gpui::transparent_black(),
        };
        let footer_shade: Hsla = gpui::black().opacity(if p.light {
            v.footer_shade_light
        } else {
            v.footer_shade_dark
        });
        div()
            .relative()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.input_border)
            .rounded(px(radius))
            .bg(p.card_panel)
            .overflow_hidden()
            .text_color(p.card_muted)
            // The tone's wash fades out down the card; no id, so it takes no input.
            .when(parts.look.hue.is_some(), |card| {
                card.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(96.0 * s))
                        .rounded_t(inner_radius)
                        .bg(gpui::linear_gradient(
                            180.0,
                            gpui::linear_color_stop(wash, 0.0),
                            gpui::linear_color_stop(wash.opacity(0.0), 1.0),
                        )),
                )
            })
            .child(header)
            .when_some(body, |card, body| card.child(body))
            .when(has_footer, |card| {
                card.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0 * s))
                        .px(px(v.pad_x * s))
                        .py(px(10.0 * s))
                        .border_t_1()
                        .border_color(p.control_border.opacity(0.65))
                        .bg(footer_shade)
                        .rounded_b(inner_radius)
                        .children(parts.footer_start)
                        .when_some(parts.footer_note, |footer, note| {
                            footer.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(v.detail_size * s))
                                    .line_height(px(v.detail_line_height * s))
                                    .text_color(p.muted)
                                    .child(note),
                            )
                        })
                        .when(!parts.footer_end.is_empty(), |footer| {
                            footer.child(tone_card_actions(parts.footer_end, s))
                        }),
                )
            })
            .into_any_element()
    }

    /// A footer button. `hint` is the keyboard shortcut drawn beside the label.
    pub(super) fn tone_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &str,
        icon: Option<&'static str>,
        kind: ToneButton,
        hint: Option<String>,
        disabled: bool,
        p: &ChatAppearance,
    ) -> Stateful<Div> {
        let s = p.scale;
        let v = &VISUAL.shell;
        let (fills, text) = match kind {
            ToneButton::Primary => (
                ChatButtonFills {
                    rest: p.control_primary,
                    hover: p.control_primary.opacity(0.88),
                    pressed: p.control_primary.opacity(0.8),
                },
                p.background,
            ),
            ToneButton::Secondary => (p.neutral_button_fills(), p.foreground),
        };
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.to_owned())
            .h(px(v.button_height * s))
            .px(px(11.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .rounded(px(v.button_radius * s))
            .border_1()
            .chat_filled(fills, !disabled)
            .text_size(px(v.button_text_size * s))
            .font_weight(FontWeight::MEDIUM)
            .text_color(text)
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| button.chat_cursor_pointer())
            .when_some(icon, |button, icon| {
                button.child(
                    gpui::svg()
                        .path(icon)
                        .size(px(14.0 * s))
                        .text_color(text)
                        .flex_shrink_0(),
                )
            })
            .child(label.to_owned())
            .when_some(hint, |button, hint| {
                let primary = kind == ToneButton::Primary;
                button.child(
                    div()
                        .px(px(5.0 * s))
                        .rounded(px(5.0 * s))
                        .border_1()
                        .border_color(if primary {
                            p.background.opacity(0.18)
                        } else {
                            p.foreground.opacity(0.12)
                        })
                        .font_family(super::fonts::CHAT_MONO)
                        .text_size(px(11.0 * s))
                        .line_height(px(16.0 * s))
                        .text_color(if primary {
                            p.background.opacity(0.7)
                        } else {
                            p.muted
                        })
                        .child(hint),
                )
            })
    }

    /// The dismiss button at a tone card header's right end, the circled X every chat card uses,
    /// filled like the Secondary footer buttons (the 2026-09-07 decision in approval.rs).
    pub(super) fn tone_icon_button(
        &self,
        id: impl Into<gpui::ElementId>,
        icon: &'static str,
        label: &'static str,
        p: &ChatAppearance,
    ) -> Stateful<Div> {
        let s = p.scale;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .size(px(24.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .chat_filled(p.neutral_button_fills(), true)
            .chat_cursor_pointer()
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .text_color(p.muted),
            )
    }
}

/// The tinted square the tone's glyph sits in.
fn tone_tile(look: &ToneLook, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let v = &VISUAL.shell;
    let (fill, ring, glyph) = match look.hue {
        Some(hue) => (hue.opacity(v.tile_fill), hue.opacity(v.tile_ring), hue),
        None => (p.foreground.opacity(0.05), p.control_border, p.muted),
    };
    div()
        .size(px(v.tile * s))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(v.tile_radius * s))
        .border_1()
        .border_color(ring)
        .bg(fill)
        .child(
            gpui::svg()
                .path(look.icon.clone())
                .size(px(v.tile_icon * s))
                .text_color(glyph),
        )
        .into_any_element()
}

/// Whether an approval's tool runs shell commands, so its command box leads with `$ `.
pub(super) fn is_shell_tool(tool: &str) -> bool {
    let tool = tool.to_ascii_lowercase();
    ["bash", "shell", "command", "exec", "terminal", "powershell"]
        .iter()
        .any(|needle| tool.contains(needle))
}

/// The command box an approval shows, with a muted `$ ` prompt sign before a shell command.
pub(super) fn command_box(command: String, shell: bool, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .min_w_0()
        .px(px(12.0 * s))
        .py(px(10.0 * s))
        .rounded(px(9.0 * s))
        .border_1()
        .border_color(p.control_border.opacity(0.65))
        .bg(p.background.opacity(0.7))
        .child(
            div()
                .id("tone-command")
                .max_h(px(160.0 * s))
                .overflow_y_scroll()
                .font_family(super::fonts::CHAT_MONO)
                .text_size(px(12.6 * s))
                .line_height(px(20.0 * s))
                .text_color(p.foreground.opacity(0.9))
                .flex()
                .when(shell, |row| {
                    row.child(div().flex_shrink_0().text_color(p.muted).child("$ "))
                })
                .child(div().flex_1().min_w_0().child(command)),
        )
        .into_any_element()
}

/// CDXC:SessionChat 2026-11-02 WHY: The footer's actions are one group that fills what the footer leaves and wraps inside itself,
/// right aligned on every line. A spacer between them and the wrapping footer pushed the first line of a four-button notice right
/// in a narrow chat and started the wrapped line at the left.
/// SEE-ALSO: apps/mobile/app/src/chat/native/cards/ToneCard.tsx
fn tone_card_actions(actions: Vec<AnyElement>, s: f32) -> Div {
    div()
        .flex_auto()
        .min_w_0()
        .flex()
        .flex_wrap()
        .justify_end()
        .items_center()
        .gap(px(8.0 * s))
        .children(actions)
}

impl NativeChatView {
    /// The card's Details toggle: a quiet "Details" text button that opens the terminal's own words
    /// (a notice's `details` and `errorLines`, an API error's line) in a mono box on the card's row
    /// surface. `None` when there is nothing to show.
    /// CDXC:SessionChat 2026-11-03 DECISION: User (Q1136): raw terminal output sits behind a Details toggle on the card, never in its sentences; the same toggle is on the phone.
    /// SEE-ALSO: apps/mobile/app/src/chat/native/cards/DetailsToggle.tsx, server/src/session_chat_notice/types.rs (`details`).
    pub(super) fn details_toggle(
        &self,
        key: &str,
        lines: Vec<String>,
        p: &ChatAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let lines: Vec<String> = lines
            .into_iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        if lines.is_empty() {
            return None;
        }
        let s = p.scale;
        let key = format!("{key}:details");
        let open = self.expanded.contains(&key);
        let motion = self.disclosure_frame(&key, open, cx);
        let body_key = key.clone();
        let mut toggle = div().flex().flex_col().gap(px(8.0 * s)).child(
            div()
                .id(gpui::ElementId::Name(
                    format!("details-toggle:{key}").into(),
                ))
                .tab_index(0)
                .role(gpui::Role::Button)
                .aria_label(if open { "Hide details" } else { "Show details" })
                .flex()
                .items_center()
                .self_start()
                .gap(px(4.0 * s))
                .chat_cursor_pointer()
                .h(px(22.0 * s))
                .text_size(px(13.0 * s))
                .text_color(p.muted)
                .hover(|style| style.text_color(p.foreground))
                .child("Details")
                .child(
                    gpui::svg()
                        .path(if open {
                            "titlebar/chevron-down.svg"
                        } else {
                            "titlebar/chevron-right.svg"
                        })
                        .size(px(12.0 * s))
                        .text_color(p.muted),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.expanded.remove(&key) {
                        this.expanded.insert(key.clone());
                    }
                    cx.notify();
                })),
        );
        if open || motion.is_some() {
            toggle = toggle.child(
                self.disclosure_body_motion(
                    &body_key,
                    motion,
                    8.0 * s,
                    div()
                        .id(gpui::ElementId::Name(
                            format!("details-text:{body_key}").into(),
                        ))
                        .max_h(px(240.0 * s))
                        .overflow_y_scroll()
                        .rounded(px(8.0 * s))
                        .border_1()
                        .border_color(p.control_border)
                        .bg(p.card_row)
                        .p(px(10.0 * s))
                        .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                        .text_size(px(12.0 * s))
                        .line_height(px(17.0 * s))
                        .text_color(p.card_muted)
                        .child(lines.join("\n"))
                        .into_any_element(),
                ),
            );
        }
        Some(toggle.into_any_element())
    }
}
