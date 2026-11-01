//! The Docs gutter: 28px line numbers, a 3px git stripe and a 16px fold lane, aligned to the
//! editor's own row layout. Git state compares the document with its HEAD version, which the file
//! bridge's `gitBaseline` supplies (local or remote project alike).
//!
//! Adapted from the Docs prototype (`docs/2026-09-24/docs-gpui-editor-research/demo/src/gutter.rs`).

use gpui::{
    AnyElement, Entity, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled as _, Transformation, div,
    prelude::FluentBuilder as _, px, radians, rgb, svg,
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
    /// Foldable headings (`(row, folded)`) and the editor whose folds they are, for the fold lane.
    pub(crate) folds: &'a [(usize, bool)],
    pub(crate) live: Option<&'a Entity<zorite_editor::EditorState>>,
}

/// The gutter's width: line numbers (2px when hidden, like the Docs page), the git stripe and the
/// fold lane.
pub(crate) fn gutter_width(numbers: bool) -> f32 {
    numbers_width(numbers) + STRIPE_W + FOLD_W
}

/// The line numbers' column widens with the Files zoom, so a four-digit number still fits.
fn numbers_width(numbers: bool) -> f32 {
    if numbers {
        NUMBERS_W * crate::ui_fonts::files_zoom_scale()
    } else {
        2.
    }
}

/// The Docs page's line-number row: the base text line (1.7 line height), so a tall heading row
/// keeps its number at the top.
fn number_row_height() -> f32 {
    crate::ui_fonts::files_text_size() * 1.7
}

/// CDXC:Docs 2026-09-28 WHY: the former React Docs page's gutter order is line numbers, git
/// stripe, fold lane (47px in all), so the fold chevrons sit right before the text; the native
/// gutter used to put the fold lane first.
pub(crate) fn render(
    model: GutterModel<'_>,
    palette: &DocsPalette,
    body: gpui::Hsla,
) -> AnyElement {
    let width = gutter_width(model.numbers);
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
    let band = model.band;
    let on_screen = move |top: Pixels, height: Pixels| {
        band.is_none_or(|(low, high)| top + height >= low && top <= high)
    };
    let muted = palette.muted.opacity(0.62);
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
                    .left(px(0.))
                    .w(px(numbers_width(true)))
                    .h((*row_height).min(px(number_row_height())))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_size(px(crate::ui_fonts::files_line_number_size()))
                    .text_color(if current { body } else { muted })
                    .child((index + 1).to_string())
            })
    });
    // A changed line's stripe covers its whole height (to the next row's top), like the Docs
    // page's; hovering widens it to 5px.
    let stripe_x = px(numbers_width(model.numbers));
    let rows = model.rows;
    let full_height = move |line: usize| -> Option<(Pixels, Pixels)> {
        let (top, height) = *rows.get(line)?;
        let next = rows
            .get(line + 1)
            .map(|(next, _)| *next)
            .filter(|next| *next > top)
            .unwrap_or(top + height);
        Some((top, next - top))
    };
    let stripes = model.changes.map(|(lines, _)| {
        lines
            .iter()
            .enumerate()
            .filter(|(_, change)| **change != LineChange::Same)
            .filter_map(move |(line, change)| {
                let (top, height) = full_height(line)
                    .filter(|(top, height)| *height > px(0.5) && on_screen(*top, *height))?;
                Some(
                    div()
                        .id(("docs-git-stripe", line))
                        .absolute()
                        .top(top)
                        .left(stripe_x)
                        .w(px(STRIPE_W))
                        .h(height)
                        .bg(if *change == LineChange::Added {
                            rgb(0x4a9e82)
                        } else {
                            rgb(0x317ae7)
                        })
                        .hover(move |style| style.left(stripe_x - px(2.)).w(px(5.)))
                        .into_any_element(),
                )
            })
            .collect::<Vec<_>>()
    });
    // The fold lane: a 14px chevron on every heading whose section has content, turned right while
    // it is folded.
    let fold_x = stripe_x + px(STRIPE_W);
    let folds = model.live.map(|live| {
        model
            .folds
            .iter()
            .filter_map(|&(row, folded)| {
                let (top, height) = *model.rows.get(row)?;
                if height <= px(0.5) || !on_screen(top, height) {
                    return None;
                }
                let live = live.clone();
                let color = if folded { body } else { muted };
                Some(
                    div()
                        .id(("docs-heading-fold", row))
                        .absolute()
                        .top(top)
                        .left(fold_x)
                        .w(px(FOLD_W))
                        .h(height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_color(color)
                        .hover(move |style| style.text_color(body))
                        .child(
                            svg()
                                .path("files-view/l-chevron-down-275.svg")
                                .size(px(14.))
                                .text_color(color)
                                .when(folded, |icon| {
                                    icon.with_transformation(Transformation::rotate(radians(
                                        -std::f32::consts::FRAC_PI_2,
                                    )))
                                }),
                        )
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(if folded {
                                "Expand section"
                            } else {
                                "Collapse section"
                            })
                            .build(window, cx)
                        })
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            live.update(cx, |editor, cx| editor.toggle_heading_fold(row, cx));
                        })
                        .into_any_element(),
                )
            })
            .collect::<Vec<_>>()
    });
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(height)
        .when_some(numbers, |gutter, numbers| gutter.children(numbers))
        .when_some(stripes, |gutter, stripes| gutter.children(stripes))
        .when_some(folds, |gutter, folds| gutter.children(folds))
        .into_any_element()
}

