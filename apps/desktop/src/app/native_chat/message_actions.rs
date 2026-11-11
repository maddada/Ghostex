use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use serde_json::{Value, json};

/// Toggle key in `menu_toggle.rs` for every message's ⋯ menu: a second press on a ⋯ while its
/// menu is up only closes it.
const MESSAGE_MENU_TRIGGER: &str = "chat-message-more";

impl NativeChatView {
    /// The focus a reply tracks for its action row; at Message spacing Compact (`compact`) your
    /// own prompts track one too, so keyboard focus can reveal their footer's buttons
    /// (message_spacing.rs).
    pub(super) fn reply_focus(
        &self,
        message: &Value,
        compact: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<(gpui::FocusHandle, bool)> {
        if !self.has_reply_actions(message) && !(compact && Self::has_user_bar(message)) {
            return None;
        }
        let state = window.use_keyed_state(
            format!("reply-focus:{}", text(message, "id")),
            cx,
            |window, cx| {
                let focus = cx.focus_handle();
                cx.on_focus_in(&focus, window, |_, _, cx| cx.notify())
                    .detach();
                cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify())
                    .detach();
                focus
            },
        );
        let focus = state.read(cx).clone();
        let focused = focus.contains_focused(window, cx);
        Some((focus, focused))
    }

    /// Whether a prompt of yours has an action row: anything to copy.
    pub(super) fn has_user_actions(message: &Value) -> bool {
        !text(message, "copyText").is_empty()
    }

    /// Whether a row of yours draws as a prompt with an action row (bubble, band, rule or names),
    /// not as a message to or from another agent, a page's answers alone, a `!` command or a
    /// status row, which carry no footer at Message spacing Compact.
    pub(super) fn has_user_bar(message: &Value) -> bool {
        message["role"] == "user"
            && message["suppressed"].is_null()
            && !message["interAgentMessage"].is_object()
            && message["shellCommand"] != true
            && !(message["pageAnswers"].is_object() && message["pageAnswers"]["withText"] != true)
            && Self::has_user_actions(message)
    }

