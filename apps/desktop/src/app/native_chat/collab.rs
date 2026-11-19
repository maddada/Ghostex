//! Ghostex Collab in the chat (`collab` in the chat core's document, `extras/collab.rs`): whose
//! page this is and the guest's note. The owner's "Message from <guest>" and a guest's own
//! messages waiting for the owner are rows of the dock (gx-chat-core `extras/dock_stack.rs`,
//! work_cards.rs), and who is typing is a quiet line inside the chat box (box_notes.rs). The
//! desktop and the web build (owner and guest pages alike) read it from this file.
//!
//! SEE-ALSO: packages/gx-chat-core/src/extras/collab.rs, server/src/server/collab_http.rs.

use super::state::NativeChatView;
use gpui::Context;

impl NativeChatView {
    /// This page is a Ghostex Collab guest's: only the shared chat, nothing the owner answers.
    pub(crate) fn collab_guest(&self) -> bool {
        self.snapshot["collab"]["role"] == "guest"
    }

    /// The guest's line under their chat box ("Message as Alper. Mohamad approves each
    /// message."), or `None` off a guest's page.
    pub(super) fn collab_guest_note(&self) -> Option<String> {
        self.snapshot["collab"]["guestNote"]
            .as_str()
            .filter(|note| !note.is_empty())
            .map(str::to_string)
    }

    /// "Sam is typing", while the chat is shared and someone types, or `None`.
    pub(super) fn collab_typing(&self, _cx: &Context<Self>) -> Option<String> {
        let collab = &self.snapshot["collab"];
        (collab["shared"] == true)
            .then(|| collab["typing"].as_str())
            .flatten()
            .filter(|line| !line.is_empty())
            .map(|line| format!("{line}\u{2026}"))
    }
}
