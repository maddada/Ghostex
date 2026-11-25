//! The reply arrow on an Inbox row and a Work panel card: the small reply form at the press, or,
//! with Settings' Inbox reply arrow set to the chat (`chatInboxReplyArrow` set to `chat`, the core's
//! `inboxReplyInChat`), what the row answers straight into the chat box as Reply in chat box puts
//! it there (`reply_popover::answer_pill`).

use super::state::NativeChatView;
use gpui::{Context, Window};
use serde_json::Value;

impl NativeChatView {
    /// A row's reply arrow (`command` is the Reply popover's: `{question, at}`).
    ///
    /// CDXC:SessionChat 2026-11-26 DECISION:
    /// A tester (issue #771): the reply arrow's small form is "two steps" before Reply in chat box
    /// or Option+Enter, "I personally always want to reply in chat". User: "add toggle in settings
    /// for my friend", then (Q5016) a two-choice row in Settings > Chat's main list under Open Live
    /// pages in: "Inbox reply arrow: Open the reply form / Reply in the chat". Open the reply form
    /// by default, so nothing changes for anyone else; Reply in the chat makes the arrow do what
    /// Reply in chat box does with no words typed: the row's pill at the end of the chat box, and
    /// the chat box focused. A row with no tag to name (a page that only waits to be seen) still
    /// opens the form. The phone follows the computer's choice
    /// (apps/mobile/app/src/chat/native/cards/workItemParts.tsx `openWorkReply`).
    pub(super) fn open_row_reply(
        &mut self,
        command: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pill = (self.snapshot["inboxReplyInChat"] == true)
            .then(|| super::reply_popover::question_reply_of(command))
            .flatten()
            .and_then(|question| super::reply_popover::answer_pill(&question));
        let Some(pill) = pill else {
            self.open_reply_popover(command, window, cx);
            return;
        };
        self.close_reply_popover(cx);
        // The row's floating panel, when the arrow was its Reply, steps aside for the chat box.
        self.close_dock_ask_modal(cx);
        self.append_prompt(&pill, cx);
        // Pressed in the floating Work card or another window: the chat's own window comes
        // forward, as the form's Reply in chat box brings it.
        if let Some(main) = self
            .main_window
            .filter(|main| *main != window.window_handle())
        {
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            });
        }
    }
}
