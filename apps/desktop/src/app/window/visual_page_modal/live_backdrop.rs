//! CPU snapshot renderer for the Animated (Live) glass backdrop styles, matching
//! `.dependencies/zed/crates/gpui_macos/src/live_backdrop.metal` so the floating Live page window
//! can show a softened frame of the active animated backdrop in the current theme's colours and
//! brightness on every desktop platform (`glass.rs`).

use gpui::{Pixels, Size, px, size};
use std::f32::consts::TAU;

/// Small 16:10 canvas: `glass.rs` blurs the result by `PICTURE_BLUR_POINTS`, where finer detail
/// disappears, so 160x100 looks identical once softened and renders in a few milliseconds.
const WIDTH: u32 = 160;
const HEIGHT: u32 = 100;

/// The same moment of the 2-minute loop `tooling/glass-live/render-posters.swift` uses, where
/// every style shows its character.
const POSTER_PHASE: f32 = 19.4;
const LIVE_PERIOD: f32 = 120.0;

const LIVE_GAIN_AURORA: f32 = 2.001;
const LIVE_GAIN_INK: f32 = 0.894;
const LIVE_GAIN_DRIFT: f32 = 1.030;
const LIVE_GAIN_NEBULA: f32 = 2.878;
const LIVE_GAIN_SILK: f32 = 1.562;
const LIVE_GAIN_BOKEH: f32 = 3.468;
const LIVE_GAIN_WAVES: f32 = 1.215;
const LIVE_GAIN_MESH: f32 = 1.017;

/// Renders (or returns the cached) JPEG snapshot of `live` in its current theme colours and
/// brightness, plus its aspect size. Returns `None` for an unknown style.
pub(super) fn render_snapshot(live: &gpui::LiveBackground) -> Option<(Vec<u8>, Size<Pixels>)> {
    type CacheKey = (String, [[u32; 3]; 3], u32);
    static LAST: std::sync::Mutex<Option<(CacheKey, Vec<u8>)>> = std::sync::Mutex::new(None);

    let style = live.style.as_ref();
    if !matches!(
        style,
        "aurora" | "ink" | "drift" | "nebula" | "silk" | "bokeh" | "waves" | "mesh"
    ) {
        return None;
    }

    let key: CacheKey = (
        style.to_string(),
        live.colors.map(|c| c.map(f32::to_bits)),
        live.brightness.to_bits(),
    );
    let aspect_size = size(px(WIDTH as f32), px(HEIGHT as f32));
    if let Ok(last) = LAST.lock()
        && let Some((last_key, jpeg)) = last.as_ref()
        && *last_key == key
    {
        return Some((jpeg.clone(), aspect_size));
    }

    let c0 = live.colors[0];
    let c1 = live.colors[1];
    let c2 = live.colors[2];
    let brightness = live.brightness.clamp(0.0, 1.0);
    let angle = TAU * POSTER_PHASE / LIVE_PERIOD;
    let aspect = WIDTH as f32 / HEIGHT as f32;

    let picture = image::RgbImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let px_coord = ((x as f32 + 0.5) / WIDTH as f32 - 0.5) * aspect;
        let py_coord = (y as f32 + 0.5) / HEIGHT as f32 - 0.5;
        let p = [px_coord, py_coord];
        let (raw, gain) = match style {
            "aurora" => (eval_aurora(p, angle, c0, c1, c2), LIVE_GAIN_AURORA),
            "ink" => (eval_ink(p, angle, c0, c1, c2), LIVE_GAIN_INK),
            "drift" => (eval_drift(p, aspect, angle, c0, c1, c2), LIVE_GAIN_DRIFT),
            "nebula" => (eval_nebula(p, angle, c0, c1, c2), LIVE_GAIN_NEBULA),
            "silk" => (eval_silk(p, angle, c0, c1, c2), LIVE_GAIN_SILK),
            "bokeh" => (eval_bokeh(p, aspect, angle, c0, c1, c2), LIVE_GAIN_BOKEH),
            "waves" => (eval_waves(p, angle, c0, c1, c2), LIVE_GAIN_WAVES),
            "mesh" => (eval_mesh(p, aspect, angle, c0, c1, c2), LIVE_GAIN_MESH),
            _ => (c0, 1.0),
        };
        let scale = (brightness * gain).clamp(0.0, 4.0);
        let dither = (live_hash([x as f32 + 0.5, y as f32 + 0.5, 3.7]) - 0.5) / 255.0;
        let ch = |i: usize| {
            let v = (c0[i] + (raw[i] - c0[i]) * scale + dither).clamp(0.0, 1.0);
            (v * 255.0).round() as u8
        };
        image::Rgb([ch(0), ch(1), ch(2)])
    });

    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&picture)
        .ok()?;
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, jpeg.clone()));
    }
    Some((jpeg, aspect_size))
}

