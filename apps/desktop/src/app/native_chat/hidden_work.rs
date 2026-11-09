//! Agent work set to Quiet (`chatAgentWorkStyle` `quiet`, gx-chat-core
//! `transcript/chat_look.rs`): the core projects one `grouped-work` row per message of yours with
//! every reply's calls moved into its work, and leaves the running turn's calls out
//! (`grouped_work::hide_work`). This draws what that row says: no line before the replies, the
//! work as a button at the left of the hover row of the reply `workCarrier` names ("✓ 24s · 5
//! tools · 2 files ›", the core's `workButton`), which opens the steps right under that row, on
//! the quiet style's rail. At every other Agent work choice nothing here runs.
//!
//! CDXC:SessionChat 2026-11-07 SEE-ALSO:
//! packages/gx-chat-core/src/transcript/chat_look.rs (the user decision), packages/gx-chat-core/src/transcript/grouped_work.rs (`workButton`), apps/desktop/src/app/native_chat/message_actions.rs (the hover row that holds the button), apps/mobile/app/src/chat/native/transcript/hiddenWork.tsx (the phone draws the same button).

use super::completed_work_row::{WorkTail, spaced_tail};
use super::quiet_work::{QuietMark, finished_mark};
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::TranscriptItem;
use ghostex_gx_chat_core::transcript::chat_look::AgentWork;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use serde_json::{Value, json};

/// The work of one `grouped-work` row under Quiet, waiting for the reply whose hover row shows it.
pub(crate) struct HiddenWork {
    /// The id of the reply that carries the button (the core's `workCarrier`); `None` draws the
    /// button on a row of its own after the tails.
    pub carrier: Option<String>,
    /// The disclosure key the button opens, the Reduced line's (`work:<id>`), which Find and
    /// Detailed open too.
    pub key: String,
    pub label: String,
    pub mark: QuietMark,
    pub expanded: bool,
    pub expandable: bool,
    /// The reads to send when it opens (`loadWork`, one per section still on the server).
    pub actions: Vec<Value>,
    /// The opened steps, drawn right under the hover row.
    pub steps: Option<AnyElement>,
}

impl ChatAppearance {
    /// Agent work is Quiet: no line before a reply, the work in its hover row.
    pub(crate) fn hidden_work(&self) -> bool {
        self.chat_look.agent_work == AgentWork::Quiet
    }
}

impl NativeChatView {
    /// The work for the reply `id` when it carries the button, taken so it is drawn once.
    pub(super) fn take_hidden_work(&mut self, id: &str) -> Option<HiddenWork> {
        if self
            .hidden_work
            .as_ref()
            .is_some_and(|work| work.carrier.as_deref() == Some(id))
        {
            self.hidden_work.take()
        } else {
            None
        }
    }

