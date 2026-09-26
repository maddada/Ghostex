//! Render a formula to a gpui image — the display primitive (the `ratex-gtk4` analog).
//!
//! `model → LaTeX → RaTeX layout → display list → ratex-render raster (PNG) → decode →
//! BGRA → gpui RenderImage`. RaTeX only exposes a PNG encoder (not the raw pixmap), so we
//! round-trip through PNG for now; a pixmap accessor (or a small fork) would skip the
//! re-decode. RaTeX rasters black-on-opaque-white; we recolor to the host's text color on a
//! transparent background (a pixel's darkness becomes the glyph alpha), so a formula blends
//! into any theme.

use crate::editor::model::Row;
use gpui::{Hsla, RenderImage, Rgba};
use image::{Frame, RgbaImage};
use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parse;
use ratex_render::{RenderOptions, render_to_png};
use std::sync::Arc;

/// Logical padding (px) RaTeX leaves around the formula. The view offsets caret/slot
/// geometry by this much, since the same value feeds `RenderOptions::padding`.
pub const PAD: f32 = 8.0;

/// A rasterized formula plus its logical (pre-DPR) size in px.
pub struct Rendered {
    pub image: Arc<RenderImage>,
    pub width: f32,
    pub height: f32,
}

/// Render a row to a gpui image at `font_size` px/em and `dpr` device-pixel-ratio.
/// `None` if the row's LaTeX fails to parse / lay out / rasterize.
pub fn render_row(row: &Row, font_size: f32, dpr: f32, color: Hsla) -> Option<Rendered> {
    render_latex(&row.to_latex(), font_size, dpr, color)
}

/// Render raw LaTeX to a gpui image — the display path for a `$$…$$` block, which has no
/// edit model. Glyphs are painted in `color` (the host's text color) on a transparent
/// background. `None` if the LaTeX fails to parse / lay out / rasterize.
pub fn render_latex(latex: &str, font_size: f32, dpr: f32, color: Hsla) -> Option<Rendered> {
    let c: Rgba = color.into();
    let rgb = [
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
    ];
    let (bgra, w, h) = rasterize(latex, font_size, dpr, rgb)?;
    let buf = RgbaImage::from_raw(w, h, bgra)?;
    Some(Rendered {
        image: Arc::new(RenderImage::new(vec![Frame::new(buf)])),
        width: w as f32 / dpr,
        height: h as f32 / dpr,
    })
}

/// The gpui-free half: LaTeX → BGRA pixels + pixel dimensions. Separated so it can be
/// unit-tested without a gpui context.
fn rasterize(latex: &str, font_size: f32, dpr: f32, rgb: [u8; 3]) -> Option<(Vec<u8>, u32, u32)> {
    let nodes = parse(latex).ok()?;
    let lbox = layout(&nodes, &LayoutOptions::default());
    let dl = to_display_list(&lbox);
    let opts = RenderOptions {
        font_size,
        padding: PAD,
        device_pixel_ratio: dpr,
        ..Default::default()
    };
    let png = render_to_png(&dl, &opts).ok()?;
    let rgba = image::load_from_memory(&png).ok()?.into_rgba8();
    let (w, h) = rgba.dimensions();
    let mut bytes = rgba.into_raw();
    // RaTeX rasters black glyphs on opaque white. Recolor to `rgb` on a TRANSPARENT background
    // so the formula blends into any theme: a pixel's darkness becomes the glyph alpha (white
    // bg → 0, black glyph → 255), painting `rgb` flat. gpui's RenderImage holds STRAIGHT
    // (non-premultiplied) BGRA, so the color must NOT be premultiplied — that double-darkens it.
    let [r, g, b] = rgb;
    for px in bytes.as_chunks_mut::<4>().0 {
        let a = 255 - px[0]; // grayscale raster: the red channel is the luminance
        px[0] = b;
        px[1] = g;
        px[2] = r;
        px[3] = a;
    }
    Some((bytes, w, h))
}