/// The git overview ruler: a 3px strip 10px from the document's right edge with a 50% mark for
/// each run of changed lines, placed by line number over the part of the track the text fills, and
/// a 5px bar where the text ends.
pub(crate) fn render_overview_ruler(
    changes: &(Vec<LineChange>, Vec<usize>),
    content_bottom: Pixels,
    scroll_height: Pixels,
    track_height: Pixels,
    palette: &DocsPalette,
    body: gpui::Hsla,
) -> Option<AnyElement> {
    let (lines, _) = changes;
    let total = lines.len();
    if total == 0 || track_height <= px(0.) || scroll_height <= px(0.) {
        return None;
    }
    let track = f32::from(track_height).floor();
    let end_ratio = (f32::from(content_bottom) / f32::from(scroll_height)).clamp(0., 1.);
    let file_end = (track * end_ratio).round().clamp(0., track);
    let drawable = file_end.max(0.);
    let mut marks = Vec::new();
    let mut line = 0;
    while line < total {
        let change = lines[line];
        if change == LineChange::Same {
            line += 1;
            continue;
        }
        let from = line;
        let mut added = false;
        let mut modified = false;
        while line < total && lines[line] != LineChange::Same {
            added |= lines[line] == LineChange::Added;
            modified |= lines[line] == LineChange::Modified;
            line += 1;
        }
        let mut top = (from as f32 / total as f32 * drawable).floor();
        let bottom = (line as f32 / total as f32 * drawable).ceil();
        let mut height = (bottom - top).max(2.);
        top = top.clamp(0., (drawable - 1.).max(0.));
        if top + height > drawable {
            if height >= drawable {
                top = 0.;
                height = drawable;
            } else {
                top = (drawable - height).max(0.);
            }
        }
        if height <= 0. {
            continue;
        }
        let color: gpui::Hsla = if modified {
            rgb(0x317ae7).into()
        } else {
            rgb(0x4a9e82).into()
        };
        let _ = added;
        marks.push(
            div()
                .absolute()
                .top(px(top))
                .left_0()
                .right_0()
                .h(px(height))
                .rounded(px(1.))
                .bg(color.opacity(0.5 * 0.95)),
        );
    }
    // CDXC:Docs 2026-10-15 DECISION: User: "i dont understand why there's a line and empty area here in the files when looking at an md file pls fix". A file with no changed lines gets no ruler at all (no lane, no border line, no end-of-file bar), so the body runs clear to the scrollbar; the ruler appears only once there is a change to mark.
    if marks.is_empty() {
        return None;
    }
    let end_line = (file_end > 0. && file_end < track).then(|| {
        div()
            .absolute()
            .top(px((file_end - 1.).clamp(0., track - 1.)))
            .left_0()
            .right_0()
            .h(px(5.))
            .bg(body.opacity(0.55 * 0.65))
    });
    Some(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(10.))
            .w(px(3.))
            .border_l_1()
            .border_color(if palette.light {
                rgb(0xd4d4d8)
            } else {
                rgb(0x303030)
            })
            .children(marks)
            .children(end_line)
            .into_any_element(),
    )
}
