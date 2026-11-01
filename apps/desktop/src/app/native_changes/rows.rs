//! One row of the Changes stream: a file header, a hunk header, a unified or split line, a
//! comment marker and the comment editor.

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledText, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

use super::comments::{CommentState, ago};
use super::model::{FileStatus, HunkLine, LineKind, Row};
use super::style::*;
use super::tokens::{DiffTokenKind, expand_tabs, tokenize_diff_line};
use super::view::{ChangesView, Confirm};
use crate::app::window::native_modal_kit::{
    MODAL_MONO_FONT, hsla, modal_icon, modal_text_area, rgba_of,
};

pub(crate) const ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";
pub(crate) const ICON_CHEVRON_RIGHT: &str = "titlebar/chevron-right.svg";
pub(crate) const ICON_COPY: &str = "titlebar/copy.svg";
pub(crate) const ICON_OPEN: &str = "titlebar/external-link.svg";
pub(crate) const ICON_MESSAGE: &str = "titlebar/message.svg";
pub(crate) const ICON_CHECK: &str = "titlebar/check.svg";
pub(crate) const ICON_PLUS: &str = "titlebar/plus.svg";
pub(crate) const ICON_CLOSE: &str = "modals/kit/x.svg";
pub(crate) const ICON_TICK: &str = "modals/git-commit/checkbox-tick.svg";

