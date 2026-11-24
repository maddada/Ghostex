/*!
Pure GPUI layout and palette for the account-switch card (`account_switch_card.rs`), shared with
`native-modal-demo` (`GHOSTEX_NATIVE_MODAL_DEMO=account-switch`) so the card can be previewed and
snapshotted in dark and light themes without launching Ghostex.
*/

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Div, ElementId, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Rgba, StatefulInteractiveElement as _,
    Styled as _, div, px, relative, rgba, svg,
};
use serde_json::Value;
use std::time::Duration;

/// Matches the composer / glass card visual language: soft outer corners (`18px`) with crisp
/// nested tile corners (`10px`).
const CARD_RADIUS: f32 = 18.0;
const TILE_RADIUS: f32 = 10.0;
/// Matches `sidebar_titlebar_background_is_light` (`theme_colours.rs`) and mobile `isLightBackground`.
const LIGHT_LUMINANCE_THRESHOLD: f32 = 0.54;

/// Resolved theme inputs for rendering the account-switch overlay.
#[derive(Clone)]
pub(crate) struct AccountSwitchCardLook {
    pub(crate) background: Hsla,
    pub(crate) card_panel: Hsla,
    pub(crate) composer_background: Hsla,
    pub(crate) composer_border: Hsla,
    pub(crate) border: Hsla,
    pub(crate) input_border: Hsla,
    pub(crate) foreground: Hsla,
    pub(crate) prose: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) accent: Hsla,
    pub(crate) brand_color: Hsla,
    pub(crate) button_rest: Hsla,
    pub(crate) button_hover: Hsla,
    pub(crate) button_pressed: Hsla,
    pub(crate) light: bool,
    pub(crate) window_glass: bool,
    pub(crate) scale: f32,
    pub(crate) font: String,
}

/// Optional host hooks for interactive actions and animations.
pub(crate) struct AccountSwitchCardHooks<'a> {
    pub(crate) on_close: Option<Box<dyn FnOnce(gpui::Stateful<Div>) -> gpui::Stateful<Div> + 'a>>,
    pub(crate) on_retry:
        Option<Box<dyn FnOnce(gpui::Stateful<Div>, Value) -> gpui::Stateful<Div> + 'a>>,
    pub(crate) animate_sweep: Option<&'a dyn Fn(Div, ElementId) -> AnyElement>,
    pub(crate) animate_entrance: bool,
}

impl Default for AccountSwitchCardHooks<'_> {
    fn default() -> Self {
        Self {
            on_close: None,
            on_retry: None,
            animate_sweep: None,
            animate_entrance: false,
        }
    }
}

/// Relative luminance in sRGB [0, 1], used to choose dark or light ink over an accent fill.
fn relative_luminance(color: Rgba) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

/// The card's resolved theme tokens: card surface from the theme's `card_panel` composited over
/// the work-area base, subtle raised tiles relative to that surface, theme hairline borders,
/// regular text hierarchy, and the theme accent for progress and step indicators.
struct Palette {
    backdrop: Rgba,
    surface: Hsla,
    tile: Hsla,
    line: Hsla,
    track: Hsla,
    foreground: Hsla,
    text: Hsla,
    muted: Hsla,
    accent: Hsla,
    accent_ink: Hsla,
}

