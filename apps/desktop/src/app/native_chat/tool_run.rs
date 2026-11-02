//! The run of tool rows under a message: a per-tool glyph, the one-line
//! preview, the "+N previous tool calls" fold, failed results in the error
//! tone, and the subagent heading. Which rows a collapsed run keeps and what
//! the fold's labels say are decided in
//! `packages/gx-chat-core/src/transcript/tool_rows.rs`; this file only lays them out.

use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::disclosure_motion::motion_clip_trailing;
use super::file_actions::FileMenuExt as _;
use super::{
    appearance::ChatAppearance, fonts::CHAT_MONO, state::NativeChatView, transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
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

/// A tool row's padding beside its glyph tile; its detail's rail is moved in by the same
/// amount so the line still runs down from the middle of the tile.
pub(super) const ROW_PAD_X: f32 = 8.0;
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
    pub(super) fn tool_rows(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        // Settings > Chat > Tool call cards: the calls and edits are one card (`tool_card.rs`).
        if message["toolCard"].is_object() {
            return self.tool_card(message, p, cx).into_iter().collect();
        }
        let tools = message["tools"].as_array().cloned().unwrap_or_default();
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
        if p.simple && !show_all {
            let key = format!("tools:{id}");
            // The run disclosure opens on demand and is never opened by verbose mode, as React's was;
            // only a call waiting on an approval opens it (gx-chat-core `approval_wait.rs`).
            let awaiting = tools.iter().any(|tool| tool["awaitingApproval"] == true);
            let expanded = self.is_expanded(&key, awaiting);
            let motion = self.disclosure_frame(&key, expanded, cx);
            let mut rows = vec![self.disclosure(
                key.clone(),
                text(message, "simpleToolLabel"),
                expanded,
                None,
                p,
                cx,
            )];
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
        // The toggle sits right before the last call, open or closed, so what it reveals lands
        // above it (gx-chat-core `tool_run_fold`, `toggleBefore`).
        let before = fold["toggleBefore"].as_u64().unwrap_or(u64::MAX) as usize;
        let split = |indices: &[usize]| -> (Vec<usize>, Vec<usize>) {
            indices.iter().copied().partition(|index| *index < before)
        };
        // While the group opens or closes, the rows it hides ease in above the ones it always
        // shows, on the rail the open group has; the rail itself is the one part that appears whole.
        if let Some(frame) = self.disclosure_frame(&run_key, run_expanded, cx) {
            let (kept, hidden): (Vec<usize>, Vec<usize>) = visible
                .iter()
                .copied()
                .partition(|index| fold["visible"][*index] == true);
            let mut body: Vec<AnyElement> = Vec::new();
            let hidden_rows = self.tool_row_list(&id, &tools, &hidden, p, cx);
            if !hidden_rows.is_empty() {
                body.push(motion_clip_trailing(
                    self.disclosure_height(&run_key),
                    frame,
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
            let (kept_above, kept_below) = split(&kept);
            body.extend(self.tool_row_list(&id, &tools, &kept_above, p, cx));
            body.push(self.tool_fold_toggle(run_key.clone(), label, run_expanded, p, cx));
            body.extend(self.tool_row_list(&id, &tools, &kept_below, p, cx));
            return vec![disclosure_body(
                p,
                DisclosureRail::Marker,
                ROW_GAP,
                run_key,
                "Show fewer tool calls",
                body,
                cx,
            )];
        }
        if run_expanded {
            let (above, below) = split(&visible);
            let mut body = self.tool_row_list(&id, &tools, &above, p, cx);
            body.push(self.tool_fold_toggle(run_key.clone(), label, true, p, cx));
            body.extend(self.tool_row_list(&id, &tools, &below, p, cx));
            return vec![disclosure_body(
                p,
                DisclosureRail::Marker,
                ROW_GAP,
                run_key,
                "Show fewer tool calls",
                body,
                cx,
            )];
        }
        let kept: Vec<usize> = visible
            .into_iter()
            .filter(|index| fold["visible"][*index] == true)
            .collect();
        let (above, below) = split(&kept);
        let mut rows = self.tool_row_list(&id, &tools, &above, p, cx);
        rows.push(self.tool_fold_toggle(run_key, label, false, p, cx));
        rows.extend(self.tool_row_list(&id, &tools, &below, p, cx));
        rows
    }

    /// A `!` command the user ran (`shellCard`, gx-chat-core's `shell_card`), in the user's bubble:
    /// a header with the terminal glyph, the command and its failure status, opening onto the
    /// output alone; its copy button floats outside the bubble, on its left. It shows in simple mode too, since it is what the user typed. It folds
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
        let bubble = |content: AnyElement| {
            div()
                .max_w(gpui::relative(0.8))
                .min_w_0()
                .rounded(px(16.0 * s))
                .bg(p.input)
                .child(content)
        };
        // The copy button floats outside the bubble on its left, the message action of an ordinary
        // prompt, shown while the row is hovered.
        let copy = self.shell_command_copy_action(message, &full_command, p, cx);
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .items_end()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0 * s))
                    .w_full()
                    .min_w_0()
                    .child(copy)
                    .child(
                        bubble(header.into_any_element())
                            .px(px(12.0 * s))
                            .py(px(8.0 * s)),
                    ),
            );
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
                        p,
                    ));
                }
            }
            let output = div()
                .flex()
                .flex_col()
                .items_end()
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

    fn tool_fold_toggle(
        &self,
        key: String,
        label: String,
        expanded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(key.clone())
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(ROW_PAD_X * s))
            .py(px(2.0 * s))
            .rounded(px(8.0 * s))
            .text_size(px(13.5 * s))
            .text_color(p.muted)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.03)))
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
            .text_size(px(13.5 * s))
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
        trigger = if !preview.is_empty() && subagent.is_empty() {
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
        // The picture the call opened from disk, under its label (gx-chat-core `read_images.rs`).
        if let Some(pictures) =
            self.read_pictures(&key, &tool["images"], ROW_PAD_X + TILE + 8.0, p, cx)
        {
            row = row.child(pictures);
        }
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
        let output = text(&detail, "output");
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
                p,
            ));
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
                        // React capped the block at 16.25 of its own lines plus its padding.
                        .max_h(px(220.75 * s))
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
