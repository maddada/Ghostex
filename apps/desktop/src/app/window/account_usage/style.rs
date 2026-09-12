pub(super) use super::text::tracked;
use crate::*;
use gpui::{Background, BoxShadow, SharedString, linear_color_stop, linear_gradient};
use std::sync::Arc;

/// CDXC:AgentProviders 2026-09-12 DECISION:
/// User: migrate usage dropdowns to GPUI with exactly the current style, UX, colors and layout, rather than adopting the Tips panel's appearance.
/// User picked Claude #d97757 and Codex #8cbbe8, with an accent glow, translucent cards, gradient bars, and tinted neutrals.
/// User wanted a lighter background and a fixed 1px outline to distinguish the popup from the chat behind it.
/// The later Redeem button color revision is owned by limits.rs.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub accent: Hsla,
    pub deep: Hsla,
    pub light: Hsla,
    pub soft: Hsla,
    pub accent_line: Hsla,
    pub line: Hsla,
    pub strong: Hsla,
    pub muted: Hsla,
    pub dim: Hsla,
    pub raised: Hsla,
    pub outline: Hsla,
    pub codex: bool,
}

pub(super) fn mix(a: Hsla, weight: f32, b: Hsla) -> Hsla {
    let a = gpui::Rgba::from(a);
    let b = gpui::Rgba::from(b);
    gpui::Rgba {
        r: a.r * weight + b.r * (1.0 - weight),
        g: a.g * weight + b.g * (1.0 - weight),
        b: a.b * weight + b.b * (1.0 - weight),
        a: a.a * weight + b.a * (1.0 - weight),
    }
    .into()
}

impl Palette {
    pub fn new(codex: bool) -> Self {
        let accent: Hsla = rgb(if codex { 0x8cbbe8 } else { 0xd97757 }).into();
        Self {
            accent,
            deep: mix(accent, 0.86, rgb(0).into()),
            light: mix(accent, 0.78, rgb(0xffffff).into()),
            soft: accent.opacity(0.14),
            accent_line: accent.opacity(0.34),
            line: mix(accent, 0.09, rgb(0x2b2a29).into()),
            strong: mix(accent, 0.12, rgb(0x363433).into()),
            muted: mix(accent, 0.12, rgb(0xa09c98).into()),
            dim: mix(accent, 0.10, rgb(0x78746f).into()),
            raised: mix(accent, 0.04, rgb(0x201f1e).into()),
            outline: mix(accent, 0.16, rgb(0x4a4846).into()),
            codex,
        }
    }

    pub fn card(self) -> gpui::Div {
        v_flex()
            .w_full()
            .flex_shrink_0()
            .p(px(14.0))
            .pt(px(13.0))
            .border_1()
            .border_color(self.line)
            .rounded(px(12.0))
            .bg(rgb(0x1a1918).opacity(0.90))
            .shadow(vec![
                shadow(0.0, 1.0, 0.0, rgb(0xffffff).opacity(0.035).into()).inset(),
                shadow(0.0, 10.0, 28.0, rgb(0).opacity(0.28).into()),
            ])
    }

    pub fn tag(self, accent: bool) -> gpui::Div {
        h_flex()
            .flex_shrink_0()
            .h(px(19.0))
            .px(px(8.0))
            .gap(px(5.0))
            .items_center()
            .rounded_full()
            .whitespace_nowrap()
            .bg(if accent {
                self.soft
            } else {
                rgb(0x222222).into()
            })
            .text_color(if accent {
                self.accent
            } else {
                rgb(0xb4b4b2).into()
            })
            .text_size(px(10.0))
            .line_height(px(14.5))
            .font_weight(usage_font_weight(550.0))
    }

    pub fn outline_button(self, id: &'static str) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id)
            .flex_1()
            .h(px(30.0))
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .border_1()
            .border_color(self.line)
            .rounded(px(8.0))
            .bg(rgb(0x1a1918).opacity(0.90))
            .text_color(rgb(0xe2dfdc))
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .shadow(vec![
                shadow(0.0, 1.0, 0.0, rgb(0xffffff).opacity(0.035).into()).inset(),
            ])
            .cursor_pointer()
            .hover(move |this| {
                this.bg(self.raised)
                    .border_color(mix(self.accent, 0.28, self.strong))
                    .text_color(rgb(0xffffff))
            })
    }

