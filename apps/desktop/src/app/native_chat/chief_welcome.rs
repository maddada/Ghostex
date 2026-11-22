//! Ask Ghostex AI's welcome card (`chiefWelcome`, packages/gx-chat-core/src/extras/chief_welcome.rs):
//! the clipboard ghost, then First steps for a new user or the live status and prompts once they
//! are set up. A step, chip or prompt fills the chat box; the white ask button only focuses it.
//! Sizes and colours are the approved spec's (cos-welcome-card-v3): the chat's own palette, its 4%
//! wash and 8% hairline, and only the white ask button, the amber "needs you" dot and the ghost's
//! own colours added.
//!
//! The ghost floats, waves and blinks from layered pictures (assets/modals/chief-ghost/), and holds
//! still under Reduce Motion.
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/ChiefWelcome.tsx (the phone's card).

use super::cursor::ChatCursor as _;
use super::{appearance::ChatAppearance, state::NativeChatView};
use ghostex_gx_chat_core::extras::chief_welcome::{ChiefWelcome, WelcomePrompt};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, img, px,
    rgb, svg,
};
use std::time::Duration;

/// The card's widest column.
const COLUMN: f32 = 440.0;
/// The set-up card's widest column: its four prompts sit two by two, and at 440 the longer ones
/// ("How do coordinators work?") were cut off with an ellipsis.
const SET_UP_COLUMN: f32 = 480.0;
/// Below this the prompts go one a row.
const TWO_PROMPTS_FROM: f32 = 410.0;
/// The amber of the "needs you" dot.
const NEEDS_YOU: u32 = 0xe5a33a;

fn wash(p: &ChatAppearance) -> Hsla {
    if p.light {
        gpui::white().opacity(0.55)
    } else {
        p.foreground.opacity(0.04)
    }
}

fn hairline(p: &ChatAppearance) -> Hsla {
    p.foreground.opacity(0.08)
}

fn tile(p: &ChatAppearance) -> Hsla {
    p.foreground.opacity(if p.light { 0.06 } else { 0.07 })
}

fn icon(name: &str, size: f32, color: Hsla) -> impl IntoElement {
    svg()
        .path(format!("titlebar/{name}.svg"))
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
}

/// One of the ghost's layers, filling its frame.
fn ghost_layer(path: String, size: f32) -> AnyElement {
    img(path)
        .absolute()
        .top_0()
        .left_0()
        .size(px(size))
        .into_any_element()
}

fn ghost_arm(frame: usize, size: f32) -> AnyElement {
    ghost_layer(format!("modals/chief-ghost/arm-{frame}.svg"), size)
}

fn ghost_eyes(frame: &str, size: f32) -> AnyElement {
    ghost_layer(format!("modals/chief-ghost/eyes-{frame}.svg"), size)
}

/// The ghost at `size`, floating, waving and blinking unless motion is reduced.
fn ghost(size: f32, still: bool) -> AnyElement {
    let arm = move |frame: usize| ghost_arm(frame, size);
    let eyes = move |frame: &str| ghost_eyes(frame, size);
    let body = move || ghost_layer("modals/chief-ghost/body.svg".to_string(), size);
    let frame = div().relative().flex_shrink_0().size(px(size));
    if still {
        return frame
            .child(arm(0))
            .child(body())
            .child(eyes("open"))
            .into_any_element();
    }
    // The wave: two swings up to 22 degrees in the first 40% of a 4s beat (the spec's keyTimes).
    let waving = div()
        .absolute()
        .top_0()
        .left_0()
        .size(px(size))
        .with_animation(
            "chief-ghost-wave",
            Animation::new(Duration::from_millis(4_000)).repeat(),
            move |this, delta| {
                let phase = (delta / 0.1).floor() as i32;
                let within = (delta / 0.1).fract();
                let swing = match phase {
                    0 | 2 => within,
                    1 | 3 => 1.0 - within,
                    _ => 0.0,
                };
                this.child(arm(((swing * 3.0).round() as usize).min(3)))
            },
        );
    // The blink: closed for a moment at the end of a 4.6s beat.
    let blinking = div()
        .absolute()
        .top_0()
        .left_0()
        .size(px(size))
        .with_animation(
            "chief-ghost-blink",
            Animation::new(Duration::from_millis(4_600)).repeat(),
            move |this, delta| {
                this.child(eyes(match delta {
                    d if d < 0.9 => "open",
                    d if d < 0.92 || d >= 0.97 => "half",
                    _ => "closed",
                }))
            },
        );
    // The float: up 4px and back over 3.4s.
    frame
        .child(
            div()
                .absolute()
                .left_0()
                .size(px(size))
                .child(waving)
                .child(body())
                .child(blinking)
                .with_animation(
                    "chief-ghost-float",
                    Animation::new(Duration::from_millis(3_400)).repeat(),
                    |this, delta| {
                        let lift = (1.0 - (delta * std::f32::consts::TAU).cos()) * 0.5;
                        this.top(px(-4.0 * lift))
                    },
                ),
        )
        .into_any_element()
}

