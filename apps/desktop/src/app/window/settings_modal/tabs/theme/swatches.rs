//! The theme colour squares' one flat colour. Each square used to be a gradient bitmap
//! (`swatchStyle`: two accent glows over a 150deg gradient and a white sheen); since
//! CDXC:Theming 2026-10-16 (in mod.rs) it is the average of that gradient without the sheen, so
//! every colour keeps the hue it had and the square is flat.
use super::colours::{Rgb, SwatchPaint};

/// How many samples per side the average takes.
const SAMPLES: u32 = 16;

fn over(color: &mut [f64; 3], source: [f64; 3], alpha: f64) {
    for index in 0..3 {
        color[index] += (source[index] - color[index]) * alpha;
    }
}

/// `radial-gradient(<rx> <ry> at <cx> <cy>, accent <alpha> 0%, accent 0 <end>)` at `(x, y)`, as the
/// alpha of the accent there (premultiplied interpolation makes it linear in alpha).
fn radial(x: f64, y: f64, center: (f64, f64), radii: (f64, f64), alpha: f64, end: f64) -> f64 {
    let distance = (((x - center.0) / radii.0).powi(2) + ((y - center.1) / radii.1).powi(2)).sqrt();
    alpha * (1.0 - (distance / end).clamp(0.0, 1.0))
}

/// Where `(x, y)` falls on a CSS linear gradient of `angle` degrees in a unit square (0 at the
/// start edge, 1 at the end edge).
fn linear(x: f64, y: f64, angle: f64) -> f64 {
    let (sin, cos) = angle.to_radians().sin_cos();
    let length = sin.abs() + cos.abs();
    ((x - 0.5) * sin - (y - 0.5) * cos) / length + 0.5
}

/// The flat colour of one square at one step.
pub(crate) fn swatch_colour(paint: &SwatchPaint) -> Rgb {
    let Rgb(top) = paint.top;
    let Rgb(base) = paint.base;
    let Rgb(accent) = paint.accent;
    let mut sum = [0.0; 3];
    for row in 0..SAMPLES {
        let y = (row as f64 + 0.5) / SAMPLES as f64;
        for column in 0..SAMPLES {
            let x = (column as f64 + 0.5) / SAMPLES as f64;
            // linear-gradient(150deg, top 0%, base 88%)
            let t = (linear(x, y, 150.0) / 0.88).clamp(0.0, 1.0);
            let mut color = [0, 1, 2].map(|index| top[index] + (base[index] - top[index]) * t);
            over(
                &mut color,
                accent,
                radial(x, y, (1.0, 1.0), (0.9, 0.7), paint.rim, 0.7),
            );
            over(
                &mut color,
                accent,
                radial(x, y, (0.2, 0.08), (1.3, 1.0), paint.glow, 0.62),
            );
            for index in 0..3 {
                sum[index] += color[index];
            }
        }
    }
    let count = (SAMPLES * SAMPLES) as f64;
    Rgb(sum.map(|channel| channel / count))
}
