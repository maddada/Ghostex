//! The Docs gutter: 16px fold lane, 28px line numbers and a 3px git stripe, aligned to the
//! editor's own row layout. Git state compares the document with its HEAD version, which the file
//! bridge's `gitBaseline` supplies (local or remote project alike).
//!
//! Adapted from the Docs prototype (`docs/2026-09-24/docs-gpui-editor-research/demo/src/gutter.rs`).

use gpui::{
    AnyElement, IntoElement, ParentElement as _, Pixels, Styled as _, div,
    prelude::FluentBuilder as _, px, rgb,
};

use super::palette::DocsPalette;

pub(crate) const FOLD_W: f32 = 16.;
pub(crate) const NUMBERS_W: f32 = 28.;
pub(crate) const STRIPE_W: f32 = 3.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineChange {
    Same,
    Added,
    Modified,
}

/// Per-line change of `text` against `base`, plus the lines that sit right after a deletion.
pub(crate) fn diff(base: &str, text: &str) -> (Vec<LineChange>, Vec<usize>) {
    let line_count = text.split('\n').count();
    let mut lines = vec![LineChange::Same; line_count];
    let mut deleted_before = Vec::new();
    let diff = similar::TextDiff::from_lines(base, text);
    for op in diff.ops() {
        match *op {
            similar::DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for line in new_index..(new_index + new_len).min(line_count) {
                    lines[line] = LineChange::Added;
                }
            }
            similar::DiffOp::Replace {
                new_index, new_len, ..
            } => {
                for line in new_index..(new_index + new_len).min(line_count) {
                    lines[line] = LineChange::Modified;
                }
            }
            similar::DiffOp::Delete { new_index, .. } => deleted_before.push(new_index),
            similar::DiffOp::Equal { .. } => {}
        }
    }
    (lines, deleted_before)
}

pub(crate) struct GutterModel<'a> {
    pub(crate) rows: &'a [(Pixels, Pixels)],
    pub(crate) caret_line: usize,
    pub(crate) numbers: bool,
    pub(crate) changes: Option<&'a (Vec<LineChange>, Vec<usize>)>,
    /// The rows on screen, in the gutter's own y, with a margin; `None` draws every row.
    pub(crate) band: Option<(Pixels, Pixels)>,
}

pub(crate) fn render(
    model: GutterModel<'_>,
    palette: &DocsPalette,
    body: gpui::Hsla,
) -> AnyElement {
    let width = FOLD_W + if model.numbers { NUMBERS_W } else { 0. } + STRIPE_W + 4.;
    let height = model
        .rows
        .last()
        .map_or(px(0.), |(top, height)| *top + *height);
    // Rows the editor collapses (hidden code fences) or covers with a rendered block (a diagram,
    // a table's delimiter row) come back with no height of their own or inside the previous row;
    // they get no number, like a folded line.
    let typical = model
        .rows
        .iter()
        .map(|(_, height)| *height)
        .fold(px(0.), |a, b| {
            if b > px(4.) && (a == px(0.) || b < a) {
                b
            } else {
                a
            }
        });
    let mut visible = Vec::with_capacity(model.rows.len());
    let mut last_bottom = px(-1.);
    for (index, (top, height)) in model.rows.iter().enumerate() {
        let shares_next_top = model
            .rows
            .get(index + 1)
            .is_some_and(|(next_top, _)| *next_top <= *top + px(1.));
        let shows = *height >= typical * 0.6 && *top >= last_bottom - px(1.) && !shares_next_top;
        if shows {
            last_bottom = *top + *height;
        }
        visible.push(shows);
    }
    // CDXC:Docs 2026-09-25 WHY:
    // A scroll redraws the whole view every frame, and one element per line of a long document
    // (numbers and stripes alike) made the gutter the costliest part of each frame. Only the rows
    // near the viewport get elements; the gutter keeps its full height, so layout is unchanged.
    let on_screen = move |top: Pixels, height: Pixels| {
        model
            .band
            .is_none_or(|(low, high)| top + height >= low && top <= high)
    };
    let numbers = model.numbers.then(|| {
        model
            .rows
            .iter()
            .enumerate()
            .filter(|(index, (top, height))| visible[*index] && on_screen(*top, *height))
            .map(|(index, (top, row_height))| {
                let current = index == model.caret_line;
                div()
                    .absolute()
                    .top(*top)
                    .left(px(FOLD_W))
                    .w(px(NUMBERS_W))
                    .h(*row_height)
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_size(px(12.))
                    .text_color(if current {
                        body
                    } else {
                        palette.muted.opacity(0.62)
                    })
                    .child((index + 1).to_string())
            })
    });
    let stripe_x = px(FOLD_W + if model.numbers { NUMBERS_W } else { 0. } + 2.);
    let stripes = model.changes.map(|(lines, deleted)| {
        let rows = model.rows;
        let changed = lines
            .iter()
            .enumerate()
            .filter(|(_, change)| **change != LineChange::Same)
            .filter_map(move |(line, change)| {
                let (top, height) = rows
                    .get(line)
                    .filter(|(top, height)| on_screen(*top, *height))?;
                Some(
                    div()
                        .absolute()
                        .top(*top + px(2.))
                        .left(stripe_x)
                        .w(px(STRIPE_W))
                        .h(*height - px(4.))
                        .rounded(px(1.))
                        .bg(if *change == LineChange::Added {
                            rgb(0x4a9e82)
                        } else {
                            rgb(0x317ae7)
                        }),
                )
            });
        let removed = deleted.iter().filter_map(move |line| {
            let top = rows
                .get(*line)
                .map(|(top, _)| *top)
                .or_else(|| rows.last().map(|(top, height)| *top + *height))
                .filter(|top| on_screen(*top, px(0.)))?;
            Some(
                div()
                    .absolute()
                    .top(top - px(3.))
                    .left(stripe_x - px(1.))
                    .w(px(5.))
                    .h(px(5.))
                    .rounded_full()
                    .bg(rgb(0xe06c75)),
            )
        });
        changed
            .map(IntoElement::into_any_element)
            .chain(removed.map(IntoElement::into_any_element))
            .collect::<Vec<_>>()
    });
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(height)
        .when_some(numbers, |gutter, numbers| gutter.children(numbers))
        .when_some(stripes, |gutter, stripes| gutter.children(stripes))
        .into_any_element()
}