    pub(super) fn has_reply_actions(&self, message: &Value) -> bool {
        message["role"] == "assistant"
            && message["actionContent"]["copyable"] == true
            && self.snapshot["finalIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == &message["id"]))
    }

    /// A turn's final answer with no tool calls of its own: it is drawn flush with the chat column,
    /// without the reply dot.
    ///
    /// CDXC:SessionChat 2026-10-09 DECISION: User: the dot before the agent's last message shifted the whole message right, so "the card for the html mockup" was narrower than the chat box; remove the dot for the agent's final reply. Commentary between tool calls keeps its dot, and an answer with tool calls keeps its chevron.
    pub(super) fn is_flush_reply(&self, message: &Value) -> bool {
        message["role"] == "assistant"
            && message["tools"]
                .as_array()
                .is_none_or(|tools| tools.is_empty())
            && self.snapshot["finalIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == &message["id"]))
    }

    pub(super) fn reply_marker(&self, p: &ChatAppearance) -> AnyElement {
        div()
            .relative()
            .w(px(16.0 * p.scale))
            .h(px(22.75 * p.scale))
            .ml(px(2.0 * p.scale))
            .flex_shrink_0()
            .child(
                div()
                    .absolute()
                    .left(px(6.0 * p.scale))
                    .top(px(9.5 * p.scale))
                    .size(px(4.0 * p.scale))
                    .rounded_full()
                    .bg(p.primary),
            )
            .into_any_element()
    }

    /// CDXC:SessionChat 2026-09-19 SEE-ALSO:
    /// React's `CopyFooter` and `.ghostex-chat-message-actions` in packages/core-ui/styles/chat.css (both deleted 2026-09-25) carried the user decision this mirrors: a final reply's actions sit in a row below it, Copy, Reply by Annotating, Save to md, then the time it arrived, starting at the prose column. Read aloud joins the row just before the time (CDXC:VoiceMode below).
    ///
    /// `work` is the reply's work under Agent work Quiet (hidden_work.rs): its button leads the
    /// row, and an opened work keeps the row shown so the button that closes it stays.
    pub(super) fn reply_actions(
        &self,
        message: &Value,
        focused: bool,
        work: Option<&super::hidden_work::HiddenWork>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let id = text(message, "id");
        let _ = focused;
        let compact = p.compact();
        let work_button = work.map(|work| {
            self.work_button(
                work,
                if compact {
                    super::message_spacing::FOOTER_HEIGHT
                } else {
                    20.0
                },
                p,
                cx,
            )
        });
        let rows = self.reply_menu_rows(message);
        let tone = if compact {
            p.muted.opacity(super::message_spacing::FOOTER_DIM)
        } else {
            p.muted
        };
        let more = self.message_more_button(&id, rows, compact, tone, p, cx);
        // The prose column: the marker's 2px inset, its 16px slot, and the 6px gap before the text,
        // or the column's own edge for an answer drawn without the dot.
        let prose_column = if self.is_flush_reply(message) {
            0.0
        } else {
            24.0
        };
        // Message spacing Compact: the work button at the footer's left, from the prose column,
        // the time and the ⋯ at its right (message_spacing.rs).
        if compact {
            return Some(
                super::message_spacing::message_footer(p)
                    .pl(px(prose_column * p.scale))
                    .tab_group()
                    .children(work_button)
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .children(footer_time(&id, message, p))
                            .child(more),
                    )
                    .into_any_element(),
            );
        }
        Some(
            message_actions_row(p)
                .pl(px(prose_column * p.scale))
                .tab_group()
                .children(work_button)
                .children(message_time(&id, message, p))
                .child(more)
                .into_any_element(),
        )
    }

    /// What a final reply's ⋯ menu offers: Copy, Reply by Annotating (in the app, for a reply that
    /// can be annotated), Save to Markdown (where it can be saved) and Read aloud or Stop reading
    /// (while Read aloud is on and this computer has a voice), the actions its row of buttons held.
    ///
    /// CDXC:VoiceMode 2026-10-21 DECISION:
    /// User (Q24): the desktop's Read aloud action goes last among the reply's actions. It reads Stop reading on the reply being read; the core decides both (packages/gx-chat-core/src/read_aloud/actions.rs), and it shows only while Read aloud is on and this computer has a voice (`readAloud.available`).
    fn reply_menu_rows(&self, message: &Value) -> Vec<Value> {
        let id = text(message, "id");
        let markdown = text(message, "copyText");
        let mut rows = vec![json!({
            "label":"Copy message","iconPath":"chat-actions/copy",
            "command":{"type":"copyText","text":markdown},
        })];
        if self.config.app.is_some() && message["actionContent"]["canAnnotate"] == true {
            rows.push(json!({
                "label":"Reply by Annotating","iconPath":"chat-actions/annotate",
                "command":{"type":"host","action":"annotateReply","markdown":markdown},
            }));
        }
        if message["actionContent"]["canSaveMarkdown"] == true {
            rows.push(json!({
                "label":"Save message to Markdown","iconPath":"chat-actions/save",
                "command":{"type":"markdownSaveOpen","markdown":markdown},
            }));
        }
        let read_aloud = &self.snapshot["readAloud"];
        if read_aloud["available"] == true {
            let reading = read_aloud["speakingId"] == message["id"];
            rows.push(json!({
                "label": if reading { "Stop reading" } else { "Read aloud" },
                "iconPath": if reading { "titlebar/player-stop.svg" } else { "titlebar/volume.svg" },
                "command":{"type":"readAloudMessage","messageId":id},
            }));
        }
        rows
    }

    /// The ⋯ after a message's time, in the time's quiet tone: it opens the message's actions in
    /// the chat's own dropdown, at the pointer.
    ///
    /// CDXC:SessionChat 2026-11-11 DECISION:
    /// User: "I don't like these appear as empty space, please make it show the time and 3 dots under messages everywhere. i click the 3 dots to see the current options. this is for user messages and agent last replies that we show time under now". The line under your prompts and under the agent's last reply of a turn is always drawn, at rest and on hover alike: the time (its tooltip has the full date), then a ⋯ in the same quiet tone, which opens the chat's dropdown with every action that line's row of icon buttons offered, each with its icon and name. No row of icon buttons any more, and nothing shows only on hover. Supersedes the 2026-09-19 row that showed on hover (React's `CopyFooter`) and, for these buttons, the 2026-11-07 Compact footer that filled in on hover (packages/gx-chat-core/src/transcript/chat_look.rs).
    fn message_more_button(
        &self,
        id: &str,
        rows: Vec<Value>,
        compact: bool,
        tone: gpui::Hsla,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let size = if compact {
            super::message_spacing::FOOTER_BUTTON
        } else {
            24.0
        };
        div()
            .id(gpui::SharedString::from(format!("message-more:{id}")))
            .group("native-chat-message-more")
            .role(gpui::Role::Button)
            .aria_label("Message actions")
            .tab_index(0)
            .size(px(size * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(if compact { 4.0 } else { 6.0 } * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
            .focus_visible(|mut style| {
                style.box_shadow = Some(vec![gpui::BoxShadow {
                    color: p.ring.opacity(0.2),
                    offset: gpui::point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(3.0 * s),
                    inset: false,
                }]);
                style
            })
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Message actions").build(window, cx)
            })
            .child(
                svg()
                    .path("titlebar/dots.svg")
                    .size(px(14.0 * s))
                    .text_color(tone)
                    .group_hover("native-chat-message-more", |style| {
                        style.text_color(p.foreground)
                    }),
            )
            .on_click(
                cx.listener(move |chat, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    if chat.chat_menu_toggled_shut(MESSAGE_MENU_TRIGGER, cx) {
                        return;
                    }
                    let at = event.position();
                    // A chat drawn in one of its own windows (the floating chat) keeps the menu there.
                    let in_main = chat
                        .open_main_window(cx)
                        .is_none_or(|main| main == window.window_handle());
                    if in_main {
                        chat.show_chat_menu_at(rows.clone(), at, 240.0, window, cx);
                    } else {
                        chat.show_chat_menu_in_window_at(rows.clone(), at, 240.0, window, cx);
                    }
                }),
            )
            .into_any_element()
    }

    /// CDXC:SavedPrompts 2026-09-06 DECISION:
    /// User: add Save prompt between Copy and Rewind on user messages, using the input box's stack-push icon.
    ///
    /// The prompt's own action row (React: `CopyFooter` on a user row). Rewind is offered only when the
    /// host can reach `/api/rewindSessionChat`, the session runs an agent whose rewind Ghostex
    /// drives, and the composer could send right now, because the daemon types the rewind into that
    /// same pane. Which prompt is a rewind target at all is decided in the core
    /// (`canRewind`, packages/gx-chat-core/src/transcript/presentation.rs).
    pub(super) fn user_actions(
        &self,
        message: &Value,
        focused: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = text(message, "id");
        let prompt = text(message, "copyText");
        // Nothing to copy, save, or rewind to: an empty prompt has no action row at all.
        if prompt.is_empty() {
            return None;
        }
        let _ = focused;
        let compact = p.compact();
        let left = p.left_aligned();
        let rows = self.user_menu_rows(message, &id, &prompt);
        let tone = if compact {
            p.muted.opacity(super::message_spacing::FOOTER_DIM)
        } else {
            p.muted
        };
        let more = self.message_more_button(&id, rows, compact, tone, p, cx);
        // The time, then the ⋯, on the message's side: at the bubble's right edge, or from the
        // left edge of a left-aligned message (user_message_look.rs). Compact keeps its footer's
        // height (message_spacing.rs).
        let line = if compact {
            super::message_spacing::message_footer(p)
                .when(!left, |row| row.justify_end().pr(px(4.0 * p.scale)))
                .children(footer_time(&id, message, p))
        } else {
            message_actions_row(p)
                .when(!left, |row| row.pr(px(4.0 * p.scale)))
                .when(left, |row| row.w_full().justify_start())
                .children(message_time(&id, message, p))
        };
        Some(line.child(more).into_any_element())
    }

    /// What a prompt's ⋯ menu offers, the actions its row of buttons held: Copy, Save prompt (where
    /// the host can stash), Rewind to here (where the agent's rewind can run now), and for a
    /// message still waiting for the agent Edit, Back to composer and Delete.
    ///
    /// CDXC:SavedPrompts 2026-09-06 DECISION:
    /// User: add Save prompt between Copy and Rewind on user messages, using the input box's stack-push icon.
    ///
    /// Rewind is offered only when the host can reach `/api/rewindSessionChat`, the session runs an
    /// agent whose rewind Ghostex drives, and the composer could send right now, because the daemon
    /// types the rewind into that same pane. Which prompt is a rewind target at all is decided in
    /// the core (`canRewind`, packages/gx-chat-core/src/transcript/presentation.rs).
    fn user_menu_rows(&self, message: &Value, id: &str, prompt: &str) -> Vec<Value> {
        let saved = self.snapshot["savedPrompts"][id].as_str().unwrap_or("");
        // A rewind types into the session's own pane, so a child transcript never offers one
        // (React's subagent viewer mounted its list without `rewindToMessage` for the same reason).
        let rewindable = !self.in_subagent
            && message["canRewind"] == true
            && self.snapshot["rewindAvailable"] == true
            && self.snapshot["rewindEnabled"] == true;
        // React passed `onSavePrompt` only when the host had a stash bridge, the same capability
        // behind the composer's Stash control, so a host without one offers Copy alone.
        let savable = self.snapshot["composerActions"]["stash"] == true;
        // A message gxserver still holds for the agent can be taken back the way a queued row can
        // (CDXC:SessionChat 2026-10-11 in server/src/session_chat_send_wake.rs).
        let waiting = &message["startupDelivery"];
        let waiting_prompt = (waiting["state"] == "queued").then(|| waiting["promptId"].clone());
        let queue_capabilities = &self.snapshot["queue"]["capabilities"];
        let editable = waiting_prompt.is_some() && queue_capabilities["canEdit"] == true;
        let removable = waiting_prompt.is_some() && queue_capabilities["canRemove"] == true;
        // A message the agent itself queued comes back to the composer (CDXC:SessionChat 2026-10-11
        // in packages/gx-chat-core/src/composer/queue_take_back.rs); the composer's Up key takes the
        // newest one back (keyboard.rs).
        let takeable = !self.in_subagent
            && self.snapshot["queueTakeBackIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|queued| queued == &message["id"]));
        let taking_back = self.snapshot["queueTakeBackBusy"] == message["id"];
        let mut rows = vec![json!({
            "label":"Copy message","iconPath":"chat-actions/copy",
            "command":{"type":"copyText","text":prompt},
        })];
        if savable {
            rows.push(json!({
                "label": match saved {
                    "saved" => "Prompt saved",
                    "saving" => "Saving prompt",
                    "error" => "Could not save prompt. Click to retry.",
                    _ => "Save prompt",
                },
                "iconPath": if saved == "saved" { "chat-actions/saved" } else { "chat-actions/savePrompt" },
                "command":{"type":"savePrompt","messageId":id,"prompt":prompt},
            }));
        }
        if rewindable {
            rows.push(json!({
                "label":"Rewind to here","iconPath":"chat-actions/rewind",
                "command":{"type":"rewindOpen","messageId":id,"prompt":prompt},
            }));
        }
        if editable {
            rows.push(json!({
                "label":"Edit message","iconPath":"titlebar/pencil.svg",
                "command":{"type":"removeQueue","promptId":waiting_prompt.clone(),"edit":true},
            }));
        }
        if takeable {
            rows.push(json!({
                "label": if taking_back { "Taking the message back\u{2026}" } else { "Back to composer" },
                "iconPath": if taking_back { "titlebar/loader2.svg" } else { "titlebar/arrow-back-up.svg" },
                "detail": text(&self.snapshot, "queueTakeBackHotkey"),
                "disabled": taking_back,
                "command":{"type":"takeBackQueued","messageId":id},
            }));
        }
        if removable {
            rows.push(json!({
                "label":"Delete message","iconPath":"titlebar/trash.svg",
                "command":{"type":"removeQueue","promptId":waiting_prompt},
            }));
        }
        rows
    }

    /// One of a prompt's action buttons: a 24px square with a 12px icon, a hover fill and a
    /// tooltip (a 20px square with a dimmer icon in a Compact footer, `compact`). `action` is
    /// `{"type":"copyPrompt","text":…}` for a copy, or any chat action.
    #[allow(clippy::too_many_arguments)]
    fn user_action_button(
        id: &str,
        key: &str,
        label: String,
        icon: &'static str,
        action: Value,
        compact: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (size, radius, tone) = if compact {
            (
                super::message_spacing::FOOTER_BUTTON,
                4.0,
                p.muted.opacity(super::message_spacing::FOOTER_DIM),
            )
        } else {
            (24.0, 6.0, p.muted)
        };
        div()
            .id(gpui::SharedString::from(format!("{key}:{id}")))
            .group("native-chat-user-action")
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .tab_index(0)
            .size(px(size * p.scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius * p.scale))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(12.0 * p.scale))
                    .text_color(tone)
                    .group_hover("native-chat-user-action", |style| {
                        style.text_color(p.foreground)
                    }),
            )
            .on_click(cx.listener(move |chat, _, _, cx| {
                if action["type"] == "copyPrompt" {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_string(text(&action, "text")),
                        cx,
                    );
                } else {
                    chat.invoke(action.clone(), cx);
                }
            }))
    }

    /// The Copy button beside a `!` command's bubble: the same button an ordinary prompt's action
    /// row ends with, hidden until the message row is hovered, and it copies the bare command.
    ///
    /// CDXC:SessionChat 2026-10-12 DECISION: User: "please move the copy button to be to the left of the command chat bubble (outside it floating, like it is for other messages)". The gx-chat-core side of this decision is in transcript/local_command.rs.
    pub(super) fn shell_command_copy_action(
        &self,
        message: &Value,
        command: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = text(message, "id");
        hover_actions_row(p)
            .child(Self::user_action_button(
                &id,
                "copy",
                "Copy message".into(),
                "chat-actions/copy",
                json!({"type":"copyPrompt","text":command}),
                false,
                p,
                cx,
            ))
            .into_any_element()
    }
}

