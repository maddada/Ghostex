//! The drawing scene every client paints, the blocks layout builds it from, and its SVG form.

use serde::Serialize;

use crate::motion::Motion;
use crate::text::line_height;
use crate::theme::{format_alpha, Color};

/// Where a text item's `x` sits relative to the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// One drawing primitive, in logical pixels from the scene's top-left corner.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Item {
    #[serde(rename_all = "camelCase")]
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
    },
    #[serde(rename_all = "camelCase")]
    Path {
        points: Vec<[f32; 2]>,
        closed: bool,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
        dashed: bool,
    },
    /// `y` is the alphabetic BASELINE of the text (SVG convention). `x` is where `anchor` sits.
    #[serde(rename_all = "camelCase")]
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        weight: u16,
        color: Color,
        anchor: Anchor,
        mono: bool,
    },
}

/// One row of a hover tooltip.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TooltipLine {
    pub label: String,
    pub value: String,
    pub color: Option<Color>,
}

/// A hover area and the tooltip it shows.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub lines: Vec<TooltipLine>,
    /// The scene item (a bar, a dot, a pie slice) this region stands for: hosts brighten it while
    /// it is hovered, and hit-test a closed path by its own outline rather than this box, so a
    /// pie slice's corners do not belong to its neighbour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<usize>,
}

/// A laid-out visual. The background is transparent and there is no outer padding; the host
/// draws the card around it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub title: Option<String>,
    pub items: Vec<Item>,
    pub regions: Vec<Region>,
    /// The data marks that draw in, by item index (see [`Scene::at`]).
    #[serde(skip)]
    pub(crate) motions: Vec<(usize, Motion)>,
}

/// A published HTML page the block points at instead of holding a drawing.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRef {
    pub title: String,
    pub url: String,
    /// The file the page was published from, which the card names instead of the page's address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The agent marked the page `"open": "popup"`. Ghostex apps before 2026-10-26 float only a
    /// marked card; the desktop now opens every card where Open chat pages in says.
    pub popup: bool,
    /// The page asks the reader questions (a `<gx-questions>` kit or its own `ghostex.submit`):
    /// the tags its questions carry, empty when they have none. `None` for a page that asks
    /// nothing, and for blocks printed before `ghostex show` wrote the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<String>>,
}

impl PageRef {
    /// The page's id when its URL is a Ghostex page route (`…/visual/<32 hex>`): the key the chat's
    /// `answeredPages` list uses.
    pub fn page_id(&self) -> Option<&str> {
        let path = self.url.split(['?', '#']).next().unwrap_or_default();
        let id = path.trim_end_matches('/').rsplit_once("/visual/")?.1;
        (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(id)
    }
}

/// What one block renders to.
#[derive(Clone, Debug)]
pub enum Visual {
    Drawing(Scene),
    Page(PageRef),
}

/// A laid-out piece of a scene with its top-left at the origin, before layout places it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Block {
    pub height: f32,
    pub items: Vec<Item>,
    pub regions: Vec<Region>,
    pub motions: Vec<(usize, Motion)>,
}

impl Block {
    pub(crate) fn new() -> Block {
        Block::default()
    }

    /// Moves `other` by (`dx`, `dy`) and adds it on top of this block.
    pub(crate) fn append(&mut self, other: Block, dx: f32, dy: f32) {
        let base = self.items.len();
        self.items
            .extend(other.items.into_iter().map(|item| translate(item, dx, dy)));
        self.regions.extend(other.regions.into_iter().map(|mut r| {
            r.x += dx;
            r.y += dy;
            r.item = r.item.map(|item| item + base);
            r
        }));
        self.motions.extend(
            other
                .motions
                .into_iter()
                .map(|(item, motion)| (item + base, motion.translate(dx, dy))),
        );
    }

    /// Makes `item` (the mark just pushed) draw in with `motion`.
    pub(crate) fn animate(&mut self, item: Option<usize>, motion: Motion) {
        if let Some(item) = item {
            self.motions.push((item, motion));
        }
    }

    pub(crate) fn push(&mut self, item: Item) {
        self.items.push(item);
    }

    /// A single line of text whose line box starts at `top`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_line(
        &mut self,
        x: f32,
        top: f32,
        text: impl Into<String>,
        size: f32,
        weight: u16,
        color: Color,
        anchor: Anchor,
    ) {
        self.items.push(Item::Text {
            x,
            y: baseline(top, size),
            text: text.into(),
            size,
            weight,
            color,
            anchor,
            mono: false,
        });
    }

    /// Text vertically centered on `center_y`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_centered(
        &mut self,
        x: f32,
        center_y: f32,
        text: impl Into<String>,
        size: f32,
        weight: u16,
        color: Color,
        anchor: Anchor,
    ) {
        self.items.push(Item::Text {
            x,
            y: center_y + size * 0.35,
            text: text.into(),
            size,
            weight,
            color,
            anchor,
            mono: false,
        });
    }

    pub(crate) fn fill_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        fill: Color,
    ) {
        self.items.push(Item::Rect {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
            radius,
            fill: Some(fill),
            stroke: None,
            stroke_width: 0.0,
        });
    }

    /// A 1px-or-wider straight line.
    pub(crate) fn line(&mut self, from: [f32; 2], to: [f32; 2], color: Color, width: f32) {
        self.items.push(Item::Path {
            points: vec![from, to],
            closed: false,
            fill: None,
            stroke: Some(color),
            stroke_width: width,
            dashed: false,
        });
    }

    pub(crate) fn circle(&mut self, cx: f32, cy: f32, r: f32, fill: Color) {
        self.fill_rect(cx - r, cy - r, r * 2.0, r * 2.0, r, fill);
    }

    /// The index the next pushed item will get, for a region that stands for that item.
    pub(crate) fn last_item(&self) -> Option<usize> {
        self.items.len().checked_sub(1)
    }

    pub(crate) fn region(
        &mut self,
        item: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        lines: Vec<TooltipLine>,
    ) {
        if lines.is_empty() || !(width > 0.0 && height > 0.0) {
            return;
        }
        self.regions.push(Region {
            x,
            y,
            width,
            height,
            lines,
            item,
        });
    }
}

