use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::StatefulInteractiveElement;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    Window, div, px, relative,
};
use serde_json::Value;

pub(crate) fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
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
        if expanded {
            self.expanded.remove(&id);
            self.collapsed.insert(id);
        } else {
            self.collapsed.remove(&id);
            self.expanded.insert(id);
        }
        self.list.remeasure();
        cx.notify();
    }

    /// Flip a row that defaults to closed, and remeasure the list its height changed.
    pub(super) fn toggle_disclosure(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.expanded.contains(id) {
            self.expanded.remove(id);
        } else {
            self.expanded.insert(id.to_string());
        }
        self.list.remeasure();
        cx.notify();
    }
    pub(crate) fn transcript_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = self.items.clone();
        self.transcript_item_row(&items, index, true, window, cx)
    }

    /// One row of a projected transcript. The subagent viewer renders its own item list
    /// through the same renderers; `main` is this session's own transcript, which is the
    /// only one the search tints and the only one a rewind may act on.
    pub(crate) fn transcript_item_row(
        &mut self,
        items: &[Value],
        index: usize,
        main: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.in_subagent = !main;
        let item = &items[index];
        let mut p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        // CDXC:SessionChat 2026-09-09 DECISION:
        // User: subagent transcripts default to the same normal display as main chat, with verbose
        // and summarized modes off. React reached that by mounting its own list inside the dialog;
        // GPUI shares one renderer, so the child's rows drop both modes here.
        if !main {
            p.verbose = false;
            p.simple = false;
        }
        let p = p;
        let s = p.scale;
        let content = if item["kind"] == "summary" {
            let id = format!("summary:{}", text(&item, "id"));
            let expanded = self.expanded.contains(&id);
            let motion = self.disclosure_frame(&id, expanded, cx);
            let mut row = div()
                .flex()
                .flex_col()
                .w_full()
                .gap(px(8.0 * s))
                .child(self.message_row(&item["user"], &p, window, cx));
            if item["final"].is_object() || item["active"] == true {
                row = row.child(
                    self.disclosure(
                        id.clone(),
                        if item["final"].is_object() {
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
                if expanded || motion.is_some() {
                    let mut body = div().flex().flex_col().w_full().gap(px(8.0 * s));
                    if item["final"].is_object() {
                        body = body.child(self.message_row(&item["final"], &p, window, cx));
                    } else {
                        for message in item["work"].as_array().into_iter().flatten() {
                            body = body.child(self.message_row(message, &p, window, cx));
                        }
                    }
                    row = row.child(self.disclosure_body_motion(
                        &id,
                        motion,
                        8.0 * s,
                        body.into_any_element(),
                    ));
                }
            }
            row.into_any_element()
        } else if item["kind"] == "completed-work" {
            self.completed_work_row(item, &p, window, cx)
        } else {
            // A prompt drawn with its turn's rows still open below it; when the same turn comes
            // back folded, `disclosure_motion.rs` folds those rows away instead of swapping them.
            if item["message"]["role"] == "user"
                && !items.get(index + 1).is_some_and(|next| {
                    next["kind"] == "completed-work" && next["id"] == item["message"]["id"]
                })
            {
                self.disclosure_motion
                    .borrow_mut()
                    .saw_live(&format!("work:{}", text(&item["message"], "id")));
            }
            self.message_row(&item["message"], &p, window, cx)
        };
        if self.disclosure_motion.borrow().running() {
            window.request_animation_frame();
        }
        div()
            .w_full()
            .flex()
            .justify_center()
            // The header is window chrome and does not zoom with the chat, so its height is unscaled.
            .when(index == 0, |this| {
                let header = if main && self.under_workarea_header {
                    crate::app::consts::WORKAREA_HEADER_HEIGHT
                } else {
                    0.0
                };
                this.pt(px(32.0 * s + header))
            })
            .when(index + 1 == items.len(), |this| {
                this.pb(px(super::transcript_layout::LAYOUT.end_padding * s))
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(768.0 * s))
                    .px(px(16.0 * s))
                    .pb(px(16.0 * s))
                    .when_some(p.transcript_width, |this, width| {
                        this.max_w(relative(1.0)).w(relative(width))
                    })
                    // Transcript search tints the rows that matched; the selected one is stronger.
                    .when_some(
                        self.search_row_tint(index, &p).filter(|_| main),
                        |this, tint| this.bg(tint).rounded(px(8.0 * s)),
                    )
                    .child(content),
            )
            .into_any_element()
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
            .child(div().flex_1().min_w_0().child(label))
            .on_click(cx.listener(move |this, _, _, cx| {
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
                this.list.remeasure();
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

    pub(super) fn message_row(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = text(message, "id");
        let body = text(message, "text");
        let s = p.scale;
        let reply_focus = self.reply_focus(message, window, cx);
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
            .gap(px(8.0 * s))
            .when_some(reply_focus, |row, (focus, _)| {
                row.track_focus(&focus).tab_stop(false)
            });
        if message["role"] == "user" && message["interAgentMessage"].is_object() {
            return row
                .child(self.inter_agent_message_card(&id, message, p, cx))
                .into_any_element();
        }
        if message["terminalTool"].is_object() {
            let activity = message["terminalTool"].clone();
            return row
                .child(self.terminal_tool_row(&activity, p, cx))
                .into_any_element();
        }
        if message["role"] == "user" && message["suppressed"].is_null() {
            // The prompt renders as markdown like the React bubble, which also makes it a selectable TextView; a plain string child cannot be selected.
            let mut bubble_appearance = p.clone();
            bubble_appearance.prose = p.primary;
            let bubble = div()
                .max_w(relative(0.8))
                .min_w_0()
                .rounded(px(16.0 * s))
                .border(px(1.0 * s))
                .border_color(gpui::transparent_black())
                .p(px(12.0 * s))
                .bg(p.input)
                .child(self.markdown(
                    format!("user:{id}"),
                    body.clone(),
                    &message["markdownReferences"],
                    &bubble_appearance,
                    cx,
                ));
            let startup_delivery = self.render_startup_delivery(message, p, cx);
            // The prompt's own pictures sit above the bubble, where their author put them.
            let thumbnails = self.user_image_thumbnails(message, p, cx);
            let actions = self.user_actions(message, p, cx);
            return row
                // A send still waiting for the terminal says so instead of showing the agent's queue label.
                .when_some(startup_delivery, |this, status| this.child(status))
                .when(
                    message["queued"] == true && message["startupDelivery"].is_null(),
                    |this| {
                        this.child(
                            div()
                                .flex()
                                .justify_end()
                                .text_size(px(11.0 * s))
                                .text_color(p.muted)
                                .child("QUEUED"),
                        )
                    },
                )
                .when_some(thumbnails, |this, images| this.child(images))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(4.0 * s))
                        .child(bubble)
                        .children(actions),
                )
                .into_any_element();
        }
        if message["suppressed"].is_object() {
            return row
                .child(self.suppressed_row(&id, message, p, cx))
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
        if !body.is_empty() {
            let reasoning = message["role"] == "reasoning";
            let tools = message["tools"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty());
            if reasoning && tools {
                let key = format!("reasoning:{id}");
                let expanded = self.is_expanded(&key, p.verbose);
                let motion = self.disclosure_frame(&key, expanded, cx);
                row = row.child(self.disclosure(
                    key.clone(),
                    text(&message["reasoning"], "headline"),
                    expanded,
                    None,
                    p,
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
                            p,
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
                let expanded = tools && self.is_expanded(&key, p.verbose);
                let motion = self.disclosure_frame(&key, expanded, cx);
                let heading = div()
                    .flex()
                    .items_start()
                    .gap(px(6.0 * s))
                    .child(if tools {
                        self.disclosure_marker(key.clone(), expanded, p, cx)
                    } else {
                        self.reply_marker(p)
                    })
                    .child(div().min_w_0().flex_1().child(self.markdown(
                        format!("body:{id}"),
                        body.clone(),
                        &message["markdownReferences"],
                        p,
                        cx,
                    )));
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
                    }
                    tools_rendered = true;
                }
            }
        }
        for card in self.file_change_cards(message, p, cx) {
            row = row.child(card);
        }
        if !tools_rendered {
            for tool in self.tool_rows(message, p, cx) {
                row = row.child(tool);
            }
        }
        // Inside a turn's work fold the raw tool pairs stay plain rows and the cards are hoisted
        // onto the turn instead, so an answer is never rendered twice (the core's questions/hoisting.rs).
        if !self.in_work_fold
            && let Some(cards) =
                self.question_exchange_cards(&format!("message:{id}"), &message["questions"], p, cx)
        {
            row = row.child(cards);
        }
        if self.has_reply_actions(message) {
            row = row.child(self.reply_actions(message, reply_focused, p, cx));
        }
        row.into_any_element()
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