/// The line under a message that holds its time and ⋯, always drawn, so a message is as tall at
/// rest as on hover (`message_more_button` has the user's words).
///
/// CDXC:SessionChat 2026-09-19 WHY:
/// The row takes no negative margin and no `flex_shrink_0`. The list measures a row at min-content height, where taffy sizes a non-shrinking item with a negative main-axis margin as `basis + basis * margin`, which made a reply's row about 94px too short and drew the next message over it.
fn message_actions_row(p: &ChatAppearance) -> gpui::Div {
    div().flex().items_center().gap(px(4.0 * p.scale))
}

/// The hover row a `!` command's floating Copy sits in: it shows while the message is hovered.
fn hover_actions_row(p: &ChatAppearance) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * p.scale))
        .opacity(0.0)
        .group_hover("native-chat-message", |style| style.opacity(1.0))
}

/// The label from packages/gx-chat-core/src/transcript/message_time.rs, with its long form
/// as the tooltip.
fn message_time(id: &str, message: &Value, p: &ChatAppearance) -> Option<AnyElement> {
    let label = text(&message["time"], "label");
    if label.is_empty() {
        return None;
    }
    let title = gpui::SharedString::from(text(&message["time"], "title"));
    Some(
        div()
            .id(gpui::SharedString::from(format!("message-time:{id}")))
            .flex_shrink_0()
            .whitespace_nowrap()
            .text_size(px(12.0 * p.scale))
            .text_color(p.muted)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
            })
            .child(label)
            .into_any_element(),
    )
}

/// The time in a Compact footer: [`message_time`] at 11.5px, as dim as the footer's icons, 6px
/// after them.
fn footer_time(id: &str, message: &Value, p: &ChatAppearance) -> Option<AnyElement> {
    let label = text(&message["time"], "label");
    if label.is_empty() {
        return None;
    }
    let title = gpui::SharedString::from(text(&message["time"], "title"));
    Some(
        div()
            .id(gpui::SharedString::from(format!("message-time:{id}")))
            .flex_shrink_0()
            .whitespace_nowrap()
            .ml(px(6.0 * p.scale))
            .text_size(px(11.5 * p.scale))
            .text_color(p.muted.opacity(super::message_spacing::FOOTER_DIM))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
            })
            .child(label)
            .into_any_element(),
    )
}
