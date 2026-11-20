//! The quiet lines inside the chat box, between its text and its toolbar: the warning a held Send
//! of something that looks like a key gets, the note that such a draft stays on this device
//! (secret_send_warning.rs), and who else is typing in a shared chat. They were lines over the
//! box; the dock above it is the one bar there.
//!
//! SEE-ALSO: packages/gx-chat-core/src/extras/dock_stack.rs (CDXC:SessionChat 2026-11-20, the
//! one-bar decision), apps/mobile/app/src/chat/native/composer/BoxNotes.tsx (the phone's).

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

impl NativeChatView {
    /// The box's notes, or `None` when it has none.
    pub(super) fn render_box_notes(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let typing = self.collab_typing(cx).map(|typing| {
            div()
                .id("collab-typing")
                .role(gpui::Role::Status)
                .aria_label(typing.clone())
                .w_full()
                .min_w_0()
                .text_size(px(12.0 * s))
                .line_height(px(16.0 * s))
                .text_color(p.muted)
                .child(typing)
                .into_any_element()
        });
        let lines: Vec<AnyElement> = [
            self.render_secret_send_warning(p),
            self.render_draft_kept_note(p),
            typing,
        ]
        .into_iter()
        .flatten()
        .collect();
        (!lines.is_empty()).then(|| {
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .children(lines)
                .into_any_element()
        })
    }
}
