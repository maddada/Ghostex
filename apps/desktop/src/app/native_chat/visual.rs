//! Visual blocks in the transcript: a finished ```visual (or ```vega-lite) fence, marked by the
//! core, drawn as its chart, stats, table or text layout, or, for a published HTML page, as a card
//! that opens the page.
//!
//! The block's JSON is laid out by the shared visual crate (`ghostex_gx_chat_core::visual`) into a
//! scene of rectangles, paths and text, and painted here with GPUI's own primitives, so the
//! desktop, the web build and the GPUI phone draw the same thing with no browser engine.
//!
//! CDXC:SessionChat 2026-10-06 DECISION:
//! User: build both kinds of chat visuals together: charts, stats and tables drawn natively in the chat from a ```visual block (charts in a strict Vega-Lite subset), and HTML pages for what a block cannot express, which open from a card rather than running inside the chat. No JavaScript engine is added to the desktop for either.
//! CDXC:SessionChat 2026-10-06 SEE-ALSO:
//! The marks come from packages/gx-chat-core/src/transcript/native_markdown.rs (`NATIVE_VISUAL_OPEN`); the layout is packages/gx-visual; the phone draws the same scene as SVG in apps/mobile/app/src/chat/native/transcript/VisualBlock.tsx; pages are published by `ghostex show` (server/src/ghostex_cli/) and agents learn both from skills/ghostex-visuals/SKILL.md.

use super::{
    appearance::ChatAppearance, fonts::CHAT_MONO, rich_markdown::PROSE_SIZE, state::NativeChatView,
};
use ghostex_gx_chat_core::visual::{self as gxv, Anchor, Item, Region, Scene, Visual};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, BorderStyle, Bounds, ClipboardItem, Context, Corners, Edges, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseMoveEvent, ParentElement as _, PathBuilder, Pixels,
    Rgba, SharedString, StatefulInteractiveElement as _, Styled as _, TextAlign, TextRun, Window,
    canvas, div, point, px, size, svg,
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub(super) const OPEN: &str = "\u{E000}visual";
pub(super) const CLOSE: &str = "\u{E000}/visual";

const PADDING: f32 = 12.0;
/// The width a block is laid out at before its card has been measured once.
const DEFAULT_WIDTH: u32 = 640;
/// Layout widths are rounded to this step, so a few pixels of resize do not lay the block out again.
const WIDTH_STEP: f32 = 8.0;
/// Laid-out blocks kept before the cache starts over.
const MAX_CACHED: usize = 256;
const TOOLTIP_WIDTH: f32 = 240.0;

type Laid = Rc<Result<Visual, String>>;

/// `packages/gx-chat-core/visual/chart-motion.json`: how long a chart takes to draw in.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartMotion {
    duration_ms: u64,
}

static CHART_MOTION: std::sync::LazyLock<ChartMotion> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/chart-motion.json"
    ))
    .expect("shared chart motion")
});

thread_local! {
    /// When each chart (by block key) was first drawn in this run of the app. A chart draws in
    /// only then (`gxv::Scene::at`), not when its row is drawn again, scrolled back to, or shown
    /// in a session the reader comes back to (see `motion.rs` in gx-visual).
    static FIRST_DRAWN: RefCell<HashMap<String, Option<web_time::Instant>>> =
        RefCell::new(HashMap::new());
}

/// How far `key`'s chart is through drawing in, or `None` once it has finished (or never draws
/// in: reduced motion, or a chart with nothing that moves).
fn draw_in_progress(key: &str, scene: &Scene, reduce_motion: bool) -> Option<f32> {
    FIRST_DRAWN.with(|first| {
        let mut first = first.borrow_mut();
        let started = *first
            .entry(key.to_owned())
            .or_insert_with(|| (!reduce_motion && scene.has_motion()).then(web_time::Instant::now));
        let elapsed = started?.elapsed().as_millis() as f32;
        let progress = elapsed / CHART_MOTION.duration_ms as f32;
        if progress >= 1.0 {
            first.insert(key.to_owned(), None);
            None
        } else {
            Some(progress)
        }
    })
}

