//! The quiet style Agent work Reduced and Quiet draw the agent's work in (`chatAgentWorkStyle`,
//! gx-chat-core `transcript/chat_look.rs`): small dim lines instead of cards. A work line (clock,
//! green check, "Worked for 59s · 9 tool calls · edited 2 files +14 −3"), the running turn's live
//! lines ("Working · 3 tool calls…", an edit line with a pencil, "Edited 2 files +14 −3
//! migrate.ts, staging.yml") and, once a line is opened, its steps on a thin rail, one per call.
//! At Default nothing here runs.

use super::completed_work_row::{FoldedCards, WorkTail, spaced_tail};
use super::file_actions::FileMenuExt as _;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::TranscriptItem;
use ghostex_gx_chat_core::transcript::chat_look::AgentWork;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, StyledText,
    Window, div, px, svg,
};
use serde_json::{Value, json};
use std::time::Duration;

/// The space between a quiet line and the agent text that follows it.
pub(super) const QUIET_TEXT_GAP: f32 = 2.0;
/// The hover group of one quiet line, which lights its icons and shows its chevron.
const QUIET_GROUP: &str = "native-chat-quiet-line";
/// The lines' type: the chat's message size, on a 22px line.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User: "please make sure everything shown here has the same font size as regular user/agent chat messages not different font". The quiet lines, tool call group headers and rows, the one-line cards ("Worked for", "Message from", tool cards) and notices use the message size (`PROSE_SIZE`, scaled with the chat's zoom) and the chat's own font; secondary text is quieter through its tone. Only metadata at a line's end stays smaller: a message's time, a call's duration and time, a count pill or badge, a failed count. Code keeps its monospace at the inline code size (0.9 of the message size). Supersedes the 2026-11-07 12.5px lines, which were the builder's pick.
pub(super) const TEXT_SIZE: f32 = super::chat_text_look::PROSE_SIZE;
pub(super) const LINE_HEIGHT: f32 = 22.0;
/// A step's argument in the chat's monospace, at the inline code size.
const CODE_SIZE: f32 = super::chat_text_look::PROSE_SIZE * 0.9;
/// The space between a line's icons and its text.
pub(super) const ICON_GAP: f32 = 6.0;
/// How much of the muted text colour a line shows at rest; hovered, it shows all of it.
const DIM: f32 = 0.7;

impl ChatAppearance {
    /// Agent work is Reduced or Quiet: the work draws in the quiet style.
    pub(crate) fn quiet_style(&self) -> bool {
        self.chat_look.agent_work != AgentWork::Default
    }

    /// The colour of a quiet line at rest.
    pub(super) fn quiet_dim(&self) -> Hsla {
        self.muted.opacity(DIM)
    }
}

/// What a work line's first icon says after the clock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QuietMark {
    /// Every call answered: the green check.
    Done,
    /// A call still runs: the turning loader instead of the check.
    Running,
    /// A call failed: the tool rows' cross in the error tone instead of the check.
    Failed,
}

/// One piece of a line's text, in its own colour when it has one.
pub(super) type QuietSegment = (String, Option<Hsla>);

/// A work line: what it says and what pressing it does.
pub(super) struct QuietLine {
    /// The disclosure key the line opens and closes.
    pub key: String,
    /// "Worked for 59s", then " · 5 tool calls" and the rest.
    pub segments: Vec<QuietSegment>,
    pub mark: QuietMark,
    pub expanded: bool,
    /// Whether the line opens at all.
    pub expandable: bool,
    /// The reads to send when the line opens (`loadWork`, one per section still on the server).
    pub actions: Vec<Value>,
}

/// An edit line: "Edited 2 files", the line counts, then the files' names.
pub(super) struct QuietEdit {
    pub key: String,
    pub label: String,
    pub added: u64,
    pub removed: u64,
    pub names: Vec<String>,
    pub expanded: bool,
}

/// `segments` as one line of text, each coloured piece highlighted in its colour.
pub(super) fn styled_segments(segments: Vec<QuietSegment>) -> StyledText {
    let mut line = String::new();
    let mut colours = Vec::new();
    for (piece, colour) in segments {
        let start = line.len();
        line.push_str(&piece);
        if let Some(colour) = colour {
            colours.push((
                start..line.len(),
                HighlightStyle {
                    color: Some(colour),
                    ..Default::default()
                },
            ));
        }
    }
    StyledText::new(line).with_highlights(colours)
}

