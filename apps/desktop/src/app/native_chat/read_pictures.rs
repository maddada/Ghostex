//! The pictures an agent opened from disk: the 48px thumbnails under a tool row that read one,
//! and the strip a folded turn keeps of them under its fold row.
//!
//! CDXC:SessionChat 2026-11-02 SEE-ALSO:
//! The user's decision and every rule (which calls read a picture, which pictures a turn keeps, a
//! picture the user already showed) are in packages/gx-chat-core/src/transcript/read_images.rs;
//! this only lays the tiles out. The phone draws the same tiles in
//! apps/mobile/app/src/chat/native/transcript/Images.tsx (`ReadImages`).

use super::{appearance::ChatAppearance, images::VISUAL, state::NativeChatView};
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div, px};
use serde_json::Value;

impl NativeChatView {
    /// A wrapping row of thumbnails, each opening the picture viewer on its picture; a file that
    /// has gone draws the missing-picture square. `key` names the row among the transcript's
    /// pictures, `inset` is the room on its left.
    pub(super) fn read_pictures(
        &mut self,
        key: &str,
        images: &Value,
        inset: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let images = images
            .as_array()
            .filter(|images| !images.is_empty())?
            .clone();
        let s = p.scale;
        let mut row = div()
            .flex()
            .flex_wrap()
            .min_w_0()
            .gap(px(VISUAL.user_row_gap * s))
            .pl(px(inset * s));
        for index in 0..images.len() {
            // The user's square placeholder for a picture that cannot be read, not a named chip.
            row = row.child(self.image_tile(key, &images, index, true, p, cx));
        }
        Some(row.into_any_element())
    }
}
