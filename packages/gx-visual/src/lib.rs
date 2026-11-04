//! Chat visuals: turns one fenced ```visual block (JSON) into a drawing scene every Ghostex client
//! paints the same way.
//!
//! Agents put a block fenced as `visual`, `vega-lite` or `vegalite` holding JSON into a chat
//! reply. Each client calls [`render`] with the block, the width it has, and its [`Theme`]; the
//! result is a [`Scene`] of rects, paths and text (plus hover [`Region`]s), or a [`PageRef`] when the
//! block points at a published HTML page. GPUI paints the scene natively; the phone and the CLI use
//! [`scene_to_svg`] of the same scene. Layout uses estimated text widths ([`text_width`]) instead of
//! fonts, so every client places labels identically.
//!
//! The block grammar:
//!
//! - `{"page": {"title": …, "url": "https://…", "file"?: …, "open"?: "popup", "questions"?: […]}}`: a
//!   published page, shown as a card; `"open": "popup"` is the floating mark `ghostex show` prints
//!   (the desktop opens every card where its Open chat pages in setting says), and `"questions"` (the tags its questions carry) marks a page that asks the reader.
//! - A Vega-Lite spec (an object with `"mark"`): one chart in a strict subset (marks `bar`, `line`,
//!   `area`, `point`/`circle`/`square`, `arc`; channels `x`, `y`, `color`, `theta`, `xOffset`,
//!   `tooltip`; inline `data.values` only).
//! - `{"title"?: …, "rows": [part, …]}`: parts stacked vertically. A part is `{"chart": spec}`
//!   (or a spec itself), `{"stats": [tiles]}`, `{"table": {"columns", "rows"}}`, `{"text": …}`,
//!   `{"columns": [parts]}` or a nested `{"rows": [parts]}`, each with an optional `"title"`. A
//!   single part may also stand at the top level.
//!
//! Errors are short sentences a person (or the agent that wrote the block) can act on; they name
//! the unsupported feature, the unknown field, or the JSON problem.
//!
//! CDXC:SessionChat 2026-10-06 SEE-ALSO:
//! `packages/gx-chat-core/src/transcript/native_markdown.rs` marks the fence;
//! `apps/desktop/src/app/native_chat/visual.rs` is the GPUI painter (also compiled by `apps/gpui-web`);
//! `apps/mobile/app/src/chat/native/transcript/VisualBlock.tsx` draws it on the phone through the chat core's `renderVisual` query ([`render_json`]);
//! `server/src/ghostex_cli/visual.rs` runs `ghostex visual check`;
//! `skills/ghostex-visuals/SKILL.md` tells agents what they may write.
//! A grammar or subset change lands in the skill in the same commit.

mod layout;
mod motion;
pub mod questions;
mod scene;
#[cfg(test)]
mod size_tests;
mod spec;
mod stats;
mod table;
mod text;
mod theme;
mod vega;

pub use motion::Motion;
pub use scene::{scene_to_svg, Anchor, Item, PageRef, Region, Scene, TooltipLine, Visual};
pub use text::text_width;
pub use theme::{Color, Theme, DEFAULT_TEXT_SIZE};

use serde_json::json;

/// The largest block (in bytes) [`render`] accepts.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;

const FENCE_LANGUAGES: [&str; 3] = ["visual", "vega-lite", "vegalite"];

/// Fence languages that hold a visual block: "visual", "vega-lite", "vegalite" (ASCII
/// case-insensitive, trimmed).
pub fn is_visual_language(language: &str) -> bool {
    let language = language.trim();
    FENCE_LANGUAGES
        .iter()
        .any(|known| language.eq_ignore_ascii_case(known))
}

