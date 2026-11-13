//! The onboarding's own controls and type, ported from packages/core-ui/onboarding/primitives.tsx (deleted 2026-10-01)
//! and styles/base.css.
//!
//! CDXC:Onboarding 2026-09-28 WHY:
//! The onboarding is one fixed 1672x941 artboard scaled as a single picture, and every control on
//! it (the 60x34 blue-glow switches, the flat indigo call to action, the tracked mono eyebrows) is
//! the prototype's own design rather than the app's shared controls. GPUI-Kit's controls draw at
//! fixed pixel metrics that do not follow the stage scale and cannot take these shapes, so, like the
//! React page did with its own primitives.tsx (deleted 2026-10-01), the stage draws them here; Kit tooltips, scrolling
//! and the window root are still used around them.
use super::fonts::{DM_SANS, MANROPE, PLEX_MONO};
use super::interact::{BUTTON_MS, hover_color, hovered, tween_color, tween_value};
use super::model::CatalogAgent;
use super::stage::*;
use super::text::tracked;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, BoxShadow, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Radians, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Svg, Transformation, canvas, div, img, linear_color_stop, linear_gradient, point,
    px, size, svg,
};
use std::time::Instant;

/// `line-height: normal` for each family (ascender + descender over the em).
pub(crate) const LH_DM_SANS: f32 = 1.302;
pub(crate) const LH_MANROPE: f32 = 1.366;
pub(crate) const LH_MONO: f32 = 1.3;

pub(crate) fn shadow(color: Hsla, x: f32, y: f32, blur: f32, spread: f32, s: S) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(s.px(x), s.px(y)),
        blur_radius: s.px(blur),
        spread_radius: s.px(spread),
        inset: false,
    }
}

pub(crate) fn inset_shadow(color: Hsla, x: f32, y: f32, blur: f32, spread: f32, s: S) -> BoxShadow {
    BoxShadow {
        inset: true,
        ..shadow(color, x, y, blur, spread, s)
    }
}

/// DM Sans text at `size` with `line-height: normal`.
pub(crate) fn sans(s: S, size: f32, weight: f32, color: Hsla) -> Div {
    let (family, weight) = super::fonts::face(DM_SANS, weight);
    div()
        .font_family(family)
        .text_size(s.px(size))
        .font_weight(FontWeight(weight))
        .line_height(s.px(size * LH_DM_SANS))
        .text_color(color)
}

/// IBM Plex Mono text at `size` with `line-height: normal`.
pub(crate) fn mono(s: S, size: f32, weight: f32, color: Hsla) -> Div {
    let (family, weight) = super::fonts::face(PLEX_MONO, weight);
    div()
        .font_family(family)
        .text_size(s.px(size))
        .font_weight(FontWeight(weight))
        .line_height(s.px(size * LH_MONO))
        .text_color(color)
}

/// Tracked single-line text (letter spacing in em).
pub(crate) fn tracked_text(
    s: S,
    text: impl Into<SharedString>,
    family: &'static str,
    weight: f32,
    size: f32,
    line_height: f32,
    tracking_em: f32,
    color: Hsla,
) -> super::text::TrackedText {
    tracked(
        text,
        family,
        weight,
        s.px(size),
        s.px(line_height),
        s.px(size * tracking_em),
        color,
    )
}

/// `.label`: 11.5px Plex Mono, 0.22em tracking, uppercase.
pub(crate) fn label(s: S, text: &str) -> super::text::TrackedText {
    tracked_text(
        s,
        text.to_uppercase(),
        PLEX_MONO,
        500.0,
        11.5,
        11.5,
        0.22,
        hex(0xcfd5e0),
    )
}

pub(crate) fn label_sized(s: S, text: &str, size: f32) -> super::text::TrackedText {
    tracked_text(
        s,
        text.to_uppercase(),
        PLEX_MONO,
        500.0,
        size,
        size,
        0.22,
        hex(0xcfd5e0),
    )
}