    /// A `grouped-work` row under Quiet: no line, the closed work's pictures where the line would
    /// be, then each turn's tail as Reduced draws it; the carrier's hover row takes the button and
    /// the opened steps. With no carrier (an interrupted turn) the button stands on its own row
    /// after the tails, always shown, so the work never goes out of reach.
    pub(super) fn hidden_work_row(
        &mut self,
        item: &TranscriptItem,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let TranscriptItem::GroupedWork {
            id,
            files_card,
            files,
            expandable,
            work,
            turns,
            read_images,
            work_button,
            work_carrier,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let key = format!("work:{id}");
        let expanded = self.is_expanded(&key, p.work_open);
        // One read per section still on the server, keyed by the section's prompt (`startId`), as
        // the Reduced line reads them (quiet_work.rs `grouped_work_row`).
        let mut sections: Vec<(String, Value)> = Vec::new();
        for turn in turns {
            if let Some(section) = turn.deferred.as_ref().filter(|work| work.is_object())
                && !sections.iter().any(|(_, seen)| seen == section)
            {
                let owner = section["startId"].as_str().unwrap_or(&turn.id).to_string();
                sections.push((owner, section.clone()));
            }
        }
        let actions = sections
            .iter()
            .map(|(owner, work)| json!({"type":"loadWork","id":owner,"work":work}))
            .collect();
        // The core moved every reply's calls into `work`, so its marks cover them all.
        let mark = finished_mark(work.iter().map(|message| &**message));
        let steps = expanded.then(|| {
            let mut steps: Vec<AnyElement> = Vec::new();
            for (owner, section) in &sections {
                if let Some(notice) = self.deferred_work_notice_for(owner, Some(section), p, cx) {
                    steps.push(notice);
                }
            }
            self.hide_file_changes = true;
            self.in_work_fold = true;
            self.quiet_steps = true;
            for message in work {
                steps.push(self.message_row(message, p, window, cx));
            }
            self.quiet_steps = false;
            self.in_work_fold = false;
            self.hide_file_changes = false;
            if !files.is_empty() {
                steps.push(
                    div()
                        .pt(px(4.0 * s))
                        .pb(px(2.0 * s))
                        .child(self.file_change_card(&key, files_card, files, p, cx))
                        .into_any_element(),
                );
            }
            self.quiet_steps(steps, p)
        });
        self.hidden_work = Some(HiddenWork {
            carrier: work_carrier.clone(),
            key: key.clone(),
            label: work_button.clone(),
            mark,
            expanded,
            expandable: *expandable,
            actions,
            steps,
        });
        let mut row = div().flex().flex_col().w_full().min_w_0();
        let mut first_gap = 0.0;
        if !expanded
            && let Some(strip) = self.read_picture_strip(
                &format!("{key}:read"),
                &Value::Array(read_images.clone()),
                0.0,
                p.background,
                p,
                cx,
            )
        {
            // The closed work keeps the pictures it read in view (gx-chat-core `read_images.rs`).
            row = row.child(strip);
            first_gap = 4.0;
        }
        let mut parts: Vec<(AnyElement, bool)> = Vec::new();
        self.hide_file_changes = true;
        for turn in turns {
            let turn_key = format!("work:{}", turn.id);
            parts.extend(self.work_tail_parts(
                WorkTail {
                    key: &turn_key,
                    artifacts: &turn.artifacts,
                    questions: &turn.questions,
                    sent_messages: &turn.sent_messages,
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
        let mut spaced = spaced_tail(parts, false, None, p);
        if let Some((_, gap)) = spaced.first_mut() {
            *gap = first_gap;
        }
        row = row.children(
            spaced
                .into_iter()
                .map(|(part, gap)| div().pt(px(gap * s)).child(part)),
        );
        // No reply took the button: it stands on its own row.
        if let Some(left) = self.hidden_work.take() {
            let button = self.work_button(&left, 20.0, p, cx);
            row = row
                .child(div().h(px(20.0 * s)).flex().items_center().child(button))
                .children(left.steps);
        }
        row.into_any_element()
    }

    /// The work button: the check (the cross when a call failed), what the core's `workButton`
    /// says, and a chevron, muted like the hover row's icons. Pressing it opens or closes the steps.
    pub(super) fn work_button(
        &self,
        work: &HiddenWork,
        height: f32,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let tone = p.muted.opacity(super::message_spacing::FOOTER_DIM);
        let mark = match work.mark {
            QuietMark::Failed => svg()
                .path("titlebar/x.svg")
                .size(px(12.0 * s))
                .flex_shrink_0()
                .text_color(p.error()),
            _ => svg()
                .path("titlebar/check.svg")
                .size(px(12.0 * s))
                .flex_shrink_0()
                .text_color(p.success().opacity(0.75)),
        };
        let chevron = svg()
            .path(if work.expanded {
                "titlebar/chevron-down.svg"
            } else {
                "titlebar/chevron-right.svg"
            })
            .size(px(12.0 * s))
            .flex_shrink_0()
            .text_color(tone);
        let key = work.key.clone();
        let expanded = work.expanded;
        let actions = work.actions.clone();
        div()
            .id(SharedString::from(format!("{}:work-button", work.key)))
            .role(gpui::Role::Button)
            .aria_label(format!("Show what the agent did: {}", work.label))
            .aria_expanded(expanded)
            .tab_index(0)
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(5.0 * s))
            .h(px(height * s))
            .px(px(6.0 * s))
            .ml(px(-6.0 * s))
            .rounded(px(4.0 * s))
            .whitespace_nowrap()
            .text_size(px(12.0 * s))
            .text_color(tone)
            .when(expanded, |button| {
                button.bg(p.foreground.opacity(0.05)).text_color(p.muted)
            })
            .child(mark)
            .child(work.label.clone())
            .child(chevron)
            .when(work.expandable, |button| {
                button
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.05)).text_color(p.muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_quiet_line(&key, expanded, &actions, cx);
                    }))
            })
            .into_any_element()
    }
}