    // SVG is only the fixed decorative background. All text, layout, controls,
    // scrolling and events are GPUI; there is no browser or rendered HTML.
    pub fn background(self, width: f32, height: f32) -> Arc<gpui::Image> {
        let color = |c: Hsla| {
            let c = gpui::Rgba::from(c);
            format!("rgb({:.3},{:.3},{:.3})", c.r * 255., c.g * 255., c.b * 255.)
        };
        let cx = width * 0.88;
        let cy = height * -0.04;
        let radius = cx.max(width - cx).hypot(cy.max(height - cy));
        let radius2 = (width * 1.1).hypot(height * 1.1);
        let top = color(mix(self.accent, 0.30, rgb(0x0e0e0e).into()));
        let middle = color(mix(self.accent, 0.12, rgb(0x0e0e0e).into()));
        let bottom = color(mix(self.accent, 0.07, rgb(0x0e0e0e).into()));
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><defs><radialGradient id="a" gradientUnits="userSpaceOnUse" cx="{cx}" cy="{cy}" r="{radius}" color-interpolation="sRGB"><stop stop-color="{top}"/><stop offset=".22" stop-color="{middle}"/><stop offset=".46" stop-color="{middle}" stop-opacity="0"/></radialGradient><radialGradient id="b" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{radius2}" color-interpolation="sRGB"><stop stop-color="{bottom}"/><stop offset=".34" stop-color="{bottom}" stop-opacity="0"/></radialGradient></defs><path fill="#111" d="M0 0H{width}V{height}H0Z"/><path fill="url(#b)" d="M0 0H{width}V{height}H0Z"/><path fill="url(#a)" d="M0 0H{width}V{height}H0Z"/></svg>"##,
            -0.1 * width,
            1.1 * height
        );
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            svg.into_bytes(),
        ))
    }
}

pub(super) fn gradient(angle: f32, from: Hsla, to: Hsla) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(from, 0.0),
        linear_color_stop(to, 1.0),
    )
}

pub(super) fn shadow(x: f32, y: f32, blur: f32, color: Hsla) -> BoxShadow {
    BoxShadow::new(px(x), px(y), color).blur_radius(px(blur))
}

/// GPUI's discrete macOS font matching rounds intermediate weights up to the next face.
/// Chromium's system-font headings in the reference use the lower face (650/620 semibold, 550 medium).
pub(super) fn usage_font_weight(css_weight: f32) -> FontWeight {
    FontWeight(if cfg!(target_os = "macos") {
        (css_weight / 100.0).floor() * 100.0
    } else {
        css_weight
    })
}

pub(super) fn heading(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(12.5))
        .line_height(px(18.125))
        .font_weight(usage_font_weight(620.0))
        .child(tracked(text, -0.05))
}

pub(super) fn section_heading() -> gpui::Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .mb(px(12.0))
}

pub(super) fn label(text: impl Into<SharedString>, size: f32, color: Hsla) -> gpui::Div {
    div()
        .text_size(px(size))
        .line_height(px(size * 1.45))
        .text_color(color)
        .child(text.into())
}

pub(super) fn tabular_numbers() -> gpui::FontFeatures {
    gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

pub(super) fn focus_ring(palette: Palette) -> Vec<BoxShadow> {
    vec![
        shadow(0.0, 0.0, 0.0, palette.accent).spread_radius(px(4.0)),
        shadow(0.0, 0.0, 0.0, rgb(0x1a1918).into()).spread_radius(px(2.0)),
    ]
}

pub(super) fn chevron(down: bool, up: bool, color: Hsla) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let points = if up {
                [(1.5, 6.0), (5.0, 2.5), (8.5, 6.0)]
            } else if down {
                [(1.5, 3.5), (5.0, 7.0), (8.5, 3.5)]
            } else {
                [(3.5, 1.5), (7.0, 5.0), (3.5, 8.5)]
            };
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (index, (x, y)) in points.into_iter().enumerate() {
                let position = bounds.origin + point(px(x), px(y));
                if index == 0 {
                    path.move_to(position);
                } else {
                    path.line_to(position);
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(10.0))
    .flex_shrink_0()
    .into_any_element()
}