/// `.nm`, `.nm.lg`, `.nm.b`.
pub(crate) fn nm(s: S, size: f32, weight: f32) -> Div {
    let (family, weight) = super::fonts::face(DM_SANS, weight);
    div()
        .font_family(family)
        .text_size(s.px(size))
        .font_weight(FontWeight(weight))
        .line_height(s.px(size * 1.25))
        .text_color(hex(0xeef1f7))
}

/// `.ss`: the secondary line under a name.
pub(crate) fn ss(s: S, size: f32) -> Div {
    div()
        .font_family(super::fonts::dm_sans())
        .text_size(s.px(size))
        .line_height(s.px(size * 1.35))
        .text_color(hex(0x9ea7b6))
        .mt(s.px(2.0))
}

/// The onboarding icon set (`<Icon n=... size sw>`).
pub(crate) fn icon(s: S, name: &str, size: f32, stroke: f32, color: Hsla) -> Svg {
    let stroke = (stroke * 10.0).round() as u32;
    svg()
        .path(SharedString::from(format!(
            "onboarding/icon/{name}/{stroke}.svg"
        )))
        .size(s.px(size))
        .flex_none()
        .text_color(color)
}

/// An icon spun by `gxob-spin` (one turn per `period` seconds).
pub(crate) fn spinning_icon(
    s: S,
    name: &str,
    size: f32,
    color: Hsla,
    epoch: Instant,
    now: Instant,
    period: f32,
) -> Svg {
    let turns = now.saturating_duration_since(epoch).as_secs_f32() / period;
    icon(s, name, size, 1.6, color).with_transformation(Transformation::rotate(Radians(
        turns.fract() * std::f32::consts::TAU,
    )))
}

/// An icon scaled from nothing with `gxob-pop` (`cubic-bezier(0.3, 1.6, 0.5, 1)`, 0.4s).
pub(crate) fn popping_icon(
    s: S,
    name: &str,
    icon_size: f32,
    stroke: f32,
    color: Hsla,
    started: Instant,
    now: Instant,
) -> Svg {
    let scale = progress(
        started,
        now,
        std::time::Duration::from_millis(400),
        Ease::Bezier(0.3, 1.6, 0.5, 1.0),
    );
    icon(s, name, icon_size, stroke, color)
        .with_transformation(Transformation::scale(size(scale, scale)))
}

/// The Ghostex logo (`<GhostexLogo size glow>`).
///
/// CDXC:Icons 2026-11-02 DECISION: User approved "Replace the six-petal logo on the computer's onboarding screens with the new seven-petal icon? Yes, in this release". `packages/core-ui/onboarding/assets/ghostex-logo.webp` is the squircle tile of `apps/desktop/resources/AppIcon.png`, and the phone mock screenshots under `apps/desktop/assets/onboarding/` (`phone/` and `intro-video.webp`) carry the same icon patched in.
pub(crate) fn ghostex_logo(s: S, logo_size: f32, glow: bool) -> AnyElement {
    let image = img("onboarding/logo.webp")
        .size(s.px(logo_size))
        .flex_none();
    if glow {
        div()
            .size(s.px(logo_size))
            .flex_none()
            .rounded(s.px(logo_size * 0.24))
            .shadow(vec![shadow(
                rgba(60, 110, 255, 0.55),
                0.0,
                0.0,
                16.0,
                0.0,
                s,
            )])
            .child(image)
            .into_any_element()
    } else {
        image.into_any_element()
    }
}

/// The sidebar logo of an agent id (`<AgentLogo>`): the brand-tinted mask, the "Other agents"
/// cluster, or the terminal glyph.
pub(crate) fn agent_logo(
    s: S,
    catalog: &[CatalogAgent],
    agent_id: &str,
    logo_size: f32,
) -> AnyElement {
    if agent_id == "other" {
        return other_agents_cluster(s, catalog, logo_size);
    }
    if agent_id == "terminal" {
        return icon(s, "terminal", logo_size, 1.6, hex(0xf1f4f9)).into_any_element();
    }
    let Some(icon_id) = catalog
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.icon.clone())
    else {
        return icon(s, "sparkle", logo_size, 1.6, hex(0xf1f4f9)).into_any_element();
    };
    let path = SharedString::from(format!("agent-icons/{icon_id}.svg"));
    if icon_id == "omp" {
        return img(path)
            .size(s.px(logo_size))
            .flex_none()
            .into_any_element();
    }
    svg()
        .path(path)
        .size(s.px(logo_size))
        .flex_none()
        .text_color(hex(agent_logo_color(&icon_id)))
        .into_any_element()
}

