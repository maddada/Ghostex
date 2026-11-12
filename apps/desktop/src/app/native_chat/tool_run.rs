//! The run of tool rows under a message: a per-tool glyph, the one-line
//! preview, the "+N previous tool calls" fold, failed results in the error
//! tone, and the subagent heading. Which rows a collapsed run keeps and what
//! the fold's labels say are decided in
//! `packages/gx-chat-core/src/transcript/tool_rows.rs`; this file only lays them out.

use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::disclosure_motion::motion_clip;
use super::file_actions::FileMenuExt as _;
use super::quiet_work::{QuietLine, QuietMark, finished_mark};
use super::{
    appearance::ChatAppearance, fonts::CHAT_MONO, state::NativeChatView, transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::transcript::chat_look::UserStyle;
use ghostex_gx_chat_core::transcript::tool_rows::tool_fold_toggle_before;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled as _, StyledText, div,
    px,
};
use gpui_component::text::{TextView, TextViewStyle};
use serde_json::Value;

/// The spacing between the rows of a run, kept the same inside an expansion as
/// it is outside one so opening a group never re-flows the rows it reveals. Each
/// row carries its own padding (the 2026-10-11 work look), so the gap is small.
const ROW_GAP: f32 = 2.0;

/// How many lines of a tool's output Tool call lines show before "Show all N lines".
const LINE_OUTPUT_LINES: usize = 12;

/// A tool row's padding beside its glyph tile; its detail's rail is moved in by the same
/// amount so the line still runs down from the middle of the tile.
pub(super) const ROW_PAD_X: f32 = 8.0;
/// The space above and below the plain rows in an opened Quiet View line's card, so one call is
/// the "Message to" card's 44px with its border.
const BOXED_PAD_Y: f32 = 5.0;
/// The glyph tile's side.
pub(super) const TILE: f32 = 22.0;
/// A described tool row's label: the agent's prose colour, 20% dimmer.
/// SEE-ALSO: apps/mobile/app/src/chat/native/transcript/ToolRows.tsx (`DESCRIBED_LABEL_OPACITY`).
pub(super) const DESCRIBED_LABEL_OPACITY: f32 = 0.8;

/// The bundled icon for each glyph `sessionChatToolGlyph` classifies a tool into.
fn glyph_icon(glyph: &str) -> &'static str {
    match glyph {
        "edit" => "titlebar/pencil.svg",
        "file" => "titlebar/file-text.svg",
        "terminal" => "titlebar/terminal-2.svg",
        "web" => "titlebar/world.svg",
        _ => "titlebar/tool.svg",
    }
}

impl NativeChatView {
    /// A message's tool calls: its tool card, or its plain rows. Inside an opened Quiet View line
    /// (`quiet_boxed`) the plain rows sit in the "Message to" card's shell, as its tool card does
    /// (tool_card.rs, CDXC:SessionChat 2026-11-12), with their words at the card's 12px inset and
    /// one call as tall as that card.
    pub(super) fn tool_rows(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let card = message["toolCard"].is_object()
            && !(p.tool_lines && message["toolCard"]["background"] == true);
        let rows = self.unboxed_tool_rows(message, p, cx);
        if !self.quiet_boxed_here() || card || rows.is_empty() {
            return rows;
        }
        let shape = &*super::cards::SINGLE_LINE_CARD;
        let s = p.scale;
        let column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .justify_center()
            // Never shorter than the "Message to" card, a folded "2 tool calls" heading too.
            .min_h(px(shape.height * s))
            .gap(px(ROW_GAP * s))
            .px(px((shape.padding_x - ROW_PAD_X) * s))
            .py(px(BOXED_PAD_Y * s))
            .children(rows);
        vec![shape.shell(column, p).into_any_element()]
    }

