use super::cards::SINGLE_LINE_CARD;
use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::TranscriptItem;
use gpui::StatefulInteractiveElement;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    Window, div, px, relative,
};
use serde_json::Value;

/// The space between the parts of one message row (its text, tools, cards and actions).
pub(super) const MESSAGE_PART_GAP: f32 = 8.0;

pub(crate) fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// `value[key]` as a slice, empty when it is not an array.
fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map_or(&[], Vec::as_slice)
}

impl NativeChatView {
    pub(super) fn is_expanded(&self, id: &str, default: bool) -> bool {
        self.expanded.contains(id) || default && !self.collapsed.contains(id)
    }

    /// Flip a row whose default comes from verbose mode, where closing it has to be recorded
    /// against that default. The chevron in the marker column, the heading beside it and the rail
    /// down the open body all land here, so whichever of them the reader pressed leaves the row in
    /// the same state.
    pub(super) fn toggle_marker_disclosure(
        &mut self,
        id: String,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.anchor_disclosure_toggle(&id, !expanded);
        if expanded {
            self.expanded.remove(&id);
            self.collapsed.insert(id);
        } else {
            self.collapsed.remove(&id);
            self.expanded.insert(id);
        }
        cx.notify();
    }

    /// Flip a row that defaults to closed, and remeasure the list its height changed.
    pub(super) fn toggle_disclosure(&mut self, id: &str, cx: &mut Context<Self>) {
        self.anchor_disclosure_toggle(id, !self.expanded.contains(id));
        if self.expanded.contains(id) {
            self.expanded.remove(id);
        } else {
            self.expanded.insert(id.to_string());
        }
        cx.notify();
    }
    pub(crate) fn transcript_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Status line in the chat: the row after the last one is the working line (status_tail.rs).
        if index >= self.items.len() {
            return self.status_tail_row(index, cx);
        }
        let items = self.items.clone();
        self.transcript_item_row(&items, index, true, window, cx)
    }

    /// The column every transcript row sits in: the transcript's width (768px, or the Transcript
    /// width percentage), held while a panel slides (render.rs), ending with the row gap.
    pub(super) fn transcript_row_column(&self, p: &ChatAppearance) -> gpui::Div {
        let s = p.scale;
        let column = div()
            .w_full()
            .max_w(px(768.0 * s))
            .px(px(16.0 * s))
            .pb(px(p.row_gap() * s));
        self.with_transcript_width(column, p)
    }

    /// The transcript column's width on `column`: the Transcript width percentage, and while a
    /// panel slides the text width the rows hold (render.rs). The rows, the lone ↓ pill
    /// (working_card.rs) and, with Chat box matches transcript width, the chat box (composer.rs)
    /// all take it, so they line up; the two outside the list sit in `transcript_list_gutters`.
    pub(super) fn with_transcript_width(&self, column: gpui::Div, p: &ChatAppearance) -> gpui::Div {
        column
            .when_some(p.transcript_width, |this, width| {
                this.max_w(relative(1.0)).w(relative(width))
            })
            .when_some(self.held_text_width, |this, width| {
                this.flex_shrink_0().w(px(width)).max_w(px(width))
            })
    }

    /// The transcript list's own width: the pane less the minimap's column and its mirror
    /// (minimap.rs). The rows' Transcript width percentage is a share of it.
    fn transcript_list_width(&self, p: &ChatAppearance) -> f32 {
        let gutter = self.minimap_gutter(p).map_or(0.0, |gutter| gutter.as_f32());
        self.bounds.get().size.width.as_f32() - 2.0 * gutter
    }

    /// The width the rows' column takes when nothing holds it (render.rs): the Transcript width
    /// percentage of the list, or the list capped at 768px.
    pub(super) fn transcript_text_width(&self, p: &ChatAppearance) -> f32 {
        let list = self.transcript_list_width(p);
        match p.transcript_width {
            Some(ratio) => list * ratio,
            None => list.min(768.0 * p.scale),
        }
    }

    /// Insets a container drawn outside the transcript list (the chat box's row, the lone ↓ pill's
    /// row) by the minimap's gutters, so a Transcript width percentage inside it is taken of the
    /// list's width and its column lines up with the rows.
    ///
    /// CDXC:SessionChat 2026-11-07 WHY:
    /// The percentage resolves against the parent, and the list sits between the minimap's column and its mirror while these rows span the whole pane, so at 90% the chat box came out about 47px wider than the rows on each side. The capped 768px column needs no inset: the rail only shows when the pane has room for it beside 768px.
    pub(super) fn transcript_list_gutters(&self, row: gpui::Div, p: &ChatAppearance) -> gpui::Div {
        let gutter = p.transcript_width.and_then(|_| self.minimap_gutter(p));
        row.when_some(gutter, |this, gutter| this.px(gutter))
    }

    /// One row of a projected transcript. The subagent viewer renders its own item list
    /// through the same renderers; `main` is this session's own transcript, which is the
    /// only one the search tints and the only one a rewind may act on.
    pub(crate) fn transcript_item_row(
        &mut self,
        items: &[TranscriptItem],
        index: usize,
        main: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.in_subagent = !main;
        // The main list's length, which holds the status row too while it shows (status_tail.rs).
        let rows = if main {
            self.transcript_list_rows()
        } else {
            items.len()
        };
        self.detail_rows.start_row(main, index, rows);
        let item = &items[index];
        let mut p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        // CDXC:SessionChat 2026-09-09 DECISION:
        // User: subagent transcripts default to the same normal display as main chat, with verbose
        // and summarized modes off. React reached that by mounting its own list inside the dialog;
        // GPUI shares one renderer, so the child's rows drop both modes here.
        if !main {
            p.simple = false;
            p.work_open = false;
        }
        let p = p;
        let s = p.scale;
        self.begin_row_find(index, main, &p);
        let content = match item {
            TranscriptItem::Summary {
                id,
                user,
                final_message,
                earlier_replies,
                active,
                work_open,
                work,
                outcome,
                latest_reply,
                cards,
                read_images,
            } => {
                let reply = final_message.as_deref().filter(|reply| reply.is_object());
                // A running turn's "Active work" fold starts open and has a key of its own, so
                // closing it holds for that turn without closing the reply fold that replaces it
                // (gx-chat-core `summary_mode_turns`).
                let key = if reply.is_some() {
                    format!("summary:{id}")
                } else {
                    format!("active-work:{id}")
                };
                let expanded = self.is_expanded(&key, *latest_reply || *work_open);
                let motion = self.disclosure_frame(&key, expanded, cx);
                let mut row = div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(8.0 * s))
                    .child(self.message_row(user, &p, window, cx));
                for output in outcome {
                    row = row.child(self.message_row(output, &p, window, cx));
                }
                // The turn's messages holding a page card, a question form or a chart, drawn in
                // full and never folded; the folds below leave them out (gx-chat-core
                // `content_rows`, where the user's decision lives).
                let content: Vec<&str> = cards
                    .iter()
                    .filter_map(|card| card["id"].as_str())
                    .collect();
                for card in cards {
                    row = row.child(self.message_row(card, &p, window, cx));
                }
                let folded = |message: &&std::sync::Arc<serde_json::Value>| {
                    !message["id"]
                        .as_str()
                        .is_some_and(|id| content.contains(&id))
                };
                let fold_rows: Vec<&std::sync::Arc<serde_json::Value>> = match reply {
                    Some(_) => earlier_replies
                        .iter()
                        .chain(final_message.iter())
                        .filter(folded)
                        .collect(),
                    None => work.iter().filter(folded).collect(),
                };
                if (reply.is_some() || *active) && !fold_rows.is_empty() {
                    row = row.child(
                        self.disclosure(
                            key.clone(),
                            if reply.is_some() {
                                "Agent reply"
                            } else {
                                "Active work"
                            }
                            .into(),
                            expanded,
                            None,
                            &p,
                            cx,
                        ),
                    );
                    // The closed "Active work" fold keeps the pictures its rows read in view
                    // (gx-chat-core `read_images.rs`).
                    if !expanded && motion.is_none() {
                        let pictures = serde_json::Value::Array(read_images.clone());
                        if let Some(strip) = self.read_picture_strip(
                            &format!("{key}:read"),
                            &pictures,
                            4.0,
                            p.background,
                            &p,
                            cx,
                        ) {
                            row = row.child(strip);
                        }
                    }
                    if expanded || motion.is_some() {
                        let mut body = div().flex().flex_col().w_full().gap(px(8.0 * s));
                        for message in fold_rows {
                            body = body.child(self.message_row(message, &p, window, cx));
                        }
                        row = row.child(self.disclosure_body_motion(
                            &key,
                            motion,
                            8.0 * s,
                            body.into_any_element(),
                        ));
                    }
                }
                row.into_any_element()
            }
            TranscriptItem::CompletedWork { .. } => self.completed_work_row(item, &p, window, cx),
            TranscriptItem::GroupedWork { .. } => self.grouped_work_row(item, &p, window, cx),
            TranscriptItem::Message { message } => {
                self.message_item_row(message, items.get(index + 1), &p, window, cx)
            }
            // A kind this build does not know draws its `message`, as every unknown kind did.
            TranscriptItem::Unknown(row) => {
                self.message_item_row(&row["message"], items.get(index + 1), &p, window, cx)
            }
        };
        self.detail_rows.end_row();
        self.row_find = None;
        self.row_label_find = None;
        if self.disclosure_motion.borrow().running() {
            window.request_animation_frame();
        }
        let question_marker = main.then(|| self.question_row_marker(index)).flatten();
        div()
            .w_full()
            .flex()
            .justify_center()
            .children(question_marker)
            // The header is window chrome and does not zoom with the chat, so its height is unscaled.
            .when(index == 0, |this| {
                let header = if main && self.under_workarea_header {
                    crate::app::consts::WORKAREA_HEADER_HEIGHT
                } else {
                    0.0
                };
                this.pt(px(32.0 * s + header))
            })
            .child(self.transcript_row_column(&p).child(content))
            .into_any_element()
    }

    /// A message row of the list, `next` the row below it.
    fn message_item_row(
        &mut self,
        message: &Value,
        next: Option<&TranscriptItem>,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // A prompt drawn with its turn's rows still open below it; when the same turn comes
        // back folded, `disclosure_motion.rs` folds those rows away instead of swapping them.
        // Under Agent work Reduced or Quiet the finished turn is a `grouped-work` row that names
        // it among its turns, so the prompt is not live there either.
        if message["role"] == "user"
            && !next.is_some_and(|next| match next {
                TranscriptItem::CompletedWork { id, .. } => message["id"] == *id,
                TranscriptItem::GroupedWork { turns, .. } => {
                    turns.iter().any(|turn| message["id"] == turn.id)
                }
                _ => false,
            })
        {
            self.disclosure_motion
                .borrow_mut()
                .saw_live(&format!("work:{}", text(message, "id")));
        }
        self.message_row(message, p, window, cx)
    }

    /// The chevron a disclosure puts in the transcript's marker column, in place of the dot a plain
    /// row hangs its first line from. React drew the same glyph through `.ghostex-chat-marker-slot`
    /// inside its own button, so a heading made of the agent's own Markdown keeps its links and code
    /// controls clickable; the press stops here for the same reason the React button's did, because
    /// the heading around it is a trigger too and would toggle the row straight back.
    pub(super) fn disclosure_marker(
        &self,
        id: String,
        expanded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(gpui::SharedString::from(id.clone()))
            .role(gpui::Role::Button)
            .aria_label(if expanded {
                "Hide tool calls for this message"
            } else {
                "Show tool calls for this message"
            })
            .w(px(16.0 * s))
            .ml(px(2.0 * s))
            .h(px(22.75 * s))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .chat_cursor_pointer()
            .child(
                gpui::svg()
                    .path(if expanded {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(14.0 * s))
                    .text_color(p.primary),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_marker_disclosure(id.clone(), expanded, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    pub(crate) fn disclosure(
        &self,
        id: String,
        label: String,
        expanded: bool,
        action: Option<Value>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(id.clone())
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .flex()
            .items_start()
            .w_full()
            .gap(px(6.0 * s))
            .rounded(px(4.0 * s))
            .text_color(p.primary)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
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
                            .path(if expanded {
                                "titlebar/chevron-down.svg"
                            } else {
                                "titlebar/chevron-right.svg"
                            })
                            .size(px(14.0 * s))
                            .text_color(p.primary),
                    ),
            )
            .child(div().flex_1().min_w_0().child(self.find_label(&id, label)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.anchor_disclosure_toggle(&id, !expanded);
                if expanded {
                    this.expanded.remove(&id);
                    this.collapsed.insert(id.clone());
                } else {
                    this.collapsed.remove(&id);
                    this.expanded.insert(id.clone());
                    if let Some(action) = &action {
                        this.invoke(action.clone(), cx);
                    }
                }
                cx.notify();
            }))
            .into_any_element()
    }

    pub(crate) fn markdown(
        &self,
        id: String,
        content: String,
        references: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.rich_markdown(id, content, references, p, cx)
    }

    /// A message row; the agent's reply that just arrived fades in (reply_arrival.rs).
    pub(super) fn message_row(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = self.message_row_drawn(message, p, window, cx);
        match message["id"].as_str() {
            Some(id) if message["role"] == "assistant" => self.reply_arrival_row(id, row, window),
            _ => row,
        }
    }

    fn message_row_drawn(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = text(message, "id");
        let body = text(message, "text");
        let s = p.scale;
        let reply_focus = self.reply_focus(message, p.compact(), window, cx);
        let reply_focused = reply_focus.as_ref().is_some_and(|(_, focused)| *focused);
        // The whole text as the label: an assistive client or an e2e run reads a message in one node instead of walking its markdown blocks (native_sidebar and the terminal do the same).
        let a11y_label = format!("{} message: {}", text(message, "role"), a11y_excerpt(&body));
        let mut row = div()
            .id(format!("message:{id}"))
            .role(gpui::Role::Article)
            .aria_label(a11y_label)
            .group("native-chat-message")
            .flex()
            .flex_col()
            .min_w_0()
            .w_full()
            .gap(px(MESSAGE_PART_GAP * s))
            .when_some(reply_focus, |row, (focus, _)| {
                // Message spacing Compact: the reply or prompt is a tab stop, since its footer's
                // buttons exist only while they show (message_spacing.rs).
                if p.compact() {
                    super::message_spacing::focusable_reply(row.track_focus(&focus), p)
                } else {
                    row.track_focus(&focus).tab_stop(false)
                }
            });
        // Message spacing Compact: the footer's buttons are built only while the pointer is on the
        // message (message_spacing.rs).
        if p.compact() && (self.has_reply_actions(message) || Self::has_user_bar(message)) {
            row = self.track_message_hover(row, &id, cx);
        }
        if message["role"] == "user" && message["interAgentMessage"].is_object() {
            return row
                .child(self.inter_agent_message_card(&id, message, p, cx))
                .into_any_element();
        }
        if message["role"] == "user" && message["pageAnswers"].is_object() {
            row = row.child(self.page_answers_card(
                &message["pageAnswers"],
                &message["startupDelivery"],
                p,
                cx,
            ));
            // The user's own words besides the answers follow as their ordinary message.
            if message["pageAnswers"]["withText"] != true {
                return row.into_any_element();
            }
        }
        if message["shellCommand"] == true {
            return row
                .child(self.shell_command_card(message, p, cx))
                .into_any_element();
        }
        if message["role"] == "user" && message["suppressed"].is_null() {
            if p.left_aligned() {
                return self.left_user_message(row, message, body, reply_focused, p, cx);
            }
            // The prompt renders as markdown like the React bubble, which also makes it a selectable TextView; a plain string child cannot be selected.
            let mut bubble_appearance = p.clone();
            bubble_appearance.prose = p.strengthened(p.primary);
            bubble_appearance.code_text = p.primary;
            // The delivery indicator sits left of the bubble, level with its last line, so it never drifts away from a short prompt. It hangs off the bubble's own left edge (6px gap, the 1px border counted) because a flex row around the bubble measured a prompt with an inline picture at min-content, one word per line.
            let indicator = self
                .delivery_indicator(message, p, cx)
                .map(|(icon, slots)| {
                    // Each 24px slot after the first adds its own width and the 2px gap between them.
                    let offset = 31.0 + 26.0 * slots.saturating_sub(1) as f32;
                    div()
                        .absolute()
                        .bottom(px(11.0 * s))
                        .left(px(-offset * s))
                        .child(icon)
                });
            let bubble = div()
                .relative()
                .max_w(relative(0.8))
                .min_w_0()
                .rounded(px(16.0 * s))
                .border(px(1.0 * s))
                .border_color(gpui::transparent_black())
                .p(px(12.0 * s))
                .bg(p.input)
                // A slash command reads as one, in the chat's monospace (gx-chat-core `is_slash_command_line`).
                .when(message["slashCommand"] == true, |bubble| {
                    bubble.font_family(super::fonts::CHAT_MONO)
                })
                .child(self.with_loading_pictures(
                    &id,
                    self.prompt_body(&id, message, body.clone(), &bubble_appearance, p, cx),
                    !body.trim().is_empty(),
                    p,
                ))
                .children(indicator);
            let failed_delivery = self.render_startup_delivery(message, false, p, cx);
            // The prompt's own pictures sit above the bubble, where their author put them.
            let thumbnails = self.user_image_thumbnails(message, p, cx);
            let actions = self.user_actions(message, reply_focused, p, cx);
            // A send that did not go out says why under its own bubble, at the bubble's side and
            // within its width (`delivery_note`, startup_delivery.rs).
            return row
                .when_some(thumbnails, |this, images| this.child(images))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(4.0 * s))
                        .child(bubble)
                        .children(failed_delivery)
                        .children(actions),
                )
                .into_any_element();
        }
        if message["sideQuestion"].is_object() {
            return row
                .child(self.side_question_row(&id, message, p, cx))
                .into_any_element();
        }
        if message["suppressed"].is_object() {
            return row
                .child(self.suppressed_row(&id, message, p, cx))
                .into_any_element();
        }
        if message["apiError"].is_object() {
            return row
                .child(self.api_error_row(&id, message, p, cx))
                .into_any_element();
        }
        if message["systemCard"].is_object() {
            return row
                .child(self.system_card(&id, message, p, cx))
                .into_any_element();
        }
        // A picture an agent shared renders as the picture, above the prose it came with.
        if let Some(images) = self.assistant_image_attachments(message, p, cx) {
            row = row.child(images);
        }
        // A reasoning headline owns the tools that followed it, so it renders them itself and the
        // run below must not render them a second time.
        let mut tools_rendered = false;
        // Settings > Chat > Tool call cards: the calls and edits are one card after the text, which
        // owns none of them (`tool_card.rs`).
        let tool_card = message["toolCard"].is_object();
        if !body.is_empty() {
            let reasoning = message["role"] == "reasoning";
            // Inside a turn fold the text owns none of its calls: they follow it at the same
            // level (gx-chat-core `list_tools_in_work_fold`).
            let tools = message["toolsFlat"] != true
                && !tool_card
                && message["tools"]
                    .as_array()
                    .is_some_and(|tools| !tools.is_empty());
            // The running turn's thought is one closed line, and so is every thought in Simple
            // mode that owns no calls (gx-chat-core `transcript/thinking_card.rs`).
            let card = reasoning && (message["thinkingCard"].is_object() || (p.simple && !tools));
            if card {
                // A running thought folds the calls it made behind its line, as a reply's sentence
                // does (gx-chat-core `thinkingCard.calls`); any other card leaves them to the run
                // below.
                row = row.child(self.thinking_card(&id, message, body.clone(), p, cx));
                tools_rendered = message["thinkingCard"]["calls"].is_string();
            } else if reasoning && tools {
                let key = format!("reasoning:{id}");
                // A running summary turn's newest row opens its tools (gx-chat-core `summary_mode_turns`).
                let expanded = self.is_expanded(&key, p.work_open || message["toolsOpen"] == true);
                let motion = self.disclosure_frame(&key, expanded, cx);
                let thought = p.muted_thinking.then(|| p.thinking_lane());
                let thought = thought.as_ref().unwrap_or(p);
                row = row.child(self.disclosure(
                    key.clone(),
                    text(&message["reasoning"], "headline"),
                    expanded,
                    None,
                    thought,
                    cx,
                ));
                if expanded || motion.is_some() {
                    // React's ReasoningRow put the thought's tail and the tool run it owns in one
                    // expansion, so both hang off the rail the headline opened.
                    let mut detail_rows: Vec<AnyElement> = Vec::new();
                    let detail = text(&message["reasoning"], "body");
                    if !detail.is_empty() {
                        detail_rows.push(self.markdown(
                            format!("reasoning-body:{id}"),
                            detail,
                            &message["markdownReferences"],
                            thought,
                            cx,
                        ));
                    }
                    detail_rows.extend(self.tool_rows(message, p, cx));
                    if !detail_rows.is_empty() {
                        let body = disclosure_body(
                            p,
                            DisclosureRail::Marker,
                            8.0,
                            key.clone(),
                            "Collapse thinking",
                            detail_rows,
                            cx,
                        );
                        row = row.child(self.disclosure_body_motion(&key, motion, 8.0 * s, body));
                    }
                } else if let Some(pictures) = self.read_picture_strip(
                    &format!("{key}:read"),
                    &message["readImages"],
                    super::read_pictures::FOLDED_CALLS_INSET,
                    p.background,
                    p,
                    cx,
                ) {
                    // The closed headline keeps the pictures its calls read in view
                    // (gx-chat-core `read_images.rs`).
                    row = row.child(pictures);
                }
                tools_rendered = true;
            } else if reasoning {
                row = row.child(self.thinking_row(
                    &id,
                    body.clone(),
                    &message["markdownReferences"],
                    p,
                    cx,
                ));
            } else {
                // React's AgentToolsDisclosure: an agent's own words are the heading its tool calls
                // hang from, so the marker column carries a chevron instead of the reply dot and
                // the rows below sit on that disclosure's rail
                // (CDXC:SessionChat 2026-09-13 DECISION in React's rows.tsx, now in git history).
                let key = format!("tools:{id}");
                let expanded =
                    tools && self.is_expanded(&key, p.work_open || message["toolsOpen"] == true);
                let motion = self.disclosure_frame(&key, expanded, cx);
                let flush = self.is_flush_reply(message);
                let reply = p.reply_lane();
                let prose = div().min_w_0().flex_1().child(self.markdown(
                    format!("body:{id}"),
                    body.clone(),
                    &message["markdownReferences"],
                    reply.as_ref().unwrap_or(p),
                    cx,
                ));
                let heading = div()
                    .flex()
                    .items_start()
                    .gap(px(6.0 * s))
                    .when(!flush, |heading| {
                        heading.child(if tools {
                            self.disclosure_marker(key.clone(), expanded, p, cx)
                        } else {
                            self.reply_marker(p)
                        })
                    })
                    .child(prose);
                // The whole heading is the trigger, not just the chevron: React's
                // `.ghostex-chat-agent-tools-heading` carried the toggle and the hover fill, and a
                // one-line commentary with a chevron beside it is read as a line to click. The
                // controls the Markdown carries (links, file pills, fence and table actions,
                // pictures) stop the press themselves, and `acts_on_row` keeps a press that
                // selected text from counting as a click on the row.
                row = row.child(if tools {
                    let toggle_key = key.clone();
                    heading
                        .id(gpui::SharedString::from(format!("tools-heading:{id}")))
                        .rounded(px(4.0 * s))
                        .pr(px(5.0 * s))
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.05)))
                        .on_click(cx.listener(move |this, event, window, cx| {
                            if super::row_click::acts_on_row(event, window, cx) {
                                this.toggle_marker_disclosure(toggle_key.clone(), expanded, cx);
                            }
                        }))
                        .into_any_element()
                } else {
                    heading.into_any_element()
                });
                if tools {
                    if expanded || motion.is_some() {
                        let work = self.tool_rows(message, p, cx);
                        if !work.is_empty() {
                            let body = disclosure_body(
                                p,
                                DisclosureRail::Marker,
                                8.0,
                                key.clone(),
                                "Collapse tool calls",
                                work,
                                cx,
                            );
                            row =
                                row.child(self.disclosure_body_motion(&key, motion, 8.0 * s, body));
                        }
                    } else if let Some(pictures) = self.read_picture_strip(
                        &format!("{key}:read"),
                        &message["readImages"],
                        super::read_pictures::FOLDED_CALLS_INSET,
                        p.background,
                        p,
                        cx,
                    ) {
                        // The sentence's closed calls keep the pictures they read in view
                        // (gx-chat-core `read_images.rs`).
                        row = row.child(pictures);
                    }
                    tools_rendered = true;
                }
            }
        }
        // Whether the part last added is a one-line card: two cards in a row are a row gap apart
        // (`SINGLE_LINE_CARD.gap_above`), whichever kinds they are.
        let mut card_above = false;
        if p.quiet_style() && !tools_rendered && !tool_card {
            // Agent work Reduced or Quiet: the edit line comes right after the work line,
            // with no gap between them (quiet_work.rs). The quiet style's lines are never cards.
            let mut tools = self.tool_rows(message, p, cx);
            let after_line = !tools.is_empty();
            // Message spacing Compact: the first line sits closer to the text above it
            // (message_spacing.rs); an opened line's steps keep their rail's spacing.
            let lift = if body.is_empty() || self.quiet_steps {
                0.0
            } else {
                p.quiet_line_lift()
            };
            let lifted =
                |element: AnyElement| div().mt(px(-lift * s)).child(element).into_any_element();
            if lift > 0.0 && after_line {
                let first = tools.remove(0);
                tools.insert(0, lifted(first));
            }
            row = row.children(tools);
            for (index, card) in self
                .file_change_cards(message, p, cx)
                .into_iter()
                .enumerate()
            {
                row = row.child(if index == 0 && after_line {
                    div()
                        .mt(px(-MESSAGE_PART_GAP * s))
                        .child(card)
                        .into_any_element()
                } else if index == 0 && lift > 0.0 {
                    lifted(card)
                } else {
                    card
                });
            }
        } else {
            if !tool_card {
                for card in self.file_change_cards(message, p, cx) {
                    row = row.child(card);
                    card_above = true;
                }
            }
            if !tools_rendered {
                let tools = self.tool_rows(message, p, cx);
                if !tools.is_empty() {
                    // With Settings > Chat > Tool call cards on, the calls are one card
                    // (`tool_card.rs`); otherwise they are plain rows.
                    card_above = tool_card;
                }
                for tool in tools {
                    row = row.child(tool);
                }
            }
        }
        // The rows' own 8px part gap is already there; a card under a card adds the rest.
        let card_gap = |card_above: bool| {
            px(
                (SINGLE_LINE_CARD.gap_above(card_above, true, MESSAGE_PART_GAP, p)
                    - MESSAGE_PART_GAP)
                    * s,
            )
        };
        // Inside a turn's work fold the raw tool pairs stay plain rows and the cards are hoisted
        // onto the turn instead, so an answer is never rendered twice (the core's questions/hoisting.rs).
        if !self.in_work_fold
            && let Some(cards) = self.question_exchange_cards(
                &format!("message:{id}"),
                list(message, "questions"),
                p,
                cx,
            )
        {
            row = row.child(div().mt(card_gap(card_above)).child(cards));
            card_above = true;
        }
        if !self.in_work_fold
            && let Some(cards) = self.sent_agent_message_cards(list(message, "sentMessages"), p, cx)
        {
            row = row.child(div().mt(card_gap(card_above)).child(cards));
            card_above = true;
        }
        if !self.in_work_fold
            && let Some(cards) = self.gbot_cards(list(message, "gbotCards"), p, cx)
        {
            row = row.child(div().mt(card_gap(card_above)).child(cards));
            card_above = true;
        }
        if !self.in_work_fold
            && let Some(cards) = self.workflow_cards(list(message, "workflows"), p, cx)
        {
            row = row.child(div().mt(card_gap(card_above)).child(cards));
        }
        if self.has_reply_actions(message) {
            // Agent work Quiet: the reply that carries the work button opens its steps right
            // under its hover row (hidden_work.rs).
            let work = self.take_hidden_work(&id);
            row = row.children(self.reply_actions(message, reply_focused, work.as_ref(), p, cx));
            if let Some(steps) = work.and_then(|work| work.steps) {
                row = row.child(steps);
            }
        }
        row.into_any_element()
    }

    /// A prompt's text in its bubble. A long one shows only its first lines (the core's
    /// `collapsedText`, `long_prompt.rs`) with Show more under them until the reader expands it,
    /// so the rest is never laid out unless it is read.
    pub(super) fn prompt_body(
        &self,
        id: &str,
        message: &Value,
        body: String,
        bubble: &ChatAppearance,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let collapsed = text(message, "collapsedText");
        if collapsed.is_empty() {
            return self.prompt_markdown(
                format!("user:{id}"),
                body,
                &message["markdownReferences"],
                bubble,
                cx,
            );
        }
        let key = format!("prompt:{id}");
        let expanded = self.expanded.contains(&key);
        let prose = if expanded {
            self.prompt_markdown(
                format!("user:{id}"),
                body,
                &message["markdownReferences"],
                bubble,
                cx,
            )
        } else {
            self.prompt_markdown(
                format!("user-collapsed:{id}"),
                collapsed,
                &message["collapsedReferences"],
                bubble,
                cx,
            )
        };
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(8.0 * p.scale))
            .child(prose)
            .child(self.show_more_toggle(format!("prompt-toggle:{id}"), key, expanded, p, cx))
            .into_any_element()
    }
}

/// The first part of a message for its accessibility label: long enough to identify and read the message, short enough that a frame's tree stays cheap to build and to send.
pub(super) fn a11y_excerpt(body: &str) -> String {
    const LIMIT: usize = 2000;
    match body.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{}\u{2026}", &body[..end]),
        None => body.to_string(),
    }
}
