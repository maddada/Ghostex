use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum ReplyAction {
    Copy,
    Annotate,
    SaveMarkdown,
}

impl NativeChatView {
    pub(super) fn reply_focus(
        &self,
        message: &Value,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<(gpui::FocusHandle, bool)> {
        if !self.has_reply_actions(message) {
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

    pub(super) fn has_reply_actions(&self, message: &Value) -> bool {
        message["role"] == "assistant"
            && message["actionContent"]["copyable"] == true
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
    /// React's `CopyFooter` and `.ghostex-chat-message-actions` in packages/core-ui/styles/chat.css carried the user decision this mirrors: a final reply's actions sit in a row below it, Copy, Reply by Annotating, Save to md, then the time it arrived, starting at the prose column.
    pub(super) fn reply_actions(
        &self,
        message: &Value,
        focused: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = text(message, "id");
        let markdown = text(message, "copyText");
        let icon_color = if p.light { p.muted } else { p.primary };
        let hover_background = if p.light {
            gpui::rgb(0xfafafa).into()
        } else {
            gpui::Hsla::from(gpui::rgb(0xffffff)).opacity(5.0 / 255.0)
        };
        let button = |action: ReplyAction, label: &'static str, icon: &'static str| {
            let click_markdown = markdown.clone();
            div()
                .id(format!("reply-action:{id}:{label}"))
                .group("native-chat-reply-action")
                .role(gpui::Role::Button)
                .aria_label(label)
                .tab_index(0)
                .size(px(24.0 * p.scale))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0 * p.scale))
                .border(px(p.scale))
                .border_color(gpui::transparent_black())
                .chat_cursor_pointer()
                .focus_visible(|mut style| {
                    style = style.border_color(p.ring);
                    style.box_shadow = Some(vec![gpui::BoxShadow {
                        color: p.ring.opacity(0.2),
                        offset: gpui::point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(3.0 * p.scale),
                        inset: false,
                    }]);
                    style
                })
                .hover(|style| style.bg(hover_background))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                })
                .child(
                    svg()
                        .path(icon)
                        .size(px(12.0 * p.scale))
                        .text_color(icon_color)
                        .group_hover("native-chat-reply-action", |style| {
                            style.text_color(p.foreground)
                        }),
                )
                .on_click(cx.listener(move |chat, _, _, cx| {
                    chat.perform_reply_action(action, &click_markdown, cx)
                }))
        };
        let can_annotate =
            self.config.app.is_some() && message["actionContent"]["canAnnotate"] == true;
        let can_save = message["actionContent"]["canSaveMarkdown"] == true;
        let buttons = div()
            .flex()
            .items_center()
            .gap(px(2.0 * p.scale))
            .child(button(
                ReplyAction::Copy,
                "Copy message",
                "chat-actions/copy",
            ))
            .when(can_annotate, |actions| {
                actions.child(button(
                    ReplyAction::Annotate,
                    "Reply by Annotating",
                    "chat-actions/annotate",
                ))
            })
            .when(can_save, |actions| {
                actions.child(button(
                    ReplyAction::SaveMarkdown,
                    "Save message to Markdown",
                    "chat-actions/save",
                ))
            });
        // The prose column: the marker's 2px inset, its 16px slot, and the 6px gap before the text.
        message_actions_row(p, focused)
            .pl(px(24.0 * p.scale))
            .tab_group()
            .child(buttons)
            .when_some(message_time(&id, message, p), |row, time| row.child(time))
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
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = text(message, "id");
        let prompt = text(message, "copyText");
        // Nothing to copy, save, or rewind to: an empty prompt has no action row at all.
        if prompt.is_empty() {
            return None;
        }
        let saved = self.snapshot["savedPrompts"][id.as_str()]
            .as_str()
            .unwrap_or("");
        // A rewind types into the session's own pane, so a child transcript never offers one
        // (React's subagent viewer mounted its list without `rewindToMessage` for the same reason).
        let rewindable = !self.in_subagent
            && message["canRewind"] == true
            && self.snapshot["rewindAvailable"] == true
            && self.snapshot["rewindEnabled"] == true;
        // React passed `onSavePrompt` only when the host had a stash bridge, the same capability
        // behind the composer's Stash control, so a host without one offers Copy alone.
        let savable = self.snapshot["composerActions"]["stash"] == true;
        /*
        CDXC:SessionChat 2026-09-19 SEE-ALSO:
        `.ghostex-chat-message-actions` in packages/core-ui/styles/chat.css carries the user decision
        this mirrors: the prompt's actions sit right-aligned below the bubble, led by the time it
        was sent, in the order Rewind, Save prompt, Copy, so Copy lands at the bubble's edge as in
        t3code.
        */
        let button = |key: &str, label: String, icon: &'static str, action: Value| {
            let icon_color = p.muted;
            div()
                .id(gpui::SharedString::from(format!("{key}:{id}")))
                .group("native-chat-user-action")
                .role(gpui::Role::Button)
                .aria_label(label.clone())
                .tab_index(0)
                .size(px(24.0 * p.scale))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0 * p.scale))
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.border.opacity(0.4)))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                })
                .child(
                    svg()
                        .path(icon)
                        .size(px(12.0 * p.scale))
                        .text_color(icon_color)
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
        };
        let buttons = div()
            .flex()
            .items_center()
            .gap(px(2.0 * p.scale))
            .when(rewindable, |buttons| {
                buttons.child(button(
                    "rewind",
                    "Rewind to here".into(),
                    "chat-actions/rewind",
                    json!({"type":"rewindOpen","messageId":id.clone(),"prompt":prompt.clone()}),
                ))
            })
            .when(savable, |buttons| {
                buttons.child(button(
                    "save-prompt",
                    match saved {
                        "saved" => "Prompt saved",
                        "saving" => "Saving prompt",
                        "error" => "Could not save prompt. Click to retry.",
                        _ => "Save prompt",
                    }
                    .into(),
                    if saved == "saved" {
                        "chat-actions/saved"
                    } else {
                        "chat-actions/savePrompt"
                    },
                    json!({"type":"savePrompt","messageId":id.clone(),"prompt":prompt.clone()}),
                ))
            })
            .child(button(
                "copy",
                "Copy message".into(),
                "chat-actions/copy",
                json!({"type":"copyPrompt","text":prompt.clone()}),
            ));
        Some(
            message_actions_row(p, false)
                .pr(px(4.0 * p.scale))
                .when_some(message_time(&id, message, p), |row, time| row.child(time))
                .child(buttons)
                .into_any_element(),
        )
    }

    fn perform_reply_action(
        &mut self,
        action: ReplyAction,
        markdown: &str,
        cx: &mut Context<Self>,
    ) {
        match action {
            ReplyAction::Copy => {
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(markdown.to_owned()),
                    cx,
                );
            }
            ReplyAction::SaveMarkdown => {
                self.invoke(json!({"type":"markdownSaveOpen","markdown":markdown}), cx)
            }
            ReplyAction::Annotate => self.host("annotateReply", json!({"markdown":markdown}), cx),
        }
    }
}

/// The row under a message that holds its actions and time; hidden until the message is hovered
/// or, for a reply, holds keyboard focus (React: `.ghostex-chat-message-actions`).
///
/// CDXC:SessionChat 2026-09-19 WHY:
/// The row takes no negative margin and no `flex_shrink_0`. The list measures a row at min-content height, where taffy sizes a non-shrinking item with a negative main-axis margin as `basis + basis * margin`, which made a reply's row about 94px too short and drew the next message over it.
fn message_actions_row(p: &ChatAppearance, focused: bool) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * p.scale))
        .opacity(if focused { 1.0 } else { 0.0 })
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