#[inline]
fn fract(x: f32) -> f32 {
    x - x.floor()
}

#[inline]
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn live_hash(p: [f32; 3]) -> f32 {
    let px = fract(p[0] * 0.3183099 + 0.71) * 17.0;
    let py = fract(p[1] * 0.3183099 + 0.113) * 17.0;
    let pz = fract(p[2] * 0.3183099 + 0.419) * 17.0;
    fract(px * py * pz * (px + py + pz))
}

fn live_noise(x: [f32; 3]) -> f32 {
    let i = [x[0].floor(), x[1].floor(), x[2].floor()];
    let mut f = [x[0] - i[0], x[1] - i[1], x[2] - i[2]];
    f = [
        f[0] * f[0] * (3.0 - 2.0 * f[0]),
        f[1] * f[1] * (3.0 - 2.0 * f[1]),
        f[2] * f[2] * (3.0 - 2.0 * f[2]),
    ];
    let n000 = live_hash([i[0], i[1], i[2]]);
    let n100 = live_hash([i[0] + 1.0, i[1], i[2]]);
    let n010 = live_hash([i[0], i[1] + 1.0, i[2]]);
    let n110 = live_hash([i[0] + 1.0, i[1] + 1.0, i[2]]);
    let n001 = live_hash([i[0], i[1], i[2] + 1.0]);
    let n101 = live_hash([i[0] + 1.0, i[1], i[2] + 1.0]);
    let n011 = live_hash([i[0], i[1] + 1.0, i[2] + 1.0]);
    let n111 = live_hash([i[0] + 1.0, i[1] + 1.0, i[2] + 1.0]);
    mix(
        mix(mix(n000, n100, f[0]), mix(n010, n110, f[0]), f[1]),
        mix(mix(n001, n101, f[0]), mix(n011, n111, f[0]), f[1]),
        f[2],
    )
}

fn live_fbm(mut p: [f32; 3]) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    for _ in 0..4 {
        value += amplitude * live_noise(p);
        p = [p[0] * 2.03 + 1.7, p[1] * 2.03 + 9.2, p[2] * 2.03 + 3.1];
        amplitude *= 0.5;
    }
    value
}

#[inline]
fn live_orbit(angle: f32, turns: f32, radius: f32, seed: f32) -> [f32; 3] {
    let a = angle * turns + seed;
    [
        radius * a.cos(),
        radius * a.sin(),
        radius * 0.6 * (a + 1.3).sin(),
    ]
}

#[inline]
fn live_palette(t: f32, c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let low = mix3(c0, c1, smoothstep(0.0, 0.6, t));
    mix3(low, c2, smoothstep(0.45, 1.0, t))
}

