//! Composition: the block title, leaves stacked in rows, side-by-side columns, paragraphs.

use crate::scene::{Anchor, Block, Scene};
use crate::spec::{Body, Leaf};
use crate::text::{line_height, truncate, wrap};
use crate::theme::Theme;
use crate::{stats, table, vega};

const GAP: f32 = 16.0;
/// Side-by-side columns are at least this many body-text sizes wide.
const MIN_COLUMN_EMS: f32 = 18.5;
/// A text part's leading, as the transcript's prose has it (22.75 at 14).
const PROSE_LEADING: f32 = 1.625;

pub(crate) fn layout(
    title: Option<String>,
    rows: &[Leaf],
    width: f32,
    theme: &Theme,
) -> Result<Scene, String> {
    // The block's own title is the card header's (desktop, web and phone draw it there), so the
    // drawing starts with its first part instead of repeating it.
    let mut block = Block::new();
    let top = 0.0;
    let body = stack(rows, width, theme)?;
    let height = top + body.height;
    block.append(body, 0.0, top);
    Ok(Scene {
        width,
        height: height.max(1.0).ceil(),
        title,
        items: block.items,
        regions: block.regions,
        motions: block.motions,
    })
}

fn stack(leaves: &[Leaf], width: f32, theme: &Theme) -> Result<Block, String> {
    let mut block = Block::new();
    let mut top = 0.0;
    for (i, leaf) in leaves.iter().enumerate() {
        if i > 0 {
            top += GAP;
        }
        let part = leaf_block(leaf, width, theme)?;
        let height = part.height;
        block.append(part, 0.0, top);
        top += height;
    }
    block.height = top;
    Ok(block)
}

fn columns(leaves: &[Leaf], width: f32, theme: &Theme) -> Result<Block, String> {
    let n = leaves.len().max(1) as f32;
    let column_width = (width - GAP * (n - 1.0)) / n;
    if column_width < MIN_COLUMN_EMS * theme.text_size {
        return stack(leaves, width, theme);
    }
    let mut block = Block::new();
    let mut height: f32 = 0.0;
    for (i, leaf) in leaves.iter().enumerate() {
        let part = leaf_block(leaf, column_width, theme)?;
        height = height.max(part.height);
        block.append(part, i as f32 * (column_width + GAP), 0.0);
    }
    block.height = height;
    Ok(block)
}

fn leaf_block(leaf: &Leaf, width: f32, theme: &Theme) -> Result<Block, String> {
    let body = match &leaf.body {
        Body::Chart(spec) => vega::render_chart(spec, width, theme)?,
        Body::Stats(tiles) => stats::layout(tiles, width, theme),
        Body::Table(spec) => table::layout(spec, width, theme),
        Body::Text(text) => paragraph(text, width, theme),
        Body::Columns(leaves) => columns(leaves, width, theme)?,
        Body::Rows(leaves) => stack(leaves, width, theme)?,
    };
    let Some(title) = &leaf.title else {
        return Ok(body);
    };
    let title_size = theme.text_size;
    let mut block = Block::new();
    block.text_line(
        0.0,
        0.0,
        truncate(title, width, title_size, 600),
        title_size,
        600,
        theme.muted,
        Anchor::Start,
    );
    let top = line_height(title_size) + 6.0;
    let height = body.height;
    block.append(body, 0.0, top);
    block.height = top + height;
    Ok(block)
}

fn paragraph(text: &str, width: f32, theme: &Theme) -> Block {
    let size = theme.text_size;
    let leading = size * PROSE_LEADING;
    let mut block = Block::new();
    let mut top = 0.0;
    for line in wrap(text, width, size, 400) {
        if !line.is_empty() {
            block.text_line(
                0.0,
                top + (leading - line_height(size)) / 2.0,
                line,
                size,
                400,
                theme.foreground,
                Anchor::Start,
            );
        }
        top += leading;
    }
    block.height = top;
    block
}
