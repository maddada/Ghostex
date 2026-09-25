//! The file-change cards under a message: the Activity-rail design React painted
//! in `session-chat-file-change-card.tsx`, with the circle marker, the single
//! start-truncated path line, the +/- counts that toggle the diff, the left
//! rail, the failed-write result, and the footer toggle.
//!
//! The counts, the shortened path, and "can this open" come from
//! `packages/gx-chat-core/src/transcript/file_change_rows.rs`; this file only lays them out.

use super::disclosure_motion::measured;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::{Value, json};

/// The stack's frame (React's `.ghostex-chat-file-changes`): 10px rounded corners, 10px side and
/// 8px end padding, and 12px between files. The files carry the padding and the gap themselves.
const STACK_RADIUS: f32 = 10.0;
const STACK_PAD_X: f32 = 10.0;
const STACK_PAD_Y: f32 = 8.0;
const STACK_GAP: f32 = 12.0;

/// The card palette, mirroring the tokens in `session-chat-file-change-card.css`.
struct Palette {
    surface: Hsla,
    code: Hsla,
    border: Hsla,
    rail: Hsla,
    added: Hsla,
    removed: Hsla,
    added_row: Hsla,
    removed_row: Hsla,
}

impl Palette {
    fn of(p: &ChatAppearance) -> Self {
        Self {
            // CDXC:Theming 2026-09-22 DECISION:
            // User: the diff cards take from the theme colour too. The surfaces read the chat's
            // derived input fill and the lines are the muted tone at the opacities that give the
            // old #2b2b2e border and #747475 rail over the neutral dark chat.
            surface: p.input,
            code: p.input,
            border: if p.light {
                p.border
            } else {
                p.muted.opacity(0.2)
            },
            rail: if p.light {
                p.border
            } else {
                p.muted.opacity(0.7)
            },
            added: rgb(if p.light { 0x16803d } else { 0x94caaa }).into(),
            removed: rgb(if p.light { 0xc53030 } else { 0xe5a0a4 }).into(),
            added_row: Hsla::from(rgb(0x22c55e)).opacity(0.12),
            removed_row: Hsla::from(rgb(0xef4444)).opacity(0.12),
        }
    }
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
        let simple_expanded = self.expanded.contains(&simple_key);
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
            let stack = self.file_change_stack(&id, &files, p, cx);
            rows.push(self.disclosure_body_motion(&simple_key, motion, 8.0 * p.scale, stack));
            return rows;
        }
        rows.push(self.file_change_stack(&id, &files, p, cx));
        rows
    }

    /// The turn's writes, folded behind "N files changed" the way React grouped a finished turn
    /// (the decision in `session-chat-message-list/list.tsx`). The label and the rows are projected
    /// together in the core (`transcript/presentation.rs`), so the label counts the files drawn.
    pub(super) fn completed_files_fold(
        &mut self,
        item: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let files = item["files"].as_array().cloned().unwrap_or_default();
        // An older turn arrives collapsed with only the paths its unread work changed; the row
        // stays and reads that work on open, as React's list did (`list.tsx`).
        let unread = files.is_empty()
            && item["deferred"]["filePaths"]
                .as_array()
                .is_some_and(|paths| !paths.is_empty());
        if files.is_empty() && !unread {
            return None;
        }
        let id = text(item, "id");
        let key = format!("work-files:{id}");
        let expanded = self.is_expanded(&key, false);
        let motion = self.disclosure_frame(&key, expanded, cx);
        let load =
            unread.then(|| json!({"type":"loadWork","id":item["id"],"work":item["deferred"]}));
        let label = text(
            item,
            if p.simple {
                "simpleFilesLabel"
            } else {
                "filesLabel"
            },
        );
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
                Some(self.file_change_stack(&format!("work:{id}"), &files, p, cx))
            };
            if let Some(body) = body {
                group = group.child(self.disclosure_body_motion(&key, motion, 8.0 * p.scale, body));
            }
        }
        Some(group.into_any_element())
    }

    fn file_change_stack(
        &mut self,
        id: &str,
        files: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = Palette::of(p);
        let s = p.scale;
        let mut stack = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .rounded(px(STACK_RADIUS * s))
            .bg(palette.surface);
        let last = files.len().saturating_sub(1);
        for (index, file) in files.iter().enumerate() {
            stack =
                stack.child(self.file_change_card(id, index, index == last, file, &palette, p, cx));
        }
        stack.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn file_change_card(
        &mut self,
        message_id: &str,
        index: usize,
        last: bool,
        file: &Value,
        palette: &Palette,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("file:{message_id}:{index}");
        let expanded = self.expanded.contains(&key);
        let failed = file["failed"] == true;
        // React's rule with the half GPUI owns: previews already show everything a short change has.
        let can_expand = !p.file_previews || file["expandableWithPreviews"] == true;
        let motion = self.disclosure_frame(&key, expanded && can_expand, cx);
        // While the diff opens or closes it is drawn whole and clipped to the frame; with previews
        // on, the clip runs between the preview's height and the full diff's.
        let moving = motion.is_some();
        let show_body = expanded || p.file_previews || moving;
        let added = file["added"].as_u64().unwrap_or(0);
        let removed = file["removed"].as_u64().unwrap_or(0);
        let parent = text(file, "parent");

        let marker_group = format!("marker:{key}");
        let marker = div()
            .id(format!("marker:{key}"))
            .group(marker_group.clone())
            .size(px(17.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border(px(1.0))
            .border_color(p.muted.opacity(0.65))
            .when(can_expand, |this| this.chat_cursor_pointer())
            .child(
                div()
                    .size(px(8.0 * s))
                    .rounded_full()
                    .bg(palette.rail)
                    .when(can_expand, |this| {
                        this.group_hover(marker_group, |style| style.bg(gpui::white()))
                    }),
            );

        let open_path = text(file, "path");
        let name = div()
            .id(format!("path:{key}"))
            .flex()
            .min_w_0()
            .flex_shrink(1.0)
            .chat_cursor_pointer()
            .text_color(p.prose)
            .hover(|style| style.text_color(p.foreground))
            // React shortened only the folder half (`direction: rtl` on the parent, `flex: 0 0 auto`
            // on the name), so the file being written is always readable. The shared row hands over
            // the whole folder, the way React's card asked for it, and this gives up what is too wide.
            //
            // The folder gives up its head, never its tail: the separator before the filename stays
            // on screen. `truncate()` cannot do that here. It ellipsises from the end, which eats
            // the separator, and it decides from a per-character width sum that disagrees with the
            // kerned width the row was measured at, so a folder that fits was still being cut. The
            // text keeps its own width and the folder box clips what does not fit, from the left.
            .when(!parent.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .justify_end()
                        .min_w_0()
                        .flex_shrink(1.0)
                        .overflow_hidden()
                        .text_color(p.muted)
                        .child(div().flex_shrink_0().whitespace_nowrap().child(parent)),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .child(text(file, "filename")),
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                // The path opens the file; every other press on the card toggles the diff.
                cx.stop_propagation();
                view.invoke(
                    json!({"type":"openMarkdownLink","href":open_path,"external":false}),
                    cx,
                )
            }));

        let counts = div()
            .id(format!("counts:{key}"))
            .flex()
            .flex_shrink_0()
            .gap(px(6.0 * s))
            .ml_auto()
            .px(px(8.0 * s))
            .py(px(4.0 * s))
            .rounded(px(6.0 * s))
            .text_size(px(12.25 * s))
            .when(can_expand, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.muted.opacity(0.16)))
            })
            .child(div().text_color(palette.added).child(format!("+{added}")))
            .child(
                div()
                    .text_color(palette.removed)
                    .child(format!("\u{2212}{removed}")),
            );

        // One press target, as React's `<section onClick>` was: the marker, the counts, the rail,
        // the code, the footer and the empty space between them all toggle the diff from here, so
        // no two listeners can flip the same row twice. Only the path stops the press to open the file.
        let card_key = key.clone();
        // CDXC:SessionChat 2026-09-23 DECISION:
        // User: "when i hover over this kind of card, you're making the only middle part change
        // color, I want all of it to change color". Each file owns its share of the stack's padding
        // and of the gap to its neighbours, so its hover fill reaches the stack's edges and, for the
        // first and last file, its rounded corners: a single file lights the whole stack. The spacing
        // is the old 8px padding and 12px gap, so the layout does not move. Supersedes the
        // 2026-09-22 fill in a 6px inset.
        let hover_fill: Hsla = if p.light {
            gpui::white().opacity(0.6)
        } else {
            p.foreground.opacity(0.05)
        };
        let first = index == 0;
        let pad_top = if first { STACK_PAD_Y } else { STACK_GAP / 2.0 } * s;
        let pad_bottom = if last { STACK_PAD_Y } else { STACK_GAP / 2.0 } * s;
        let radius = px(STACK_RADIUS * s);
        let mut card = div()
            .id(format!("card:{key}"))
            .on_click(cx.listener(move |view, _, _, cx| {
                if can_expand {
                    view.toggle_disclosure(&card_key, cx);
                }
            }))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .relative()
            .px(px(STACK_PAD_X * s))
            .pt(px(pad_top))
            .pb(px(pad_bottom))
            .when(first, |this| this.rounded_t(radius))
            .when(last, |this| this.rounded_b(radius))
            .when(can_expand, |this| {
                this.hover(move |style| style.bg(hover_fill))
            })
            // The hairline React drew down the marker column joining one circle to the next
            // (`.ghostex-chat-file-change-card::before`); visual only, and it reaches into the
            // gap below every card but the last. A collapsed preview draws no rail through its code.
            .when(!show_body || expanded, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(STACK_PAD_X * s + 8.0 * s))
                        .top(px(pad_top + 22.0 * s))
                        // Down to the next file's first line, through the half gap it owns.
                        .bottom(px(if last {
                            pad_bottom
                        } else {
                            -STACK_GAP / 2.0 * s
                        }))
                        .w(px(1.0))
                        .bg(palette.rail),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .gap(px(12.0 * s))
                    .min_h(px(32.0 * s))
                    .child(marker)
                    .child(name)
                    .when(failed, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.25 * s))
                                .text_color(p.error())
                                .child("Failed"),
                        )
                    })
                    .child(counts),
            );
        if !show_body {
            return card.into_any_element();
        }
        let detail = self.row_detail(
            &key,
            "file",
            file["messageId"].as_str().unwrap_or_default(),
            file["index"].as_u64().unwrap_or_default(),
        );
        if detail.is_null() {
            return card.into_any_element();
        }
        let mut code = div()
            .flex()
            .flex_col()
            .min_w_0()
            .flex_1()
            .py(px(10.0 * s))
            .border_1()
            .border_color(palette.border)
            .rounded(px(11.0 * s))
            .bg(palette.code)
            .overflow_hidden();
        let lines: Vec<&Value> = detail["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|line| expanded || moving || line["kind"] != "meta")
            .take(if expanded || moving { usize::MAX } else { 7 })
            .collect();
        if lines.is_empty() {
            code = code.child(div().px(px(12.0 * s)).text_color(p.muted).child(
                if file["action"] == "Delete" {
                    "File removed"
                } else {
                    "Empty file"
                },
            ));
        }
        for line in lines {
            let (color, background, sign) = match line["kind"].as_str() {
                Some("add") => (palette.added, Some(palette.added_row), "+"),
                Some("del") => (palette.removed, Some(palette.removed_row), "-"),
                Some("meta") => (p.muted, None, " "),
                _ => (p.prose, None, " "),
            };
            code = code.child(
                div()
                    .flex()
                    .px(px(10.0 * s))
                    .when_some(background, |this, background| this.bg(background))
                    .font_family("JetBrainsMono Nerd Font")
                    .text_size(px(12.6 * s))
                    .text_color(color)
                    .child(
                        div()
                            .w(px(14.0 * s))
                            .flex_shrink_0()
                            .opacity(0.6)
                            .child(sign),
                    )
                    .child(div().min_w_0().child(text(line, "text"))),
            );
        }
        let mut detail_column = div().flex().flex_col().flex_1().min_w_0().child(code);
        if expanded && failed {
            detail_column = detail_column.child(
                div()
                    .mt(px(8.0 * s))
                    .min_w_0()
                    .text_color(p.error())
                    .child(text(file, "error")),
            );
        }
        if can_expand {
            detail_column = detail_column.child(
                div()
                    .flex()
                    .justify_end()
                    .w_full()
                    .pt(px(12.0 * s))
                    .pb(px(7.0 * s))
                    .child(
                        div()
                            .px(px(8.0 * s))
                            .py(px(4.0 * s))
                            .rounded(px(6.0 * s))
                            .text_size(px(12.25 * s))
                            .text_color(p.muted)
                            .chat_cursor_pointer()
                            .hover(|style| style.bg(p.muted.opacity(0.16)))
                            .child(if expanded {
                                "Collapse changes"
                            } else {
                                "Show all changes"
                            }),
                    ),
            );
        }
        let rail_group = format!("rail:{key}");
        let body =
            div()
                .flex()
                .w_full()
                .min_w_0()
                .mt(px(8.0 * s))
                .gap(px(12.0 * s))
                .child(
                    // The rail is the grab target React gave the open code, not an invisible
                    // overlay: a real column beside the code and the footer (React's
                    // `grid-row: 2 / 4`) whose line lights up white under the mouse.
                    div()
                        .group(rail_group.clone())
                        .w(px(17.0 * s))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .when(can_expand, |this| this.chat_cursor_pointer())
                        .when(expanded, |this| {
                            this.child(div().w(px(1.0)).h_full().bg(palette.rail).when(
                                can_expand,
                                |this| {
                                    this.group_hover(rail_group, |style| style.bg(gpui::white()))
                                },
                            ))
                        }),
                )
                .child(detail_column)
                .into_any_element();
        card = card.child(match motion {
            Some(frame) if p.file_previews => self.capped_body_motion(&key, frame, 0.0, body),
            Some(_) => self.disclosure_body_motion(&key, motion, 0.0, body),
            None if p.file_previews && !expanded && can_expand => {
                measured(self.disclosure_floor(&key), body)
            }
            None => body,
        });
        card.into_any_element()
    }
}
