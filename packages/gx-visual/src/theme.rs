//! Colors and the theme a scene is drawn in.

use serde::{Serialize, Serializer};
use serde_json::Value;

/// An sRGB color with straight alpha (0..=1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Color {
    pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    pub(crate) const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
        Color { r, g, b, a }
    }

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (the `#` is optional).
    pub fn hex(s: &str) -> Option<Color> {
        let s = s.trim();
        let h = s.strip_prefix('#').unwrap_or(s);
        if h.is_empty() || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let nibble = |i: usize| -> u8 {
            h.as_bytes()
                .get(i)
                .and_then(|b| (*b as char).to_digit(16))
                .unwrap_or(0) as u8
        };
        let pair = |i: usize| -> u8 { nibble(i) * 16 + nibble(i + 1) };
        match h.len() {
            3 | 4 => {
                let a = if h.len() == 4 {
                    f32::from(nibble(3) * 17) / 255.0
                } else {
                    1.0
                };
                Some(Color::rgba(
                    nibble(0) * 17,
                    nibble(1) * 17,
                    nibble(2) * 17,
                    a,
                ))
            }
            6 | 8 => {
                let a = if h.len() == 8 {
                    f32::from(pair(6)) / 255.0
                } else {
                    1.0
                };
                Some(Color::rgba(pair(0), pair(2), pair(4), a))
            }
            _ => None,
        }
    }

    /// The same color with alpha `a` (clamped to 0..=1).
    pub fn with_alpha(self, a: f32) -> Color {
        Color {
            a: clamp_alpha(a),
            ..self
        }
    }

    /// CSS `rgba(r,g,b,a)`.
    pub fn css(&self) -> String {
        format!(
            "rgba({},{},{},{})",
            self.r,
            self.g,
            self.b,
            format_alpha(self.a)
        )
    }

    /// `#rrggbb`, without the alpha (SVG writes that as an opacity attribute).
    pub(crate) fn hex_rgb(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.css())
    }
}

