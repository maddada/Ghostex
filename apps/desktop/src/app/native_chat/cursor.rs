//! The one place the native chat view decides what the mouse cursor looks like.
//!
//! Every chat element that used to call `.cursor_pointer()` calls
//! [`ChatCursor::chat_cursor_pointer`] instead, so a future chat element inherits the rule by
//! using the same helper. Flip the body of that one method to bring the hand cursor back
//! everywhere at once.

use gpui::Styled;

/// CDXC:SessionChat 2026-09-20 DECISION:
/// User: "let's please just use default pointer in the chat view of the gpui app when hovering over text and reference pills ... still allow selection of course. Don't change cursor in the gpui chat view pls."
/// So the whole GPUI chat view keeps the plain arrow: no I-beam over transcript text, no hand over reference pills, links, buttons, disclosures, tool rows, file cards or the chat's own child windows. Only the cursor shape changes; drag selection, double and triple click selection, link clicks and right-click menus are untouched, the transcript's `TextViewStyle::default_cursor` carries the same rule into gpui-component's text rendering, and the composer text input keeps its I-beam because React's `.ghostex-chat-composer-lexical-content` does (`cursor: text`).
/// Superseding the 2026-09-19 version of this decision, the user asked on 2026-09-20 for one exception: the picture in the full-size image preview shows a zoom-in or zoom-out cursor while a click still changes its size (`image_viewer/render.rs`). Thumbnails and every other chat element keep the arrow.
pub(crate) trait ChatCursor: Styled + Sized {
    /// What a chat control asks for where it would otherwise call `.cursor_pointer()`.
    ///
    /// The arrow is requested explicitly rather than left unset, so a chat control nested in a
    /// region that does set a cursor (the composer input's I-beam) still shows the arrow: GPUI
    /// resolves the cursor from the last hovered hitbox that asked for one, and a child paints
    /// after its ancestors.
    fn chat_cursor_pointer(self) -> Self {
        self.cursor_default()
    }
}

impl<E: Styled + Sized> ChatCursor for E {}