/// The transcript's visual blocks, laid out once per source, width and theme.
///
/// Asked for while a row renders with the view borrowed shared, so the maps are behind cells, as
/// the transcript's diagrams are (`mermaid.rs`).
#[derive(Default)]
pub(crate) struct ChatVisualCache {
    laid_out: RefCell<HashMap<(String, u32, bool), Laid>>,
    /// The width each block's card last measured, in unscaled logical pixels.
    widths: Rc<RefCell<HashMap<String, u32>>>,
    /// The tooltip region the pointer is over, per block, and where the pointer is.
    hovered: RefCell<HashMap<String, Hover>>,
    /// The blocks the reader switched to their source.
    showing_source: HashSet<String>,
    /// The pictures of the transcript's page cards (`page_card.rs`).
    pub(super) page_thumbnails: super::page_card::PageThumbnails,
}

/// The region under the pointer and the pointer's place in scene units.
#[derive(Clone, Copy, PartialEq)]
struct Hover {
    region: usize,
    x: f32,
    y: f32,
}

fn width_bucket(width: f32) -> u32 {
    ((width / WIDTH_STEP).round() * WIDTH_STEP).max(WIDTH_STEP) as u32
}

fn to_color(color: Hsla) -> gxv::Color {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    gxv::Color {
        r: channel(rgba.r),
        g: channel(rgba.g),
        b: channel(rgba.b),
        a: rgba.a,
    }
}

fn to_hsla(color: gxv::Color) -> Hsla {
    Hsla::from(Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: color.a,
    })
}

/// The chat's own colors for the scene's text, lines and fills; the series palette and the
/// good/bad tones stay the crate's, which were chosen to read on both themes.
fn theme(p: &ChatAppearance) -> gxv::Theme {
    let mut theme = if p.light {
        gxv::Theme::light()
    } else {
        gxv::Theme::dark()
    };
    theme.foreground = to_color(p.prose);
    theme.muted = to_color(p.muted);
    theme.border = to_color(p.border);
    theme.grid = to_color(p.border.opacity(0.6));
    theme.background = to_color(p.input);
    // CDXC:SessionChat 2026-10-31 DECISION: User: text inside the chat's graphs and visuals was smaller than the transcript's; "make it same as the font size of text in the chat transcript". Every label, cell, note, legend and axis tick is drawn at the transcript's prose size and the scene is painted at the chat's zoom, so it follows the chat's text size setting; only the stat values stay larger, in proportion.
    // CDXC:SessionChat 2026-10-31 SEE-ALSO: apps/mobile/app/src/chat/native/transcript/VisualBlock.tsx (`visualTheme` sends `textSize`) and packages/gx-visual/src/theme.rs (`text_size`).
    theme.text_size = PROSE_SIZE;
    theme
}

/// The hovered bar, dot or slice: a touch lighter on the dark theme, a touch darker on the light.
fn hovered_color(color: gxv::Color, light: bool) -> gxv::Color {
    let (target, amount) = if light { (0.0, 0.14) } else { (255.0, 0.22) };
    let mix =
        |channel: u8| (f32::from(channel) + (target - f32::from(channel)) * amount).round() as u8;
    gxv::Color {
        r: mix(color.r),
        g: mix(color.g),
        b: mix(color.b),
        a: color.a,
    }
}

/// Whether `(x, y)` is inside the closed outline `points` (even-odd rule).
fn inside_polygon(points: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut previous = points.len().wrapping_sub(1);
    for (index, [px_, py_]) in points.iter().enumerate() {
        let [qx, qy] = points[previous];
        if (*py_ > y) != (qy > y) && x < (qx - px_) * (y - py_) / (qy - py_) + px_ {
            inside = !inside;
        }
        previous = index;
    }
    inside
}