/// An icon of a quiet line, lit with the line on hover.
pub(super) fn line_icon(path: &'static str, size: f32, p: &ChatAppearance) -> AnyElement {
    svg()
        .path(path)
        .size(px(size * p.scale))
        .flex_shrink_0()
        .text_color(p.quiet_dim())
        .group_hover(QUIET_GROUP, |style| style.text_color(p.muted))
        .into_any_element()
}

/// The green check of a finished line or step.
pub(super) fn check(p: &ChatAppearance) -> AnyElement {
    svg()
        .path("titlebar/check.svg")
        .size(px(12.0 * p.scale))
        .flex_shrink_0()
        .text_color(p.success())
        .into_any_element()
}

/// The cross of a line or step whose call failed, as a failed tool row marks it.
fn cross(p: &ChatAppearance) -> AnyElement {
    svg()
        .path("titlebar/x.svg")
        .size(px(12.0 * p.scale))
        .flex_shrink_0()
        .text_color(p.error())
        .into_any_element()
}

/// The mark for calls that are all answered: the cross when one of `messages`' calls failed,
/// else the check.
pub(super) fn finished_mark<'a>(messages: impl IntoIterator<Item = &'a Value>) -> QuietMark {
    let failed = messages.into_iter().any(|message| {
        message["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["failed"] == true))
    });
    if failed {
        QuietMark::Failed
    } else {
        QuietMark::Done
    }
}

/// The turning loader of a line or step whose call still runs.
pub(super) fn loader(key: &str, p: &ChatAppearance, reduce_motion: bool) -> AnyElement {
    let glyph = svg()
        .path("titlebar/loader2.svg")
        .size(px(12.0 * p.scale))
        .flex_shrink_0()
        .text_color(p.muted);
    if reduce_motion {
        return glyph.into_any_element();
    }
    glyph
        .with_throttled_animation(
            SharedString::from(format!("{key}:loader")),
            Duration::from_secs(1),
            |glyph, progress| {
                glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(progress)))
            },
        )
        .into_any_element()
}

impl NativeChatView {
    /// The quiet style's work line. Agent work Reduced draws everything the agent did for one message of yours as one dim line (a clock, a green check, "Worked for 1m 16s · 9 tool calls · edited 2 files +14 −3"), with no fill, border or radius, and the agent's messages after it; opening the line lists its calls on a thin rail, one per line, and opening one shows today's detail. The running turn's "N tool calls" and "Edited N files" draw as live quiet lines under Reduced and Quiet. Detailed opens them.
    ///
    /// CDXC:SessionChat 2026-11-07 WHY:
    /// The 12.5px type of the lines is the builder's pick from the mockups, not the user's.
    ///
    /// CDXC:SessionChat 2026-11-07 SEE-ALSO:
    /// packages/gx-chat-core/src/transcript/chat_look.rs (the user decision), packages/gx-chat-core/src/transcript/grouped_work.rs (the Reduced line), apps/mobile/app/src/chat/native/transcript/quietWork.tsx (the phone draws the same lines).
    pub(super) fn quiet_work_line(
        &self,
        line: QuietLine,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mark = match line.mark {
            QuietMark::Done => check(p),
            QuietMark::Running => loader(&line.key, p, cx.reduce_motion()),
            QuietMark::Failed => cross(p),
        };
        let spoken = line
            .segments
            .iter()
            .map(|(piece, _)| piece.as_str())
            .collect::<String>();
        self.quiet_line(
            line.key,
            spoken,
            vec![line_icon("titlebar/clock.svg", 13.0, p), mark],
            styled_segments(line.segments),
            line.expanded,
            line.expandable,
            line.actions,
            p,
            cx,
        )
    }

    /// The edit line: a pencil, "Edited 2 files", `+14` and `−3` in the file card's hues, then the
    /// files' names.
    pub(super) fn quiet_edit_line(
        &self,
        edit: QuietEdit,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (_, _, _, _, added, removed) = super::file_change_card::card_colors(p);
        let mut segments: Vec<QuietSegment> = vec![(edit.label, None)];
        if edit.added > 0 || edit.removed > 0 {
            segments.push((" ".into(), None));
            segments.push((format!("+{}", edit.added), Some(added)));
            segments.push((" ".into(), None));
            segments.push((format!("\u{2212}{}", edit.removed), Some(removed)));
        }
        if !edit.names.is_empty() {
            segments.push((format!(" {}", edit.names.join(", ")), None));
        }
        let spoken = segments
            .iter()
            .map(|(piece, _)| piece.as_str())
            .collect::<String>();
        self.quiet_line(
            edit.key,
            spoken,
            vec![line_icon("titlebar/pencil.svg", 13.0, p)],
            styled_segments(segments),
            edit.expanded,
            true,
            Vec::new(),
            p,
            cx,
        )
    }