fn clamp_alpha(a: f32) -> f32 {
    if a.is_finite() {
        a.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

pub(crate) fn format_alpha(a: f32) -> String {
    let text = format!("{:.3}", clamp_alpha(a));
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// A CSS color as agents write it: hex, `rgb()`/`rgba()`, or a common color name.
pub(crate) fn parse_css_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.starts_with('#') {
        return Color::hex(s);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner
            .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() != 3 && parts.len() != 4 {
            return None;
        }
        let channel = |p: &str| -> Option<u8> {
            let v = p.parse::<f32>().ok()?;
            v.is_finite().then(|| v.clamp(0.0, 255.0).round() as u8)
        };
        let alpha = match parts.get(3) {
            Some(p) => match p.strip_suffix('%') {
                Some(pct) => pct.parse::<f32>().ok()? / 100.0,
                None => p.parse::<f32>().ok()?,
            },
            None => 1.0,
        };
        return Some(Color::rgba(
            channel(parts[0])?,
            channel(parts[1])?,
            channel(parts[2])?,
            clamp_alpha(alpha),
        ));
    }
    if lower == "transparent" {
        return Some(Color::rgba(0, 0, 0, 0.0));
    }
    NAMED_COLORS
        .iter()
        .find(|(name, _)| *name == lower)
        .and_then(|(_, hex)| Color::hex(hex))
        .or_else(|| Color::hex(s).filter(|_| s.len() == 6 || s.len() == 8))
}

const NAMED_COLORS: &[(&str, &str)] = &[
    ("black", "#000000"),
    ("white", "#ffffff"),
    ("gray", "#808080"),
    ("grey", "#808080"),
    ("silver", "#c0c0c0"),
    ("lightgray", "#d3d3d3"),
    ("lightgrey", "#d3d3d3"),
    ("darkgray", "#a9a9a9"),
    ("darkgrey", "#a9a9a9"),
    ("red", "#ff0000"),
    ("crimson", "#dc143c"),
    ("tomato", "#ff6347"),
    ("coral", "#ff7f50"),
    ("salmon", "#fa8072"),
    ("orange", "#ffa500"),
    ("gold", "#ffd700"),
    ("yellow", "#ffff00"),
    ("olive", "#808000"),
    ("lime", "#00ff00"),
    ("green", "#008000"),
    ("darkgreen", "#006400"),
    ("seagreen", "#2e8b57"),
    ("teal", "#008080"),
    ("cyan", "#00ffff"),
    ("aqua", "#00ffff"),
    ("blue", "#0000ff"),
    ("navy", "#000080"),
    ("steelblue", "#4682b4"),
    ("royalblue", "#4169e1"),
    ("skyblue", "#87ceeb"),
    ("indigo", "#4b0082"),
    ("purple", "#800080"),
    ("violet", "#ee82ee"),
    ("magenta", "#ff00ff"),
    ("fuchsia", "#ff00ff"),
    ("pink", "#ffc0cb"),
    ("hotpink", "#ff69b4"),
    ("brown", "#a52a2a"),
    ("maroon", "#800000"),
    ("chocolate", "#d2691e"),
    ("tan", "#d2b48c"),
];

/// The colors a scene is drawn with. Hosts pass their own chat theme; `dark()` and `light()` are
/// the defaults the CLI uses and that `from_json` falls back to per key.
#[derive(Clone, Debug)]
pub struct Theme {
    pub light: bool,
    /// Main text.
    pub foreground: Color,
    /// Axis labels and secondary text.
    pub muted: Color,
    /// Gridlines.
    pub grid: Color,
    /// Tile and table borders.
    pub border: Color,
    /// Tile and table-header fill.
    pub surface: Color,
    /// What the scene sits on (donut separators and the like).
    pub background: Color,
    pub good: Color,
    pub bad: Color,
    /// Categorical palette, at least 8 colors.
    pub series: Vec<Color>,
    /// The size, in scene units, of the transcript's body text; every label, cell, note, legend
    /// and axis tick is drawn at it, and the stat values and row heights are in proportion to it.
    pub text_size: f32,
}

/// The transcript's prose size in logical pixels (`PROSE_SIZE` in the desktop's
/// `native_chat/rich_markdown.rs` and the phone's `transcript/theme.ts`); what hosts that do not
/// say (the CLI) draw at.
pub const DEFAULT_TEXT_SIZE: f32 = 14.0;

impl Theme {
    pub fn dark() -> Theme {
        Theme {
            light: false,
            foreground: Color::rgb(0xe8, 0xe6, 0xe3),
            muted: Color::rgb(0x9a, 0x9c, 0xa3),
            grid: Color::rgba(255, 255, 255, 0.08),
            border: Color::rgba(255, 255, 255, 0.12),
            surface: Color::rgba(255, 255, 255, 0.04),
            background: Color::rgb(0x18, 0x18, 0x1b),
            good: Color::rgb(0x34, 0xd3, 0x99),
            bad: Color::rgb(0xf8, 0x71, 0x71),
            series: palette(&[
                "#60a5fa", "#f59e0b", "#34d399", "#f472b6", "#a78bfa", "#22d3ee", "#fb923c",
                "#a3e635",
            ]),
            text_size: DEFAULT_TEXT_SIZE,
        }
    }

    pub fn light() -> Theme {
        Theme {
            light: true,
            foreground: Color::rgb(0x18, 0x18, 0x1b),
            muted: Color::rgb(0x5f, 0x61, 0x68),
            grid: Color::rgba(0, 0, 0, 0.07),
            border: Color::rgba(0, 0, 0, 0.12),
            surface: Color::rgba(0, 0, 0, 0.03),
            background: Color::rgb(0xff, 0xff, 0xff),
            good: Color::rgb(0x05, 0x96, 0x69),
            bad: Color::rgb(0xdc, 0x26, 0x26),
            series: palette(&[
                "#2563eb", "#d97706", "#059669", "#db2777", "#7c3aed", "#0891b2", "#ea580c",
                "#65a30d",
            ]),
            text_size: DEFAULT_TEXT_SIZE,
        }
    }

    /// From JSON `{"light":bool,"foreground":"#hex",…,"series":["#hex",…],"textSize":14}`; a missing or
    /// unreadable key falls back to `dark()` or `light()` (per `"light"`).
    pub fn from_json(value: &Value) -> Theme {
        let light = value.get("light").and_then(Value::as_bool).unwrap_or(false);
        let mut theme = if light { Theme::light() } else { Theme::dark() };
        let pick = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .and_then(parse_css_color)
        };
        let slots: [(&str, &mut Color); 8] = [
            ("foreground", &mut theme.foreground),
            ("muted", &mut theme.muted),
            ("grid", &mut theme.grid),
            ("border", &mut theme.border),
            ("surface", &mut theme.surface),
            ("background", &mut theme.background),
            ("good", &mut theme.good),
            ("bad", &mut theme.bad),
        ];
        for (key, slot) in slots {
            if let Some(color) = pick(key) {
                *slot = color;
            }
        }
        if let Some(size) = value.get("textSize").and_then(Value::as_f64) {
            if size.is_finite() && size > 0.0 {
                theme.text_size = (size as f32).clamp(8.0, 40.0);
            }
        }
        if let Some(list) = value.get("series").and_then(Value::as_array) {
            let colors: Vec<Color> = list
                .iter()
                .filter_map(Value::as_str)
                .filter_map(parse_css_color)
                .collect();
            if !colors.is_empty() {
                theme.series = colors;
            }
        }
        theme
    }

    /// Series `index` of the categorical palette, wrapping around.
    pub(crate) fn series_color(&self, index: usize) -> Color {
        if self.series.is_empty() {
            return self.foreground;
        }
        self.series[index % self.series.len()]
    }
}

fn palette(hexes: &[&str]) -> Vec<Color> {
    hexes.iter().filter_map(|h| Color::hex(h)).collect()
}
