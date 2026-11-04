//! Color legends: a wrapping row above the plot, or a column beside a pie.

use crate::scene::{Anchor, Block};
use crate::text::{text_width, truncate};
use crate::theme::{Color, Theme};

/// Swatch, row heights and the text are in proportion to the body-text size `theme.text_size`.
const SWATCH_EMS: f32 = 0.9;
const SWATCH_GAP: f32 = 6.0;
const ENTRY_GAP: f32 = 14.0;
const ROW_HEIGHT_EMS: f32 = 1.45;
const COLUMN_ROW_HEIGHT_EMS: f32 = 1.65;
const MAX_ENTRIES: usize = 24;
const MAX_LINES: usize = 3;

pub(crate) struct Entry {
    pub label: String,
    pub color: Color,
}

fn more_text(hidden: usize) -> String {
    format!("+{hidden} more")
}

fn swatch(block: &mut Block, x: f32, center_y: f32, size: f32, color: Color) {
    let side = size * SWATCH_EMS;
    block.fill_rect(x, center_y - side / 2.0, side, side, 2.0, color);
}

/// A wrapping row of swatches and labels at (`x`, `y`) within `width`; returns its height.
pub(crate) fn row(
    block: &mut Block,
    entries: &[Entry],
    x: f32,
    y: f32,
    width: f32,
    theme: &Theme,
) -> f32 {
    if entries.is_empty() {
        return 0.0;
    }
    let size = theme.text_size;
    let swatch_size = size * SWATCH_EMS;
    let row_height = size * ROW_HEIGHT_EMS;
    let label_limit = (width - swatch_size - SWATCH_GAP).clamp(20.0, 220.0);
    // (line, x offset, label) for each entry that fits.
    let mut placed: Vec<(usize, f32, String, Color)> = Vec::new();
    let mut line = 0;
    let mut cx = 0.0;
    for entry in entries.iter().take(MAX_ENTRIES) {
        let label = truncate(&entry.label, label_limit, size, 400);
        let w = swatch_size + SWATCH_GAP + text_width(&label, size, 400);
        if cx > 0.0 && cx + w > width {
            line += 1;
            cx = 0.0;
        }
        if line >= MAX_LINES {
            break;
        }
        placed.push((line, cx, label, entry.color));
        cx += w + ENTRY_GAP;
    }
    let mut hidden = entries.len() - placed.len();
    let mut more_at: Option<(usize, f32)> = None;
    if hidden > 0 {
        loop {
            let last_line = placed.last().map(|p| p.0).unwrap_or(0);
            let end = placed
                .last()
                .map(|(_, px, label, _)| {
                    px + swatch_size + SWATCH_GAP + text_width(label, size, 400) + ENTRY_GAP
                })
                .unwrap_or(0.0);
            let need = text_width(&more_text(hidden), size, 400);
            if end + need <= width || placed.len() <= 1 {
                more_at = Some((last_line, end));
                break;
            }
            placed.pop();
            hidden += 1;
        }
    }
    let lines = placed.iter().map(|p| p.0 + 1).max().unwrap_or(1);
    for (line, px, label, color) in placed {
        let center = y + line as f32 * row_height + row_height / 2.0;
        swatch(block, x + px, center, size, color);
        block.text_centered(
            x + px + swatch_size + SWATCH_GAP,
            center,
            label,
            size,
            400,
            theme.muted,
            Anchor::Start,
        );
    }
    if let Some((line, px)) = more_at {
        let center = y + line as f32 * row_height + row_height / 2.0;
        block.text_centered(
            x + px,
            center,
            more_text(hidden),
            size,
            400,
            theme.muted,
            Anchor::Start,
        );
    }
    lines as f32 * row_height
}

/// The width a legend column needs for `entries`, at most `max_width`.
pub(crate) fn column_width(entries: &[Entry], max_width: f32, theme: &Theme) -> f32 {
    let size = theme.text_size;
    let widest = entries
        .iter()
        .take(MAX_ENTRIES)
        .map(|e| text_width(&e.label, size, 400))
        .fold(0.0, f32::max);
    (size * SWATCH_EMS + SWATCH_GAP + widest).min(max_width)
}

/// A column of swatches and labels, vertically centered on `center_y` within `max_height`.
pub(crate) fn column(
    block: &mut Block,
    entries: &[Entry],
    x: f32,
    center_y: f32,
    width: f32,
    max_height: f32,
    theme: &Theme,
) {
    let size = theme.text_size;
    let swatch_size = size * SWATCH_EMS;
    let row_height = size * COLUMN_ROW_HEIGHT_EMS;
    let fit = ((max_height / row_height).floor() as usize).clamp(1, MAX_ENTRIES);
    let (shown, hidden) = if entries.len() > fit {
        (fit.saturating_sub(1), entries.len() - fit.saturating_sub(1))
    } else {
        (entries.len(), 0)
    };
    let rows = shown + usize::from(hidden > 0);
    let mut row_center = center_y - (rows as f32 * row_height) / 2.0 + row_height / 2.0;
    let label_limit = (width - swatch_size - SWATCH_GAP).max(0.0);
    for entry in entries.iter().take(shown) {
        swatch(block, x, row_center, size, entry.color);
        block.text_centered(
            x + swatch_size + SWATCH_GAP,
            row_center,
            truncate(&entry.label, label_limit, size, 400),
            size,
            400,
            theme.muted,
            Anchor::Start,
        );
        row_center += row_height;
    }
    if hidden > 0 {
        block.text_centered(
            x,
            row_center,
            more_text(hidden),
            size,
            400,
            theme.muted,
            Anchor::Start,
        );
    }
}
