//! The transcript minimap's geometry and its hover previews, ported from
//! `packages/shared/session-chat-presentation/minimap.ts`.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! One dash per user prompt, drawn by apps/desktop/src/app/native_chat/minimap.rs, which reads the
//! same minimap.json.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `minimap.json`, the one table both renderers already read. Included rather than ported, the way
/// `docs/2026-09-21/rust-chat/SEAM.md` section 4 requires of the presentation tables.
const GEOMETRY_JSON: &str = include_str!("../../visual/minimap.json");

/// The rail's dimensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimapGeometry {
    pub minimum_turns: usize,
    /// The one fractional field; every other dimension is a whole pixel, and stays an integer so a
    /// `1` can never be written back as `1.0`.
    pub scale: f64,
    pub spacing: i64,
    pub dash_height: i64,
    pub rail_width: i64,
    pub line_offset: i64,
    pub column_width: i64,
    pub padding_block: i64,
    pub dash_widths: Vec<i64>,
    /// The most UTF-16 units one preview (the prompt or the reply) keeps.
    pub preview_limit: usize,
    /// How many lines the hover card shows, the prompt and its reply together.
    pub preview_lines: usize,
    /// The hover card's widest, before the scale.
    pub preview_max_width: i64,
    pub preview_font_size: i64,
    pub preview_line_height: i64,
}

/// `SESSION_CHAT_MINIMAP`.
pub fn geometry() -> MinimapGeometry {
    serde_json::from_str(GEOMETRY_JSON).expect("minimap.json ships with this crate")
}

/// One marker row on the wire: the dash, the transcript row it jumps to, and what the hover
/// preview says.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimapMarkerRow {
    pub id: String,
    pub item: usize,
    pub prompt: String,
    pub reply: String,
}

/// The prompt's own words as Markdown, one paragraph per text block.
pub fn minimap_preview_text(message: Option<&Value>) -> String {
    let blocks = message
        .and_then(|message| message.get("blocks"))
        .and_then(Value::as_array);
    let joined = blocks
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .map(|block| {
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default();
    crate::extras::agent_tasks::js_trim(&joined).to_string()
}

/// [`minimap_preview_text`] over a typed message.
pub fn message_preview_text(message: Option<&ghostex_gx_protocol::chat::ChatMessage>) -> String {
    use ghostex_gx_protocol::chat::ChatBlock;
    let joined = message
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ChatBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default();
    crate::extras::agent_tasks::js_trim(&joined).to_string()
}

/// A message's hover preview: its first `lines` lines of Markdown, and how many lines it kept.
pub fn message_preview(
    message: Option<&ghostex_gx_protocol::chat::ChatMessage>,
    lines: usize,
    preview_limit: usize,
) -> (String, usize) {
    cut_preview(&message_preview_text(message), lines, preview_limit)
}

/// [`message_preview`] over a raw message.
pub fn minimap_preview(
    message: Option<&Value>,
    lines: usize,
    preview_limit: usize,
) -> (String, usize) {
    cut_preview(&minimap_preview_text(message), lines, preview_limit)
}

/// The one cut both previews make: the first `max_lines` lines, then at most `preview_limit`
/// UTF-16 code units, with an ellipsis when anything was left out.
///
/// CDXC:SessionChat 2026-09-26 WHY:
/// The hover card renders this as Markdown and shows `previewLines` rows (the user's decision is on
/// `minimap_preview_card` in apps/desktop/src/app/native_chat/minimap.rs). A run of blank lines
/// folds into one, so a paragraph break costs the card exactly one line, the row its paragraph gap
/// takes.
fn cut_preview(text: &str, max_lines: usize, preview_limit: usize) -> (String, usize) {
    let mut kept: Vec<&str> = Vec::new();
    let mut truncated = false;
    for line in text.lines() {
        let line = trim_end_js(line);
        if line.is_empty() && kept.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        if kept.len() == max_lines {
            truncated = true;
            break;
        }
        kept.push(line);
    }
    while kept.last().is_some_and(|last| last.is_empty()) {
        kept.pop();
    }
    let mut preview = kept.join("\n");
    let units: Vec<u16> = preview.encode_utf16().collect();
    if units.len() > preview_limit {
        preview = trim_end_js(&String::from_utf16_lossy(&units[..preview_limit])).to_string();
        truncated = true;
    }
    let count = preview.lines().count();
    if truncated && !preview.is_empty() {
        preview.push('\u{2026}');
    }
    (preview, count)
}

/// `sessionChatMinimapVisible`: a single prompt has nothing to navigate between, so the rail stays
/// away until there are two.
pub fn minimap_visible(turn_count: usize, minimum_turns: usize) -> bool {
    turn_count >= minimum_turns
}

/// `sessionChatMinimapDashWidth`: dashes grow towards the one the pointer is on.
pub fn minimap_dash_width(distance: i64, widths: &[i64]) -> i64 {
    if widths.is_empty() {
        return 0;
    }
    let index = distance.clamp(0, widths.len() as i64 - 1) as usize;
    widths[index]
}

/// `sessionChatMinimapIndexAt`: which dash a pointer at `progress` means.
pub fn minimap_index_at(progress: f64, turn_count: usize) -> i64 {
    let clamped = progress.clamp(0.0, 1.0);
    crate::extras::activity::js_round(clamped * (turn_count as f64 - 1.0)) as i64
}

/// `sessionChatMinimapRailHeight`: one `spacing` step between neighbouring dashes.
pub fn minimap_rail_height(turn_count: usize, spacing: i64) -> i64 {
    (turn_count as i64 - 1).max(0) * spacing
}

/// `String.prototype.trimEnd`.
fn trim_end_js(value: &str) -> &str {
    value.trim_end_matches(|character: char| is_js_space(character))
}

/// The `\s` class: Unicode `White_Space` plus the byte order mark.
fn is_js_space(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}
