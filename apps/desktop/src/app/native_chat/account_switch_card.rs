/*!
The account-switch card: the modal-style card centered over the whole chat pane
while a Claude or Codex account switch runs. A port of `AccountSwitchCard`
(packages/core-ui/accounts/account-switch-card.tsx) and its
account-switch-card.css, drawn from the core's `accountSwitchCard` projection
(packages/gx-chat-core/src/menus/native_accounts.rs). The copy, steps and
usage levels come from `menus/accounts_presentation.rs` in the core.

CDXC:AgentProviders 2026-09-25 DECISION:
User: make the card bigger when the pane has room, drop the bars under the usage tiles, stop the tile name (Fable) looking the same as its reset time (3d 4h), and show the agent icon once, left of the title, not on both account rows (docs/2026-09-24/account-switch-card-roomy/01-no-bars.html). The card is 430px in a narrow pane and scales up to 516px / 576px from 600px / 760px pane widths. Each tile shows a semibold name, a smaller muted reset line with a clock, and a large percent; the tile at 100% keeps its stronger outline. This supersedes the 2026-09-23 tile-edge bars and "smaller" card; the rest of that decision (neutral ink, accent for the target and progress, numbered steps with the moving line) stands, as does the 2026-09-16 backdrop that blocks the pointer until the switch finishes.
*/

use super::{
    appearance::ChatAppearance, new_session_welcome::brand_logo_color, state::NativeChatView,
    working_spark::css_ease_in_out,
};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use crate::app::window::native_modal_kit::css_mix;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Rgba, StatefulInteractiveElement as _, Styled as _, div, px,
    relative, rgba, svg,
};
use serde_json::{Value, json};
use std::time::Duration;

/// The card-scoped `--as-*` variables: every surface is a step off the chat's own background.
struct Palette {
    backdrop: Rgba,
    surface: Rgba,
    tile: Rgba,
    line: Hsla,
    track: Hsla,
    foreground: Hsla,
    text: Hsla,
    muted: Hsla,
    accent: Hsla,
}

impl Palette {
    fn new(p: &ChatAppearance) -> Self {
        let background = Rgba::from(p.background);
        let white = Rgba::from(gpui::white());
        let black = Rgba::from(gpui::black());
        Self {
            backdrop: rgba(if p.light { 0x00000061 } else { 0x00000094 }),
            surface: if p.light {
                css_mix(white, 0.7, background)
            } else {
                css_mix(white, 0.04, background)
            },
            tile: if p.light {
                css_mix(black, 0.03, background)
            } else {
                css_mix(white, 0.07, background)
            },
            line: p.foreground.opacity(if p.light { 0.12 } else { 0.10 }),
            track: p.foreground.opacity(if p.light { 0.10 } else { 0.09 }),
            foreground: p.foreground,
            text: p.prose,
            muted: p.muted,
            accent: p.accent,
        }
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// A usage tile: the limit's name over its reset time on the left, the percent on the right.
fn usage_tile(card: &Value, target: bool, palette: &Palette, s: f32) -> AnyElement {
    let level = card["level"].as_str().unwrap_or("unknown");
    let used = card["used"].as_f64();
    let strong = matches!(level, "high" | "exhausted");
    let reset = card["reset"].as_str().map(str::to_owned);
    div()
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(6.0 * s))
        .py(px(9.0 * s))
        .px(px(11.0 * s))
        .rounded(px(9.0 * s))
        .border_1()
        .border_color(if level == "exhausted" {
            palette.foreground.opacity(0.35)
        } else {
            palette.line
        })
        .bg(palette.tile)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.0 * s))
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.5 * s))
                        .line_height(px(16.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette.foreground)
                        .child(text(card, "label")),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(4.0 * s))
                        .text_size(px(11.0 * s))
                        .line_height(px(14.0 * s))
                        .text_color(palette.muted)
                        .when(reset.is_some(), |this| {
                            this.child(
                                svg()
                                    .path("titlebar/clock.svg")
                                    .flex_shrink_0()
                                    .size(px(11.0 * s))
                                    .text_color(palette.muted),
                            )
                        })
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(reset.unwrap_or_else(|| "No reset yet".to_owned())),
                        ),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_baseline()
                .text_size(px(20.0 * s))
                .line_height(px(20.0 * s))
                .font_weight(if strong {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .text_color(if level == "unknown" {
                    palette.muted
                } else if strong || target {
                    palette.foreground
                } else {
                    palette.text
                })
                .child(used.map_or("–".to_owned(), |used| (used.round() as i64).to_string()))
                .when(used.is_some(), |this| {
                    this.child(
                        div()
                            .ml(px(1.0 * s))
                            .text_size(px(11.0 * s))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.muted)
                            .child("%"),
                    )
                }),
        )
        .into_any_element()
}