/// The region under the scene point `(x, y)`: a slice by its own outline, anything else by its box.
fn region_at(scene: &Scene, x: f32, y: f32) -> Option<usize> {
    scene.regions.iter().position(|region| {
        let in_box = x >= region.x
            && x <= region.x + region.width
            && y >= region.y
            && y <= region.y + region.height;
        match region.item.and_then(|item| scene.items.get(item)) {
            Some(Item::Path {
                points,
                closed: true,
                ..
            }) => in_box && inside_polygon(points, x, y),
            _ => in_box,
        }
    })
}

/// Paints one laid-out scene into `bounds`, `scale` device-independent pixels per scene unit.
///
/// CDXC:SessionChat 2026-10-06 DECISION: User: no square highlight behind a hovered pie slice; "just slightly change the color of the part i'm hovering". A region that stands for a shape (bar, dot, slice) recolours that shape; only a line chart's column, which has no shape of its own, keeps a faint band.
#[allow(clippy::too_many_arguments)]
fn paint_scene(
    scene: &Scene,
    hovered: Option<&Region>,
    hover_fill: Hsla,
    light: bool,
    bounds: Bounds<Pixels>,
    scale: f32,
    font: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let origin = bounds.origin;
    let at = |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px(y * scale));
    let transparent = gpui::transparent_black();
    let highlighted = hovered.and_then(|region| region.item);
    if let Some(region) = hovered.filter(|region| region.item.is_none()) {
        window.paint_quad(gpui::fill(
            Bounds::new(
                at(region.x, region.y),
                size(px(region.width * scale), px(region.height * scale)),
            ),
            hover_fill,
        ));
    }
    for (index, item) in scene.items.iter().enumerate() {
        let lit = |fill: &Option<gxv::Color>| {
            fill.map(|color| {
                if highlighted == Some(index) {
                    hovered_color(color, light)
                } else {
                    color
                }
            })
        };
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
                let bounds = Bounds::new(at(*x, *y), size(px(width * scale), px(height * scale)));
                let border = if stroke.is_some() {
                    stroke_width * scale
                } else {
                    0.0
                };
                window.paint_quad(gpui::quad(
                    bounds,
                    Corners::all(px(radius * scale)),
                    lit(fill).map(to_hsla).unwrap_or(transparent),
                    Edges::all(px(border)),
                    stroke.map(to_hsla).unwrap_or(transparent),
                    BorderStyle::Solid,
                ));
            }
            Item::Path {
                points,
                closed,
                fill,
                stroke,
                stroke_width,
                dashed,
            } => {
                if points.len() < 2 {
                    continue;
                }
                let points = points.iter().map(|[x, y]| at(*x, *y)).collect::<Vec<_>>();
                if let Some(fill) = lit(fill).filter(|_| points.len() >= 3) {
                    let mut builder = PathBuilder::fill();
                    builder.add_polygon(&points, true);
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, to_hsla(fill));
                    }
                }
                if let Some(stroke) = stroke {
                    let mut builder = PathBuilder::stroke(px(stroke_width * scale));
                    if *dashed {
                        builder = builder.dash_array(&[px(4.0 * scale), px(3.0 * scale)]);
                    }
                    builder.add_polygon(&points, *closed);
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, to_hsla(*stroke));
                    }
                }
            }
            Item::Text {
                x,
                y,
                text,
                size: text_size,
                weight,
                color,
                anchor,
                mono,
            } => {
                if text.is_empty() {
                    continue;
                }
                let family = if *mono {
                    CHAT_MONO.to_string()
                } else {
                    font.to_string()
                };
                let run = TextRun {
                    len: text.len(),
                    font: gpui::Font {
                        weight: FontWeight(f32::from(*weight)),
                        ..gpui::font(family)
                    },
                    color: to_hsla(*color),
                    ..Default::default()
                };
                let line = window.text_system().shape_line(
                    SharedString::from(text.clone()),
                    px(text_size * scale),
                    &[run],
                    None,
                );
                let width = f32::from(line.width);
                let left = match anchor {
                    Anchor::Start => x * scale,
                    Anchor::Middle => x * scale - width / 2.0,
                    Anchor::End => x * scale - width,
                };
                // The scene places text by its baseline; a line box exactly as tall as the glyphs
                // puts the baseline `ascent` below the box's top.
                let top = origin.y + px(y * scale) - line.ascent;
                let _ = line.paint(
                    point(origin.x + px(left), top),
                    line.ascent + line.descent,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }
    }
}