/// `AGENT_LOGO_COLORS` (packages/core-ui/agent-logos.ts (deleted 2026-10-01)); the onboarding is always dark, so the
/// light-mode overrides never apply.
pub(crate) fn agent_logo_color(icon: &str) -> u32 {
    match icon {
        "antigravity-cli" => 0x749bff,
        "browser" => 0x82b7ff,
        "claude" => 0xd97757,
        "codebuddy" => 0x72d6ff,
        "command-code" => 0x22d3ee,
        "cursor-cli" => 0xedecec,
        "devin" => 0x3ea6ff,
        "dsh" => 0x4d6bfe,
        "empryo" => 0x1fa31d,
        "factory-droid" => 0xff7a1a,
        "hermes-agent" => 0xf3c46b,
        "kimi" => 0x7b6cf6,
        "kiro" => 0xa6e3ff,
        "omp" => 0xa663ed,
        "openclaude" => 0xf0a68a,
        "opencode" => 0x6d96c0,
        "pi" => 0xc8ff62,
        "qoder" => 0xa991ff,
        "rovo-dev" => 0x4fc3a1,
        _ => 0xffffff,
    }
}

/// The three agents the prototype shows inside the "Other agents" cluster.
pub(crate) const OTHER_AGENT_SAMPLE_IDS: [&str; 3] = ["copilot", "opencode", "pi"];

fn other_agents_cluster(s: S, catalog: &[CatalogAgent], cluster_size: f32) -> AnyElement {
    let cell = (cluster_size * 0.47).round();
    let cell_box = |child: AnyElement| {
        div()
            .size(s.px(cluster_size / 2.0))
            .flex()
            .items_center()
            .justify_center()
            .child(child)
    };
    let more = div()
        .size(s.px(cell))
        .flex()
        .items_center()
        .justify_center()
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(700.0))
        .text_size(s.px((cell * 0.9).max(8.0)))
        .line_height(s.px((cell * 0.9).max(8.0)))
        .text_color(hex(0xaab2c1))
        .child("+");
    div()
        .size(s.px(cluster_size))
        .flex_none()
        .flex()
        .flex_wrap()
        .children(
            OTHER_AGENT_SAMPLE_IDS
                .iter()
                .map(|id| cell_box(agent_logo(s, catalog, id, cell))),
        )
        .child(cell_box(more.into_any_element()))
        .into_any_element()
}

