//! The file change card under a message: a header that sums up the edit (pencil tile, "Edited N
//! files", the folder the files share, total +/-, a ratio bar and Review all), then a row per file
//! (chevron, file icon, name, folder, counts and a five-block change bar)
//! that opens to its diff with old and new line numbers, "Show all N changed lines" and Open file.
//!
//! What the card says (the title, folder, totals, bars, labels and the preview's length)
//! comes from `packages/gx-chat-core/src/transcript/file_change_rows.rs` (the user's pick is the
//! `CDXC:SessionChat 2026-10-11 DECISION` on `file_change_card` there); this file only lays it out.
//! The phone draws the same card in `apps/mobile/app/src/chat/native/transcript/FileChanges.tsx`.

use super::cards::SINGLE_LINE_CARD;
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::TranscriptItem;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::{Value, json};

const MONO: &str = "JetBrainsMono Nerd Font";

/// The card's colours: the surfaces come from the chat theme (CDXC:Theming 2026-09-22, the diff
/// cards take from the theme colour too) and the hues name what happened to a file.
struct Palette {
    surface: Hsla,
    border: Hsla,
    hairline: Hsla,
    hover: Hsla,
    well: Hsla,
    edited: Hsla,
    new: Hsla,
    deleted: Hsla,
    added: Hsla,
    removed: Hsla,
    added_row: Hsla,
    removed_row: Hsla,
    added_text: Hsla,
    removed_text: Hsla,
    bar_add: Hsla,
    bar_del: Hsla,
    bar_none: Hsla,
    hunk: Hsla,
    hunk_row: Hsla,
    line_number: Hsla,
}

impl Palette {
    fn of(p: &ChatAppearance) -> Self {
        let hue =
            |dark: u32, light: u32| -> Hsla { rgb(if p.light { light } else { dark }).into() };
        Self {
            surface: p.input,
            border: if p.light {
                p.border
            } else {
                p.foreground.opacity(0.075)
            },
            hairline: p.foreground.opacity(if p.light { 0.08 } else { 0.045 }),
            hover: p.foreground.opacity(if p.light { 0.04 } else { 0.03 }),
            well: if p.light {
                p.foreground.opacity(0.03)
            } else {
                p.background
            },
            edited: hue(0x60a5fa, 0x2563eb),
            new: hue(0x34d399, 0x059669),
            deleted: hue(0xfb7185, 0xe11d48),
            added: hue(0x94caaa, 0x16803d),
            removed: hue(0xe5a0a4, 0xc53030),
            added_row: Hsla::from(rgb(0x3fb37f)).opacity(0.11),
            removed_row: Hsla::from(rgb(0xe06c75)).opacity(0.11),
            added_text: hue(0xb9e3c9, 0x14532d),
            removed_text: hue(0xf1c1c5, 0x7f1d1d),
            bar_add: hue(0x3fb37f, 0x16a34a),
            bar_del: hue(0xe06c75, 0xdc2626),
            bar_none: if p.light {
                p.border
            } else {
                rgb(0x2e2e2e).into()
            },
            hunk: hue(0x7f8ea3, 0x52617a),
            hunk_row: Hsla::from(rgb(0x60a5fa)).opacity(0.07),
            line_number: p.muted.opacity(0.5),
        }
    }

    fn status(&self, status: &str) -> Hsla {
        match status {
            "new" => self.new,
            "deleted" => self.deleted,
            _ => self.edited,
        }
    }
}

fn status_icon(status: &str) -> &'static str {
    match status {
        "new" => "titlebar/file-plus.svg",
        "deleted" => "titlebar/file-minus.svg",
        _ => "titlebar/file-text.svg",
    }
}

fn icon(path: &'static str, size: f32, color: Hsla) -> AnyElement {
    gpui::svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
        .into_any_element()
}

