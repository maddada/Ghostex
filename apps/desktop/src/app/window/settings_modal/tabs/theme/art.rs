//! The decorative pictures of the glass flow (`.theme-glass-source-art.is-*` and
//! `.theme-glass-picture-thumb.is-*` in packages/core-ui/styles/settings-theme.css (deleted 2026-11-06)). They are CSS
//! gradient stacks with radial layers and hard stops, which GPUI cannot fill, so each is painted
//! once into a bitmap by a small CSS gradient rasterizer.
use gpui::RenderImage;
use std::sync::Arc;

type Rgba = [f64; 4];

fn hex(color: u32) -> Rgba {
    [
        ((color >> 16) & 0xff) as f64,
        ((color >> 8) & 0xff) as f64,
        (color & 0xff) as f64,
        1.0,
    ]
}

fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> Rgba {
    [red, green, blue, alpha]
}

/// One CSS gradient layer. Stop positions are fractions of the gradient line (linear) or of the
/// farthest-corner radius (radial `circle at x y`).
enum Layer {
    Linear {
        angle: f64,
        stops: Vec<(f64, Rgba)>,
    },
    Radial {
        center: (f64, f64),
        stops: Vec<(f64, Rgba)>,
    },
}

/// A colour stop list sampled at `t` in premultiplied space, as browsers interpolate.
fn sample(stops: &[(f64, Rgba)], t: f64) -> Rgba {
    let premultiply = |c: Rgba| [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
    let first = stops[0];
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            let span = b.0 - a.0;
            let f = if span <= 0.0 { 1.0 } else { (t - a.0) / span };
            let (pa, pb) = (premultiply(a.1), premultiply(b.1));
            let mixed: Rgba = [0, 1, 2, 3].map(|index| pa[index] + (pb[index] - pa[index]) * f);
            let alpha = mixed[3];
            return if alpha <= 0.0 {
                [0.0; 4]
            } else {
                [mixed[0] / alpha, mixed[1] / alpha, mixed[2] / alpha, alpha]
            };
        }
    }
    stops[stops.len() - 1].1
}