    /// One quiet line: its icons, its text truncated to the column, and the chevron that shows on
    /// hover or while it is open. The line is as wide as what it says, so its hover covers only it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn quiet_line(
        &self,
        key: String,
        spoken: String,
        icons: Vec<AnyElement>,
        label: StyledText,
        expanded: bool,
        expandable: bool,
        actions: Vec<Value>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let chevron = svg()
            .path(if expanded {
                "titlebar/chevron-down.svg"
            } else {
                "titlebar/chevron-right.svg"
            })
            .size(px(12.0 * s))
            .flex_shrink_0()
            .text_color(if expanded {
                p.quiet_dim()
            } else {
                gpui::transparent_black()
            })
            .group_hover(QUIET_GROUP, |style| style.text_color(p.muted));
        let line = div()
            .id(SharedString::from(format!("{key}:quiet")))
            .group(QUIET_GROUP)
            .role(gpui::Role::Button)
            .aria_label(spoken)
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(ICON_GAP * s))
            .pr(px(4.0 * s))
            .text_size(px(TEXT_SIZE * s))
            .line_height(px(LINE_HEIGHT * s))
            .text_color(p.quiet_dim())
            .children(icons)
            .child(div().min_w_0().truncate().child(label))
            .when(expandable, |line| {
                line.child(chevron)
                    .chat_cursor_pointer()
                    .hover(|style| style.text_color(p.muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_quiet_line(&key, expanded, &actions, cx);
                    }))
            });
        div()
            .flex()
            .w_full()
            .min_w_0()
            .child(line)
            .into_any_element()
    }

    /// Open or close a quiet line, recording the close against a default that opened it (Detailed)
    /// and sending its reads as it opens.
    pub(super) fn toggle_quiet_line(
        &mut self,
        key: &str,
        expanded: bool,
        actions: &[Value],
        cx: &mut Context<Self>,
    ) {
        self.anchor_disclosure_toggle(key, !expanded);
        if expanded {
            self.expanded.remove(key);
            self.collapsed.insert(key.to_string());
        } else {
            self.collapsed.remove(key);
            self.expanded.insert(key.to_string());
            for action in actions {
                self.invoke(action.clone(), cx);
            }
        }
        cx.notify();
    }

    /// An opened line's steps: a 1px rail at a tenth of the foreground, the rows 16px in from it;
    /// a click on the rail shuts the line `key` (disclosure_body.rs `closing_rail`).
    pub(super) fn quiet_steps(
        &self,
        key: &str,
        rows: Vec<AnyElement>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .pt(px(2.0 * s))
            .pb(px(2.0 * s))
            .min_w_0()
            .child(super::disclosure_body::closing_rail(
                p,
                key.to_string(),
                6.0,
                23.0,
                0.0,
                0.0,
                rows,
                cx,
            ))
            .into_any_element()
    }

    /// One call among an opened line's steps: its check (a cross when it failed, the loader while
    /// it runs), the tool's name in the muted colour, then its argument in mono. Pressing it opens
    /// today's detail for the call, under the same key its tool row uses.
    pub(super) fn quiet_step_row(
        &mut self,
        message_id: &str,
        index: usize,
        tool: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("tool:{message_id}:{index}");
        let expanded = self.is_expanded(&key, tool["awaitingApproval"] == true);
        let has_detail = tool["hasDetail"] == true;
        let motion = self.disclosure_frame(&key, expanded && has_detail, cx);
        let status = text(tool, "status");
        let mark = match status.as_str() {
            "failed" => cross(p),
            "stopped" => svg()
                .path("titlebar/circle-dashed.svg")
                .size(px(12.0 * s))
                .flex_shrink_0()
                .text_color(p.muted)
                .into_any_element(),
            "running" => loader(&key, p, cx.reduce_motion()),
            _ => check(p),
        };
        // The same text Find searches (gx-chat-core `quiet_step_text`).
        let (name, preview) =
            ghostex_gx_chat_core::extras::transcript_search::quiet_step_text(tool);
        let toggle_key = key.clone();
        let file_path = text(tool, "filePath");
        let mut trigger = div()
            .id(SharedString::from(format!("{key}:quiet-step")))
            .file_menu((!file_path.is_empty()).then_some(file_path), cx)
            .role(gpui::Role::Button)
            .aria_label(format!("Tool {name}: {preview}"))
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(8.0 * s))
            .text_size(px(TEXT_SIZE * s))
            .line_height(px(LINE_HEIGHT * s))
            .text_color(p.quiet_dim())
            .child(mark)
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if status == "failed" {
                        p.error()
                    } else {
                        p.muted
                    })
                    .child(self.find_label(&key, name)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(super::fonts::CHAT_MONO)
                    .text_size(px(CODE_SIZE * s))
                    .child(self.find_label(&format!("tool-preview:{message_id}:{index}"), preview)),
            )
            .when(has_detail, |row| {
                row.chat_cursor_pointer()
                    .hover(|style| style.text_color(p.muted))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_marker_disclosure(toggle_key.clone(), expanded, cx)
                    }))
            });
        // The picture the call opened from disk, at the step's right (gx-chat-core
        // `read_images.rs`): an opened line shows each step's own.
        if let Some(pictures) = self.read_pictures(&key, &tool["images"], p, cx) {
            trigger = trigger.child(pictures);
        }
        // A Task or Agent call names its subagent after the step, and the name opens that child's
        // transcript, as the full tool row does (tool_run.rs).
        let subagent = text(&tool["subagent"], "name");
        let trigger = if subagent.is_empty() {
            trigger.into_any_element()
        } else {
            let link = match Self::subagent_open_command(&tool["subagent"]) {
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
            };
            div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(8.0 * s))
                .text_size(px(TEXT_SIZE * s))
                .line_height(px(LINE_HEIGHT * s))
                .child(trigger)
                .child(link)
                .into_any_element()
        };
        let mut row = div().flex().flex_col().min_w_0().w_full().child(trigger);
        if (expanded || motion.is_some())
            && has_detail
            && let Some(body) = self.tool_detail_body(&key, message_id, index, tool, p, cx)
        {
            row = row.child(
                self.disclosure_body_motion(
                    &key,
                    motion,
                    4.0 * s,
                    div()
                        .pt(px(4.0 * s))
                        .pb(px(4.0 * s))
                        .child(body)
                        .into_any_element(),
                ),
            );
        }
        row.into_any_element()
    }
}