/// Parses and lays out one block at `width` logical pixels (clamped to 200..=2000). `Err` is a
/// short, user-readable message, shown in the chat and printed by the CLI.
///
/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: agents can put a visual block in a chat reply and it is drawn inline in the chat on every client.
/// User: no JavaScript engine; visuals are drawn natively from a declarative spec.
/// User: charts use a strict Vega-Lite subset; a feature outside it is an error that names the feature, never a silent approximation.
pub fn render(source: &str, width: f32, theme: &Theme) -> Result<Visual, String> {
    let width = if width.is_finite() {
        width.clamp(200.0, 2000.0)
    } else {
        640.0
    };
    let value = spec::parse_json(source)?;
    match spec::parse_doc(&value)? {
        spec::Doc::Page(page) => Ok(Visual::Page(page)),
        spec::Doc::Drawing { title, rows } => {
            layout::layout(title, &rows, width, theme).map(Visual::Drawing)
        }
    }
}

/// The page reference a block holds, if it is one (cheap; no layout).
pub fn page_reference(source: &str) -> Option<PageRef> {
    if source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let value = spec::parse_json(source).ok()?;
    spec::page_of(&value)
}

/// A drawing block's title (its own, else its first part's), or `None` inside when it has none;
/// the outer `None` when the block is a page or does not parse (cheap; no layout). The chat's
/// list of generated content names a chart by it.
pub fn drawing_title(source: &str) -> Option<Option<String>> {
    if source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let value = spec::parse_json(source).ok()?;
    match spec::parse_doc(&value).ok()? {
        spec::Doc::Page(_) => None,
        spec::Doc::Drawing { title, rows } => {
            Some(title.or_else(|| rows.iter().find_map(|leaf| leaf.title.clone())))
        }
    }
}

/// For the phone (through the chat core's query): `theme_json` is read with
/// [`Theme::from_json`]. Returns JSON text:
/// `{"kind":"drawing","width","height","title","svg","regions"}`, `{"kind":"page","title","url"}`
/// or `{"kind":"error","message"}`.
/// The regions with the outline of the closed path each stands for (`shape`), since the phone
/// only has the SVG and still has to tell which pie slice a tap landed on.
fn phone_regions(scene: &Scene) -> Vec<serde_json::Value> {
    scene
        .regions
        .iter()
        .map(|region| {
            let mut value = serde_json::to_value(region).unwrap_or_default();
            let shape = region.item.and_then(|item| match scene.items.get(item) {
                Some(Item::Path {
                    points,
                    closed: true,
                    ..
                }) => Some(points.clone()),
                _ => None,
            });
            if let (Some(shape), Some(object)) = (shape, value.as_object_mut()) {
                object.insert("shape".to_string(), json!(shape));
            }
            value
        })
        .collect()
}

pub fn render_json(source: &str, width: f32, theme_json: &str, font_family: &str) -> String {
    render_json_at(source, width, theme_json, font_family, None)
}

/// [`render_json`] with the drawing `progress` (0 to 1) of the way through drawing in
/// ([`Scene::at`]); `None` is the finished drawing. `motion` says whether the chart has marks
/// that draw in at all; the regions are always the finished drawing's.
pub fn render_json_at(
    source: &str,
    width: f32,
    theme_json: &str,
    font_family: &str,
    progress: Option<f32>,
) -> String {
    let theme_value = serde_json::from_str::<serde_json::Value>(theme_json).unwrap_or_default();
    let theme = Theme::from_json(&theme_value);
    let value = match render(source, width, &theme) {
        Ok(Visual::Drawing(scene)) => {
            let drawn = match progress {
                Some(progress) if progress < 1.0 => scene.at(progress),
                _ => scene.clone(),
            };
            json!({
                "kind": "drawing",
                "width": scene.width,
                "height": scene.height,
                "title": scene.title,
                "svg": scene_to_svg(&drawn, font_family),
                "regions": phone_regions(&scene),
                "motion": scene.has_motion(),
            })
        }
        Ok(Visual::Page(page)) => json!({
            "kind": "page",
            "title": page.title,
            "url": page.url,
            "file": page.file,
            "open": if page.popup { "popup" } else { "browser" },
            "pageId": page.page_id(),
            "questions": page.questions,
        }),
        Err(message) => json!({
            "kind": "error",
            "message": message,
        }),
    };
    value.to_string()
}