fn eval_aurora(p: [f32; 2], angle: f32, c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 3] {
    let o1 = live_orbit(angle, 1.0, 1.2, 0.0);
    let o2 = live_orbit(angle, 2.0, 0.8, 2.1);
    let ribbon1 = -0.08 + 1.3 * (live_fbm(add3([p[0] * 0.8, 0.0, 0.0], o1)) - 0.5);
    let ribbon2 = 0.14 + 1.1 * (live_fbm(add3([p[0] * 0.7, 3.0, 0.0], o2)) - 0.5);
    let d1 = p[1] - ribbon1;
    let d2 = p[1] - ribbon2;
    let band1 = (-d1 * d1 / if d1 < 0.0 { 0.045 } else { 0.006 }).exp();
    let band2 = (-d2 * d2 / if d2 < 0.0 { 0.035 } else { 0.005 }).exp();
    let folds = 0.7
        + 0.3
            * live_fbm(add3(
                [p[0] * 2.6, p[1] * 0.25, 0.0],
                [o1[0] * 0.5, o1[1] * 0.5, o1[2] * 0.5],
            ));
    let color = mix3(c0, c1, (band1 * folds * 1.15).clamp(0.0, 1.0));
    mix3(color, c2, (band2 * folds).clamp(0.0, 1.0))
}

fn eval_ink(p: [f32; 2], angle: f32, c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 3] {
    let base = [p[0] * 1.6, p[1] * 1.6, 0.0];
    let q = [
        live_fbm(add3(base, live_orbit(angle, 1.0, 0.6, 0.0))),
        live_fbm(add3(
            add3(base, [5.2, 1.3, 0.0]),
            live_orbit(angle, 1.0, 0.6, 1.7),
        )),
    ];
    let r = [
        live_fbm(add3(
            add3(base, [3.0 * q[0] + 1.7, 3.0 * q[1] + 9.2, 0.0]),
            live_orbit(angle, 2.0, 0.35, 0.4),
        )),
        live_fbm(add3(
            add3(base, [3.0 * q[0] + 8.3, 3.0 * q[1] + 2.8, 0.0]),
            live_orbit(angle, 2.0, 0.35, 2.9),
        )),
    ];
    let f = live_fbm(add3(base, [3.0 * r[0], 3.0 * r[1], 0.0]));
    let color = live_palette(f * 1.35 - 0.1, c0, c1, c2);
    let q_len = (q[0] * q[0] + q[1] * q[1]).sqrt();
    mix3(color, c2, (q_len * 0.35 - 0.1).clamp(0.0, 0.5) * 0.6)
}

fn eval_drift(
    p: [f32; 2],
    aspect: f32,
    angle: f32,
    c0: [f32; 3],
    c1: [f32; 3],
    c2: [f32; 3],
) -> [f32; 3] {
    let mut color = mix3(c0, c1, 0.25);
    let mid = mix3(c1, c2, 0.5);
    let muted_c2 = [
        c2[0] * 0.8 + c0[0] * 0.2,
        c2[1] * 0.8 + c0[1] * 0.2,
        c2[2] * 0.8 + c0[2] * 0.2,
    ];
    for i in 0..5 {
        let seed = i as f32 * 1.618;
        let turns = if i % 2 == 0 { 1.0 } else { 2.0 };
        let direction = if i < 3 { 1.0 } else { -1.0 };
        let cx = 0.38 * aspect * (angle * turns * direction + seed * 2.0).sin();
        let cy = 0.3 * (angle * turns + seed).cos();
        let radius = 0.42 + 0.08 * (angle + seed * 3.0).sin();
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        let weight = (-(dx * dx + dy * dy) / (radius * radius)).exp();
        let blob = match i {
            0 | 3 => c1,
            1 => c2,
            2 => mid,
            _ => muted_c2,
        };
        color = mix3(color, blob, (weight * 0.9).clamp(0.0, 1.0));
    }
    color
}

fn eval_nebula(p: [f32; 2], angle: f32, c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 3] {
    let px = p[0] * 1.3;
    let py = p[1] * 1.3;
    let large = live_fbm(add3(
        [px * 1.2, py * 1.2, 0.0],
        live_orbit(angle, 1.0, 0.8, 0.3),
    ));
    let small = live_fbm(add3(
        [px * 3.0, py * 3.0, 4.0],
        live_orbit(angle, 2.0, 0.5, 1.9),
    ));
    let cloud = smoothstep(0.35, 0.85, large * 0.75 + small * 0.4);
    let glow = smoothstep(0.45, 0.95, small) * cloud;
    let color = mix3(c0, c1, cloud);
    mix3(color, c2, glow * 0.8)
}