/// The alphabetic baseline of a line of `size` text whose line box starts at `top`.
pub(crate) fn baseline(top: f32, size: f32) -> f32 {
    top + (line_height(size) - size) / 2.0 + size * 0.85
}

/// Snaps a 1px line's coordinate to a pixel center so it draws crisp.
pub(crate) fn crisp(v: f32) -> f32 {
    v.floor() + 0.5
}

fn translate(item: Item, dx: f32, dy: f32) -> Item {
    match item {
        Item::Rect {
            x,
            y,
            width,
            height,
            radius,
            fill,
            stroke,
            stroke_width,
        } => Item::Rect {
            x: x + dx,
            y: y + dy,
            width,
            height,
            radius,
            fill,
            stroke,
            stroke_width,
        },
        Item::Path {
            points,
            closed,
            fill,
            stroke,
            stroke_width,
            dashed,
        } => Item::Path {
            points: points.into_iter().map(|[x, y]| [x + dx, y + dy]).collect(),
            closed,
            fill,
            stroke,
            stroke_width,
            dashed,
        },
        Item::Text {
            x,
            y,
            text,
            size,
            weight,
            color,
            anchor,
            mono,
        } => Item::Text {
            x: x + dx,
            y: y + dy,
            text,
            size,
            weight,
            color,
            anchor,
            mono,
        },
    }
}

const MONO_STACK: &str =
    "ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace";

/// The scene as a standalone SVG document: `width`/`height`/`viewBox` are the scene size, the
/// background is transparent, and every text uses `font_family` (or a monospace stack when `mono`).
pub fn scene_to_svg(scene: &Scene, font_family: &str) -> String {
    let width = num(scene.width);
    let height = num(scene.height);
    let mut out = String::with_capacity(256 + scene.items.len() * 120);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">"
    ));
    for item in &scene.items {
        match item {
            Item::Rect {
                x,
                y,
                width,
                height,
                radius,
                fill,
                stroke,
                stroke_width,
            } => {
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    num(*x),
                    num(*y),
                    num(width.max(0.0)),
                    num(height.max(0.0))
                ));
                if *radius > 0.0 {
                    out.push_str(&format!(" rx=\"{}\"", num(*radius)));
                }
                paint(&mut out, "fill", *fill);
                stroke_attrs(&mut out, *stroke, *stroke_width, false);
                out.push_str("/>");
            }
            Item::Path {
                points,
                closed,
                fill,
                stroke,
                stroke_width,
                dashed,
            } => {
                if points.is_empty() {
                    continue;
                }
                let mut d = String::with_capacity(points.len() * 14);
                for (i, [x, y]) in points.iter().enumerate() {
                    d.push(if i == 0 { 'M' } else { 'L' });
                    d.push_str(&num(*x));
                    d.push(' ');
                    d.push_str(&num(*y));
                }
                if *closed {
                    d.push('Z');
                }
                out.push_str(&format!("<path d=\"{d}\""));
                paint(&mut out, "fill", *fill);
                stroke_attrs(&mut out, *stroke, *stroke_width, *dashed);
                if stroke.is_some() {
                    out.push_str(" stroke-linejoin=\"round\" stroke-linecap=\"round\"");
                }
                out.push_str("/>");
            }
            Item::Text {
                x,
                y,
                text,
                size,
                weight,
                color,
                anchor,
                mono,
            } => {
                let family = if *mono { MONO_STACK } else { font_family };
                let anchor = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                out.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" text-anchor=\"{}\"",
                    num(*x),
                    num(*y),
                    escape(family),
                    num(*size),
                    weight,
                    anchor
                ));
                paint(&mut out, "fill", Some(*color));
                out.push('>');
                out.push_str(&escape(text));
                out.push_str("</text>");
            }
        }
    }
    out.push_str("</svg>");
    out
}

fn paint(out: &mut String, attr: &str, color: Option<Color>) {
    match color {
        Some(c) => {
            out.push_str(&format!(" {attr}=\"{}\"", c.hex_rgb()));
            if c.a < 1.0 {
                out.push_str(&format!(" {attr}-opacity=\"{}\"", format_alpha(c.a)));
            }
        }
        None => out.push_str(&format!(" {attr}=\"none\"")),
    }
}

fn stroke_attrs(out: &mut String, stroke: Option<Color>, width: f32, dashed: bool) {
    if let Some(c) = stroke {
        if width > 0.0 {
            paint(out, "stroke", Some(c));
            out.push_str(&format!(" stroke-width=\"{}\"", num(width)));
            if dashed {
                out.push_str(" stroke-dasharray=\"4 3\"");
            }
        }
    }
}

/// A coordinate with at most two decimals and no trailing zeros.
fn num(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let text = format!("{:.2}", v);
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text.as_str()
    };
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not allowed in XML 1.0.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
