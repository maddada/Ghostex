//! Flattens the marks onto the picture for saving, copying and sending.
//!
//! The marks are written as one SVG the size of the picture and drawn by resvg (already used for
//! Mermaid, with the system fonts loaded for text), then blended over the picture.

use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};

use image::RgbaImage;

use super::model::{EditorDoc, Mark, Metrics};

fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The three points of an arrow's head, and where the shaft should stop so its square end does
/// not poke through the tip.
pub(crate) fn arrow_head(
    from: (f32, f32),
    to: (f32, f32),
    size: f32,
) -> ([(f32, f32); 3], (f32, f32)) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / length, dy / length);
    let size = size.min(length * 0.6);
    let base = (to.0 - ux * size, to.1 - uy * size);
    let (px, py) = (-uy * size * 0.55, ux * size * 0.55);
    (
        [to, (base.0 + px, base.1 + py), (base.0 - px, base.1 - py)],
        (to.0 - ux * size * 0.8, to.1 - uy * size * 0.8),
    )
}

fn hex(color: u32) -> String {
    format!("#{color:06x}")
}

/// The SVG of one text label (all its lines).
fn text_svg(at: &super::model::P, text: &str, color: u32, metrics: Metrics) -> String {
    let size = metrics.text_size();
    let mut svg = String::new();
    for (line_index, line) in text.lines().enumerate() {
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" font-family="Inter, Helvetica Neue, Segoe UI, Arial, sans-serif" font-weight="700" font-size="{size}" fill="{}" stroke="rgba(0,0,0,0.55)" stroke-width="{}" paint-order="stroke">{}</text>"#,
            at.x + size * 0.2,
            at.y + size * (1.0 + line_index as f32 * 1.25),
            hex(color),
            size * 0.12,
            escape(line)
        );
    }
    svg
}

/// Where a label's glyphs actually land, measured by laying it out alone.
fn text_bounds(
    width: u32,
    height: u32,
    at: &super::model::P,
    text: &str,
    color: u32,
    metrics: Metrics,
) -> Option<(f32, f32, f32, f32)> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{}</svg>"#,
        text_svg(at, text, color, metrics)
    );
    let options = resvg::usvg::Options {
        fontdb: fontdb(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &options).ok()?;
    let bounds = tree.root().abs_bounding_box()?;
    Some((bounds.x(), bounds.y(), bounds.width(), bounds.height()))
}

pub(crate) fn marks_svg(width: u32, height: u32, marks: &[Mark], metrics: Metrics) -> String {
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    let stroke = metrics.stroke();
    for mark in marks {
        match mark {
            Mark::Arrow { from, to, color } => {
                let (head, shaft_end) =
                    arrow_head((from.x, from.y), (to.x, to.y), metrics.arrow_head());
                let _ = write!(
                    svg,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{stroke}" stroke-linecap="round"/><polygon points="{},{} {},{} {},{}" fill="{}"/>"#,
                    from.x,
                    from.y,
                    shaft_end.0,
                    shaft_end.1,
                    hex(*color),
                    head[0].0,
                    head[0].1,
                    head[1].0,
                    head[1].1,
                    head[2].0,
                    head[2].1,
                    hex(*color)
                );
            }
            Mark::Rect { area, color } => {
                let _ = write!(
                    svg,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{}" stroke-width="{stroke}" rx="{}"/>"#,
                    area.left,
                    area.top,
                    area.width(),
                    area.height(),
                    hex(*color),
                    stroke
                );
            }
            Mark::Text {
                at,
                text,
                color,
                background,
            } => {
                if *background
                    && let Some((x, y, w, h)) =
                        text_bounds(width, height, at, text, *color, metrics)
                {
                    let pad = metrics.text_size() * 0.25;
                    let [r, g, b, a] = super::view::TEXT_BACKGROUND.to_be_bytes();
                    let _ = write!(
                        svg,
                        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="rgb({r},{g},{b})" fill-opacity="{}"/>"#,
                        x - pad,
                        y - pad * 0.6,
                        w + pad * 2.0,
                        h + pad * 1.2,
                        metrics.text_size() * 0.22,
                        a as f32 / 255.0
                    );
                }
                svg.push_str(&text_svg(at, text, *color, metrics));
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

/// The picture with every mark drawn into it.
pub(crate) fn flatten(doc: &EditorDoc) -> RgbaImage {
    let mut base = doc.image.as_ref().clone();
    if doc.marks.is_empty() {
        return base;
    }
    let (width, height) = (base.width(), base.height());
    let svg = marks_svg(width, height, &doc.marks, doc.metrics);
    let options = resvg::usvg::Options {
        fontdb: fontdb(),
        ..Default::default()
    };
    let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &options) else {
        return base;
    };
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(width, height) else {
        return base;
    };
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    for (pixel, overlay) in base.pixels_mut().zip(pixmap.pixels()) {
        let alpha = overlay.alpha() as u32;
        if alpha == 0 {
            continue;
        }
        // tiny-skia pixels are premultiplied: out = overlay + base * (1 - alpha).
        let inverse = 255 - alpha;
        pixel.0[0] = (overlay.red() as u32 + pixel.0[0] as u32 * inverse / 255).min(255) as u8;
        pixel.0[1] = (overlay.green() as u32 + pixel.0[1] as u32 * inverse / 255).min(255) as u8;
        pixel.0[2] = (overlay.blue() as u32 + pixel.0[2] as u32 * inverse / 255).min(255) as u8;
        pixel.0[3] = 255;
    }
    base
}