/// A file's A/M/D badge.
pub(crate) fn status_badge(s: &ChangesStyle, status: FileStatus) -> AnyElement {
    let color = match status {
        FileStatus::Added => s.badge_added,
        FileStatus::Modified => s.badge_modified,
        FileStatus::Deleted => s.badge_deleted,
    };
    div()
        .flex_none()
        .size(px(CODE_SIZE + 4.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(hsla(rgba_of(color, 0.14)))
        .text_color(hsla(color))
        .text_size(px(SMALL_SIZE - 1.0))
        .font_weight(FontWeight::BOLD)
        .child(status.letter())
        .into_any_element()
}

pub(crate) fn stat(s: &ChangesStyle, additions: usize, deletions: usize) -> AnyElement {
    h_flex()
        .flex_none()
        .gap(px(5.0))
        .font_family(MODAL_MONO_FONT)
        .text_size(px(CODE_SIZE))
        .child(
            div()
                .text_color(hsla(s.plus))
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .text_color(hsla(s.minus))
                .child(format!("\u{2212}{deletions}")),
        )
        .into_any_element()
}

pub(crate) fn checkbox(s: &ChangesStyle, checked: bool) -> AnyElement {
    div()
        .flex_none()
        .size(px(CODE_SIZE + 2.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(hsla(if checked {
            s.accent
        } else {
            rgba_of(s.muted, 0.7)
        }))
        .when(checked, |this| this.bg(hsla(s.accent)))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |this| {
            this.child(modal_icon(
                ICON_TICK,
                CODE_SIZE - 3.0,
                if s.p.light {
                    gpui::rgb(0xffffff)
                } else {
                    gpui::rgb(0x111111)
                },
            ))
        })
        .into_any_element()
}

/// Token colours plus the word-level change background, as non-overlapping highlights over the
/// tab-expanded text.
fn line_highlights(
    s: &ChangesStyle,
    text: &str,
    changed: &[Range<usize>],
    kind: LineKind,
    plain: bool,
) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
    let (display, offsets) = expand_tabs(text);
    let map = |byte: usize| {
        if offsets.is_empty() {
            byte
        } else {
            offsets.get(byte).copied().unwrap_or(display.len())
        }
    };
    let word_bg = match kind {
        LineKind::Added => Some(s.added_word),
        LineKind::Removed => Some(s.removed_word),
        LineKind::Context => None,
    };
    // Boundaries where either a token or a changed span starts or ends.
    let tokens = if plain {
        Vec::new()
    } else {
        tokenize_diff_line(text)
    };
    let mut cuts: Vec<usize> = vec![0, text.len()];
    for (range, _) in &tokens {
        cuts.push(range.start);
        cuts.push(range.end);
    }
    for range in changed {
        cuts.push(range.start.min(text.len()));
        cuts.push(range.end.min(text.len()));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut highlights = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        let token = tokens
            .iter()
            .find(|(range, _)| range.start <= start && end <= range.end)
            .map(|(_, kind)| *kind);
        let color = match token {
            Some(DiffTokenKind::Keyword) => Some(s.keyword),
            Some(DiffTokenKind::Type) | Some(DiffTokenKind::Literal) => Some(s.type_name),
            Some(DiffTokenKind::Function) => Some(s.function),
            Some(DiffTokenKind::String) => Some(s.string),
            Some(DiffTokenKind::Number) => Some(s.number),
            Some(DiffTokenKind::Comment) => Some(s.comment),
            Some(DiffTokenKind::Attribute) => Some(s.function),
            _ => None,
        };
        let in_change = changed
            .iter()
            .any(|range| range.start <= start && end <= range.end);
        let background = word_bg.filter(|_| in_change);
        if color.is_none() && background.is_none() {
            continue;
        }
        highlights.push((
            map(start)..map(end),
            HighlightStyle {
                color: color.map(hsla),
                background_color: background.map(hsla),
                font_style: (token == Some(DiffTokenKind::Comment))
                    .then_some(gpui::FontStyle::Italic),
                ..HighlightStyle::default()
            },
        ));
    }
    (display.into(), highlights)
}

pub(crate) fn code_cell(s: &ChangesStyle, line: &HunkLine, wrap: bool, plain: bool) -> AnyElement {
    let (display, highlights) = line_highlights(s, &line.text, &line.changed, line.kind, plain);
    div()
        .flex_1()
        .min_w_0()
        .pr(px(8.0))
        .text_color(hsla(s.text))
        .when(!wrap, |this| this.whitespace_nowrap().overflow_hidden())
        .child(StyledText::new(display).with_highlights(highlights))
        .into_any_element()
}

pub(crate) fn number_cell(
    _s: &ChangesStyle,
    number: Option<u32>,
    background: Option<Rgba>,
    color: Rgba,
) -> AnyElement {
    div()
        .flex_none()
        .w(px(diff_number_width()))
        .h_full()
        .pr(px(6.0))
        .flex()
        .justify_end()
        .text_size(px(diff_number_size()))
        .text_color(hsla(color))
        .when_some(background, |this, bg| this.bg(hsla(bg)))
        .child(number.map(|n| n.to_string()).unwrap_or_default())
        .into_any_element()
}

pub(crate) fn marker_cell(s: &ChangesStyle, kind: LineKind) -> AnyElement {
    let (text, color) = match kind {
        LineKind::Added => ("+", s.plus),
        LineKind::Removed => ("\u{2212}", s.minus),
        LineKind::Context => ("", s.faint),
    };
    div()
        .flex_none()
        .w(px(diff_marker_width()))
        .flex()
        .justify_center()
        .text_color(hsla(color))
        .child(text)
        .into_any_element()
}

pub(crate) fn kind_colors(s: &ChangesStyle, kind: LineKind) -> (Option<Rgba>, Option<Rgba>, Rgba) {
    match kind {
        LineKind::Added => (
            Some(s.added_row),
            Some(s.added_gutter),
            rgba_of(s.plus, 0.85),
        ),
        LineKind::Removed => (
            Some(s.removed_row),
            Some(s.removed_gutter),
            rgba_of(s.minus, 0.85),
        ),
        LineKind::Context => (None, None, s.faint),
    }
}

impl ChangesView {
    pub(crate) fn render_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self.rows.get(index).copied() else {
            return div().into_any_element();
        };
        let Some(s) = self.style else {
            return div().into_any_element();
        };
        match row {
            Row::File(file) => self.render_file_header(&s, file, false, cx),
            Row::Hunk(file, hunk) => self.render_hunk_header(&s, index, file, hunk, cx),
            Row::Line(..) | Row::Split(..) => self.render_line_row(&s, index, row, cx),
            Row::Binary(file) => {
                let label = match self.data.as_ref().and_then(|data| data.files.get(file)) {
                    Some(file) if file.hunks.is_empty() && !file.binary => {
                        "No changes to show (mode or permission change)."
                    }
                    Some(file) if file.hunks.is_empty() && file.status == FileStatus::Added => {
                        "Binary or large file, not shown."
                    }
                    _ => "Binary file, not shown.",
                };
                div()
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_size(px(CODE_SIZE))
                    .text_color(hsla(s.muted))
                    .child(label)
                    .into_any_element()
            }
            Row::Comment(id) => self.render_comment(&s, id, cx),
            Row::Editor => self.render_editor(&s, window, cx),
        }
    }

    /// A file's header, in the stream or pinned over it (`sticky`).
    pub(crate) fn render_file_header(
        &mut self,
        s: &ChangesStyle,
        file_index: usize,
        sticky: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(file) = self
            .data
            .as_ref()
            .and_then(|data| data.files.get(file_index))
            .cloned()
        else {
            return div().into_any_element();
        };
        let path = file.path.clone();
        let collapsed = self.collapsed.contains(&path) || self.viewed.contains_key(&path);
        let viewed = self.viewed.contains_key(&path);
        let included = file.uncommitted && !self.excluded.contains(&path);
        let confirming = self.confirm == Some(Confirm::RevertFile(path.clone()));
        let id: SharedString = format!(
            "changes-file-{}-{}",
            if sticky { "sticky" } else { "row" },
            path
        )
        .into();
        let toggle_path = path.clone();
        let menu_path = path.clone();
        let check_path = path.clone();
        let copy_path = path.clone();
        let open_path = path.clone();
        let viewed_path = path.clone();
        let revert_path = path.clone();
        // CDXC:Changes 2026-10-25 DECISION: User: "please hide these buttons until i hover on a file", then "please make the text shift like that only when i hover on each header not always". Copy path, Open in Files and Viewed are left out of the row (so the path, name and counts take its whole width) unless the pointer is over that header or the user moved into that file with the keys (never by default: a stale cursor left the last file's buttons up); Viewed stays on a file already marked viewed so its state still reads.
        let hover_path = path.clone();
        let show_actions = self.key_file.as_deref() == Some(path.as_str())
            || self
                .hovered_header
                .as_ref()
                .is_some_and(|(hovered, pinned)| hovered == &path && *pinned == sticky);
        h_flex()
            .id(id)
            .w_full()
            .h(px(FILE_HEADER_HEIGHT))
            .px(px(8.0))
            .gap(px(6.0))
            .bg(hsla(if sticky { s.sticky_bg } else { s.header_bg }))
            .border_b_1()
            .border_color(hsla(s.hairline))
            .when(!sticky, |this| this.border_t_1())
            .text_size(px(UI_SIZE))
            .cursor_pointer()
            .hover(|this| this.bg(hsla(if sticky { s.sticky_hover } else { s.hover })))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_collapsed(&toggle_path, cx)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let key = (hover_path.clone(), sticky);
                if *hovered {
                    this.hovered_header = Some(key);
                } else if this.hovered_header.as_ref() == Some(&key) {
                    this.hovered_header = None;
                } else {
                    return;
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.file_menu = Some((menu_path.clone(), event.position - this.view_origin));
                    cx.notify();
                }),
            )
            // The row's leading marks keep their size when a long path fills the row.
            .child(div().flex_none().child(modal_icon(
                if collapsed {
                    ICON_CHEVRON_RIGHT
                } else {
                    ICON_CHEVRON_DOWN
                },
                ICON_SIZE - 1.0,
                s.muted,
            )))
            .when(file.uncommitted, |this| {
                this.child(
                    div()
                        .flex_none()
                        .id(SharedString::from(format!(
                            "changes-include-{}-{path}",
                            sticky
                        )))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_included(&check_path, cx);
                        }))
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(if included {
                                "In the next commit"
                            } else {
                                "Left out of the next commit"
                            })
                            .build(window, cx)
                        })
                        .child(checkbox(s, included)),
                )
            })
            .child(status_badge(s, file.status))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_shrink(1.0)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_shrink(1.0)
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(hsla(s.muted))
                            .child(file.folder().to_string()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(hsla(s.text))
                            .font_weight(FontWeight::MEDIUM)
                            .child(file.file_name().to_string()),
                    ),
            )
            .child(stat(s, file.additions, file.deletions))
            .child(div().flex_1())
            .when(confirming, |this| {
                this.child(
                    div()
                        .text_color(hsla(s.danger))
                        .child("Revert every change in this file?"),
                )
                .child(small_button(
                    s,
                    "changes-file-revert-cancel",
                    "Cancel",
                    false,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.confirm = None;
                        cx.notify();
                    }),
                ))
                .child(small_button(
                    s,
                    "changes-file-revert-yes",
                    "Revert",
                    true,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.revert(Confirm::RevertFile(revert_path.clone()), window, cx);
                    }),
                ))
            })
            .when(!confirming && show_actions, |this| {
                this.child(
                    h_flex()
                        .id(SharedString::from(format!(
                            "changes-file-actions-{sticky}-{path}"
                        )))
                        .flex_none()
                        .gap(px(2.0))
                        .child(icon_button(
                            s,
                            format!("changes-copy-{sticky}-{path}"),
                            ICON_COPY,
                            "Copy path",
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    copy_path.clone(),
                                ));
                                (this.host)(super::view::ChangesRequest::Copied, window, cx);
                            }),
                        ))
                        .child(icon_button(
                            s,
                            format!("changes-open-{sticky}-{path}"),
                            ICON_OPEN,
                            "Open in Files",
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_in_files(&open_path, None, window, cx);
                            }),
                        )),
                )
            })
            .when(!confirming && (show_actions || viewed), |this| {
                this.child(
                    h_flex()
                        .id(SharedString::from(format!(
                            "changes-viewed-{sticky}-{path}"
                        )))
                        .flex_none()
                        .gap(px(5.0))
                        .px(px(8.0))
                        .h(px(BUTTON_HEIGHT))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(hsla(s.hairline))
                        .text_size(px(SMALL_SIZE))
                        .text_color(hsla(if viewed { s.text } else { s.muted }))
                        .hover(|this| this.bg(hsla(s.hover)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_viewed(&viewed_path, cx);
                        }))
                        .child(checkbox(s, viewed))
                        .child("Viewed"),
                )
            })
            .into_any_element()
    }

    fn render_hunk_header(
        &mut self,
        s: &ChangesStyle,
        row: usize,
        file_index: usize,
        hunk_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(file) = self
            .data
            .as_ref()
            .and_then(|data| data.files.get(file_index))
        else {
            return div().into_any_element();
        };
        let Some(hunk) = file.hunks.get(hunk_index) else {
            return div().into_any_element();
        };
        let path = file.path.clone();
        let header = hunk.header.clone();
        let confirming = self.confirm == Some(Confirm::RevertHunk(path.clone(), hunk_index));
        let on_cursor = self.cursor == Some(row);
        let group: SharedString = format!("changes-hunk-{row}").into();
        let revert_path = path.clone();
        let ask_path = path.clone();
        h_flex()
            .id(SharedString::from(format!(
                "changes-hunk-{path}-{hunk_index}"
            )))
            .group(group.clone())
            .w_full()
            .h(px(diff_hunk_height()))
            .bg(hsla(s.hunk_row))
            .when(on_cursor, |this| {
                this.border_l_2().border_color(hsla(s.accent))
            })
            .font_family(MODAL_MONO_FONT)
            .text_size(px(diff_code_size() - 1.0))
            .child(
                div()
                    .flex_none()
                    .w(px(diff_number_width() * 2.0 + diff_marker_width())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(hsla(s.hunk_text))
                    .child(header),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(4.0))
                    .pr(px(8.0))
                    .font_family(crate::app::window::native_modal_kit::MODAL_UI_FONT)
                    .when(confirming, |this| {
                        this.child(
                            div()
                                .text_size(px(SMALL_SIZE))
                                .text_color(hsla(s.danger))
                                .child("Revert this hunk?"),
                        )
                        .child(small_button(
                            s,
                            "changes-hunk-revert-cancel",
                            "Cancel",
                            false,
                            cx.listener(|this, _, _, cx| {
                                this.confirm = None;
                                cx.notify();
                            }),
                        ))
                        .child(small_button(
                            s,
                            "changes-hunk-revert-yes",
                            "Revert",
                            true,
                            cx.listener(move |this, _, window, cx| {
                                this.revert(
                                    Confirm::RevertHunk(revert_path.clone(), hunk_index),
                                    window,
                                    cx,
                                );
                            }),
                        ))
                    })
                    .when(!confirming, |this| {
                        this.opacity(0.0)
                            .group_hover(group.clone(), |this| this.opacity(1.0))
                            .child(small_button(
                                s,
                                "changes-hunk-revert",
                                "Revert",
                                false,
                                cx.listener(move |this, _, _, cx| {
                                    this.confirm =
                                        Some(Confirm::RevertHunk(ask_path.clone(), hunk_index));
                                    cx.notify();
                                }),
                            ))
                    }),
            )
            .into_any_element()
    }

    fn render_line_row(
        &mut self,
        s: &ChangesStyle,
        index: usize,
        row: Row,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(data) = &self.data else {
            return div().into_any_element();
        };
        let selected = self.row_selected(index);
        let selection_end = self
            .selection
            .is_some_and(|(a, b)| a.max(b) == index && !self.dragging);
        let wrap = self.wrap || self.split;
        let group: SharedString = format!("changes-line-{index}").into();
        let plain = match row {
            Row::Line(file, ..) | Row::Split(file, ..) => data
                .files
                .get(file)
                .is_some_and(|file| is_prose(&file.path)),
            _ => false,
        };
        let body = match row {
            Row::Line(file, hunk, line) => {
                let Some(line) = data
                    .files
                    .get(file)
                    .and_then(|f| f.hunks.get(hunk))
                    .and_then(|h| h.lines.get(line))
                else {
                    return div().into_any_element();
                };
                let (row_bg, gutter_bg, number_color) = kind_colors(s, line.kind);
                h_flex()
                    .w_full()
                    .items_start()
                    .when_some(row_bg, |this, bg| this.bg(hsla(bg)))
                    .child(number_cell(s, line.old, gutter_bg, number_color))
                    .child(number_cell(s, line.new, gutter_bg, number_color))
                    .child(marker_cell(s, line.kind))
                    .child(code_cell(s, line, wrap, plain))
            }
            Row::Split(file, hunk, left, right) => {
                let Some(lines) = data
                    .files
                    .get(file)
                    .and_then(|f| f.hunks.get(hunk))
                    .map(|h| &h.lines)
                else {
                    return div().into_any_element();
                };
                let half = |side: Option<usize>, old_side: bool| {
                    let line = side.and_then(|index| lines.get(index));
                    match line {
                        Some(line) => {
                            let kind = match (line.kind, old_side) {
                                (LineKind::Context, _) => LineKind::Context,
                                (kind, _) => kind,
                            };
                            let (row_bg, gutter_bg, number_color) = kind_colors(s, kind);
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .items_start()
                                .when_some(row_bg, |this, bg| this.bg(hsla(bg)))
                                .child(number_cell(
                                    s,
                                    if old_side { line.old } else { line.new },
                                    gutter_bg,
                                    number_color,
                                ))
                                .child(marker_cell(s, kind))
                                .child(code_cell(s, line, true, plain))
                                .into_any_element()
                        }
                        None => div()
                            .flex_1()
                            .min_w_0()
                            .self_stretch()
                            .bg(hsla(rgba_of(s.muted, 0.05)))
                            .into_any_element(),
                    }
                };
                h_flex()
                    .w_full()
                    .items_stretch()
                    .child(half(left, true))
                    .child(
                        div()
                            .flex_none()
                            .w(px(1.0))
                            .self_stretch()
                            .bg(hsla(s.hairline)),
                    )
                    .child(half(right, false))
            }
            _ => return div().into_any_element(),
        };
        div()
            .id(SharedString::from(format!("changes-row-{index}")))
            .group(group.clone())
            .relative()
            .w_full()
            .min_h(px(diff_row_height()))
            .font_family(MODAL_MONO_FONT)
            .text_size(px(diff_code_size()))
            .line_height(px(diff_row_height()))
            .bg(hsla(s.code_bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.begin_selection(index, event.modifiers.shift, cx);
                }),
            )
            .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                this.extend_selection(index, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.end_selection();
                    cx.notify();
                }),
            )
            .child(body)
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(hsla(rgba_of(s.accent, 0.16)))
                        .border_l_2()
                        .border_color(hsla(s.accent)),
                )
            })
            // The gutter's comment button, on hover.
            .when(!selected, |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("changes-plus-{index}")))
                        .absolute()
                        .top(px(2.0))
                        .left(px(diff_number_width() * 2.0 - 2.0))
                        .size(px(diff_row_height() - 4.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(hsla(gpui::rgb(0x2f5d8a)))
                        .opacity(0.0)
                        .group_hover(group.clone(), |this| this.opacity(1.0))
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.selection = Some((index, index));
                                this.dragging = false;
                                this.open_editor(window, cx);
                            }),
                        )
                        .child(modal_icon(ICON_PLUS, diff_code_size(), gpui::rgb(0xffffff))),
                )
            })
            // CDXC:Changes 2026-10-25 DECISION: User: "make the copy button and comment button both have same bg and both have less height so they fit within that row and they should just be icon buttons. don't make their bg fully transparent make it have frosting so they're visible". Both are icon buttons of one size on the sidebar tooltip bubble's in-window fill, inside the row's height.
            .when(selection_end, |this| {
                let (fill, border) = s.menu_bubble();
                let button = |id: &'static str, icon: &'static str, tip: &'static str| {
                    div()
                        .id(id)
                        .flex_none()
                        .size(px(diff_row_height() - 4.0))
                        .rounded(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(border)
                        .bg(fill)
                        .cursor_pointer()
                        .hover(|this| this.bg(fill.blend(hsla(s.hover))))
                        // A button inside a diff row must not start a line selection under it.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tip).build(window, cx)
                        })
                        .child(modal_icon(icon, ICON_SIZE - 2.0, s.text))
                };
                this.child(
                    h_flex()
                        .absolute()
                        .right(px(8.0))
                        .top(px(2.0))
                        .gap(px(4.0))
                        .child(
                            button("changes-selection-copy", ICON_COPY, "Copy").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.copy_selection(window, cx);
                                }),
                            ),
                        )
                        .child(
                            button("changes-selection-comment", ICON_MESSAGE, "Comment").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_editor(window, cx);
                                }),
                            ),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_comment(&mut self, s: &ChangesStyle, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(comment) = self
            .comments
            .iter()
            .find(|comment| comment.id == id)
            .cloned()
        else {
            return div().into_any_element();
        };
        let open = comment.is_open();
        let (edge, pill_text, pill_color) = match &comment.state {
            CommentState::Sent { at_ms } => {
                let mut text = format!("Sent {}", ago(*at_ms));
                if !comment.target.is_empty() {
                    text.push_str(&format!(" to {}", comment.target));
                }
                (s.accent, text, s.accent)
            }
            CommentState::Resolved { auto: true } => (
                s.resolved,
                "Resolved: the lines changed".to_string(),
                s.resolved,
            ),
            CommentState::Resolved { auto: false } => {
                (s.resolved, "Resolved".to_string(), s.resolved)
            }
        };
        h_flex()
            .id(SharedString::from(format!("changes-comment-{id}")))
            .w_full()
            .items_start()
            .gap(px(8.0))
            .pl(px(diff_number_width() * 2.0 + diff_marker_width()))
            .pr(px(8.0))
            .py(px(6.0))
            .bg(hsla(rgba_of(edge, if open { 0.07 } else { 0.04 })))
            .border_l_2()
            .border_color(hsla(rgba_of(edge, 0.6)))
            .when(!open, |this| this.opacity(0.75))
            .child(modal_icon(
                if open { ICON_MESSAGE } else { ICON_CHECK },
                ICON_SIZE,
                edge,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .text_size(px(SMALL_SIZE))
                            .text_color(hsla(s.muted))
                            .child(comment.range.label.clone())
                            .child(
                                div()
                                    .px(px(6.0))
                                    .rounded(px(8.0))
                                    .border_1()
                                    .border_color(hsla(rgba_of(pill_color, 0.45)))
                                    .text_color(hsla(pill_color))
                                    .child(pill_text),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(CODE_SIZE))
                            .text_color(hsla(s.text))
                            .child(comment.text.clone()),
                    ),
            )
            .when(open, |this| {
                this.child(small_button(
                    s,
                    "changes-comment-resolve",
                    "Resolve",
                    false,
                    cx.listener(move |this, _, _, cx| {
                        this.resolve_comment(id, cx);
                    }),
                ))
            })
            .child(icon_button(
                s,
                format!("changes-comment-delete-{id}"),
                ICON_CLOSE,
                "Remove",
                cx.listener(move |this, _, _, cx| {
                    this.delete_comment(id, cx);
                }),
            ))
            .into_any_element()
    }

    fn render_editor(
        &mut self,
        s: &ChangesStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.editor.as_ref() else {
            return div().into_any_element();
        };
        let label = format!(
            "Comment on {} {}",
            editor.range.file.rsplit('/').next().unwrap_or(""),
            editor.range.label
        );
        let empty = editor.input.read(cx).value().trim().is_empty();
        v_flex()
            .w_full()
            .gap(px(6.0))
            .pl(px(diff_number_width() * 2.0 + diff_marker_width()))
            .pr(px(10.0))
            .py(px(8.0))
            .bg(hsla(s.header_bg))
            .border_y_1()
            .border_color(hsla(s.hairline))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.cancel_editor(window, cx);
                }
            }))
            .child(
                div()
                    .text_size(px(SMALL_SIZE))
                    .text_color(hsla(s.muted))
                    .child(label),
            )
            .child(modal_text_area(
                &s.p,
                &editor.input,
                Some(diff_row_height() * 3.0),
                false,
                window,
                cx,
            ))
            .child(
                h_flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(SMALL_SIZE))
                            .text_color(hsla(s.faint))
                            .child(
                                "\u{2318}\u{21b5} adds it to the chat composer \u{b7} Esc cancels",
                            ),
                    )
                    .child(small_button(
                        s,
                        "changes-editor-cancel",
                        "Cancel",
                        false,
                        cx.listener(|this, _, window, cx| {
                            this.cancel_editor(window, cx);
                        }),
                    ))
                    .child(
                        small_button(
                            s,
                            "changes-editor-save",
                            "Add to chat",
                            true,
                            cx.listener(|this, _, window, cx| {
                                this.save_comment(window, cx);
                            }),
                        )
                        .when(empty, |this| this.opacity(0.5)),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn open_in_files(
        &mut self,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.target.clone() else {
            return;
        };
        (self.host)(
            super::view::ChangesRequest::OpenInFiles {
                project_key: target.project_key,
                path: path.to_string(),
                line,
            },
            window,
            cx,
        );
    }
}

