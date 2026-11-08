use super::tone_card::{ToneButton, ToneCardParts, command_box, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, StatefulInteractiveElement as _};
use serde_json::{Value, json};

impl NativeChatView {
    /// The approval card, in the tone card's approval tone: a plain-words title from the chat core
    /// ("Claude Code wants to run a command"), the tool and folder under it, the command, then
    /// Copy on the left and Deny / Allow on the right with their keyboard hints (keyboard.rs).
    pub(super) fn render_approval(
        &self,
        prompt: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let card = &self.snapshot["questionCard"];
        // An answered approval waits here, dimmed and held, until its prompt is gone
        // (`approvalPending`, the chat core's questions/pending_approval.rs).
        let pending = self.snapshot["approvalPending"]["card"] == "prompt";
        let busy = pending || card["busy"] == true;
        let title = Some(text(card, "approvalTitle"))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Approval request".to_string());
        let summary = text(prompt, "summary");
        let tool = text(prompt, "tool");
        let mut body = Vec::new();
        if summary.is_empty() {
            // Nothing to show but the question itself, so the body asks it.
            let ask = Some(text(card, "approvalAsk"))
                .filter(|ask| !ask.is_empty())
                .unwrap_or_else(|| "Allow this tool?".to_string());
            body.push(gpui::div().child(ask).into_any_element());
        } else {
            body.push(command_box(
                summary.clone(),
                crate::app::native_chat::tone_card::is_shell_tool(&tool),
                p,
            ));
        }
        if card["reconnecting"] == true {
            body.push(super::question::reconnecting_note(p));
        }
        let mut footer_start = Vec::new();
        if pending {
            footer_start.push(super::tone_card::pending_answer_note(
                "approval-answer-pending",
                &text(&self.snapshot["approvalPending"], "sending"),
                p,
            ));
        } else if !summary.is_empty() {
            footer_start.push(
                self.tone_button(
                    "approval-copy",
                    "Copy",
                    Some("titlebar/copy.svg"),
                    ToneButton::Secondary,
                    None,
                    false,
                    p,
                )
                .on_click(move |_, _, cx| {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_string(summary.clone()),
                        cx,
                    );
                })
                .into_any_element(),
            );
        }
        let hints = self.snapshot["showShortcutLabels"] != false;
        let footer_end = [
            ("approval-deny", "Deny", ToneButton::Secondary, "Esc", ""),
            (
                "approval-allow",
                "Allow",
                ToneButton::Primary,
                if super::keyboard::chat_mac_keys() {
                    "⌘ ↵"
                } else {
                    "Ctrl ↵"
                },
                "1",
            ),
        ]
        .into_iter()
        .map(|(id, label, kind, hint, send)| {
            let button = self.tone_button(
                id,
                label,
                None,
                kind,
                hints.then(|| hint.to_string()),
                busy,
                p,
            );
            if busy {
                button.into_any_element()
            } else {
                let action =
                    json!({"type":"answer","answer":{"kind":"approval","approvalSend":send}});
                button
                    .on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
                    .into_any_element()
            }
        })
        .collect();
        // CDXC:SessionChat 2026-10-14 DECISION: User (2026-09-07): card X buttons match Open terminal's (now Terminal view) color, outline and background, rather than a custom border and background of their own. Since "always use filled buttons everywhere" (CDXC:AppModal 2026-10-14 on `neutral_button_fills`) that is the neutral fill with no outline.
        let dismiss = self
            .tone_icon_button("approval-cancel", "titlebar/x.svg", "Dismiss", p)
            .on_click(
                cx.listener(|this, _, _, cx| this.invoke(json!({"type":"questionCancel"}), cx)),
            )
            .into_any_element();
        self.tone_card(
            ToneCardParts {
                look: tone_look("approval", p),
                title,
                detail: text(card, "approvalDetail"),
                trailing: if pending { Vec::new() } else { vec![dismiss] },
                body: super::tone_card::dim_pending(body, pending),
                footer_start,
                footer_note: None,
                footer_end,
            },
            None,
            None,
            p,
        )
    }
}
