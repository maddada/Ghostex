//! Stat tiles: a label, a big value and an optional note, laid out in wrapping rows.

use serde_json::Value;

use crate::scene::{Anchor, Block};
use crate::text::{format_number, line_height, truncate};
use crate::theme::Theme;

/// A tile is at least this many body-text sizes wide, so a label of that size still fits.
const MIN_TILE_EMS: f32 = 11.5;
const GAP: f32 = 12.0;
const PAD: f32 = 12.0;
/// The stat value is this many times the body text: big enough that the tile reads as a stat.
const VALUE_RATIO: f32 = 26.0 / 14.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Good,
    Bad,
    Neutral,
}

pub(crate) struct Tile {
    label: String,
    value: String,
    note: Option<String>,
    tone: Tone,
}

pub(crate) fn parse(value: &Value) -> Result<Vec<Tile>, String> {
    let Value::Array(items) = value else {
        return Err(
            "\"stats\" must be an array of tiles like {\"label\": …, \"value\": …}.".to_string(),
        );
    };
    if items.is_empty() {
        return Err("\"stats\" is empty; add at least one tile.".to_string());
    }
    items
        .iter()
        .map(|item| {
            let Value::Object(tile) = item else {
                return Err(
                    "Each stats tile must be an object with \"label\" and \"value\".".to_string(),
                );
            };
            let tone = match tile.get("tone").and_then(Value::as_str) {
                Some("good") | Some("positive") | Some("up") => Tone::Good,
                Some("bad") | Some("negative") | Some("down") => Tone::Bad,
                _ => Tone::Neutral,
            };
            Ok(Tile {
                label: tile.get("label").map(display).unwrap_or_default(),
                value: tile
                    .get("value")
                    .map(display)
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "–".to_string()),
                note: tile.get("note").map(display).filter(|n| !n.is_empty()),
                tone,
            })
        })
        .collect()
}

fn display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map(format_number).unwrap_or_default(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn layout(tiles: &[Tile], width: f32, theme: &Theme) -> Block {
    let mut block = Block::new();
    let label_size = theme.text_size;
    let note_size = theme.text_size;
    let value_size = theme.text_size * VALUE_RATIO;
    let count = tiles.len().max(1);
    let fit = ((width + GAP) / (MIN_TILE_EMS * theme.text_size + GAP))
        .floor()
        .max(1.0) as usize;
    let per_row = fit.min(count);
    let tile_width = ((width - GAP * (per_row as f32 - 1.0)) / per_row as f32).max(1.0);
    let inner = (tile_width - PAD * 2.0).max(0.0);
    let mut top = 0.0;
    for row in tiles.chunks(per_row) {
        let has_note = row.iter().any(|t| t.note.is_some());
        let height = PAD * 2.0
            + line_height(label_size)
            + 4.0
            + line_height(value_size)
            + if has_note {
                2.0 + line_height(note_size)
            } else {
                0.0
            };
        for (i, tile) in row.iter().enumerate() {
            let x = i as f32 * (tile_width + GAP);
            block.push(crate::scene::Item::Rect {
                x,
                y: top,
                width: tile_width,
                height,
                radius: 8.0,
                fill: Some(theme.surface),
                stroke: Some(theme.border),
                stroke_width: 1.0,
            });
            let mut y = top + PAD;
            block.text_line(
                x + PAD,
                y,
                truncate(&tile.label, inner, label_size, 400),
                label_size,
                400,
                theme.muted,
                Anchor::Start,
            );
            y += line_height(label_size) + 4.0;
            block.text_line(
                x + PAD,
                y,
                truncate(&tile.value, inner, value_size, 600),
                value_size,
                600,
                theme.foreground,
                Anchor::Start,
            );
            y += line_height(value_size) + 2.0;
            if let Some(note) = &tile.note {
                let color = match tile.tone {
                    Tone::Good => theme.good,
                    Tone::Bad => theme.bad,
                    Tone::Neutral => theme.muted,
                };
                block.text_line(
                    x + PAD,
                    y,
                    truncate(note, inner, note_size, 400),
                    note_size,
                    400,
                    color,
                    Anchor::Start,
                );
            }
        }
        top += height + GAP;
    }
    block.height = (top - GAP).max(0.0);
    block
}