impl NativeChatView {
    /// The block laid out at `width`, laid out now the first time it is asked for. Layout is pure
    /// arithmetic over a bounded block, cheap enough for the render pass.
    fn visual_layout(&self, source: &str, width: u32, p: &ChatAppearance) -> Laid {
        let key = (source.to_owned(), width, p.light);
        if let Some(laid) = self.visual.laid_out.borrow().get(&key) {
            return laid.clone();
        }
        let laid = Rc::new(gxv::render(source, width as f32, &theme(p)));
        let mut cache = self.visual.laid_out.borrow_mut();
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, laid.clone());
        laid
    }

    fn toggle_visual_source(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.visual.showing_source.remove(&key) {
            self.visual.showing_source.insert(key);
        }
        self.list.remeasure();
        cx.notify();
    }

    /// One marked ```visual fence: a page card for a published page, otherwise the drawing in the
    /// card a fenced block sits in.
    pub(super) fn visual_card(
        &self,
        key: String,
        fence: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let source = super::mermaid::fence_source(fence);
        if let Some(page) = gxv::page_reference(&source) {
            let page_id = page.page_id().map(str::to_string);
            let card = self.visual_page_card(&key, page, p, cx);
            return self.open_question_frame(page_id.as_deref(), card, p);
        }
        let s = p.scale;
        let width = self
            .visual
            .widths
            .borrow()
            .get(&key)
            .copied()
            .unwrap_or(DEFAULT_WIDTH);
        let laid = self.visual_layout(&source, width, p);
        let showing_source = self.visual.showing_source.contains(&key);
        let title = match &*laid {
            Ok(Visual::Drawing(scene)) => scene.title.clone(),
            _ => None,
        };
        let drawable = matches!(&*laid, Ok(Visual::Drawing(_)));
        let chat = cx.weak_entity();
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0 * s))
            .w_full()
            .pl(px(12.0 * s))
            .pr(px(6.0 * s))
            .py(px(2.0 * s))
            .border_b(px(1.0))
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(5.0 * s))
                    .text_size(px(PROSE_SIZE * s))
                    .text_color(p.muted)
                    .child(
                        svg()
                            .path("titlebar/chart-bar.svg")
                            .size(px(13.0 * s))
                            .text_color(p.muted)
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(title.unwrap_or_else(|| "visual".to_string())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(2.0 * s))
                    .when(drawable, |this| {
                        let chat = chat.clone();
                        let key = key.clone();
                        let (icon, label) = if showing_source {
                            ("titlebar/chart-bar.svg", "Chart")
                        } else {
                            ("titlebar/code.svg", "Source")
                        };
                        this.child(super::mermaid::button(
                            "visual-toggle-source",
                            icon,
                            Some(label),
                            p,
                            move |cx| {
                                let key = key.clone();
                                let _ =
                                    chat.update(cx, |chat, cx| chat.toggle_visual_source(key, cx));
                            },
                        ))
                    })
                    .child({
                        let source = source.clone();
                        super::mermaid::button(
                            "visual-copy",
                            "titlebar/copy.svg",
                            None,
                            p,
                            move |cx| {
                                crate::app::helpers::gpui_copy_to_clipboard(
                                    ClipboardItem::new_string(source.clone()),
                                    cx,
                                );
                            },
                        )
                    }),
            );
        let content = match &*laid {
            Ok(Visual::Drawing(scene)) if !showing_source => {
                self.visual_scene(&key, scene, width, p, cx)
            }
            Err(message) => self.visual_source(&key, &source, Some(message.clone()), p),
            _ => self.visual_source(&key, &source, None, p),
        };
        let card = div()
            .id(SharedString::from(format!("visual:{key}")))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .bg(p.input)
            .border_1()
            .border_color(p.border)
            .rounded(px(12.0 * s))
            .child(header)
            .child(content)
            .into_any_element();
        // A jump from the ? pill's list flashes the chart it named (gx-chat-core
        // questions/content.rs `chart_id`).
        let chart = ghostex_gx_chat_core::questions::content::chart_id(&source);
        self.open_question_frame(Some(&chart), card, p)
    }

    /// The block's JSON, with the reason it is shown instead of the drawing when there is one.
    fn visual_source(
        &self,
        key: &str,
        source: &str,
        note: Option<String>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .when_some(note, |this, note| {
                this.child(
                    div()
                        .px(px(PADDING * s))
                        .pt(px(PADDING * s))
                        .text_size(px(PROSE_SIZE * s))
                        .text_color(p.muted)
                        .child(note),
                )
            })
            .child(
                self.nested_scroll(
                    format!("visual-source:{key}"),
                    div()
                        .max_h(px(480.0 * s))
                        .p(px(PADDING * s))
                        .font_family(CHAT_MONO)
                        .text_size(px(12.0 * s))
                        .line_height(px(19.2 * s))
                        .text_color(p.code_text)
                        .children(source.lines().map(|line| {
                            div().child(if line.is_empty() {
                                " ".to_string()
                            } else {
                                line.to_string()
                            })
                        })),
                ),
            )
            .into_any_element()
    }

    /// The drawing, painted at the card's own width, with the tooltip of the region under the
    /// pointer.
    fn visual_scene(
        &self,
        key: &str,
        scene: &Scene,
        laid_width: u32,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let drawing_in = draw_in_progress(key, scene, cx.reduce_motion());
        let scene = Rc::new(scene.clone());
        let drawn = Rc::new(match drawing_in {
            Some(progress) => scene.at(progress),
            None => (*scene).clone(),
        });
        let hovered = self
            .visual
            .hovered
            .borrow()
            .get(key)
            .copied()
            .filter(|hover| hover.region < scene.regions.len());
        let bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
        let chat = cx.weak_entity();
        let hover_fill = p.prose.opacity(0.07);
        let font = p.font.clone();
        let light = p.light;
        let painter = canvas(
            {
                let bounds_cell = bounds_cell.clone();
                let widths = self.visual.widths.clone();
                let chat = chat.clone();
                let key = key.to_owned();
                move |bounds: Bounds<Pixels>, _: &mut Window, cx: &mut App| {
                    bounds_cell.set(Some(bounds));
                    // The block is laid out for the width its card had last time; when the card
                    // turns out wider or narrower, lay it out again for the next frame.
                    let measured = width_bucket(f32::from(bounds.size.width) / s);
                    if measured != laid_width && widths.borrow().get(&key) != Some(&measured) {
                        widths.borrow_mut().insert(key.clone(), measured);
                        let chat = chat.clone();
                        cx.defer(move |cx| {
                            let _ = chat.update(cx, |this, cx| {
                                this.list.remeasure();
                                cx.notify();
                            });
                        });
                    }
                }
            },
            {
                let scene = scene.clone();
                move |bounds: Bounds<Pixels>, _: (), window: &mut Window, cx: &mut App| {
                    let region = hovered.and_then(|hover| scene.regions.get(hover.region));
                    paint_scene(
                        &drawn, region, hover_fill, light, bounds, s, &font, window, cx,
                    );
                    // The transcript is a cached view: a chart drawing in asks for its own frames.
                    if drawing_in.is_some() {
                        window.request_animation_frame();
                    }
                }
            },
        )
        .size_full();
        let tooltip = hovered.and_then(|hover| {
            scene
                .regions
                .get(hover.region)
                .filter(|region| !region.lines.is_empty())
                .map(|region| tooltip(region, hover, &scene, p))
        });
        div()
            .p(px(PADDING * s))
            .w_full()
            .child(
                div()
                    .id(SharedString::from(format!("visual-scene:{key}")))
                    .relative()
                    .w_full()
                    .h(px(scene.height * s))
                    .child(painter)
                    .on_mouse_move({
                        let bounds_cell = bounds_cell.clone();
                        let scene = scene.clone();
                        let chat = chat.clone();
                        let key = key.to_owned();
                        move |event: &MouseMoveEvent, _, cx| {
                            let Some(bounds) = bounds_cell.get() else {
                                return;
                            };
                            let x = f32::from(event.position.x - bounds.origin.x) / s;
                            let y = f32::from(event.position.y - bounds.origin.y) / s;
                            let hit = region_at(&scene, x, y).map(|region| Hover { region, x, y });
                            let key = key.clone();
                            let _ = chat.update(cx, |this, cx| {
                                let changed = {
                                    let mut hovered = this.visual.hovered.borrow_mut();
                                    match hit {
                                        Some(hover) => hovered.insert(key, hover) != Some(hover),
                                        None => hovered.remove(&key).is_some(),
                                    }
                                };
                                if changed {
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .on_hover({
                        let chat = chat.clone();
                        let key = key.to_owned();
                        move |inside, _, cx| {
                            if *inside {
                                return;
                            }
                            let _ = chat.update(cx, |this, cx| {
                                if this.visual.hovered.borrow_mut().remove(&key).is_some() {
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .when_some(tooltip, |this, tooltip| this.child(tooltip)),
            )
            .into_any_element()
    }
}

/// The values under the pointer, beside the region they belong to.
///
/// It sits just below and right of the pointer, flipping to the other side where it would run
/// past the drawing, so it stays beside the bar or slice being read even when the region is wide.
fn tooltip(region: &Region, hover: Hover, scene: &Scene, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    const OFFSET: f32 = 12.0;
    const TEXT: f32 = PROSE_SIZE;
    let width = region
        .lines
        .iter()
        .map(|line| {
            let swatch = if line.color.is_some() { 14.0 } else { 0.0 };
            let label = if line.label.is_empty() {
                0.0
            } else {
                gxv::text_width(&line.label, TEXT, 400) + 6.0
            };
            swatch + label + gxv::text_width(&line.value, TEXT, 500)
        })
        .fold(0.0_f32, f32::max)
        .min(TOOLTIP_WIDTH - 20.0)
        + 20.0;
    let lines = region.lines.len() as f32;
    let height = lines * TEXT * 1.4 + (lines - 1.0).max(0.0) * 3.0 + 16.0;
    let left = if hover.x + OFFSET + width <= scene.width {
        hover.x + OFFSET
    } else {
        (hover.x - OFFSET - width).max(0.0)
    };
    let top = if hover.y + OFFSET + height <= scene.height {
        hover.y + OFFSET
    } else {
        (hover.y - OFFSET - height).max(0.0)
    };
    // CDXC:SessionChat 2026-10-06 DECISION: User: the tooltip was "too transparent", so it is a solid card rather than the frosted menu surface.
    let background: Hsla = if p.light {
        gpui::rgb(0xffffff).into()
    } else {
        gpui::rgb(0x1f1f23).into()
    };
    div()
        .absolute()
        .left(px(left * s))
        .top(px(top * s))
        .max_w(px(TOOLTIP_WIDTH * s))
        .flex()
        .flex_col()
        .gap(px(3.0 * s))
        .px(px(9.0 * s))
        .py(px(7.0 * s))
        .rounded(px(8.0 * s))
        .bg(background)
        .border_1()
        .border_color(p.border)
        .shadow_md()
        .text_size(px(PROSE_SIZE * s))
        .children(region.lines.iter().map(|line| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .min_w_0()
                .when_some(line.color, |this, color| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .size(px(8.0 * s))
                            .rounded(px(2.0 * s))
                            .bg(to_hsla(color)),
                    )
                })
                .when(!line.label.is_empty(), |this| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.muted)
                            .child(line.label.clone()),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.prose)
                        .child(line.value.clone()),
                )
        }))
        .into_any_element()
}
