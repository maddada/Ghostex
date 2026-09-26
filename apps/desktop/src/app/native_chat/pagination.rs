//! Paging older turns into the transcript.
//!
//! React had no standing "load earlier" button inside a filled transcript: the
//! reader scrolls and the next page starts on its own near the top, then the
//! keyed prepend leaves the rows they were reading exactly where they were
//! (session-chat-message-list/list.tsx, use-session-chat-scroll-restoration.ts).
//! The native list follows the same two rules here, and the button stays only
//! for the one case React kept it: a transcript with no rows yet.

use super::state::NativeChatView;
use gpui::{Context, ListOffset, px};
use serde_json::json;

/// React's `LOAD_EARLIER_SCROLL_TOP_PX`: how close to the top starts the next page.
const LOAD_EARLIER_SCROLL_TOP_PX: f32 = 320.0;

impl NativeChatView {
    /// Start the next page once the reader is near the top, and again after a
    /// page settles: the prepend may move them off the boundary, but a viewport
    /// that is still short or still near the top continues without another
    /// wheel event.
    pub(super) fn load_earlier_if_near_top(&mut self, cx: &mut Context<Self>) {
        if self.snapshot["hasMore"] != true || self.snapshot["loadingEarlier"] == true {
            return;
        }
        // Before the first layout the list reports a scroll offset of zero for a
        // transcript it has not measured, which would page the whole history in
        // while the first rows are still arriving.
        if self.list.item_count() == 0 || self.list.viewport_bounds().size.height <= px(0.0) {
            return;
        }
        let scale = super::appearance::ChatAppearance::current(&self.snapshot).scale;
        let offset = -self.list.scroll_px_offset_for_scrollbar().y;
        if offset < px(LOAD_EARLIER_SCROLL_TOP_PX * scale) {
            self.invoke(json!({"type":"loadEarlier"}), cx);
        }
    }

    /// Apply the host's transcript window and keep the reader on the row they
    /// were reading.
    ///
    /// gpui's list already shifts a scroll anchor that sits after the replaced
    /// window, which covers an append. A prepend replaces the head instead, and
    /// the anchor is usually inside it (the reader is near the top, which is
    /// what started the page), where the list would reset them to row zero, that
    /// is, to the oldest turn just loaded. The replaced rows reappear at the end
    /// of the inserted block, so the same row is found by measuring back from
    /// that end, keeping the offset inside it.
    pub(crate) fn splice_transcript(&mut self, start: usize, end: usize, inserted: usize) {
        let anchor = (!self.list.is_following_tail()).then(|| self.list.logical_scroll_top());
        self.list.splice(start..end, inserted);
        if let Some(anchor) = anchor
            && anchor.item_ix >= start
            && anchor.item_ix < end
        {
            self.list.scroll_to(ListOffset {
                item_ix: (start + inserted).saturating_sub(end - anchor.item_ix),
                offset_in_item: anchor.offset_in_item,
            });
        }
    }
}