impl Palette {
    fn new(p: &AccountSwitchCardLook) -> Self {
        // Match the composer / pill strip glass look: near-black translucent glass in dark mode
        // (`composer_background`, e.g. Polar Night's `base+6%/80%`), and crisp warm-light glass in
        // light mode (`card_panel`), falling back to a composited fill when `chatBox` is a faint
        // wash so background chat text never crowds the card.
        let base = p.background.alpha(1.0);
        let surface = if p.light {
            let solid = base.blend(p.card_panel);
            if p.window_glass {
                solid.alpha(0.94)
            } else {
                solid.alpha(1.0)
            }
        } else if p.window_glass {
            if p.composer_background.a >= 0.72 {
                p.composer_background
            } else {
                base.blend(p.composer_background).alpha(0.84)
            }
        } else {
            base.blend(p.composer_background).alpha(1.0)
        };
        // Faint raised surfaces matching the composer pill strip's tint (`STRIP_TINT = 0.035`).
        let tile = if p.light {
            gpui::black().opacity(0.035)
        } else {
            gpui::white().opacity(0.038)
        };
        let line = if p.light && !p.window_glass {
            p.input_border
        } else {
            p.composer_border
        };
        let accent_ink = if relative_luminance(Rgba::from(p.accent)) > LIGHT_LUMINANCE_THRESHOLD {
            gpui::rgb(0x141518).into()
        } else {
            gpui::white()
        };
        Self {
            backdrop: if p.light {
                rgba(0x00000038)
            } else if p.window_glass {
                Rgba::from(p.background.opacity(0.42))
            } else {
                rgba(0x00000088)
            },
            surface,
            tile,
            line,
            track: if p.light {
                gpui::black().opacity(0.08)
            } else {
                gpui::white().opacity(0.09)
            },
            foreground: p.foreground,
            text: p.prose,
            muted: p.muted,
            accent: p.accent,
            accent_ink,
        }
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// A usage tile: the limit's name over its reset time on the left, the percent on the right.
fn usage_tile(card: &Value, palette: &Palette, s: f32) -> AnyElement {
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
        .rounded(px(TILE_RADIUS * s))
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
                } else {
                    palette.foreground
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
        tiles = tiles.child(usage_tile(card, palette, s));
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
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.muted)
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
/// shows an accent check, the active one an accent-filled number and the moving accent line.
fn step(
    index: usize,
    value: &Value,
    id: &str,
    palette: &Palette,
    s: f32,
    animate_sweep: Option<&dyn Fn(Div, ElementId) -> AnyElement>,
) -> AnyElement {
    let state = value["state"].as_str().unwrap_or("pending");
    let (color, label_weight, number_bg, number_border, number_color, number_weight) = match state {
        "active" => (
            palette.foreground,
            FontWeight::MEDIUM,
            palette.accent,
            gpui::transparent_black(),
            palette.accent_ink,
            FontWeight::SEMIBOLD,
        ),
        "done" => (
            palette.text,
            FontWeight::NORMAL,
            palette.accent.opacity(0.18),
            gpui::transparent_black(),
            palette.accent,
            FontWeight::MEDIUM,
        ),
        _ => (
            palette.muted,
            FontWeight::NORMAL,
            gpui::transparent_black(),
            palette.line,
            palette.muted,
            FontWeight::NORMAL,
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
            palette.accent.opacity(0.65)
        } else {
            palette.track
        });
    if state == "active" {
        let bar = div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.4))
            .rounded(px(2.0 * s))
            .bg(palette.accent);
        let sweep = match animate_sweep {
            Some(animate) => animate(
                bar,
                ElementId::Name(format!("account-switch-motion:{id}:{index}").into()),
            ),
            None => bar.left(relative(0.18)).into_any_element(),
        };
        line = line.child(sweep);
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
        .font_weight(label_weight)
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
                .font_weight(number_weight)
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

pub(crate) fn render_account_switch_overlay(
    card: &Value,
    pane_width: f32,
    appearance: &AccountSwitchCardLook,
    mut hooks: AccountSwitchCardHooks<'_>,
) -> AnyElement {
    let zoom = appearance.scale;
    // A wide pane draws the card, text and all, at the former 1.34 scale shrunk 30% then grown 15%.
    let s = zoom
        * if pane_width >= 760.0 {
            1.34 * 0.7 * 1.15
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
        .when(card["dismissible"] == true, |this| this.pr(px(28.0 * s)))
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
                        .text_color(appearance.brand_color),
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
            list = list.child(step(index, value, &id, &palette, s, hooks.animate_sweep));
        }
        list.into_any_element()
    } else {
        let retry = card["retry"].clone();
        let on_retry = hooks.on_retry.take();
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
                let rest = appearance.button_rest;
                let hover = appearance.button_hover;
                let pressed = appearance.button_pressed;
                let mut button = div()
                    .id("account-switch-retry")
                    .role(gpui::Role::Button)
                    .aria_label("Retry switch")
                    .h(px(28.0 * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .rounded(px(TILE_RADIUS * s))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .bg(rest)
                    .text_size(px(11.0 * s))
                    .text_color(palette.foreground)
                    .when(busy, |this| this.opacity(0.5))
                    .when(!busy, |this| {
                        this.cursor_default()
                            .hover(move |style| style.bg(hover))
                            .active(move |style| style.bg(pressed))
                    })
                    .child(
                        svg()
                            .path("titlebar/refresh.svg")
                            .size(px(14.0 * s))
                            .text_color(palette.foreground),
                    )
                    .child("Retry switch");
                if !busy && let Some(attach) = on_retry {
                    button = attach(button, retry);
                }
                this.child(div().flex().justify_end().child(button))
            })
            .into_any_element()
    };
    let on_close = hooks.on_close.take();
    let shadow_alpha = if appearance.light { 0.16 } else { 0.4 };
    let card_div = div()
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
        .rounded(px(CARD_RADIUS * s))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surface)
        .shadow(vec![
            gpui::BoxShadow {
                color: gpui::black().opacity(shadow_alpha),
                offset: gpui::point(px(0.0), px(16.0 * s)),
                blur_radius: px(40.0 * s),
                spread_radius: px(-8.0 * s),
                inset: false,
            },
            gpui::BoxShadow {
                color: gpui::black().opacity(shadow_alpha * 0.5),
                offset: gpui::point(px(0.0), px(4.0 * s)),
                blur_radius: px(12.0 * s),
                spread_radius: px(-4.0 * s),
                inset: false,
            },
        ])
        .occlude()
        .child(heading)
        .when(card["dismissible"] == true, |this| {
            let mut close = div()
                .id("account-switch-close")
                .role(gpui::Role::Button)
                .aria_label("Close")
                .absolute()
                .top(px(10.0 * s))
                .right(px(10.0 * s))
                .size(px(24.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0 * s))
                .cursor_default()
                .hover(|style| style.bg(palette.tile))
                .child(
                    svg()
                        .path("titlebar/x.svg")
                        .size(px(14.0 * s))
                        .text_color(palette.muted),
                );
            if let Some(attach) = on_close {
                close = attach(close);
            }
            this.child(close)
        })
        .child(account(&card["from"], verified, &palette, s))
        .child(account(&card["to"], verified, &palette, s))
        .child(footer);

    let card_element = if hooks.animate_entrance {
        card_div
            .with_animation(
                ElementId::Name(format!("account-switch-card:{id}").into()),
                Animation::new(Duration::from_millis(260)).with_easing(gpui::ease_out_quint()),
                move |card, delta| card.opacity(delta).top(px((1.0 - delta) * 14.0 * s)),
            )
            .into_any_element()
    } else {
        card_div.into_any_element()
    };

    let overlay = div()
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
        .child(card_element);

    if hooks.animate_entrance {
        overlay
            .with_animation(
                ElementId::Name(format!("account-switch-backdrop:{id}").into()),
                Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint()),
                |overlay, delta| overlay.opacity(delta),
            )
            .into_any_element()
    } else {
        overlay.into_any_element()
    }
}