/// `<OtherAgentsStrip>`: three sample logos and `+N`.
pub(crate) fn other_agents_strip(
    s: S,
    catalog: &[CatalogAgent],
    logo_size: f32,
    more: usize,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(s.px(7.0))
        .children(
            OTHER_AGENT_SAMPLE_IDS
                .iter()
                .map(|id| agent_logo(s, catalog, id, logo_size)),
        )
        .when(more > 0, |this| {
            this.child(
                mono(s, 11.0, 600.0, hex(0xaab2c1))
                    .ml(s.px(1.0))
                    .child(format!("+{more}")),
            )
        })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToggleSize {
    Lg,
    Sm,
}

/// `.tg`: the onboarding switch. `thumb` is the animated 0..1 position of the thumb.
pub(crate) fn toggle(
    s: S,
    id: impl Into<SharedString>,
    on: bool,
    thumb: f32,
    toggle_size: ToggleSize,
    disabled: bool,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    // `transition: background 0.2s, border-color 0.2s, box-shadow 0.2s`. The on gradient is an
    // image, which does not interpolate: it appears at once while the flat colour fades.
    let flat = tween_color(
        &id,
        "bg",
        if on {
            hex(0x171a22).opacity(0.0)
        } else {
            hex(0x171a22)
        },
        200,
    );
    let border = tween_color(
        &id,
        "border",
        if on {
            rgba(140, 170, 255, 0.55)
        } else {
            white(0.08)
        },
        200,
    );
    let glow = tween_color(
        &id,
        "shadow",
        if on {
            rgba(50, 90, 240, 0.32)
        } else {
            rgba(50, 90, 240, 0.0)
        },
        200,
    );
    let (width, height, knob, top, left_off, left_on) = match toggle_size {
        ToggleSize::Lg => (60.0, 34.0, 24.0, 4.0, 4.0, 30.0),
        ToggleSize::Sm => (32.0, 19.0, 12.0, 2.5, 3.0, 16.0),
    };
    let left = left_off + (left_on - left_off) * thumb;
    div()
        .id(id)
        .relative()
        .flex_none()
        .w(s.px(width))
        .h(s.px(height))
        .rounded_full()
        .border_1()
        .border_color(border)
        .shadow(vec![shadow(glow, 0.0, 0.0, 14.0, 0.0, s)])
        .map(|this| {
            if on {
                this.bg(linear_gradient(
                    180.0,
                    linear_color_stop(hex(0x2d5ae8), 0.0),
                    linear_color_stop(hex(0x1f44c6), 1.0),
                ))
            } else {
                this.bg(flat)
            }
        })
        .when(disabled, |this| this.opacity(0.35))
        .when(!disabled, |this| this.cursor_pointer())
        .child(
            div()
                .absolute()
                .top(s.px(top))
                .left(s.px(left))
                .size(s.px(knob))
                .rounded_full()
                .bg(hex(0xf4f6fa))
                .shadow(vec![shadow(black(0.4), 0.0, 1.0, 3.0, 0.0, s)]),
        )
}

/// `.cta`: the panel's call to action.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CtaSize {
    /// The footer slot: 46px tall, 22px sides.
    Foot,
    /// `.cta.sm`: 38px tall, 16px sides, 14px type.
    Small,
}

pub(crate) fn cta(
    s: S,
    id: impl Into<SharedString>,
    label: impl IntoElement,
    filled: bool,
    arrow: bool,
    disabled: bool,
    cta_size: CtaSize,
) -> Stateful<Div> {
    let (height, side, font) = match cta_size {
        CtaSize::Foot => (46.0, 22.0, 15.5),
        CtaSize::Small => (38.0, 16.0, 14.0),
    };
    let id: SharedString = id.into();
    // `transition: filter 0.2s, border-color 0.2s, background 0.2s`; the arrow's `transform 0.2s`.
    let hover = !disabled && hovered(&id);
    let (bg, border) = match (filled, hover) {
        (true, false) => (hex(0x3c4570), white(0.14)),
        (true, true) => (hex(0x485282), white(0.22)),
        // The secondary call to action is a neutral white wash, never an outline
        // (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
        (false, false) => (white(0.10), white(0.0)),
        (false, true) => (white(0.14), white(0.0)),
    };
    let bg = tween_color(&id, "bg", bg, 200);
    let border = tween_color(&id, "border", border, 200);
    let nudge = tween_value(&id, "arrow", if hover { 3.0 } else { 0.0 }, 200);
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(14.0))
        .h(s.px(height))
        .px(s.px(side))
        .rounded(s.px(10.0))
        .border_1()
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(500.0))
        .text_size(s.px(font))
        .line_height(s.px(font * LH_DM_SANS))
        .text_color(gpui::white())
        .whitespace_nowrap()
        .bg(bg)
        .border_color(border)
        .when(filled, |this| {
            this.shadow(vec![
                shadow(black(0.45), 0.0, 2.0, 10.0, 0.0, s),
                inset_shadow(white(0.07), 0.0, 1.0, 0.0, 0.0, s),
            ])
        })
        .when(disabled, |this| this.opacity(0.5))
        .when(!disabled, |this| this.cursor_pointer())
        .when(!disabled && !filled, |this| {
            this.active(|this| this.bg(white(0.18)))
        })
        .child(label)
        .when(arrow, |this| {
            this.child(div().relative().left(s.px(nudge)).child(icon(
                s,
                "arrowR",
                18.0,
                1.6,
                gpui::white(),
            )))
        })
}