fn eval_silk(p: [f32; 2], angle: f32, c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 3] {
    let px = p[0] * 1.2;
    let py = p[1] * 1.2;
    let warp = 2.0 * live_fbm(add3([px, py, 0.0], live_orbit(angle, 1.0, 0.55, 0.0)));
    let fold = ((px + py * 0.6) * 4.0 + warp * 4.0 + angle * 2.0).sin();
    let sheen = smoothstep(0.35, 1.0, fold);
    let shade = 0.5 + 0.5 * ((px * 0.7 - py) * 2.5 + warp * 2.0 - angle).sin();
    let color = mix3(c0, c1, shade * 0.9);
    mix3(color, c2, sheen * 0.75)
}

fn eval_bokeh(
    p: [f32; 2],
    aspect: f32,
    angle: f32,
    c0: [f32; 3],
    c1: [f32; 3],
    c2: [f32; 3],
) -> [f32; 3] {
    let mut color = [
        c0[0] * 0.9 + c1[0] * 0.1,
        c0[1] * 0.9 + c1[1] * 0.1,
        c0[2] * 0.9 + c1[2] * 0.1,
    ];
    for i in 0..10 {
        let seed = i as f32 * 2.399;
        let turns = (1 + i % 3) as f32;
        let hx = (fract((seed * 12.9898).sin() * 43758.5453) - 0.5) * aspect;
        let hy = fract((seed * 78.233).sin() * 43758.5453) - 0.5;
        let cx = hx + 0.07 * (angle * turns + seed).cos();
        let cy = hy + 0.07 * (angle * turns + seed * 1.7).sin();
        let radius = 0.07 + 0.09 * fract(seed * 0.37);
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        let disc = (-(dx * dx + dy * dy) / (radius * radius)).exp();
        let light = if i % 2 == 0 { c1 } else { c2 };
        color = mix3(color, light, (disc * 0.75).clamp(0.0, 1.0));
    }
    color
}

fn eval_waves(p: [f32; 2], angle: f32, c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 3] {
    let mut color = c0;
    for i in 0..4 {
        let layer = i as f32;
        let height = -0.25 + layer * 0.17;
        let crest = height
            + 0.06 * (p[0] * (2.0 + layer * 0.7) + angle * (1.0 + layer) + layer * 1.3).sin()
            + 0.03 * (p[0] * (3.7 - layer * 0.4) - angle * 2.0 + layer).sin();
        let fill = smoothstep(crest - 0.05, crest + 0.05, p[1]);
        let tone = live_palette(0.3 + layer * 0.22, c0, c1, c2);
        color = mix3(color, tone, fill * 0.85);
    }
    color
}

fn eval_mesh(
    p: [f32; 2],
    aspect: f32,
    angle: f32,
    c0: [f32; 3],
    c1: [f32; 3],
    c2: [f32; 3],
) -> [f32; 3] {
    let mid = mix3(c1, c2, 0.5);
    let c3 = [
        mid[0] * 0.9 + c0[0] * 0.1,
        mid[1] * 0.9 + c0[1] * 0.1,
        mid[2] * 0.9 + c0[2] * 0.1,
    ];
    let colors = [c0, c1, c2, c3];
    let corners = [
        [-0.35 * aspect, -0.35],
        [0.35 * aspect, -0.3],
        [0.3 * aspect, 0.35],
        [-0.3 * aspect, 0.3],
    ];
    let mut color = [0.0f32; 3];
    let mut total = 0.0f32;
    for i in 0..4 {
        let seed = i as f32 * 1.9;
        let px = corners[i][0] + 0.18 * (angle * (1.0 + (i % 2) as f32) + seed).cos();
        let py = corners[i][1] + 0.18 * (angle + seed * 1.3).sin();
        let dx = p[0] - px;
        let dy = p[1] - py;
        let weight = 1.0 / (dx * dx + dy * dy + 0.02).powf(1.6);
        color[0] += colors[i][0] * weight;
        color[1] += colors[i][1] * weight;
        color[2] += colors[i][2] * weight;
        total += weight;
    }
    [color[0] / total, color[1] / total, color[2] / total]
}