/// Prose and data files whose words the code colouring would misread as keywords.
pub(crate) fn is_prose(path: &str) -> bool {
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "md" | "markdown" | "mdx" | "txt" | "rst" | "adoc" | "csv" | "tsv" | "log"
    )
}

/// A text button sized to the view's secondary text; `primary` fills it with the accent.
pub(crate) fn small_button(
    s: &ChangesStyle,
    id: &'static str,
    label: &'static str,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(BUTTON_HEIGHT))
        .px(px(10.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .text_size(px(SMALL_SIZE))
        .cursor_pointer()
        // A button inside a diff row must not start a line selection under it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when(primary, |this| {
            this.bg(hsla(gpui::rgb(0x2f5d8a)))
                .text_color(hsla(gpui::rgb(0xffffff)))
                .font_weight(FontWeight::MEDIUM)
                .hover(|this| this.bg(hsla(gpui::rgb(0x376a9c))))
        })
        .when(!primary, |this| {
            this.text_color(hsla(s.text))
                .border_1()
                .border_color(hsla(s.hairline))
                .hover(|this| this.bg(hsla(s.hover)))
        })
        .on_click(on_click)
        .child(label)
}

pub(crate) fn icon_button(
    s: &ChangesStyle,
    id: String,
    icon: &'static str,
    tooltip: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .flex_none()
        .size(px(BUTTON_HEIGHT + 2.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|this| this.bg(hsla(s.hover)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tooltip).build(window, cx))
        .on_click(on_click)
        .child(modal_icon(icon, ICON_SIZE, s.muted))
        .into_any_element()
}