/// A folder that gives up its start, never its end, so the part nearest the file stays readable.
/// `truncate()` ellipsises from the end, so the text keeps its own width and this box clips it
/// from the left (the old card's fix for the same paths).
fn folder_label(folder: String, color: Hsla, size: f32) -> AnyElement {
    div()
        .flex()
        .justify_end()
        .min_w_0()
        .flex_shrink(1.0)
        .overflow_hidden()
        .text_color(color)
        .text_size(px(size))
        .child(div().flex_shrink_0().whitespace_nowrap().child(folder))
        .into_any_element()
}

/// The card surfaces and the added and removed hues a tool-call card shares with the file card:
/// `(surface, border, hairline, hover, added, removed)`.
pub(super) fn card_colors(p: &ChatAppearance) -> (Hsla, Hsla, Hsla, Hsla, Hsla, Hsla) {
    let palette = Palette::of(p);
    (
        palette.surface,
        palette.border,
        palette.hairline,
        palette.hover,
        palette.added,
        palette.removed,
    )
}

fn counts(added: u64, removed: u64, palette: &Palette, s: f32) -> AnyElement {
    div()
        .flex()
        .flex_shrink_0()
        .gap(px(4.0 * s))
        .font_family(MONO)
        .text_size(px(12.0 * s))
        .whitespace_nowrap()
        .child(div().text_color(palette.added).child(format!("+{added}")))
        .child(
            div()
                .text_color(palette.removed)
                .child(format!("\u{2212}{removed}")),
        )
        .into_any_element()
}

impl NativeChatView {
    pub(super) fn file_change_cards(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let files = message["files"].as_array().cloned().unwrap_or_default();
        // React passed `hideFileChanges` to every row of a finished turn: the writes belong to that
        // turn's "N files changed" fold instead, and a card must never be drawn in both places.
        if files.is_empty() || self.hide_file_changes {
            return Vec::new();
        }
        let id = text(message, "id");
        let mut rows = Vec::new();
        let simple_key = format!("files:{id}");
        // A write waiting on an approval opens the group (gx-chat-core `approval_wait.rs`).
        let awaiting = files.iter().any(|file| file["awaitingApproval"] == true);
        let simple_expanded = self.is_expanded(&simple_key, awaiting);
        if p.simple {
            let motion = self.disclosure_frame(&simple_key, simple_expanded, cx);
            rows.push(self.disclosure(
                simple_key.clone(),
                text(message, "simpleFileLabel"),
                simple_expanded,
                None,
                p,
                cx,
            ));
            if !simple_expanded && motion.is_none() {
                return rows;
            }
            let card = self.file_change_card(&id, &message["fileCard"], &files, p, cx);
            rows.push(self.disclosure_body_motion(&simple_key, motion, 8.0 * p.scale, card));
            return rows;
        }
        rows.push(self.file_change_card(&id, &message["fileCard"], &files, p, cx));
        rows
    }

    /// An edit row's diff inside a tool-call card (`tool_card.rs`), the same diff a file card's
    /// row opens onto; `None` until the core has the lines.
    pub(super) fn file_entry_diff(
        &mut self,
        key: &str,
        file: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let detail = self.row_detail(
            key,
            "file",
            file["messageId"].as_str().unwrap_or_default(),
            file["index"].as_u64().unwrap_or_default(),
        );
        if detail.is_null() {
            return None;
        }
        let palette = Palette::of(p);
        Some(self.file_change_diff(key, file, &detail, &palette, p, cx))
    }

