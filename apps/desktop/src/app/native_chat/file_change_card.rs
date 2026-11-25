//! The file change card under a message: a header that sums up the edit (pencil tile, "Edited N
//! files", the folder the files share, total +/-, a ratio bar and Review all), then a row per file
//! (chevron, file icon, name, folder, counts and a five-block change bar)
//! that opens to its diff with old and new line numbers, "Show all N changed lines" and Open file.
//!
//! What the card says (the title, folder, totals, bars, labels and the preview's length)
//! comes from `packages/gx-chat-core/src/transcript/file_change_rows.rs` (the user's pick is the
//! `CDXC:SessionChat 2026-10-11 DECISION` on `file_change_card` there); this file only lays it out.
//! The phone draws the same card in `apps/mobile/app/src/chat/native/transcript/FileChanges.tsx`.

use super::cards::{card_hover_fill, SINGLE_LINE_CARD, TITLE_ROW_GAP};
use super::chat_text_look::PROSE_SIZE;
use super::disclosure_body::{disclosure_body, DisclosureRail};
use super::quiet_work::{file_names, QuietEdit};
use super::tool_run::{
    boxed_card_body, boxed_header_corners, DESCRIBED_LABEL_OPACITY, ROW_PAD_X, TILE,
};
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::transcript::file_change_rows::{turn_files_fold, TurnFilesFold};
use ghostex_gx_chat_core::TranscriptItem;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, rgb, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
};
use serde_json::{json, Value};

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
            // CDXC:Theming 2026-11-20 (helpers/chrome_palette.rs): under window glass the dark
            // well is a half-strength code wash (`glass_wash`, white 3% like light mode's ink 3%),
            // not the opaque chat background, which read as a black slab on the glass.
            well: if p.light {
                p.foreground.opacity(0.03)
            } else if p.window_glass {
                crate::app::helpers::glass_wash(p.light, 0.5)
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

fn file_call_status(file: &Value) -> &str {
    file["callStatus"]
        .as_str()
        .unwrap_or(if file["failed"] == true {
            "failed"
        } else {
            "done"
        })
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
        // Agent work Reduced or Quiet: the edits are one live edit line in every view, open in
        // Detailed (quiet_work.rs).
        let quiet = p.quiet_style();
        let simple_expanded = self.is_expanded(&simple_key, awaiting || (quiet && p.work_open));
        if p.simple || quiet {
            if quiet {
                let motion = self.disclosure_frame(&simple_key, simple_expanded, cx);
                let summary = &message["fileCard"];
                rows.push(self.quiet_edit_line(
                    QuietEdit {
                        key: simple_key.clone(),
                        label: text(message, "simpleFileLabel"),
                        added: summary["added"].as_u64().unwrap_or(0),
                        removed: summary["removed"].as_u64().unwrap_or(0),
                        names: file_names(summary["paths"].as_array().into_iter().flatten()),
                        expanded: simple_expanded,
                    },
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
            let boxed = self.quiet_boxed_here();
            rows.push(self.files_fold_row(
                &simple_key,
                &id,
                text(message, "simpleFileLabel"),
                &message["fileCard"],
                &files,
                simple_expanded,
                boxed,
                p,
                cx,
            ));
            return rows;
        }
        rows.push(self.file_change_card(&id, &message["fileCard"], &files, p, cx));
        rows
    }

    /// An edit row's diff inside a tool-call card (`tool_card.rs`), the same diff a file card's
    /// row opens onto, with Review all for `review` (the card's edited files) in its footer, as the
    /// file card's header has it; `None` until the core has the lines.
    pub(super) fn file_entry_diff(
        &mut self,
        key: &str,
        file: &Value,
        review: Vec<String>,
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
        Some(self.file_change_diff(key, file, &detail, Some(review), &palette, true, p, cx))
    }

    /// The turn's writes, folded behind "N files changed" ("Edited N files" in simple mode) right
    /// under the turn's card while that card is open; a closed card shows none (gx-chat-core
    /// `turn_files_fold`, where the user's decision is). The label and the rows are projected
    /// together in the core (`transcript/presentation.rs`), so the label counts the files drawn.
    ///
    /// CDXC:SessionChat 2026-11-24 DECISION: User, of the "> Edited 2 files" disclosure inside an
    /// expanded tool-call group: "make edited files look like the other rows shown there please not
    /// like this". The fold row wears the tool row's shape (`tool_run.rs`: 22px neutral tile with
    /// the pencil icon, 8px inset, 5px vertical pad, prose size at 80% opacity, +/- counts and a
    /// right-side check or error mark), and expands on a rail into file rows matching that same
    /// style, each opening onto its diff. The phone matches in `FileChanges.tsx`.
    pub(super) fn completed_files_fold(
        &mut self,
        item: &TranscriptItem,
        turn_open: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let TranscriptItem::CompletedWork {
            id,
            files_card,
            files,
            files_label,
            simple_files_label,
            ..
        } = item
        else {
            return None;
        };
        if turn_files_fold(item, turn_open) == TurnFilesFold::Hidden {
            return None;
        }
        let key = format!("work-files:{id}");
        let expanded = self.is_expanded(&key, false);
        let label = if p.simple {
            simple_files_label.clone()
        } else {
            files_label.clone()
        };
        Some(self.files_fold_row(
            &key,
            &format!("work:{id}"),
            label,
            files_card,
            files,
            expanded,
            false,
            p,
            cx,
        ))
    }

    /// The "Edited N files" fold row styled like the tool and thought rows around it, expanding on
    /// a rail from the pencil tile's centre into per-file rows in the same style. `boxed` draws it
    /// as an opened Quiet View line's one-line card instead.
    ///
    /// CDXC:SessionChat 2026-11-25 DECISION: User, of an opened Working line where the thoughts and a background command were one-line cards and the "Edited 1 file" rows between them were bare tool rows: "file editing looks different from the others pls fix". Inside an opened Quiet View line the fold is the same `SINGLE_LINE_CARD` as the "N tool calls" and thought cards there (`boxed_card_heading`, `boxed_thinking_card`): its shell, height, inset and title type, the pencil in the card's tile, the +N −N counts and the check where those cards keep their pill; opened, its files sit under the card's hairline with no rail, as the tool card's calls do. A finished turn's "N files changed" fold stays a row like the log rows beside it. The phone matches (`FileChanges.tsx`).
    #[allow(clippy::too_many_arguments)]
    fn files_fold_row(
        &mut self,
        key: &str,
        stack_id: &str,
        label: String,
        summary: &Value,
        files: &[Value],
        expanded: bool,
        boxed: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let palette = Palette::of(p);
        let motion = self.disclosure_frame(key, expanded, cx);
        let body_shown = expanded || motion.is_some();
        let any_running = files.iter().any(|file| file_call_status(file) == "running");
        let any_failed = files.iter().any(|file| file_call_status(file) == "failed");
        let all_failed =
            !files.is_empty() && files.iter().all(|file| file_call_status(file) == "failed");
        let shimmer = any_running && self.snapshot["working"] == true && !cx.reduce_motion();
        let added = summary["added"].as_u64().unwrap_or(0);
        let removed = summary["removed"].as_u64().unwrap_or(0);
        let toggle_key = key.to_string();
        let shape = &*SINGLE_LINE_CARD;
        let trigger = div()
            .id(SharedString::from(key.to_string()))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .when(all_failed, |this| this.aria_description("failed"))
            .flex()
            .items_center()
            .min_w_0()
            .w_full()
            .when(all_failed, |this| this.bg(p.error().opacity(0.06)))
            .chat_cursor_pointer();
        let trigger = if boxed {
            let hover = card_hover_fill(p);
            let title_el = if shimmer {
                self.running_shimmer(
                    SharedString::from(format!("{key}:shimmer")),
                    shape.title(div().flex_1().min_w_0().truncate(), p),
                    label,
                    p,
                )
            } else {
                shape
                    .title(div().flex_1().min_w_0().truncate(), p)
                    .when(all_failed, |this| this.text_color(p.error()))
                    .child(self.find_label(key, label))
                    .into_any_element()
            };
            boxed_header_corners(
                trigger
                    .gap(px(TITLE_ROW_GAP * s))
                    .px(px(shape.padding_x * s))
                    .h(px(shape.header_height(s)))
                    .text_color(p.foreground)
                    .hover(move |style| style.bg(hover)),
                body_shown,
                p,
            )
            .child(shape.tile("titlebar/pencil.svg", p))
            .child(title_el)
        } else {
            let title_el = if shimmer {
                self.running_shimmer(
                    SharedString::from(format!("{key}:shimmer")),
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::NORMAL)
                        .text_size(px(PROSE_SIZE * s)),
                    label,
                    p,
                )
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::NORMAL)
                    .text_size(px(PROSE_SIZE * s))
                    .text_color(if all_failed {
                        p.error()
                    } else {
                        p.prose.opacity(DESCRIBED_LABEL_OPACITY)
                    })
                    .child(self.find_label(key, label))
                    .into_any_element()
            };
            trigger
                .gap(px(8.0 * s))
                .px(px(ROW_PAD_X * s))
                .py(px(5.0 * s))
                .rounded(px(8.0 * s))
                .text_size(px(PROSE_SIZE * s))
                .hover(|style| style.bg(p.foreground.opacity(0.03)))
                .child(
                    div()
                        .size(px(TILE * s))
                        .rounded(px(6.0 * s))
                        .bg(p.foreground.opacity(0.05))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .child(icon("titlebar/pencil.svg", 13.0 * s, p.muted)),
                )
                .child(title_el)
        };
        let status_mark = if any_running {
            None
        } else if any_failed {
            Some(("titlebar/x.svg", p.error()))
        } else {
            Some(("titlebar/check.svg", p.success()))
        };
        let trigger = trigger
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(toggle_key.clone(), expanded, cx);
            }))
            .when(added + removed > 0, |this| {
                this.child(counts(added, removed, &palette, s))
            })
            .when_some(status_mark, |this, (path, color)| {
                this.child(icon(path, 13.0 * s, color))
            });
        let review_paths: Vec<String> = summary["paths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|path| path.as_str().map(str::to_string))
            .collect();

        if boxed {
            let mut parts = vec![trigger.into_any_element()];
            if body_shown {
                let entry_rows: Vec<AnyElement> = files
                    .iter()
                    .enumerate()
                    .map(|(index, file)| {
                        self.file_fold_entry_row(
                            stack_id,
                            index,
                            file,
                            &review_paths,
                            &palette,
                            p,
                            cx,
                        )
                    })
                    .collect();
                let body = boxed_card_body(entry_rows, p);
                parts.push(self.disclosure_body_motion(key, motion, 0.0, body));
            }
            let column = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .children(parts);
            return shape.shell(column, p).into_any_element();
        }

        let mut group = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(4.0 * s))
            .child(trigger);
        if body_shown {
            let entry_rows: Vec<AnyElement> = files
                .iter()
                .enumerate()
                .map(|(index, file)| {
                    self.file_fold_entry_row(stack_id, index, file, &review_paths, &palette, p, cx)
                })
                .collect();
            let rail = disclosure_body(
                p,
                DisclosureRail::Marker,
                2.0,
                key.to_string(),
                "Collapse edited files",
                entry_rows,
                cx,
            );
            // Align the rail directly under the middle of the 22px pencil tile, as `tool_detail_body` does.
            let body = div()
                .pl(px((ROW_PAD_X + TILE / 2.0 - 10.0) * s))
                .min_w_0()
                .child(rail)
                .into_any_element();
            group = group.child(self.disclosure_body_motion(key, motion, 4.0 * s, body));
        }
        group.into_any_element()
    }

    /// One file row inside an expanded "Edited N files" fold, matching the tool row style and
    /// opening onto its diff on a rail.
    #[allow(clippy::too_many_arguments)]
    fn file_fold_entry_row(
        &mut self,
        stack_id: &str,
        index: usize,
        file: &Value,
        review_paths: &[String],
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let place = file["disclosureIndex"].as_u64().unwrap_or(index as u64);
        let key = format!("file:{stack_id}:{place}");
        let open = self.is_expanded(&key, p.file_previews || file["awaitingApproval"] == true);
        let motion = self.disclosure_frame(&key, open, cx);
        let status = file["status"].as_str().unwrap_or("edited");
        let glyph_color = match status {
            "new" => palette.new,
            "deleted" => palette.deleted,
            _ => p.muted,
        };
        let call_status = file_call_status(file);
        let failed = call_status == "failed";
        let running = call_status == "running";
        let shimmer = running && self.snapshot["working"] == true && !cx.reduce_motion();
        let added = file["added"].as_u64().unwrap_or(0);
        let removed = file["removed"].as_u64().unwrap_or(0);
        let filename = text(file, "filename");
        let parent = text(file, "parent");
        let menu_path = text(file, "path");
        let toggle_key = key.clone();
        let filename_el = if shimmer {
            self.running_shimmer(
                SharedString::from(format!("{key}:shimmer")),
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .font_weight(FontWeight::NORMAL)
                    .text_size(px(PROSE_SIZE * s)),
                filename.clone(),
                p,
            )
        } else {
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .font_weight(FontWeight::NORMAL)
                .text_size(px(PROSE_SIZE * s))
                .text_color(if failed {
                    p.error()
                } else {
                    p.prose.opacity(DESCRIBED_LABEL_OPACITY)
                })
                .child(filename.clone())
                .into_any_element()
        };
        let status_mark = match call_status {
            "done" => Some(("titlebar/check.svg", p.success())),
            "failed" => Some(("titlebar/x.svg", p.error())),
            _ => None,
        };
        let header = div()
            .id(SharedString::from(format!("file-row:{key}")))
            .role(gpui::Role::Button)
            .aria_label(if parent.is_empty() {
                filename.clone()
            } else {
                format!("{parent}{filename}")
            })
            .aria_expanded(open)
            .when(failed, |this| this.aria_description("failed"))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(ROW_PAD_X * s))
            .py(px(5.0 * s))
            .min_w_0()
            .w_full()
            .rounded(px(8.0 * s))
            .text_size(px(PROSE_SIZE * s))
            .when(failed, |this| this.bg(p.error().opacity(0.06)))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.03)))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_marker_disclosure(toggle_key.clone(), open, cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.file_secondary_press(menu_path.clone(), event, window, cx);
                }),
            )
            .child(
                div()
                    .size(px(TILE * s))
                    .rounded(px(6.0 * s))
                    .bg(p.foreground.opacity(0.05))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(icon(status_icon(status), 13.0 * s, glyph_color)),
            )
            .child(filename_el)
            .when(!parent.is_empty(), |this| {
                this.child(folder_label(parent, p.muted, 12.5 * s))
            })
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
            .when(added + removed > 0, |this| {
                this.child(counts(added, removed, palette, s))
            })
            .when_some(status_mark, |this, (path, color)| {
                this.child(icon(path, 13.0 * s, color))
            });
        let mut row = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(4.0 * s))
            .child(header);
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
        let diff = self.file_change_diff(
            &key,
            file,
            &detail,
            Some(review_paths.to_vec()),
            palette,
            false,
            p,
            cx,
        );
        let framed = div()
            .w_full()
            .min_w_0()
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(palette.border)
            .overflow_hidden()
            .child(diff)
            .into_any_element();
        let rail = disclosure_body(
            p,
            DisclosureRail::ToolDetail,
            4.0,
            key.clone(),
            format!("Collapse {filename}"),
            [framed],
            cx,
        );
        let body = div()
            .pl(px((ROW_PAD_X + TILE / 2.0 - 10.0) * s))
            .min_w_0()
            .child(rail)
            .into_any_element();
        row = row.child(self.disclosure_body_motion(&key, motion, 4.0 * s, body));
        row.into_any_element()
    }

    /// One card: the header, then a row per file.
    pub(super) fn file_change_card(
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
        // Agent work Quiet draws only the edit waiting on an approval, which keeps the key of its
        // place in the message (gx-chat-core `grouped_work::only_what_waits`).
        let place = file["disclosureIndex"].as_u64().unwrap_or(index as u64);
        let key = format!("file:{stack_id}:{place}");
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
            .child(
                super::disclosure_motion::turned_chevron(self.fold_turn(&key, open, cx), true)
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
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
        let body = self.file_change_diff(&key, file, &detail, None, palette, true, p, cx);
        row = row.child(self.disclosure_body_motion(&key, motion, 0.0, body));
        row.into_any_element()
    }

    /// An open row's diff on its darker well, with the buttons under it.
    /// `review`, when given, adds Review all for those paths to the footer.
    #[allow(clippy::too_many_arguments)]
    fn file_change_diff(
        &mut self,
        key: &str,
        file: &Value,
        detail: &Value,
        review: Option<Vec<String>>,
        palette: &Palette,
        border_top: bool,
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
            .when(border_top, |this| {
                this.border_t_1().border_color(palette.hairline)
            })
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
            })
            .when_some(review.filter(|paths| !paths.is_empty()), |this, paths| {
                this.child(
                    ghost(
                        format!("file-review-all:{key}"),
                        "titlebar/git-compare.svg",
                        "Review all".to_string(),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        view.invoke(json!({"type":"reviewFileChanges","paths":paths}), cx);
                    })),
                )
            });
        well.child(footer).into_any_element()
    }
}