fn paint(width: u32, height: u32, layers: &[Layer]) -> Option<Arc<RenderImage>> {
    let (w, h) = (width as f64, height as f64);
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    for row in 0..height {
        let y = row as f64 + 0.5;
        for column in 0..width {
            let x = column as f64 + 0.5;
            // Painted bottom layer first, source-over.
            let mut color: Rgba = [0.0; 4];
            for layer in layers {
                let source = match layer {
                    Layer::Linear { angle, stops } => {
                        let (sin, cos) = angle.to_radians().sin_cos();
                        let length = (w * sin).abs() + (h * cos).abs();
                        let t = ((x - w / 2.0) * sin - (y - h / 2.0) * cos) / length + 0.5;
                        sample(stops, t)
                    }
                    Layer::Radial { center, stops } => {
                        let (cx, cy) = (center.0 * w, center.1 * h);
                        let radius = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
                            .iter()
                            .map(|(px, py)| ((px - cx).powi(2) + (py - cy).powi(2)).sqrt())
                            .fold(0.0, f64::max);
                        let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                        sample(stops, distance / radius)
                    }
                };
                let alpha = source[3] + color[3] * (1.0 - source[3]);
                if alpha > 0.0 {
                    for index in 0..3 {
                        color[index] = (source[index] * source[3]
                            + color[index] * color[3] * (1.0 - source[3]))
                            / alpha;
                    }
                }
                color[3] = alpha;
            }
            let index = ((row * width + column) * 4) as usize;
            bgra[index] = color[2].round().clamp(0.0, 255.0) as u8;
            bgra[index + 1] = color[1].round().clamp(0.0, 255.0) as u8;
            bgra[index + 2] = color[0].round().clamp(0.0, 255.0) as u8;
            bgra[index + 3] = (color[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

fn transparent(color: Rgba) -> Rgba {
    [color[0], color[1], color[2], 0.0]
}

/// A disc of `color` from 0 to `edge`, transparent from `edge + 1%` (`color 0 12%, transparent 13%`).
fn disc(center: (f64, f64), color: u32, edge: f64) -> Layer {
    let color = hex(color);
    Layer::Radial {
        center,
        stops: vec![
            (0.0, color),
            (edge, color),
            (edge + 0.01, transparent(color)),
        ],
    }
}

/// The art above a "Behind the glass" card: `desktopAndWindows`, `wallpaper`,
/// `customImage` or `live`, 2x its 147x46 box.
pub(super) fn source_art(source: &str) -> Option<Arc<RenderImage>> {
    let (width, height) = (294, 92);
    let layers = match source {
        "wallpaper" => vec![Layer::Linear {
            angle: 135.0,
            stops: vec![
                (0.0, hex(0x2f6f8f)),
                (0.6, hex(0x3d8f6a)),
                (1.0, hex(0xd8a24b)),
            ],
        }],
        "customImage" => vec![
            Layer::Linear {
                angle: 180.0,
                stops: vec![
                    (0.0, hex(0x7aa6d8)),
                    (0.55, hex(0x3b5b8f)),
                    (0.56, hex(0x2f4a2f)),
                    (1.0, hex(0x2f4a2f)),
                ],
            },
            disc((0.7, 0.35), 0xf3c46a, 0.12),
        ],
        "live" => {
            let blue = rgba(95.0, 178.0, 230.0, 0.9);
            let violet = rgba(170.0, 120.0, 230.0, 0.8);
            vec![
                Layer::Linear {
                    angle: 135.0,
                    stops: vec![(0.0, hex(0x10203a)), (1.0, hex(0x2a1d44))],
                },
                Layer::Radial {
                    center: (0.75, 0.7),
                    stops: vec![(0.0, violet), (0.55, transparent(violet))],
                },
                Layer::Radial {
                    center: (0.25, 0.3),
                    stops: vec![(0.0, blue), (0.55, transparent(blue))],
                },
            ]
        }
        // `is-desktop`: the default art; its two window shapes are drawn over it as elements.
        _ => vec![Layer::Linear {
            angle: 135.0,
            stops: vec![(0.0, hex(0x3b5b8f)), (1.0, hex(0x6b3f6f))],
        }],
    };
    paint(width, height, &layers)
}

/// The decorative thumb of a chosen dark or light glass picture, 2x its 323x72 box.
pub(super) fn picture_thumb(light: bool) -> Option<Arc<RenderImage>> {
    let layers = if light {
        vec![
            Layer::Linear {
                angle: 180.0,
                stops: vec![
                    (0.0, hex(0xbcdcff)),
                    (0.58, hex(0xeaf4ff)),
                    (0.59, hex(0xa9cf93)),
                    (1.0, hex(0xa9cf93)),
                ],
            },
            disc((0.72, 0.3), 0xffe29a, 0.11),
        ]
    } else {
        vec![
            Layer::Linear {
                angle: 180.0,
                stops: vec![
                    (0.0, hex(0x0f1733)),
                    (0.6, hex(0x27325f)),
                    (0.61, hex(0x121a2e)),
                    (1.0, hex(0x121a2e)),
                ],
            },
            disc((0.3, 0.3), 0x6d8fd6, 0.1),
        ]
    };
    paint(646, 144, &layers)
}

/// `repeating-linear-gradient(45deg, <fg 4%> 0 8px, transparent 8px 16px)` over a 323x72 empty
/// thumb, 2x.
pub(super) fn empty_thumb_stripes(foreground: gpui::Rgba) -> Option<Arc<RenderImage>> {
    let (width, height) = (646u32, 144u32);
    let color = [
        (foreground.r as f64 * 255.0).round(),
        (foreground.g as f64 * 255.0).round(),
        (foreground.b as f64 * 255.0).round(),
    ];
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    let (sin, cos) = 45f64.to_radians().sin_cos();
    for row in 0..height {
        for column in 0..width {
            // Distance along the 45deg gradient line in CSS pixels (the bitmap is 2x).
            let along = ((column as f64 + 0.5) * sin - (row as f64 + 0.5) * -cos) / 2.0;
            let phase = along.rem_euclid(16.0);
            let alpha = if phase < 8.0 { 0.04 } else { 0.0 };
            let index = ((row * width + column) * 4) as usize;
            bgra[index] = color[2] as u8;
            bgra[index + 1] = color[1] as u8;
            bgra[index + 2] = color[0] as u8;
            bgra[index + 3] = (alpha * 255.0_f64).round() as u8;
        }
    }
    let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}