    fn unboxed_tool_rows(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        // Settings > Chat > Tool call cards: the calls and edits are one card (`tool_card.rs`).
        // Tool call lines leave a background task's run as these rows (the core's `background`).
        if message["toolCard"].is_object()
            && !(p.tool_lines && message["toolCard"]["background"] == true)
        {
            return self.tool_card(message, p, cx).into_iter().collect();
        }
        let tools = message["tools"].as_array().map_or(&[][..], Vec::as_slice);
        if tools.is_empty() {
            return Vec::new();
        }
        let id = text(message, "id");
        // The run already sits under a heading that collapses it (a reasoning row, an assistant
        // commentary heading), so it shows every row and adds no group of its own: React passed
        // `showAllRows` at exactly those call sites, and the core carries the rule in tool_rows.rs.
        let show_all = message["toolsShowAllRows"] == true;
        // An answered question renders as its own exchange card, and a message sent to another
        // agent as its own message card. Each stays a plain row only where its card already shows
        // somewhere else, which was React's `questionPairsAsRows` (now the core's
        // questions/hoisting.rs).
        let cards_as_rows = show_all || self.in_work_fold;
        let visible: Vec<usize> = (0..tools.len())
            .filter(|index| {
                cards_as_rows
                    || (tools[*index]["exchange"] != true
                        && tools[*index]["sentMessage"] != true
                        && !self.has_workflow_card(&tools[*index]))
            })
            .collect();
        if visible.is_empty() {
            return Vec::new();
        }
        // An opened quiet line lists each call as one of its steps (quiet_work.rs), one column with
        // no gap between them, as the mockup and the phone draw them; as separate parts of the
        // message they would sit the message's part gap apart.
        if self.quiet_steps {
            let steps: Vec<AnyElement> = visible
                .iter()
                .map(|index| self.quiet_step_row(&id, *index, &tools[*index], p, cx))
                .collect();
            return vec![
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .children(steps)
                    .into_any_element(),
            ];
        }
        // Agent work Reduced or Quiet: the run is one live quiet line in every view, open in
        // Detailed (quiet_work.rs).
        let quiet = p.quiet_style();
        if (p.simple || quiet) && !show_all {
            // Under kept words the line has a key of its own, apart from their chevron's
            // (gx-chat-core `grouped_work::only_what_waits`).
            let key = message["toolsLineKey"]
                .as_str()
                .map_or_else(|| format!("tools:{id}"), str::to_string);
            // The run disclosure opens on demand and is never opened by verbose mode, as React's was;
            // only a call waiting on an approval opens it (gx-chat-core `approval_wait.rs`).
            let awaiting = tools.iter().any(|tool| tool["awaitingApproval"] == true);
            let expanded = self.is_expanded(&key, awaiting || (quiet && p.work_open));
            let motion = self.disclosure_frame(&key, expanded, cx);
            let heading = if quiet {
                let running = tools.iter().any(|tool| tool["status"] == "running");
                let label = text(message, "simpleToolLabel");
                self.quiet_work_line(
                    QuietLine {
                        key: key.clone(),
                        segments: vec![(
                            if running {
                                format!("Working \u{b7} {label}\u{2026}")
                            } else {
                                label
                            },
                            None,
                        )],
                        mark: if running {
                            QuietMark::Running
                        } else {
                            finished_mark([message])
                        },
                        expanded,
                        expandable: true,
                        actions: Vec::new(),
                    },
                    p,
                    cx,
                )
            } else {
                self.disclosure(
                    key.clone(),
                    text(message, "simpleToolLabel"),
                    expanded,
                    None,
                    p,
                    cx,
                )
            };
            let mut rows = vec![heading];
            if expanded || motion.is_some() {
                let body = self.tool_row_list(&id, &tools, &visible, p, cx);
                let body = disclosure_body(
                    p,
                    DisclosureRail::Marker,
                    ROW_GAP,
                    key.clone(),
                    "Collapse tool calls",
                    body,
                    cx,
                );
                rows.push(self.disclosure_body_motion(&key, motion, ROW_GAP * p.scale, body));
            } else if let Some(pictures) = self.read_picture_strip(
                &format!("{key}:read"),
                &message["readImages"],
                if quiet {
                    0.0
                } else {
                    super::read_pictures::FOLDED_CALLS_INSET
                },
                p.background,
                p,
                cx,
            ) {
                // The closed group keeps the pictures its calls read in view (gx-chat-core
                // `read_images.rs`).
                rows.push(pictures);
            }
            return rows;
        }
        let fold = message["toolFold"].clone();
        let hidden = fold["hiddenCount"].as_u64().unwrap_or(0);
        if show_all || hidden == 0 {
            return self.tool_row_list(&id, &tools, &visible, p, cx);
        }
        let run_key = format!("tool-run:{id}");
        let run_expanded = self.expanded.contains(&run_key);
        let label = text(
            &fold,
            if run_expanded {
                "expandedLabel"
            } else {
                "collapsedLabel"
            },
        );
        // Closed, the toggle sits right before the last call; open, it moves to the top of the
        // group and every call follows below it in order, so it is never between the calls it
        // reveals (gx-chat-core `tool_fold_toggle_before`).
        let split = |indices: &[usize], open: bool| -> (Vec<usize>, Vec<usize>) {
            let before = tool_fold_toggle_before(&fold, open);
            indices.iter().copied().partition(|index| *index < before)
        };
        // While the group opens or closes, the rows it hides ease in below the toggle, which stays
        // put, ahead of the ones it always shows, on the rail the open group has; the rail itself
        // is the one part that appears whole.
        if let Some(frame) = self.disclosure_frame(&run_key, run_expanded, cx) {
            let (kept, hidden): (Vec<usize>, Vec<usize>) = visible
                .iter()
                .copied()
                .partition(|index| fold["visible"][*index] == true);
            let mut body: Vec<AnyElement> =
                vec![self.tool_fold_toggle(run_key.clone(), label, run_expanded, None, p, cx)];
            let hidden_rows = self.tool_row_list(&id, &tools, &hidden, p, cx);
            if !hidden_rows.is_empty() {
                body.push(motion_clip(
                    self.disclosure_height(&run_key),
                    frame,
                    0.0,
                    ROW_GAP * p.scale,
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap(px(ROW_GAP * p.scale))
                        .children(hidden_rows)
                        .into_any_element(),
                ));
            }
            body.extend(self.tool_row_list(&id, &tools, &kept, p, cx));
            // The whole group is measured, toggle to last call, so a reveal shows the last
            // call's end and keeps the toggle on screen (disclosure_reveal.rs).
            let group = disclosure_body(
                p,
                DisclosureRail::Marker,
                ROW_GAP,
                run_key.clone(),
                "Show fewer tool calls",
                body,
                cx,
            );
            return vec![self.reveal_extent(&run_key, true, group)];
        }
        if run_expanded {
            let (above, below) = split(&visible, true);
            let mut body = self.tool_row_list(&id, &tools, &above, p, cx);
            body.push(self.tool_fold_toggle(run_key.clone(), label, true, None, p, cx));
            body.extend(self.tool_row_list(&id, &tools, &below, p, cx));
            let group = disclosure_body(
                p,
                DisclosureRail::Marker,
                ROW_GAP,
                run_key.clone(),
                "Show fewer tool calls",
                body,
                cx,
            );
            return vec![self.reveal_extent(&run_key, true, group)];
        }
        let kept: Vec<usize> = visible
            .into_iter()
            .filter(|index| fold["visible"][*index] == true)
            .collect();
        let (above, below) = split(&kept, false);
        // The pictures the folded calls read stay in view under the toggle, except one the
        // user's own message already shows, which only its open row keeps (gx-chat-core
        // `read_images.rs`, `is_user_shown`).
        let mut folded_pictures: Vec<Value> = Vec::new();
        for image in (0..tools.len())
            .filter(|index| fold["visible"][*index] != true)
            .filter_map(|index| tools[index]["images"].as_array())
            .flatten()
            .filter(|image| !ghostex_gx_chat_core::transcript::read_images::is_user_shown(image))
        {
            if !folded_pictures
                .iter()
                .any(|shown| shown.get("path") == image.get("path"))
            {
                folded_pictures.push(image.clone());
            }
        }
        let mut rows = self.tool_row_list(&id, &tools, &above, p, cx);
        let pictures = self.read_picture_strip(
            &format!("{run_key}:read"),
            &Value::Array(folded_pictures),
            super::read_pictures::FOLDED_CALLS_INSET,
            p.background,
            p,
            cx,
        );
        // In an opened Quiet View line's card the toggle is the card's header wherever it
        // touches the card's edges (`tool_fold_toggle`).
        let bleed = self
            .quiet_boxed_here()
            .then(|| (above.is_empty(), pictures.is_none() && below.is_empty()));
        rows.push(self.tool_fold_toggle(run_key.clone(), label, false, bleed, p, cx));
        rows.extend(pictures);
        rows.extend(self.tool_row_list(&id, &tools, &below, p, cx));
        rows
    }