impl NativeChatView {
    /// A `grouped-work` row (Agent work Reduced, gx-chat-core `transcript/grouped_work.rs`): one
    /// quiet line for every turn the agent ran for one message of yours, "Worked for 1m 16s · 9
    /// tool calls · edited 2 files +14 −3", its steps when opened (and the merged turns' file card
    /// after them, the one place their diffs show), then each turn's tail as a finished turn draws
    /// it (`work_tail_parts`, completed_work_row.rs).
    pub(super) fn grouped_work_row(
        &mut self,
        item: &TranscriptItem,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Agent work Quiet: no line, the work in the reply's hover row (hidden_work.rs).
        if p.hidden_work() {
            return self.hidden_work_row(item, p, window, cx);
        }
        let TranscriptItem::GroupedWork {
            id,
            label,
            stats,
            files_card,
            files,
            simple_files_label,
            expandable,
            work,
            turns,
            read_images,
            cards_in_fold,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let key = format!("work:{id}");
        // Each merged turn's messages between agents and answers, folded into the line (gx-chat-core
        // `folded_cards.rs`): drawn at the top of its opened steps and never in the tails.
        let folded: Vec<FoldedCards<'_>> = turns
            .iter()
            .map(|turn| {
                FoldedCards::of(
                    *cards_in_fold,
                    &turn.artifacts,
                    &turn.questions,
                    &turn.sent_messages,
                )
            })
            .collect();
        let expanded = self.is_expanded(&key, p.work_open);
        // One read per section still on the server: several wake-up turns can share their
        // prompt's section, which is read once, through the turn that first holds it.
        let mut sections: Vec<(&str, &Value)> = Vec::new();
        for turn in turns {
            if let Some(section) = turn.deferred.as_ref().filter(|work| work.is_object())
                && !sections.iter().any(|(_, seen)| *seen == section)
            {
                sections.push((&turn.id, section));
            }
        }
        // The read is keyed by the section's prompt (`startId`), which every turn it holds looks
        // its rows up by (gx-chat-core `presentation.rs`), so a run that starts on a wake-up still
        // fills each of its turns.
        let sections: Vec<(&str, &Value)> = sections
            .into_iter()
            .map(|(turn, work)| (work["startId"].as_str().unwrap_or(turn), work))
            .collect();
        let actions = sections
            .iter()
            .map(|(owner, work)| json!({"type":"loadWork","id":owner,"work":work}))
            .collect();
        let mut segments = quiet_segments(label, stats, p);
        if stats.iter().any(|stat| stat["kind"] == "files") {
            let (_, _, _, _, plus, minus) = super::file_change_card::card_colors(p);
            let added = files_card["added"].as_u64().unwrap_or(0);
            let removed = files_card["removed"].as_u64().unwrap_or(0);
            segments.push((format!(" \u{b7} {}", lower_first(simple_files_label)), None));
            if added > 0 || removed > 0 {
                segments.push((" ".into(), None));
                segments.push((format!("+{added}"), Some(plus)));
                segments.push((" ".into(), None));
                segments.push((format!("\u{2212}{removed}"), Some(minus)));
            }
        }
        let line = self.quiet_work_line(
            QuietLine {
                key: key.clone(),
                segments,
                mark: finished_mark(work.iter().map(|message| &**message)),
                expanded,
                expandable: *expandable,
                actions,
            },
            p,
            cx,
        );
        let mut row = div().flex().flex_col().w_full().min_w_0().child(line);
        self.hide_file_changes = true;
        if expanded {
            let mut steps: Vec<AnyElement> = Vec::new();
            for (turn, cards) in turns.iter().zip(&folded) {
                let turn_key = format!("work:{}", turn.id);
                if let Some(cards) = self.folded_cards(&turn_key, cards, p, window, cx) {
                    steps.push(div().py(px(2.0 * s)).child(cards).into_any_element());
                }
            }
            for (owner, section) in &sections {
                if let Some(notice) = self.deferred_work_notice_for(owner, Some(section), p, cx) {
                    steps.push(notice);
                }
            }
            self.in_work_fold = true;
            self.quiet_steps = true;
            for message in work {
                steps.push(self.message_row(message, p, window, cx));
            }
            self.quiet_steps = false;
            self.in_work_fold = false;
            if !files.is_empty() {
                steps.push(
                    div()
                        .pt(px(4.0 * s))
                        .pb(px(2.0 * s))
                        .child(self.file_change_card(&key, files_card, files, p, cx))
                        .into_any_element(),
                );
            }
            if !steps.is_empty() {
                row = row.child(self.quiet_steps(&key, steps, p, cx));
            }
        } else if let Some(strip) = self.read_picture_strip(
            &format!("{key}:read"),
            &Value::Array(read_images.clone()),
            0.0,
            p.background,
            p,
            cx,
        ) {
            // The closed line keeps the pictures its work read in view (gx-chat-core
            // `read_images.rs`).
            row = row.child(div().pt(px(4.0 * s)).child(strip));
        }
        let mut parts: Vec<(AnyElement, bool)> = Vec::new();
        for (turn, cards) in turns.iter().zip(&folded) {
            let turn_key = format!("work:{}", turn.id);
            parts.extend(self.work_tail_parts(
                WorkTail {
                    key: &turn_key,
                    artifacts: &cards.kept,
                    questions: cards.tail_questions,
                    sent_messages: cards.tail_sent,
                    gbot_cards: &turn.gbot_cards,
                    workflows: &turn.workflows,
                    final_message: turn.final_message.as_deref(),
                },
                p,
                window,
                cx,
            ));
        }
        self.hide_file_changes = false;
        row.children(
            spaced_tail(parts, false, Some(QUIET_TEXT_GAP), p)
                .into_iter()
                .map(|(part, gap)| div().pt(px(gap * s)).child(part)),
        )
        .into_any_element()
    }
}

/// A work line's text: the label, then each count after " · ", a failure in the error tone. The
/// files count is left out: the line says it with its line counts after them.
fn quiet_segments(label: &str, stats: &[Value], p: &ChatAppearance) -> Vec<QuietSegment> {
    let mut segments: Vec<QuietSegment> = vec![(label.to_string(), None)];
    for stat in stats {
        let kind = text(stat, "kind");
        if kind == "files" {
            continue;
        }
        let failed = matches!(kind.as_str(), "failed" | "recovered");
        segments.push((" \u{b7} ".into(), None));
        segments.push((text(stat, "label"), failed.then(|| p.error())));
    }
    segments
}

/// `label` with its first letter in lower case, for a label read mid-line.
fn lower_first(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The last part of each path, for an edit line's names, each once.
pub(super) fn file_names<'a>(paths: impl IntoIterator<Item = &'a Value>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for path in paths.into_iter().filter_map(Value::as_str) {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}