impl NativeChatView {
    /// A step, chip or prompt: its text in the chat box, on a new line after anything already
    /// typed there, never replacing it.
    fn fill_from_chief_welcome(&mut self, fill: &str, cx: &mut Context<Self>) {
        let typed = self.draft.trim_end();
        let next = if typed.is_empty() {
            fill.to_string()
        } else {
            format!("{typed}\n{fill}")
        };
        self.replace_draft(&next, false, false, cx);
    }

    /// The card, centred in the empty transcript.
    pub(super) fn render_chief_welcome(
        &mut self,
        welcome: &ChiefWelcome,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let room = (self.bounds.get().size.width.as_f32() - 48.0 * s).max(0.0);
        let new_user = welcome.card == "newUser";
        let column = (if new_user { COLUMN } else { SET_UP_COLUMN } * s).min(room);
        let still = cx.reduce_motion();
        let mut region = div()
            .flex_1()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.0 * s))
            .pt(px(28.0 * s))
            .px(px(24.0 * s))
            .child(ghost(if new_user { 92.0 } else { 88.0 } * s, still))
            .child(
                div()
                    .text_size(px(22.0 * s))
                    .line_height(px(27.5 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(welcome.title.clone()),
            );
        if let Some(line) = welcome.line.as_ref() {
            region = region.child(
                div()
                    .mt(px(-6.0 * s))
                    .max_w(px(380.0 * s))
                    .text_center()
                    .text_size(px(14.0 * s))
                    .line_height(px(20.0 * s))
                    .text_color(p.muted)
                    .child(line.clone()),
            );
        }
        if let Some(status) = welcome.status.as_ref() {
            let fill = status.fill.clone();
            region = region.child(
                div()
                    .id("chief-welcome-status")
                    .mt(px(-4.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(7.0 * s))
                    .px(px(11.0 * s))
                    .py(px(5.0 * s))
                    .rounded_full()
                    .bg(tile(p))
                    .text_size(px(12.5 * s))
                    .text_color(p.muted)
                    .chat_cursor_pointer()
                    .child(div().size(px(7.0 * s)).rounded_full().flex_shrink_0().bg(
                        if status.attention {
                            rgb(NEEDS_YOU).into()
                        } else {
                            p.muted
                        },
                    ))
                    .child(status.text.clone())
                    .when_some(status.needs.clone(), |this, needs| {
                        this.child("·").child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.foreground)
                                .child(needs),
                        )
                    })
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.fill_from_chief_welcome(&fill, cx)),
                    ),
            );
        }
        if new_user {
            region = region.child(self.chief_welcome_steps(welcome, column, p, cx));
        }
        region = region.child(self.chief_welcome_ask(welcome, column, p, cx));
        if !new_user {
            // Two by two, one a row on a narrow chat. Rows of two that share the width, not
            // items sized to exactly half of it, which wrapped to one a row when rounding left the
            // pair a hair wider than the column.
            let per_row = if column >= TWO_PROMPTS_FROM * s { 2 } else { 1 };
            let prompts: Vec<AnyElement> = welcome
                .prompts
                .iter()
                .enumerate()
                .map(|(index, prompt)| self.chief_welcome_prompt(index, prompt, p, cx))
                .collect();
            let mut rows = Vec::new();
            let mut prompts = prompts.into_iter().peekable();
            while prompts.peek().is_some() {
                rows.push(
                    div()
                        .w_full()
                        .flex()
                        .gap(px(8.0 * s))
                        .children(prompts.by_ref().take(per_row)),
                );
            }
            region = region.child(
                div()
                    .w(px(column))
                    .flex()
                    .flex_col()
                    .gap(px(8.0 * s))
                    .children(rows),
            );
        }
        // The new user's four asks and Report a bug; on the set-up card Report a bug alone.
        if !welcome.chips.is_empty() {
            region = region.child(
                div()
                    .max_w(px(column))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(6.0 * s))
                    .children(
                        welcome
                            .chips
                            .iter()
                            .enumerate()
                            .map(|(index, chip)| self.chief_welcome_chip(index, chip, p, cx)),
                    ),
            );
        }
        region.into_any_element()
    }

    fn chief_welcome_steps(
        &mut self,
        welcome: &ChiefWelcome,
        column: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let done = welcome.steps.iter().filter(|step| step.done).count();
        let total = welcome.steps.len().max(1);
        let clip = if p.light {
            rgb(0x2b2b33).into()
        } else {
            p.foreground.opacity(0.22)
        };
        let rows = welcome.steps.iter().enumerate().map(|(index, step)| {
            let fill = step.fill.clone();
            let mark = div()
                .size(px(18.0 * s))
                .flex_shrink_0()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .when(step.done, |this| {
                    this.bg(p.foreground.opacity(0.85))
                        .child(icon("check", 11.0 * s, p.background))
                })
                .when(!step.done, |this| {
                    this.border(px(1.5 * s)).border_color(p.muted.opacity(0.7))
                });
            div()
                .id(("chief-welcome-step", index))
                .w_full()
                .flex()
                .items_center()
                .gap(px(11.0 * s))
                .px(px(8.0 * s))
                .py(px(9.0 * s))
                .rounded(px(10.0 * s))
                .text_size(px(13.5 * s))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if step.done { p.muted } else { p.foreground })
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.07)))
                .child(mark)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .when(step.done, |this| this.line_through())
                        .child(step.label.clone()),
                )
                .when(!step.done, |this| {
                    this.child(icon("chevron-right", 13.0 * s, p.foreground.opacity(0.35)))
                })
                .on_click(
                    cx.listener(move |this, _, _, cx| this.fill_from_chief_welcome(&fill, cx)),
                )
        });
        div()
            .relative()
            .w(px(column))
            .mt(px(4.0 * s))
            .pt(px(16.0 * s))
            .px(px(8.0 * s))
            .pb(px(8.0 * s))
            .rounded(px(18.0 * s))
            .bg(wash(p))
            .border_1()
            .border_color(hairline(p))
            // The clip on top: the steps card is the ghost's clipboard.
            .child(
                div()
                    .absolute()
                    .top(px(-6.0 * s))
                    .left(px(column / 2.0 - 18.0 * s))
                    .w(px(36.0 * s))
                    .h(px(10.0 * s))
                    .rounded(px(4.0 * s))
                    .bg(clip),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .px(px(8.0 * s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.0 * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .child("First steps"),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.0 * s))
                            .text_color(p.muted)
                            .child(format!("{done} of {}", welcome.steps.len())),
                    ),
            )
            .child(
                div()
                    .h(px(3.0 * s))
                    .mx(px(8.0 * s))
                    .mt(px(8.0 * s))
                    .mb(px(6.0 * s))
                    .rounded(px(2.0 * s))
                    .bg(tile(p))
                    .child(
                        div()
                            .h_full()
                            .rounded(px(2.0 * s))
                            .bg(p.foreground.opacity(0.55))
                            .w(px((column - 32.0 * s) * done as f32 / total as f32)),
                    ),
            )
            .children(rows)
            .into_any_element()
    }

    /// The one white button: it puts the cursor in the chat box.
    fn chief_welcome_ask(
        &mut self,
        welcome: &ChiefWelcome,
        column: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let ink: Hsla = if p.light {
            rgb(0x27272a).into()
        } else {
            rgb(0x0d0d0d).into()
        };
        div()
            .id("chief-welcome-ask")
            .w(px(column))
            .flex()
            .items_center()
            .gap(px(11.0 * s))
            .px(px(14.0 * s))
            .py(px(12.0 * s))
            .rounded(px(14.0 * s))
            .bg(if p.light {
                gpui::white()
            } else {
                rgb(0xfcfcfc).into()
            })
            .when(p.light, |this| {
                this.border_1()
                    .border_color(rgb(0x27272a).opacity(0.10))
                    .shadow_sm()
            })
            .text_color(ink)
            .chat_cursor_pointer()
            .child(icon("message-circle", 18.0 * s, ink))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(14.0 * s))
                            .line_height(px(18.0 * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(welcome.ask_title.clone()),
                    )
                    .child(
                        div()
                            .mt(px(1.0 * s))
                            .text_size(px(12.0 * s))
                            .line_height(px(16.0 * s))
                            .text_color(ink.opacity(0.62))
                            .child(welcome.ask_detail.clone()),
                    ),
            )
            .child(icon("chevron-right", 15.0 * s, ink.opacity(0.55)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.focus_requested = true;
                cx.notify();
            }))
            .into_any_element()
    }

    fn chief_welcome_chip(
        &mut self,
        index: usize,
        chip: &WelcomePrompt,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let fill = chip.fill.clone();
        div()
            .id(("chief-welcome-chip", index))
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .px(px(11.0 * s))
            .py(px(6.0 * s))
            .rounded_full()
            .border_1()
            .border_color(p.composer_border)
            .text_size(px(12.5 * s))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.foreground)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.07)))
            .child(icon(&chip.icon, 13.0 * s, p.foreground.opacity(0.65)))
            .child(chip.label.clone())
            .on_click(cx.listener(move |this, _, _, cx| this.fill_from_chief_welcome(&fill, cx)))
            .into_any_element()
    }

    fn chief_welcome_prompt(
        &mut self,
        index: usize,
        prompt: &WelcomePrompt,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let fill = prompt.fill.clone();
        div()
            .id(("chief-welcome-prompt", index))
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(9.0 * s))
            .px(px(12.0 * s))
            .py(px(10.0 * s))
            .rounded(px(12.0 * s))
            .bg(wash(p))
            .border_1()
            .border_color(hairline(p))
            .text_size(px(13.0 * s))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.foreground)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.07)))
            .child(icon(&prompt.icon, 15.0 * s, p.foreground.opacity(0.7)))
            // A label too long for its half wraps onto a second line instead of losing its end.
            .child(div().flex_1().min_w_0().child(prompt.label.clone()))
            .on_click(cx.listener(move |this, _, _, cx| this.fill_from_chief_welcome(&fill, cx)))
            .into_any_element()
    }
}