    /// A `!` command the user ran (`shellCard`, gx-chat-core's `shell_card`), in the user's bubble:
    /// a header with the terminal glyph, the command and its failure status, opening onto the
    /// output alone; its copy button floats beside the bubble (on its left at Default). It shows in simple mode too, since it is what the user typed. It folds
    /// like every disclosure: the header and the rail both flip it, and a running card that opened
    /// on its own records the close against that default.
    pub(super) fn shell_command_card(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let card = &message["shellCard"];
        let key = format!("shell:{}", text(message, "id"));
        let has_body = card["hasBody"] == true;
        let open = has_body && self.is_expanded(&key, card["openByDefault"] == true);
        let motion = self.disclosure_frame(&key, open, cx);
        let failed = card["failed"] == true;
        let command = text(card, "command");
        let full_command = text(card, "fullCommand");
        let status = text(card, "status");
        let toggle_key = key.clone();
        let tooltip = full_command.clone();
        let header = div()
            .id(SharedString::from(key.clone()))
            .role(gpui::Role::Button)
            .aria_label(format!("Shell command: {full_command}"))
            .aria_expanded(open)
            .when(failed, |this| this.aria_description("failed"))
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(6.0 * s))
            .rounded(px(4.0 * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .when(has_body, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border.opacity(0.4)))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_marker_disclosure(toggle_key.clone(), open, cx)
                    }))
            })
            .child(
                div()
                    .w(px(16.0 * s))
                    .ml(px(2.0 * s))
                    .h(px(22.75 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        gpui::svg()
                            .path(glyph_icon("terminal"))
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(CHAT_MONO)
                    .text_size(px(12.6 * s))
                    .text_color(if failed { p.error() } else { p.primary })
                    .child(command),
            )
            .when(has_body, |this| {
                this.child(
                    gpui::svg()
                        .path(if open {
                            "titlebar/chevron-down.svg"
                        } else {
                            "titlebar/chevron-right.svg"
                        })
                        .size(px(12.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
            })
            .child(div().flex_1())
            .when(!status.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.25 * s))
                        .text_color(if failed { p.error() } else { p.muted })
                        .child(status),
                )
            });
        // Under the chat look the card sits where your other messages do (user_message_look.rs):
        // Left aligned keeps the bubble at the left edge with Copy on its right, and Band draws the
        // header and its output across the column on the band's fill, Copy at the header's right end.
        let left = p.left_aligned();
        let band = p.chat_look.user_style == UserStyle::Band;
        let bubble = |content: AnyElement| {
            let frame = div().min_w_0().child(content);
            if band {
                frame.w_full().rounded(px(8.0 * s)).bg(p.user_band)
            } else {
                frame
                    .max_w(gpui::relative(0.8))
                    .rounded(px(16.0 * s))
                    .bg(p.input)
            }
        };
        // The copy button floats beside the bubble on its inner side, the message action of an
        // ordinary prompt, shown while the row is hovered; a `!` command keeps it at Compact too,
        // with no footer row (`has_user_bar`).
        let copy = self.shell_command_copy_action(message, &full_command, p, cx);
        let header_row = if band {
            bubble(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .w_full()
                    .min_w_0()
                    .child(header.flex_1())
                    .child(copy)
                    .into_any_element(),
            )
            .px(px(12.0 * s))
            .py(px(8.0 * s))
        } else {
            let header_bubble = bubble(header.into_any_element())
                .px(px(12.0 * s))
                .py(px(8.0 * s));
            let row = div()
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .w_full()
                .min_w_0();
            if left {
                row.justify_start().child(header_bubble).child(copy)
            } else {
                row.justify_end().child(copy).child(header_bubble)
            }
        };
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .when(!left, |column| column.items_end())
            .when(left, |column| column.items_start())
            .w_full()
            .min_w_0()
            .child(header_row);
        // The output opens in a bubble of its own below the header, which keeps its collapsed size;
        // the header is its toggle, so it carries no rail.
        if (open || motion.is_some()) && has_body {
            let mut blocks: Vec<AnyElement> = Vec::new();
            for (part, error) in [("commandBody", false), ("stdout", false), ("stderr", true)] {
                let content = text(card, part);
                if !content.trim().is_empty() {
                    blocks.push(self.tool_body(
                        format!("{part}:{key}"),
                        None,
                        content,
                        error,
                        None,
                        false,
                        p,
                    ));
                }
            }
            let output = div()
                .flex()
                .flex_col()
                .when(!left, |output| output.items_end())
                .when(left, |output| output.items_start())
                .w_full()
                .min_w_0()
                .child(
                    bubble(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap(px(4.0 * s))
                            .children(blocks)
                            .into_any_element(),
                    )
                    .px(px(2.0 * s))
                    .py(px(2.0 * s)),
                )
                .into_any_element();
            column = column.child(self.disclosure_body_motion(&key, motion, 6.0 * s, output));
        }
        column.into_any_element()
    }

    /// The tool's name and its mono preview as ONE line of text, two font runs.
    ///
    /// CDXC:SessionChat 2026-10-07 WHY: drawn as two text elements side by side on a centred row, the JetBrains Mono preview sat a pixel below the prose name ("Shell echo …"), at every row position and whatever the font metrics said, because each element places and snaps its own baseline. Runs of one line share one baseline, so the name and the command line up on every platform; the en space stands in for the old 6px gap.
    /// The name and the preview as one line of two runs; `name_marks` and `preview_marks` are the
    /// find's highlights in each (search.rs, `label_highlights`).
    fn name_and_preview(
        name: String,
        preview: String,
        heading: gpui::Hsla,
        name_weight: gpui::FontWeight,
        name_marks: &[(std::ops::Range<usize>, gpui::Hsla)],
        preview_marks: &[(std::ops::Range<usize>, gpui::Hsla)],
        p: &ChatAppearance,
    ) -> StyledText {
        let gap = "\u{2002}";
        let run = |len: usize, family: String, weight: gpui::FontWeight, color: gpui::Hsla| {
            let mut font = gpui::font(family);
            font.weight = weight;
            gpui::TextRun {
                len,
                font,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        };
        let runs = vec![
            run(name.len() + gap.len(), p.font.clone(), name_weight, heading),
            run(
                preview.len(),
                CHAT_MONO.to_string(),
                gpui::FontWeight::NORMAL,
                p.muted,
            ),
        ];
        let offset = name.len() + gap.len();
        let marks: Vec<_> = name_marks
            .iter()
            .cloned()
            .chain(
                preview_marks
                    .iter()
                    .map(|(range, color)| (range.start + offset..range.end + offset, *color)),
            )
            .collect();
        StyledText::new(format!("{name}{gap}{preview}")).with_runs(marked_runs(runs, &marks))
    }

    fn tool_row_list(
        &mut self,
        id: &str,
        tools: &[Value],
        indices: &[usize],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        indices
            .iter()
            .map(|index| self.tool_row(id, *index, &tools[*index], p, cx))
            .collect()
    }

    /// The "N tool calls" toggle of a folded group. `bleed` is set inside an opened Quiet View
    /// line's card (`tool_rows`), with whether the toggle is the card's first row and its last:
    /// there it reaches past the card's inset to the card's sides, and to its top or bottom edge
    /// where it is the first or last row, taking the card's rounded corners and the one-line
    /// cards' hover fill, so its hover lights the card's whole header. Its chevron and words stay
    /// where they were.
    ///
    /// CDXC:SessionChat 2026-11-12 DECISION:
    /// User, of the collapsed "3 tool calls" card whose hover lit only a strip inset inside it: "make hovering here show the hover effect on the whole card header, not just a small part like now". The phone has no hover there; its press feedback is the row's own.
    fn tool_fold_toggle(
        &self,
        key: String,
        label: String,
        expanded: bool,
        bleed: Option<(bool, bool)>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*super::cards::SINGLE_LINE_CARD;
        let toggle = div()
            .id(key.clone())
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .text_size(px(super::chat_text_look::PROSE_SIZE * s))
            .text_color(p.muted)
            .chat_cursor_pointer();
        let toggle = match bleed {
            None => toggle
                .px(px(ROW_PAD_X * s))
                .py(px(2.0 * s))
                .rounded(px(8.0 * s))
                .hover(|style| style.bg(p.foreground.opacity(0.03))),
            Some((first, last)) => {
                let inset = shape.padding_x - ROW_PAD_X;
                // Inside the shell's one-pixel border.
                let corner = px(((shape.radius - 1.0) * s).max(0.0));
                let edge = |at: bool| if at { BOXED_PAD_Y } else { 0.0 };
                toggle
                    .mx(px(-inset * s))
                    .px(px(shape.padding_x * s))
                    .mt(px(-edge(first) * s))
                    .mb(px(-edge(last) * s))
                    .pt(px((2.0 + edge(first)) * s))
                    .pb(px((2.0 + edge(last)) * s))
                    // Alone in the card it is the card's whole height inside its border.
                    .when(first && last, |this| this.h(px(shape.header_height(s))))
                    .when(first, |this| this.rounded_t(corner))
                    .when(last, |this| this.rounded_b(corner))
                    .hover(|style| style.bg(super::cards::card_hover_fill(p)))
            }
        };
        toggle
            .child(
                // The chevron sits in a slot as wide as the rows' glyph tiles, so the label
                // starts where their names do.
                div()
                    .w(px(TILE * s))
                    .h(px(22.75 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        gpui::svg()
                            .path(if expanded {
                                "titlebar/chevron-down.svg"
                            } else {
                                "titlebar/chevron-right.svg"
                            })
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    ),
            )
            .child(label)
            .on_click(cx.listener(move |view, _, _, cx| view.toggle_disclosure(&key, cx)))
            .into_any_element()
    }

    fn tool_row(
        &mut self,
        message_id: &str,
        index: usize,
        tool: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("tool:{message_id}:{index}");
        // A call waiting on an approval starts open so its command shows beside the card, and
        // folds back once it is answered unless the reader toggled it (gx-chat-core
        // `approval_wait.rs`).
        let expanded = self.is_expanded(&key, tool["awaitingApproval"] == true);
        let has_detail = tool["hasDetail"] == true;
        let motion = self.disclosure_frame(&key, expanded && has_detail, cx);
        let failed = tool["failed"] == true;
        // A call the agent described is named by that description in every mode, without its
        // command preview (gx-chat-core `tool_description`); opening the row shows the command.
        let described = !text(tool, "description").is_empty();
        let name = match text(tool, "label") {
            label if label.is_empty() => text(tool, "name"),
            label => label,
        };
        let preview = text(tool, "preview");
        let failure = text(tool, "failure");
        let duration = text(tool, "duration");
        let status = text(tool, "status");
        let subagent = text(&tool["subagent"], "name");
        // A description reads as the agent's own prose, not as a tool name: the reply's font, size
        // and weight, a fifth dimmer (`DESCRIBED_LABEL_OPACITY`).
        let heading = match (failed, described) {
            (true, _) => p.error(),
            // A background task stopped before it finished: the tool card's stopped look.
            _ if status == "stopped" => p.muted,
            (false, true) => p.prose.opacity(DESCRIBED_LABEL_OPACITY),
            (false, false) => p.foreground,
        };
        let name_weight = if described {
            gpui::FontWeight::NORMAL
        } else {
            gpui::FontWeight::MEDIUM
        };
        let toggle_key = key.clone();
        let file_path = text(tool, "filePath");
        let a11y_label = match (subagent.is_empty(), preview.is_empty()) {
            (true, true) => format!("Tool {name}"),
            (true, false) => format!("Tool {name}: {preview}"),
            (false, _) => format!("Tool {name} ({subagent}): {preview}"),
        };
        // The preview is the tool's argument, which simple mode and a description hide, followed
        // by what failed when it failed, which every mode shows.
        let preview = if p.simple || described {
            String::new()
        } else {
            preview
        };
        let preview = match (preview.is_empty(), failure.is_empty()) {
            (_, true) => preview,
            (true, false) => failure,
            (false, false) => format!("{preview} \u{b7} {failure}"),
        };
        let mut trigger = div()
            .id(key.clone())
            .file_menu((!file_path.is_empty()).then_some(file_path), cx)
            .role(gpui::Role::Button)
            .aria_label(a11y_label)
            .aria_expanded(expanded)
            .when(failed, |this| this.aria_description("failed"))
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(8.0 * s))
            .px(px(ROW_PAD_X * s))
            .py(px(5.0 * s))
            .rounded(px(8.0 * s))
            .text_size(px(super::chat_text_look::PROSE_SIZE * s))
            .when(failed, |this| this.bg(p.error().opacity(0.06)))
            .when(has_detail, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.03)))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_marker_disclosure(toggle_key.clone(), expanded, cx)
                    }))
            })
            .child(
                div()
                    .size(px(TILE * s))
                    .rounded(px(6.0 * s))
                    .bg(p.foreground.opacity(0.05))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        gpui::svg()
                            .path(glyph_icon(tool["glyph"].as_str().unwrap_or("tool")))
                            .size(px(13.0 * s))
                            .text_color(p.muted),
                    ),
            );
        // A subagent row shows the agent instead of the preview. The text takes the row's free
        // width and truncates, so the duration and the status sit at the right edge.
        let fills = subagent.is_empty();
        let name_key = key.clone();
        let preview_key = format!("tool-preview:{message_id}:{index}");
        // A call still running sweeps the Tool call cards' shimmer across its words, in every
        // look (the 2026-11-09 decision in gx-chat-core `transcript/tool_card.rs`).
        let shimmer =
            status == "running" && self.snapshot["working"] == true && !cx.reduce_motion();
        trigger = if shimmer {
            // Drawn over the row, not in it (row_motion.rs).
            trigger.child(
                self.running_shimmer(
                    SharedString::from(format!("{key}:shimmer")),
                    div()
                        .when(fills, |this| this.flex_1())
                        .min_w_0()
                        .truncate()
                        .font_weight(name_weight)
                        .when(described, |this| this.text_size(px(14.0 * s))),
                    name,
                    p,
                ),
            )
        } else if !preview.is_empty() && subagent.is_empty() {
            let name_marks = self.label_highlights(&name_key, &name);
            let preview_marks = self.label_highlights(&preview_key, &preview);
            trigger.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(Self::name_and_preview(
                        name,
                        preview,
                        heading,
                        name_weight,
                        &name_marks,
                        &preview_marks,
                        p,
                    )),
            )
        } else {
            trigger.child(
                div()
                    .when(fills, |this| this.flex_1())
                    .min_w_0()
                    .truncate()
                    .font_weight(name_weight)
                    .when(described, |this| this.text_size(px(14.0 * s)))
                    .text_color(heading)
                    .child(self.find_label(&name_key, name)),
            )
        };
        // The picture the call opened from disk, at the row's right before its duration
        // (gx-chat-core `read_images.rs`, `READ_TILE`).
        if let Some(pictures) = self.read_pictures(&key, &tool["images"], p, cx) {
            trigger = trigger.child(pictures);
        }
        if !duration.is_empty() {
            trigger = trigger.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * s))
                    .text_color(p.muted.opacity(0.75))
                    .child(duration),
            );
        }
        let status_icon = match status.as_str() {
            "done" => Some(("titlebar/check.svg", p.success())),
            "failed" => Some(("titlebar/x.svg", p.error())),
            "stopped" => Some(("titlebar/circle-dashed.svg", p.muted)),
            _ => None,
        };
        if let Some((icon, color)) = status_icon {
            trigger = trigger.child(
                gpui::svg()
                    .path(icon)
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            );
        }
        let mut row = div().flex().flex_col().min_w_0().w_full().gap(px(4.0 * s));
        row = row.child(if subagent.is_empty() {
            trigger.into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(8.0 * s))
                .child(trigger)
                // The chip opens that child's transcript, like React's `SessionChatSubagentLink`;
                // a selector pointing back at this conversation stays plain text.
                .child(match Self::subagent_open_command(&tool["subagent"]) {
                    Some(command) => self.subagent_link(
                        format!("subagent:{key}"),
                        subagent.clone(),
                        Some(
                            tool["subagent"]["agentType"]
                                .as_str()
                                .unwrap_or(&subagent)
                                .to_string(),
                        ),
                        command,
                        p,
                        cx,
                    ),
                    None => div()
                        .min_w_0()
                        .truncate()
                        .text_color(p.control_primary)
                        .child(subagent)
                        .into_any_element(),
                })
                .into_any_element()
        });
        if (expanded || motion.is_some()) && has_detail {
            if let Some(body) = self.tool_detail_body(&key, message_id, index, tool, p, cx) {
                row = row.child(self.disclosure_body_motion(&key, motion, 4.0 * s, body));
            }
        }
        row.into_any_element()
    }

    /// An open tool row's detail: the command or arguments and the result, each a labelled block,
    /// on the rail that hangs from the row's glyph tile. The tool-call card opens its rows onto
    /// the same detail (`tool_card.rs`).
    pub(super) fn tool_detail_body(
        &mut self,
        key: &str,
        message_id: &str,
        index: usize,
        tool: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let failed = tool["failed"] == true;
        let detail = self.row_detail(key, "tool", message_id, index as u64);
        let input = text(&detail, "input");
        let mut output = text(&detail, "output");
        // Tool call lines show a long output's first lines and a button for the rest, instead of a
        // box that scrolls inside the chat (Q1962 in gx-chat-core `transcript/tool_card.rs`).
        let show_all_key = format!("show-all:{key}");
        let output_lines = output.lines().count();
        let shown_all = self.expanded.contains(&show_all_key);
        let capped = p.tool_lines && output_lines > LINE_OUTPUT_LINES && !shown_all;
        if capped {
            output = output
                .lines()
                .take(LINE_OUTPUT_LINES)
                .collect::<Vec<_>>()
                .join("\n");
        }
        let has_call = tool["hasCall"] == true;
        let command = tool["glyph"] == "terminal";
        // The first block's label row carries the copy button, which copies the whole call.
        let mut copy = Some(text(&detail, "copyText")).filter(|copy| !copy.is_empty());
        let mut detail: Vec<AnyElement> = Vec::new();
        if !input.is_empty() {
            let label = if command {
                Some("Command")
            } else if !output.is_empty() {
                Some("Input")
            } else {
                None
            };
            detail.push(self.tool_body(
                format!("input:{key}"),
                label,
                input,
                false,
                copy.take(),
                false,
                p,
            ));
        }
        if !output.is_empty() {
            let label = has_call.then_some("Result");
            detail.push(self.tool_body(
                format!("output:{key}"),
                label,
                output,
                failed,
                copy.take(),
                // The 12 lines and the full output after "Show all" both show without a scroll box.
                p.tool_lines,
                p,
            ));
            if capped {
                let toggle = show_all_key.clone();
                detail.push(
                    div()
                        .id(SharedString::from(toggle.clone()))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Show all {output_lines} lines"))
                        .flex_none()
                        .self_start()
                        .px(px(10.0 * s))
                        .py(px(3.0 * s))
                        .rounded_full()
                        .border_1()
                        .border_color(p.border)
                        .text_size(px(12.0 * s))
                        .text_color(p.foreground)
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.04)))
                        .child(format!("Show all {output_lines} lines"))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.expanded.insert(toggle.clone());
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
        }
        if !detail.is_empty() {
            let body = disclosure_body(
                p,
                DisclosureRail::ToolDetail,
                8.0,
                key.to_string(),
                format!("Collapse {}", tool["name"].as_str().unwrap_or_default()),
                detail,
                cx,
            );
            // The rail hangs from the middle of the glyph tile, which the row's padding moved
            // right of the marker column the rail is measured from.
            let body = div()
                .pl(px((ROW_PAD_X + TILE / 2.0 - 10.0) * s))
                .min_w_0()
                .child(body)
                .into_any_element();
            return Some(body);
        }
        None
    }

    /// One labelled block of a tool's detail: its arguments, the command it ran,
    /// or what it reported back. React painted it as a plain monospaced `<pre>`
    /// (`.ghostex-chat-tool-body`), so this is verbatim text in a scroll-capped
    /// box, never a Markdown code block card: that would put a language header on
    /// output the agent never wrote as code. `copy` is the whole call's text for
    /// the copy button on the first block's label row.
    ///
    /// CDXC:SessionChat 2026-10-05 DECISION: "please let me select text here and copy". The text is a selectable `TextView` (one headerless fenced block, so it stays verbatim), the selection mechanism the transcript's prose already uses, so a drag selects it and Cmd/Ctrl+C or the transcript menu copies it.
    fn tool_body(
        &self,
        key: String,
        label: Option<&str>,
        content: String,
        failed: bool,
        copy: Option<String>,
        uncapped: bool,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let color = if failed { p.error() } else { p.muted };
        let header = (label.is_some() || copy.is_some()).then(|| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0 * s))
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.25 * s))
                        .text_color(p.muted)
                        .child(label.unwrap_or_default().to_string()),
                )
                .when_some(copy, |this, copy| {
                    this.child(super::code_block::action(
                        SharedString::from(format!("copy:{key}")),
                        "titlebar/copy.svg",
                        "Copy",
                        p,
                        move |cx| {
                            crate::app::helpers::gpui_copy_to_clipboard(
                                ClipboardItem::new_string(copy.clone()),
                                cx,
                            )
                        },
                    ))
                })
        });
        let mut style = TextViewStyle::default().default_cursor(true);
        // The box below is the frame; the block inside it adds no card of its own.
        style.code_block = StyleRefinement::default()
            .p(px(0.0))
            .bg(gpui::transparent_black())
            .font_family(CHAT_MONO)
            .text_size(px(12.6 * s))
            .line_height(px(20.5 * s))
            .text_color(color);
        let text_view = TextView::markdown(
            SharedString::from(format!("{key}:text")),
            verbatim_fence(&content),
        )
        // A row's first height must be its real one (`rich_markdown.rs`, `text_view`).
        .parse_synchronously(true)
        .selectable(true)
        .style(style)
        .min_w_0()
        .text_color(color);
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(4.0 * s))
            .children(header)
            .child(
                self.nested_scroll(
                    key,
                    div()
                        .min_w_0()
                        // React capped the block at 16.25 of its own lines plus its padding; Tool
                        // call lines cut the output at 12 lines instead (`LINE_OUTPUT_LINES`).
                        .when(!uncapped, |this| this.max_h(px(220.75 * s)))
                        .px(px(10.0 * s))
                        .py(px(8.0 * s))
                        .bg(p.input)
                        .rounded(px(6.0 * s))
                        .font_family(CHAT_MONO)
                        .text_size(px(12.6 * s))
                        .line_height(px(20.5 * s))
                        .text_color(color)
                        .child(text_view),
                ),
            )
            .into_any_element()
    }
}

/// `text` as one fenced Markdown block, so it renders and copies verbatim: the fence is longer
/// than any backtick run inside it, which no line of the text can then close.
fn verbatim_fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let newline = if text.ends_with('\n') { "" } else { "\n" };
    format!("{fence}\n{text}{newline}{fence}\n")
}

/// `runs` split at the find's highlights, each highlighted piece given its background.
fn marked_runs(
    runs: Vec<gpui::TextRun>,
    marks: &[(std::ops::Range<usize>, gpui::Hsla)],
) -> Vec<gpui::TextRun> {
    if marks.is_empty() {
        return runs;
    }
    let mut out = Vec::new();
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        let mut cuts = vec![start, end];
        for (range, _) in marks {
            for at in [range.start, range.end] {
                if at > start && at < end {
                    cuts.push(at);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let mut piece = run.clone();
            piece.len = pair[1] - pair[0];
            piece.background_color = marks
                .iter()
                .find(|(range, _)| range.start <= pair[0] && pair[1] <= range.end)
                .map(|(_, color)| *color);
            out.push(piece);
        }
        start = end;
    }
    out
}