    /// The turn's writes, folded behind "N files changed" the way React grouped a finished turn
    /// (the decision in `session-chat-message-list/list.tsx`). The label and the rows are projected
    /// together in the core (`transcript/presentation.rs`), so the label counts the files drawn.
    pub(super) fn completed_files_fold(
        &mut self,
        item: &TranscriptItem,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let TranscriptItem::CompletedWork {
            id,
            files_card,
            files,
            files_label,
            simple_files_label,
            deferred,
            ..
        } = item
        else {
            return None;
        };
        // An older turn arrives collapsed with only the paths its unread work changed; the row
        // stays and reads that work on open, as React's list did (`list.tsx`).
        let unread = files.is_empty()
            && deferred
                .as_ref()
                .and_then(|work| work["filePaths"].as_array())
                .is_some_and(|paths| !paths.is_empty());
        if files.is_empty() && !unread {
            return None;
        }
        let key = format!("work-files:{id}");
        let expanded = self.is_expanded(&key, false);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let load = unread.then(|| json!({"type":"loadWork","id":id,"work":deferred}));
        let label = if p.simple {
            simple_files_label.clone()
        } else {
            files_label.clone()
        };
        let mut group = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(8.0 * p.scale))
            .child(self.disclosure(key.clone(), label, expanded, load, p, cx));
        if expanded || motion.is_some() {
            let body = if unread {
                self.deferred_work_notice(item, p, cx)
            } else {
                Some(self.file_change_card(&format!("work:{id}"), files_card, files, p, cx))
            };
            if let Some(body) = body {
                group = group.child(self.disclosure_body_motion(&key, motion, 8.0 * p.scale, body));
            }
        }
        Some(group.into_any_element())
    }

    /// One card: the header, then a row per file.
    fn file_change_card(
        &mut self,
        id: &str,
        summary: &Value,
        files: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = Palette::of(p);
        let s = p.scale;
        let mut card = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .rounded(px(14.0 * s))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .text_color(p.card_muted)
            .child(self.file_change_header(id, summary, &palette, p, cx));
        for (index, file) in files.iter().enumerate() {
            card = card.child(self.file_change_row(id, index, file, &palette, p, cx));
        }
        card.into_any_element()
    }

    fn file_change_header(
        &self,
        id: &str,
        summary: &Value,
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let folder = text(summary, "folder");
        let share = summary["addShare"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
        let changed =
            summary["added"].as_u64().unwrap_or(0) + summary["removed"].as_u64().unwrap_or(0);
        let paths = summary["paths"].clone();
        let ratio_width = 90.0 * s;
        div()
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .px(px(14.0 * s))
            .py(px(12.0 * s))
            .min_w_0()
            // The one-line cards' neutral tile (Q296 A, the SessionChat 2026-10-18 DECISION on
            // `SINGLE_LINE_CARD`): the counts and the bar are the card's only colour.
            .child(SINGLE_LINE_CARD.tile("titlebar/pencil.svg", p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_color(p.foreground)
                            .text_size(px(13.5 * s))
                            .line_height(px(18.0 * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text(summary, "title")),
                    )
                    .when(!folder.is_empty(), |this| {
                        this.child(div().flex().line_height(px(16.0 * s)).child(folder_label(
                            folder,
                            p.muted,
                            12.0 * s,
                        )))
                    }),
            )
            .child(counts(
                summary["added"].as_u64().unwrap_or(0),
                summary["removed"].as_u64().unwrap_or(0),
                palette,
                s,
            ))
            .when(changed > 0, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .w(px(ratio_width))
                        .h(px(5.0 * s))
                        .rounded(px(3.0 * s))
                        .overflow_hidden()
                        .bg(palette.bar_none)
                        .child(
                            div()
                                .h_full()
                                .w(px(ratio_width * share))
                                .bg(palette.bar_add),
                        )
                        .child(div().h_full().flex_1().bg(palette.bar_del)),
                )
            })
            .child(
                div()
                    .id(format!("file-review:{id}"))
                    .flex_shrink_0()
                    .h(px(28.0 * s))
                    .px(px(11.0 * s))
                    .flex()
                    .items_center()
                    .rounded(px(8.0 * s))
                    .text_size(px(13.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .chat_filled_hover_text(p.neutral_button_fills(), true, Some(p.foreground))
                    .chat_cursor_pointer()
                    .child("Review all")
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        view.invoke(json!({"type":"reviewFileChanges","paths":paths}), cx);
                    })),
            )
            .into_any_element()
    }

    fn file_change_row(
        &mut self,
        stack_id: &str,
        index: usize,
        file: &Value,
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("file:{stack_id}:{index}");
        // Settings > Chat > Show file edit previews opens every row to its preview, and a write
        // waiting on an approval opens its own row so the diff shows beside the card (gx-chat-core
        // `approval_wait.rs`).
        let open = self.is_expanded(&key, p.file_previews || file["awaitingApproval"] == true);
        let motion = self.disclosure_frame(&key, open, cx);
        let status = file["status"].as_str().unwrap_or("edited");
        let hue = palette.status(status);
        let failed = file["failed"] == true;
        let toggle_key = key.clone();
        let menu_path = text(file, "path");
        let header = div()
            .id(format!("file-row:{key}"))
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .px(px(14.0 * s))
            .py(px(8.0 * s))
            .min_w_0()
            .border_t_1()
            .border_color(palette.hairline)
            .chat_cursor_pointer()
            .hover(|style| style.bg(palette.hover))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(toggle_key.clone(), open, cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.file_secondary_press(menu_path.clone(), event, window, cx);
                }),
            )
            .child(icon(
                if open {
                    "titlebar/chevron-down.svg"
                } else {
                    "titlebar/chevron-right.svg"
                },
                13.0 * s,
                p.muted,
            ))
            .child(icon(status_icon(status), 15.0 * s, hue))
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_color(p.foreground)
                    .text_size(px(13.5 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .child(text(file, "filename")),
            )
            .child(folder_label(text(file, "parent"), p.muted, 12.5 * s))
            .when(failed, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.error())
                        .child("Failed"),
                )
            })
            .child(div().flex_1())
            .child(counts(
                file["added"].as_u64().unwrap_or(0),
                file["removed"].as_u64().unwrap_or(0),
                palette,
                s,
            ))
            .child(div().flex().flex_shrink_0().gap(px(2.0 * s)).children(
                file["bars"].as_array().into_iter().flatten().map(|bar| {
                    div()
                        .size(px(6.0 * s))
                        .rounded(px(1.5 * s))
                        .bg(match bar.as_str() {
                            Some("add") => palette.bar_add,
                            Some("del") => palette.bar_del,
                            _ => palette.bar_none,
                        })
                }),
            ));
        let mut row = div().flex().flex_col().w_full().min_w_0().child(header);
        if !open && motion.is_none() {
            return row.into_any_element();
        }
        let detail = self.row_detail(
            &key,
            "file",
            file["messageId"].as_str().unwrap_or_default(),
            file["index"].as_u64().unwrap_or_default(),
        );
        if detail.is_null() {
            return row.into_any_element();
        }
        let body = self.file_change_diff(&key, file, &detail, palette, p, cx);
        row = row.child(self.disclosure_body_motion(&key, motion, 0.0, body));
        row.into_any_element()
    }

    /// An open row's diff on its darker well, with the buttons under it.
    fn file_change_diff(
        &mut self,
        key: &str,
        file: &Value,
        detail: &Value,
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let all_key = format!("file-all:{key}");
        let show_all = self.expanded.contains(&all_key);
        let has_more = file["hasMore"] == true;
        let failed = file["failed"] == true;
        let lines = detail["lines"].as_array().cloned().unwrap_or_default();
        let preview = detail["preview"]
            .as_u64()
            .map_or(lines.len(), |end| end as usize);
        let shown = if show_all || !has_more {
            &lines[..]
        } else {
            &lines[..preview.min(lines.len())]
        };
        let numbered = lines
            .iter()
            .any(|line| line.get("old").is_some() || line.get("new").is_some());
        let number_width = 38.0 * s;
        let mut well = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .pt(px(6.0 * s))
            .pb(px(8.0 * s))
            .bg(palette.well)
            .border_t_1()
            .border_color(palette.hairline)
            .font_family(MONO)
            .text_size(px(12.4 * s))
            .line_height(px(20.0 * s));
        if shown.is_empty() {
            well = well.child(div().px(px(14.0 * s)).text_color(p.muted).child(
                if file["status"] == "deleted" {
                    "File removed"
                } else {
                    "Empty file"
                },
            ));
        }
        let number = |value: &Value| -> AnyElement {
            div()
                .w(px(number_width))
                .flex_shrink_0()
                .flex()
                .justify_end()
                .pr(px(8.0 * s))
                .text_color(palette.line_number)
                .child(value.as_u64().map(|n| n.to_string()).unwrap_or_default())
                .into_any_element()
        };
        for line in shown {
            let kind = line["kind"].as_str().unwrap_or("context");
            let body = text(line, "text");
            if kind == "meta" {
                let hunk = body.starts_with("@@");
                well = well.child(
                    div()
                        .px(px(14.0 * s))
                        .py(px(2.0 * s))
                        .when(hunk, |this| this.bg(palette.hunk_row))
                        .text_size(px(11.5 * s))
                        .text_color(if hunk { palette.hunk } else { p.muted })
                        .child(body),
                );
                continue;
            }
            let (color, sign_color, background, sign) = match kind {
                "add" => (
                    palette.added_text,
                    palette.bar_add,
                    Some(palette.added_row),
                    "+",
                ),
                "del" => (
                    palette.removed_text,
                    palette.bar_del,
                    Some(palette.removed_row),
                    "\u{2212}",
                ),
                _ => (p.prose, p.muted, None, " "),
            };
            well = well.child(
                div()
                    .flex()
                    .w_full()
                    .min_w_0()
                    .when(!numbered, |this| this.pl(px(14.0 * s)))
                    .pr(px(14.0 * s))
                    .when_some(background, |this, background| this.bg(background))
                    .text_color(color)
                    .when(numbered, |this| {
                        this.child(number(&line["old"])).child(number(&line["new"]))
                    })
                    .child(
                        div()
                            .w(px(16.0 * s))
                            .flex_shrink_0()
                            .text_color(sign_color)
                            .child(sign),
                    )
                    .child(div().flex_1().min_w_0().child(body)),
            );
        }
        if failed && (show_all || !has_more) {
            well = well.child(
                div()
                    .px(px(14.0 * s))
                    .pt(px(8.0 * s))
                    .font_family(p.font.clone())
                    .text_color(p.error())
                    .child(text(file, "error")),
            );
        }
        let ghost = |id: String, glyph: &'static str, label: String| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .h(px(28.0 * s))
                .px(px(11.0 * s))
                .rounded(px(8.0 * s))
                .font_family(p.font.clone())
                .text_size(px(13.0 * s))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.muted)
                .chat_filled_hover_text(p.neutral_button_fills(), true, Some(p.foreground))
                .chat_cursor_pointer()
                .child(icon(glyph, 13.0 * s, p.muted))
                .child(label)
        };
        let open_path = text(file, "path");
        let copy_path = open_path.clone();
        let locate_path = open_path.clone();
        let deleted = file["status"] == "deleted";
        let locatable = !deleted && !open_path.is_empty() && self.file_locate_available();
        let toggle_all = all_key.clone();
        let footer = div()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(14.0 * s))
            .pt(px(8.0 * s))
            .when(has_more, |this| {
                this.child(
                    ghost(
                        format!("file-show-all:{key}"),
                        if show_all {
                            "titlebar/chevron-up.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        },
                        if show_all {
                            "Show fewer lines".to_string()
                        } else {
                            text(file, "showAllLabel")
                        },
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_disclosure(&toggle_all, cx);
                    })),
                )
            })
            .child(div().flex_1())
            .when(!copy_path.is_empty(), |this| {
                this.child(
                    ghost(
                        format!("file-copy-path:{key}"),
                        "titlebar/copy.svg",
                        "Copy path".to_string(),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.copy_file_path(&copy_path, cx);
                    })),
                )
            })
            .when(locatable, |this| {
                this.child(
                    ghost(
                        format!("file-locate:{key}"),
                        "titlebar/folder-open.svg",
                        "Open file location".to_string(),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.locate_file(&locate_path, cx);
                    })),
                )
            })
            .when(!deleted, |this| {
                this.child(
                    ghost(
                        format!("file-open:{key}"),
                        "titlebar/external-link.svg",
                        "Open file".to_string(),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.invoke(
                            json!({"type":"openMarkdownLink","href":open_path,"external":false}),
                            cx,
                        )
                    })),
                )
            });
        well.child(footer).into_any_element()
    }
}
