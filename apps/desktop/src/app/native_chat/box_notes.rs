//! The quiet lines inside the chat box, between its text and its toolbar: who else is typing in
//! a shared chat. They were lines over the box; the dock above it is the one bar there.
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
        let typing = self.collab_typing(cx)?;
        Some(
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
                .into_any_element(),
        )
    }
}