/// Render raw LaTeX to PNG file bytes (black on white) at the given font size and DPR.
/// Suitable for writing directly to a `.png` file.
pub fn render_latex_to_png(latex: &str, font_size: f32, dpr: f32) -> Option<Vec<u8>> {
    let nodes = parse(latex).ok()?;
    let lbox = layout(&nodes, &LayoutOptions::default());
    let dl = to_display_list(&lbox);
    let opts = RenderOptions {
        font_size,
        padding: PAD,
        device_pixel_ratio: dpr,
        ..Default::default()
    };
    render_to_png(&dl, &opts).ok()
}

/// Render raw LaTeX to an SVG string at the given font size.
pub fn render_latex_to_svg(latex: &str, font_size: f32) -> Option<String> {
    let nodes = parse(latex).ok()?;
    let lbox = layout(&nodes, &LayoutOptions::default());
    let dl = to_display_list(&lbox);
    let opts = ratex_svg::SvgOptions {
        font_size: font_size as f64,
        padding: PAD as f64,
        // Embed glyph outlines so the file is self-contained — otherwise it references KaTeX
        // font-families the viewer lacks and renders with fallback shapes.
        embed_glyphs: true,
        ..Default::default()
    };
    Some(ratex_svg::render_to_svg(&dl, &opts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::Row;

    #[test]
    fn rasterizes_to_nonempty_bgra() {
        let (bytes, w, h) =
            rasterize(&Row::syms("abc").to_latex(), 40.0, 2.0, [0, 0, 0]).expect("renders");
        assert!(w > 0 && h > 0, "non-empty dims");
        assert_eq!(
            bytes.len(),
            w as usize * h as usize * 4,
            "BGRA buffer matches dims"
        );
    }

    #[test]
    fn dpr_scales_pixels() {
        let (_, w1, _) = rasterize(&Row::syms("x").to_latex(), 40.0, 1.0, [0, 0, 0]).unwrap();
        let (_, w2, _) = rasterize(&Row::syms("x").to_latex(), 40.0, 2.0, [0, 0, 0]).unwrap();
        assert!(w2 > w1, "2x DPR yields more pixels ({w2} vs {w1})");
    }

    #[test]
    fn empty_row_rasterizes() {
        // The editor starts empty -> "\square"; if RaTeX can't render that, it's blank.
        let (_, w, h) = rasterize(&Row::new().to_latex(), 48.0, 2.0, [0, 0, 0])
            .expect("empty row (\\square) must rasterize");
        assert!(w > 0 && h > 0, "non-empty dims, got {w}x{h}");
    }

    #[test]
    fn render_latex_renders_raw_latex() {
        // The display path: a $$…$$ block's raw LaTeX, no edit model in the loop.
        let r = render_latex(r"\frac{1}{2} + \sqrt{x}", 18.0, 2.0, gpui::black()).expect("renders");
        assert!(
            r.width > 0.0 && r.height > 0.0,
            "non-empty: {}x{}",
            r.width,
            r.height
        );
    }

    #[test]
    fn svg_export_is_self_contained() {
        // The exported SVG must embed glyph outlines (`<path>`), not `<text>` that references
        // KaTeX font-families a standalone viewer won't have (which renders the wrong shapes).
        let svg = render_latex_to_svg(r"\sqrt{7x \cdot 3}", 48.0).expect("renders");
        assert!(svg.contains("<svg"), "is an SVG document");
        assert!(svg.contains("<path"), "embeds glyph outlines");
        assert!(
            !svg.contains("<text"),
            "no <text> elements that depend on the viewer's fonts"
        );
    }

    #[test]
    fn png_export_is_png() {
        let png = render_latex_to_png(r"\sqrt{x}", 48.0, 4.0).expect("renders");
        assert_eq!(&png[1..4], b"PNG", "PNG magic header");
    }
}
