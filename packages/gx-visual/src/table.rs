//! Tables: a header row, body rows with horizontal rules, numeric columns right-aligned.

use serde_json::Value;

use crate::scene::{crisp, Anchor, Block};
use crate::text::{format_number, format_plain, line_height, text_width, truncate};
use crate::theme::Theme;

const MAX_ROWS_DRAWN: usize = 60;
/// A row is its text's line plus this much padding, so rows grow with the text.
const ROW_PADDING: f32 = 14.0;
const CELL_PAD: f32 = 10.0;

pub(crate) struct TableSpec {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

pub(crate) fn parse(value: &Value) -> Result<TableSpec, String> {
    let Value::Object(table) = value else {
        return Err("\"table\" must be an object with \"columns\" and \"rows\".".to_string());
    };
    let Some(Value::Array(rows)) = table.get("rows") else {
        return Err("A table needs \"rows\": an array of arrays or of objects.".to_string());
    };
    let mut columns: Vec<String> = match table.get("columns") {
        Some(Value::Array(cols)) => cols.iter().map(header_text).collect(),
        Some(_) => return Err("A table's \"columns\" must be an array of names.".to_string()),
        None => Vec::new(),
    };
    let object_rows = rows.first().is_some_and(Value::is_object);
    if columns.is_empty() {
        match rows.first() {
            Some(Value::Object(first)) => columns = first.keys().cloned().collect(),
            Some(_) => {
                return Err(
                    "A table with array rows needs \"columns\" naming each column.".to_string(),
                )
            }
            None => return Err("This table has no rows.".to_string()),
        }
    }
    if columns.is_empty() {
        return Err("This table has no columns.".to_string());
    }
    let rows = rows
        .iter()
        .map(|row| match row {
            Value::Array(cells) => Ok((0..columns.len())
                .map(|i| cells.get(i).cloned().unwrap_or(Value::Null))
                .collect()),
            Value::Object(cells) if object_rows => Ok(columns
                .iter()
                .map(|c| cells.get(c).cloned().unwrap_or(Value::Null))
                .collect()),
            _ => Err(
                "Every table row must have the same shape (all arrays or all objects).".to_string(),
            ),
        })
        .collect::<Result<Vec<Vec<Value>>, String>>()?;
    Ok(TableSpec { columns, rows })
}

fn header_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn cell_text(value: &Value, year_column: bool) -> String {
    match value {
        Value::String(s) => s.replace(['\n', '\r', '\t'], " "),
        Value::Number(n) => match n.as_f64() {
            Some(v) if year_column => format_plain(v),
            Some(v) => format_number(v),
            None => n.to_string(),
        },
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Column widths that fill `width`: narrow columns keep their natural width and the widest ones
/// share what is left.
fn fit_widths(natural: &[f32], width: f32) -> Vec<f32> {
    let total: f32 = natural.iter().sum();
    if total <= 0.0 {
        let n = natural.len().max(1) as f32;
        return natural.iter().map(|_| width / n).collect();
    }
    if total <= width {
        let scale = width / total;
        return natural.iter().map(|w| w * scale).collect();
    }
    let (mut lo, mut hi) = (0.0f32, natural.iter().cloned().fold(0.0, f32::max));
    for _ in 0..40 {
        let cap = (lo + hi) / 2.0;
        let used: f32 = natural.iter().map(|w| w.min(cap)).sum();
        if used > width {
            hi = cap;
        } else {
            lo = cap;
        }
    }
    natural.iter().map(|w| w.min(lo)).collect()
}

pub(crate) fn layout(table: &TableSpec, width: f32, theme: &Theme) -> Block {
    let mut block = Block::new();
    let size = theme.text_size;
    let row_height = line_height(size) + ROW_PADDING;
    let cols = table.columns.len();
    let drawn = &table.rows[..table.rows.len().min(MAX_ROWS_DRAWN)];
    let numeric: Vec<bool> = (0..cols)
        .map(|c| {
            let mut any = false;
            let all = drawn.iter().all(|row| match row.get(c) {
                Some(Value::Number(_)) => {
                    any = true;
                    true
                }
                Some(Value::Null) | None => true,
                Some(_) => false,
            });
            all && any
        })
        .collect();
    let year: Vec<bool> = table
        .columns
        .iter()
        .map(|c| {
            let c = c.trim().to_ascii_lowercase();
            c == "year" || c.ends_with(" year") || c == "yr"
        })
        .collect();
    let cells: Vec<Vec<String>> = drawn
        .iter()
        .map(|row| {
            (0..cols)
                .map(|c| {
                    row.get(c)
                        .map(|v| cell_text(v, year.get(c).copied().unwrap_or(false)))
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    let natural: Vec<f32> = (0..cols)
        .map(|c| {
            let header = text_width(&table.columns[c], size, 600);
            let body = cells
                .iter()
                .map(|row| text_width(row.get(c).map(String::as_str).unwrap_or(""), size, 400))
                .fold(0.0, f32::max);
            header.max(body).min(480.0) + CELL_PAD * 2.0
        })
        .collect();
    let widths = fit_widths(&natural, width);
    let mut lefts = Vec::with_capacity(cols);
    let mut x = 0.0;
    for w in &widths {
        lefts.push(x);
        x += w;
    }

    block.fill_rect(0.0, 0.0, width, row_height, 6.0, theme.surface);
    for c in 0..cols {
        draw_cell(
            &mut block,
            &table.columns[c],
            lefts[c],
            widths[c],
            row_height / 2.0,
            numeric[c],
            600,
            size,
            theme,
        );
    }
    block.line(
        [0.0, crisp(row_height)],
        [width, crisp(row_height)],
        theme.border,
        1.0,
    );
    let mut top = row_height;
    for (r, row) in cells.iter().enumerate() {
        for c in 0..cols {
            let text = row.get(c).map(String::as_str).unwrap_or("");
            draw_cell(
                &mut block,
                text,
                lefts[c],
                widths[c],
                top + row_height / 2.0,
                numeric[c],
                400,
                size,
                theme,
            );
        }
        top += row_height;
        let color = if r + 1 == cells.len() {
            theme.border
        } else {
            theme.grid
        };
        block.line([0.0, crisp(top)], [width, crisp(top)], color, 1.0);
    }
    let hidden = table.rows.len().saturating_sub(drawn.len());
    if hidden > 0 {
        top += 6.0;
        block.text_line(
            CELL_PAD,
            top,
            format!(
                "+{} more {}",
                format_number(hidden as f64),
                if hidden == 1 { "row" } else { "rows" }
            ),
            size,
            400,
            theme.muted,
            Anchor::Start,
        );
        top += line_height(size);
    }
    block.height = top + 1.0;
    block
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    block: &mut Block,
    text: &str,
    left: f32,
    width: f32,
    center_y: f32,
    right_align: bool,
    weight: u16,
    size: f32,
    theme: &Theme,
) {
    let inner = (width - CELL_PAD * 2.0).max(0.0);
    let text = truncate(text, inner, size, weight);
    if text.is_empty() {
        return;
    }
    let (x, anchor) = if right_align {
        (left + width - CELL_PAD, Anchor::End)
    } else {
        (left + CELL_PAD, Anchor::Start)
    };
    let color = theme.foreground;
    block.text_centered(x, center_y, text, size, weight, color, anchor);
}