fn account(value: &Value, verified: bool, palette: &Palette, s: f32) -> AnyElement {
    let target = value["target"] == true;
    let role = match (target, verified) {
        (true, true) => "Active",
        (true, false) => "To",
        (false, true) => "Previous",
        (false, false) => "From",
    };
    let mut tiles = div().flex().gap(px(7.0 * s));
    for card in value["usage"].as_array().into_iter().flatten() {
        tiles = tiles.child(usage_tile(card, target, palette, s));
    }
    div()
        .flex()
        .flex_col()
        .gap(px(7.0 * s))
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.0 * s))
                .min_w_0()
                .text_size(px(12.5 * s))
                .line_height(px(17.0 * s))
                .child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(32.0 * s))
                        .text_size(px(11.5 * s))
                        .text_color(if target {
                            palette.accent
                        } else {
                            palette.muted
                        })
                        .child(role),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.foreground)
                        .child(text(value, "label")),
                )
                .when(target && verified, |this| {
                    this.child(
                        svg()
                            .path("titlebar/check.svg")
                            .flex_shrink_0()
                            .size(px(13.0 * s))
                            .text_color(palette.accent),
                    )
                }),
        )
        .child(tiles)
        .into_any_element()
}

/// `.gx-account-switch-step`: a small numbered circle and label over a 2px line; a finished step
/// shows an accent check, the active one a filled number and the moving accent line.
fn step(index: usize, value: &Value, id: &str, palette: &Palette, s: f32) -> AnyElement {
    let state = value["state"].as_str().unwrap_or("pending");
    let (color, number_bg, number_border, number_color) = match state {
        "active" => (
            palette.foreground,
            palette.foreground,
            gpui::transparent_black(),
            Hsla::from(palette.surface),
        ),
        "done" => (
            palette.text,
            palette.accent.opacity(0.22),
            gpui::transparent_black(),
            palette.accent,
        ),
        _ => (
            palette.muted,
            gpui::transparent_black(),
            palette.line,
            palette.muted,
        ),
    };
    let mut line = div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(2.0 * s))
        .rounded(px(2.0 * s))
        .overflow_hidden()
        .bg(if state == "done" {
            palette.accent.opacity(0.55)
        } else {
            palette.track
        });
    if state == "active" {
        // `gx-account-switch-step-sweep`: a 40% bar crossing from -100% to 250% of its own width.
        line = line.child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(0.4))
                .rounded(px(2.0 * s))
                .bg(palette.accent)
                .with_throttled_animation(
                    gpui::ElementId::Name(format!("account-switch-motion:{id}:{index}").into()),
                    Duration::from_millis(1600),
                    |bar, phase| bar.left(relative(-0.4 + 1.4 * css_ease_in_out(phase))),
                ),
        );
    }
    // `.gx-account-switch-progress` columns: the middle label is the longest, so it grows widest.
    let grow = [1.0, 1.3, 1.1].get(index).copied().unwrap_or(1.0);
    let mut row = div();
    row.style().flex_grow = Some(grow);
    row.relative()
        .flex_basis(px(0.0))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(7.0 * s))
        .pb(px(8.0 * s))
        .text_size(px(11.5 * s))
        .line_height(px(15.0 * s))
        .text_color(color)
        .child(
            div()
                .flex_shrink_0()
                .size(px(17.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(number_border)
                .bg(number_bg)
                .text_size(px(10.5 * s))
                .text_color(number_color)
                .when(state == "done", |this| {
                    this.child(
                        svg()
                            .path("titlebar/check.svg")
                            .size(px(10.0 * s))
                            .text_color(number_color),
                    )
                })
                .when(state != "done", |this| this.child((index + 1).to_string())),
        )
        .child(div().min_w_0().truncate().child(text(value, "label")))
        .child(line)
        .into_any_element()
}

impl NativeChatView {
    /// The overlay React rendered as `.gx-account-switch-overlay` while `accountStatus.visible` holds.
    pub(crate) fn render_account_switch_card(
        &self,
        appearance: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = &self.snapshot["accountSwitchCard"];
        if !card.is_object() {
            return None;
        }
        let zoom = appearance.scale;
        // The card is 430px in a narrow pane and grows, text and all, as the pane gets wider.
        let pane_width = f32::from(self.bounds.get().size.width) / zoom;
        let s = zoom
            * if pane_width >= 760.0 {
                1.34
            } else if pane_width >= 600.0 {
                1.2
            } else {
                1.0
            };
        let palette = Palette::new(appearance);
        let id = text(card, "id");
        let provider = card["provider"].as_str().unwrap_or("claude");
        let verified = card["verified"] == true;
        let phase = card["phase"].as_str().unwrap_or("switching");
        let heading = div()
            .id("account-switch-heading")
            .role(gpui::Role::Status)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .text_size(px(15.0 * s))
                    .line_height(px(20.0 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.foreground)
                    .child(
                        svg()
                            .path(format!("agent-icons/{provider}.svg"))
                            .flex_shrink_0()
                            .size(px(17.0 * s))
                            .text_color(brand_logo_color(provider, appearance)),
                    )
                    .child(div().min_w_0().child(text(card, "heading"))),
            )
            .child(
                div()
                    .mt(px(3.0 * s))
                    .text_size(px(12.5 * s))
                    .line_height(px(17.4 * s))
                    .text_color(palette.muted)
                    .child(text(card, "lede")),
            );
        let footer = if let Some(steps) = card["steps"].as_array() {
            let mut list = div()
                .id("account-switch-steps")
                .role(gpui::Role::List)
                .flex()
                .gap(px(6.0 * s))
                .pt(px(2.0 * s));
            for (index, value) in steps.iter().enumerate() {
                list = list.child(step(index, value, &id, &palette, s));
            }
            list.into_any_element()
        } else {
            let retry = card["retry"].clone();
            div()
                .id("account-switch-failure")
                .role(gpui::Role::Alert)
                .flex()
                .flex_col()
                .gap(px(10.0 * s))
                .text_size(px(12.0 * s))
                .line_height(px(17.4 * s))
                .child(
                    div()
                        .py(px(2.0 * s))
                        .pl(px(10.0 * s))
                        .border_l(px(2.0 * s))
                        .border_color(palette.foreground)
                        .text_color(palette.text)
                        .child(text(card, "failure")),
                )
                .when(retry.is_object(), |this| {
                    let busy = retry["busy"] == true;
                    let command = json!({"type":"accounts","request":{"operation":"select","accountId":retry["accountId"]}});
                    this.child(
                        div().flex().justify_end().child(
                            div()
                                .id("account-switch-retry")
                                .role(gpui::Role::Button)
                                .aria_label("Retry switch")
                                .h(px(28.0 * s))
                                .px(px(10.0 * s))
                                .flex()
                                .items_center()
                                .gap(px(6.0 * s))
                                .rounded(px(8.0 * s))
                                .border_1()
                                .border_color(palette.line)
                                .text_size(px(11.0 * s))
                                .text_color(palette.foreground)
                                .when(busy, |this| this.opacity(0.5))
                                .when(!busy, |this| {
                                    this.chat_cursor_pointer()
                                        .hover(|style| style.bg(palette.tile))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.invoke(command.clone(), cx)
                                        }))
                                })
                                .child(
                                    svg()
                                        .path("titlebar/refresh.svg")
                                        .size(px(14.0 * s))
                                        .text_color(palette.foreground),
                                )
                                .child("Retry switch"),
                        ),
                    )
                })
                .into_any_element()
        };
        let card_element = div()
            .id("account-switch-card")
            .role(gpui::Role::Group)
            .aria_label("Account switch status")
            .relative()
            .w_full()
            .max_w(px(430.0 * s))
            .flex_shrink_0()
            // Auto margins center the card like `align-items: safe center`: a card taller
            // than the pane starts at the top and scrolls instead of clipping.
            .my_auto()
            .flex()
            .flex_col()
            .gap(px(14.0 * s))
            .p(px(16.0 * s))
            .rounded(px(14.0 * s))
            .border_1()
            .border_color(palette.line)
            .bg(palette.surface)
            .shadow(vec![gpui::BoxShadow {
                color: rgba(if appearance.light {
                    0x0000001a
                } else {
                    0x00000040
                })
                .into(),
                offset: gpui::point(px(0.0), px(12.0 * s)),
                blur_radius: px(32.0 * s),
                spread_radius: px(0.0),
                inset: false,
            }])
            .occlude()
            .child(heading)
            .child(account(&card["from"], verified, &palette, s))
            .child(account(&card["to"], verified, &palette, s))
            .child(footer)
            .with_animation(
                gpui::ElementId::Name(format!("account-switch-card:{id}").into()),
                Animation::new(Duration::from_millis(260)).with_easing(gpui::ease_out_quint()),
                move |card, delta| card.opacity(delta).top(px((1.0 - delta) * 14.0 * s)),
            );
        Some(
            div()
                .id("account-switch-overlay")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(palette.backdrop)
                // A switch in flight blocks the pane; a failed one lets the pointer through.
                .when(phase != "failed", |this| this.occlude())
                .flex()
                .flex_col()
                .items_center()
                .overflow_y_scroll()
                .py(px(24.0 * zoom))
                .px(px(16.0 * zoom))
                .font_family(appearance.font.clone())
                .child(card_element)
                .with_animation(
                    gpui::ElementId::Name(format!("account-switch-backdrop:{id}").into()),
                    Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()),
                    |overlay, delta| overlay.opacity(delta),
                )
                .into_any_element(),
        )
    }
}