/// `.ghost`: a text button.
/// The modal host's base-layer `button:disabled { opacity: 0.58 }` (core-ui styles/theme.css), which every
/// onboarding button without its own `:disabled` opacity (`.ghost`, `.rescan`) inherits.
pub(crate) const DISABLED_BUTTON_OPACITY: f32 = 0.58;

pub(crate) fn ghost(
    s: S,
    id: impl Into<SharedString>,
    text: &str,
    font: f32,
    disabled: bool,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    // `.ghost { transition: color 0.2s }`; its `:hover` has no `:not(:disabled)`.
    let color = hover_color(&id, "color", hex(0xd6dbe5), gpui::white(), 200);
    div()
        .id(id)
        .relative()
        .flex()
        .items_center()
        .h(s.px(44.0))
        .px(s.px(4.0))
        .flex_none()
        .font_family(super::fonts::dm_sans())
        .text_size(s.px(font))
        .line_height(s.px(font * LH_DM_SANS))
        .text_color(color)
        .whitespace_nowrap()
        .when(!disabled, |this| this.cursor_pointer())
        .when(disabled, |this| this.opacity(DISABLED_BUTTON_OPACITY))
        .child(text.to_string())
}

/// `.icon-btn`: 28px square icon button.
pub(crate) fn icon_button(
    s: S,
    id: impl Into<SharedString>,
    name: &str,
    icon_size: f32,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    // `.icon-btn` has no transition of its own: the host's 120ms button transition.
    let bg = hover_color(&id, "bg", white(0.0), white(0.07), BUTTON_MS);
    let color = hover_color(&id, "color", hex(0xaab2c1), gpui::white(), BUTTON_MS);
    div()
        .id(id)
        .relative()
        .size(s.px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(s.px(7.0))
        .text_color(color)
        .bg(bg)
        .cursor_pointer()
        .child(
            svg()
                .path(SharedString::from(format!("onboarding/icon/{name}/16.svg")))
                .size(s.px(icon_size))
                .text_color(color),
        )
}

/// `<Eyebrow>`: the blue dot and the tracked mono kicker above a heading.
pub(crate) fn eyebrow(s: S, x: f32, y: f32, width: Option<f32>, text: &str) -> Div {
    abs(s, x, y, width, None)
        .flex()
        .items_center()
        .gap(s.px(14.0))
        .when(width.is_some(), |this| this.justify_center())
        .child(
            div()
                .size(s.px(8.0))
                .flex_none()
                .rounded_full()
                .bg(hex(0x2849b8))
                .shadow(vec![shadow(
                    rgba(60, 100, 255, 0.55),
                    0.0,
                    0.0,
                    8.0,
                    0.0,
                    s,
                )]),
        )
        .child(tracked_text(
            s,
            text.to_uppercase(),
            PLEX_MONO,
            500.0,
            12.5,
            12.5,
            0.26,
            hex(0xcdd3de),
        ))
}

/// `<Heading>`: one or two lines of Manrope 700, the second in the blue gradient.
pub(crate) fn heading(
    s: S,
    x: f32,
    y: f32,
    width: f32,
    font: f32,
    line1: &str,
    line2: Option<&str>,
    center: bool,
) -> Div {
    let line = |text: &str| {
        tracked_text(
            s,
            text.to_string(),
            MANROPE,
            700.0,
            font,
            font,
            -0.04,
            hex(0xf3f5f9),
        )
    };
    abs(s, x, y, Some(width), None)
        .flex()
        .flex_col()
        .when(center, |this| this.items_center())
        .child(line(line1))
        .when_some(line2, |this, line2| {
            this.child(line(line2).gradient(vec![
                (0.0, hex(0xd5ddff)),
                (0.55, hex(0xaebff5)),
                (1.0, hex(0x93a9ee)),
            ]))
        })
}

/// `<Sub>`: the lead paragraph under a heading.
pub(crate) fn sub(s: S, x: f32, y: f32, width: f32, font: f32, center: bool) -> Div {
    abs(s, x, y, Some(width), None)
        .font_family(super::fonts::dm_sans())
        .text_size(s.px(font))
        .line_height(s.px(font * 1.45))
        .text_color(hex(0xb2b9c6))
        .when(center, |this| this.text_center())
}

/// `.glass`: the frosted card surface.
pub(crate) fn glass(s: S) -> Div {
    div()
        .bg(linear_gradient(
            180.0,
            linear_color_stop(rgba(21, 24, 33, 0.74), 0.0),
            linear_color_stop(rgba(11, 13, 19, 0.74), 1.0),
        ))
        .border_1()
        .border_color(rgba(150, 165, 205, 0.12))
        .rounded(s.px(14.0))
        .shadow(vec![inset_shadow(white(0.045), 0.0, 1.0, 0.0, 0.0, s)])
}

/// `.ibox`: the icon tile.
pub(crate) fn ibox(s: S, box_size: f32, radius: f32) -> Div {
    div()
        .size(s.px(box_size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(s.px(radius))
        .border_1()
        .border_color(white(0.1))
        .bg(white(0.03))
}

/// `.abox`: the agent logo tile.
pub(crate) fn abox(s: S, box_size: f32, radius: f32, margin_right: f32) -> Div {
    div()
        .size(s.px(box_size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(s.px(radius))
        .border_1()
        .border_color(white(0.08))
        .bg(white(0.03))
        .mr(s.px(margin_right))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PillKind {
    Run,
    Done,
    Idle,
    Need,
}

/// `<StatusPill>` (`.dpill`) at its default size; `pill_size` is (height, side padding, font, gap).
pub(crate) fn status_pill(
    s: S,
    kind: PillKind,
    text: &str,
    epoch: Instant,
    now: Instant,
    (height, side, font, gap): (f32, f32, f32, f32),
) -> Div {
    let (color, background, border) = match kind {
        PillKind::Run => (
            hex(0x3be3a2),
            rgba(59, 227, 162, 0.08),
            rgba(59, 227, 162, 0.3),
        ),
        PillKind::Done => (
            hex(0x9ec2ff),
            rgba(80, 120, 255, 0.1),
            rgba(110, 150, 255, 0.35),
        ),
        PillKind::Idle => (hex(0x7c8598), white(0.03), white(0.08)),
        PillKind::Need => (
            hex(0xffc46b),
            rgba(255, 180, 70, 0.08),
            rgba(255, 180, 70, 0.35),
        ),
    };
    let dot_opacity = match kind {
        PillKind::Run => breathe(epoch, now, 1.4, 0.0),
        PillKind::Need => breathe(epoch, now, 0.8, 0.0),
        _ => 1.0,
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(gap))
        .h(s.px(height))
        .px(s.px(side))
        .rounded(s.px(7.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(500.0))
        .text_size(s.px(font))
        .line_height(s.px(font * LH_DM_SANS))
        .text_color(color)
        .whitespace_nowrap()
        .child(
            div()
                .size(s.px(6.0))
                .rounded_full()
                .bg(color)
                .opacity(dot_opacity),
        )
        .child(text.to_string())
}

pub(crate) const PILL_DEFAULT: (f32, f32, f32, f32) = (24.0, 9.0, 11.5, 6.0);

/// `<Lights>`: the three window buttons.
pub(crate) fn lights(s: S, small: bool) -> Div {
    let (dot, gap) = if small { (5.0, 4.0) } else { (10.0, 9.0) };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(gap))
        .children(
            [0xff5f57, 0xfebc2e, 0x28c840]
                .into_iter()
                .map(move |color| div().size(s.px(dot)).rounded_full().bg(hex(color))),
        )
}

/// `.spinner`: a ring with a lighter top quarter, one turn per 0.9s.
pub(crate) fn spinner(
    s: S,
    spinner_size: f32,
    border: f32,
    epoch: Instant,
    now: Instant,
) -> impl IntoElement {
    let turns = now.saturating_duration_since(epoch).as_secs_f32() / 0.9;
    let angle = turns.fract() * std::f32::consts::TAU;
    let scale = s.0;
    div().size(s.px(spinner_size)).flex_none().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let size_px = bounds.size.width.as_f32();
                let width = border * scale;
                let radius = (size_px - width) / 2.0;
                let center = bounds.center();
                let ring = |start: f32, sweep: f32| {
                    let mut path = gpui::PathBuilder::stroke(px(width));
                    let steps = 24;
                    for step in 0..=steps {
                        let a = start + sweep * step as f32 / steps as f32;
                        let p = point(
                            center.x + px(radius * a.cos()),
                            center.y + px(radius * a.sin()),
                        );
                        if step == 0 {
                            path.move_to(p);
                        } else {
                            path.line_to(p);
                        }
                    }
                    path.build().ok()
                };
                if let Some(path) = ring(0.0, std::f32::consts::TAU) {
                    window.paint_path(path, white(0.14));
                }
                let top = -std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4 + angle;
                if let Some(path) = ring(top, std::f32::consts::FRAC_PI_2) {
                    window.paint_path(path, hex(0xc3cad6));
                }
            },
        )
        .size_full(),
    )
}

/// `<QrCode>`: the decorative pairing code drawn in the previews.
pub(crate) fn qr_code(s: S, qr_size: f32) -> impl IntoElement {
    const CELLS: &str = "0,0 5,0 10,0 15,0 20,0 25,0 30,0 45,0 55,0 70,0 75,0 80,0 85,0 90,0 95,0 100,0 0,5 30,5 50,5 70,5 100,5 0,10 10,10 15,10 20,10 30,10 45,10 60,10 70,10 80,10 85,10 90,10 100,10 0,15 10,15 15,15 20,15 30,15 45,15 70,15 80,15 85,15 90,15 100,15 0,20 10,20 15,20 20,20 30,20 55,20 60,20 70,20 80,20 85,20 90,20 100,20 0,25 30,25 50,25 70,25 100,25 0,30 5,30 10,30 15,30 20,30 25,30 30,30 40,30 50,30 60,30 70,30 75,30 80,30 85,30 90,30 95,30 100,30 30,40 50,40 10,45 20,45 25,45 35,45 55,45 65,45 75,45 80,45 85,45 90,45 100,45 0,50 10,50 25,50 30,50 40,50 45,50 70,50 75,50 85,50 90,50 10,55 15,55 20,55 35,55 45,55 50,55 55,55 60,55 65,55 75,55 0,60 25,60 30,60 45,60 50,60 60,60 65,60 70,60 75,60 85,60 90,60 95,60 45,65 50,65 55,65 65,65 75,65 95,65 100,65 0,70 5,70 10,70 15,70 20,70 25,70 30,70 45,70 55,70 60,70 65,70 70,70 75,70 85,70 95,70 100,70 0,75 30,75 45,75 65,75 70,75 75,75 100,75 0,80 10,80 15,80 20,80 30,80 50,80 75,80 100,80 0,85 10,85 15,85 20,85 30,85 50,85 55,85 70,85 75,85 80,85 85,85 95,85 100,85 0,90 10,90 15,90 20,90 30,90 55,90 60,90 65,90 85,90 90,90 95,90 100,90 0,95 30,95 50,95 75,95 100,95 0,100 5,100 10,100 15,100 20,100 25,100 30,100 45,100 50,100 65,100 70,100 80,100 85,100 95,100";
    div().size(s.px(qr_size)).flex_none().child(
        canvas(
            |_, _, _| {},
            |bounds, _, window, _| {
                // viewBox -6 -6 117 117: a light rounded plate with 5x5 dark modules.
                let unit = bounds.size.width.as_f32() / 117.0;
                let origin = bounds.origin;
                window.paint_quad(gpui::fill(bounds, hex(0xf1f3f7)).corner_radii(px(6.0 * unit)));
                for cell in CELLS.split(' ') {
                    let Some((x, y)) = cell.split_once(',') else {
                        continue;
                    };
                    let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) else {
                        continue;
                    };
                    let module = gpui::Bounds::new(
                        point(
                            origin.x + px((x + 6.0) * unit),
                            origin.y + px((y + 6.0) * unit),
                        ),
                        size(px(5.0 * unit), px(5.0 * unit)),
                    );
                    window.paint_quad(gpui::fill(module, hex(0x0b0d12)));
                }
            },
        )
        .size_full(),
    )
}

/// `.detpill`: the status chip at the end of an agent row.
pub(crate) fn detpill(
    s: S,
    text: impl IntoElement,
    tone: DetTone,
    (height, side, font): (f32, f32, f32),
) -> Div {
    let (color, border, background) = match tone {
        DetTone::Detected => (
            hex(0x3f63cf),
            rgba(63, 99, 207, 0.24),
            rgba(40, 70, 160, 0.07),
        ),
        DetTone::Wait => (hex(0x6b7383), white(0.08), rgba(40, 70, 160, 0.07)),
        DetTone::On => (
            hex(0x3be3a2),
            rgba(59, 227, 162, 0.4),
            rgba(59, 227, 162, 0.07),
        ),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(s.px(height))
        .px(s.px(side))
        .rounded(s.px(9.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .font_family(super::fonts::dm_sans())
        .text_size(s.px(font))
        .line_height(s.px(font * LH_DM_SANS))
        .text_color(color)
        .whitespace_nowrap()
        .child(text)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetTone {
    Detected,
    Wait,
    On,
}

pub(crate) const DETPILL_DEFAULT: (f32, f32, f32) = (32.0, 16.0, 14.0);

/// `.install-btn`: the Install / Retry / Add to PATH button.
pub(crate) fn install_button(
    s: S,
    id: impl Into<SharedString>,
    icon_name: &str,
    text: &str,
    disabled: bool,
    (height, left, right, font, icon_size): (f32, f32, f32, f32, f32),
) -> Stateful<Div> {
    let id: SharedString = id.into();
    // `transition: border-color 0.2s, background 0.2s`.
    let hover = !disabled && hovered(&id);
    let border = tween_color(
        &id,
        "border",
        if hover {
            rgba(150, 180, 255, 0.7)
        } else {
            rgba(110, 150, 255, 0.45)
        },
        200,
    );
    let bg = tween_color(
        &id,
        "bg",
        if hover {
            rgba(60, 90, 200, 0.24)
        } else {
            rgba(60, 90, 200, 0.14)
        },
        200,
    );
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(7.0))
        .h(s.px(height))
        .pl(s.px(left))
        .pr(s.px(right))
        .rounded(s.px(9.0))
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(500.0))
        .text_size(s.px(font))
        .line_height(s.px(font * LH_DM_SANS))
        .text_color(hex(0xeef1f7))
        .whitespace_nowrap()
        .when(!disabled, |this| this.cursor_pointer())
        .when(disabled, |this| this.opacity(0.5))
        .child(icon(s, icon_name, icon_size, 2.0, hex(0xeef1f7)))
        .child(text.to_string())
}

pub(crate) const INSTALL_BUTTON_DEFAULT: (f32, f32, f32, f32, f32) = (36.0, 13.0, 16.0, 14.0, 15.0);

/// `.guide-btn`: the "Install guide" button, a neutral white wash rather than the prototype's
/// outline (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
pub(crate) fn guide_button(s: S, id: impl Into<SharedString>) -> Stateful<Div> {
    let id: SharedString = id.into();
    // No transition of its own: the host's 120ms button transition.
    let bg = hover_color(&id, "bg", white(0.10), white(0.14), BUTTON_MS);
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .h(s.px(40.0))
        .px(s.px(18.0))
        .rounded(s.px(10.0))
        .border_1()
        .border_color(white(0.0))
        .bg(bg)
        .active(|this| this.bg(white(0.18)))
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(500.0))
        .text_size(s.px(15.0))
        .line_height(s.px(15.0 * LH_DM_SANS))
        .text_color(hex(0xeef1f7))
        .whitespace_nowrap()
        .cursor_pointer()
        .child("Install guide")
}
