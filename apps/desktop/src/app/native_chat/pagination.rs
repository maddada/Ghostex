//! Paging older turns into the transcript.
//!
//! React had no standing "load earlier" button inside a filled transcript: the
//! reader scrolls and the next page starts on its own near the top, then the
//! keyed prepend leaves the rows they were reading exactly where they were
//! (session-chat-message-list/list.tsx, use-session-chat-scroll-restoration.ts).
//! The native list follows the same two rules here, and the button stays only
//! for the one case React kept it: a transcript with no rows yet.

use super::state::NativeChatView;
use ghostex_gx_chat_core::TranscriptItem;
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

    /// The identities of the row the reader's scroll position is anchored in (its own, then every
    /// message it holds), read before the host's window replaces it; empty while the list follows
    /// its tail, which keeps no anchor.
    pub(crate) fn transcript_anchor_key(&self) -> Vec<String> {
        if self.list.is_following_tail() {
            return Vec::new();
        }
        self.items
            .get(self.list.logical_scroll_top().item_ix)
            .map(row_ids)
            .unwrap_or_default()
    }

    /// Apply the host's transcript window and keep the reader on the row they
    /// were reading.
    pub(crate) fn splice_transcript(&mut self, start: usize, end: usize, inserted: usize) {
        self.splice_transcript_keyed(start, end, inserted, Vec::new());
    }

    /// gpui's list already shifts a scroll anchor that sits after the replaced
    /// window, which covers an append. An anchor inside the window would be reset to
    /// the window's first row, so the anchored row is found again in the inserted rows
    /// by `anchor_key` (its identities before the splice: the same row, else the row that now
    /// holds one of its messages, as when a finished turn regroups into a summary), keeping the
    /// offset inside it.
    /// A row without an identity falls back to its position: a prepend replaces the
    /// head and the replaced rows reappear at the end of the inserted block, so a
    /// window at the head measures back from that end; any other window keeps the
    /// row's index.
    ///
    /// CDXC:SessionChat 2026-11-02 WHY: The core's splices are a common prefix and suffix diff, so a new turn arriving under an open question form replaces the rows from the form down, and OpenCode's regrouping rewrites the whole window with the form one index earlier. Remapping by index moved the anchor off the form onto the new message row, and the list then filled the viewport up from the bottom, sliding the form 60px under the caret of its focused note field (Claude and OpenCode). The anchored row is found by identity instead.
    pub(crate) fn splice_transcript_keyed(
        &mut self,
        start: usize,
        end: usize,
        inserted: usize,
        anchor_key: Vec<String>,
    ) {
        let anchor = (!self.list.is_following_tail()).then(|| self.list.logical_scroll_top());
        self.list.splice(start..end, inserted);
        if let Some(anchor) = anchor
            && anchor.item_ix >= start
            && anchor.item_ix < end
        {
            let window = start..start + inserted;
            let same_row = anchor_key.first().and_then(|key| {
                window.clone().find(|&at| {
                    self.items
                        .get(at)
                        .and_then(|row| row_ids(row).into_iter().next())
                        .is_some_and(|id| &id == key)
                })
            });
            let found = same_row.or_else(|| {
                window.clone().find(|&at| {
                    self.items
                        .get(at)
                        .is_some_and(|row| row_ids(row).iter().any(|id| anchor_key.contains(id)))
                })
            });
            let item_ix = match found {
                Some(at) => at,
                None if start == 0 => (start + inserted).saturating_sub(end - anchor.item_ix),
                None if inserted == 0 => start,
                None => anchor.item_ix.min(start + inserted - 1),
            };
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: anchor.offset_in_item,
            });
        }
    }
}

/// A transcript row's identities: its own id first (the message id, or the id of a turn summary
/// or a completed-work group), then the ids of the messages it holds.
fn row_ids(item: &TranscriptItem) -> Vec<String> {
    let id = |message: &serde_json::Value| message["id"].as_str().map(str::to_owned);
    match item {
        TranscriptItem::Message { message } => id(message).into_iter().collect(),
        TranscriptItem::Summary {
            id: own,
            user,
            final_message,
            earlier_replies,
            work,
            outcome,
            cards,
            ..
        } => std::iter::once(Some(own.clone()))
            .chain([id(user)])
            .chain(final_message.iter().map(|message| id(message)))
            .chain(
                earlier_replies
                    .iter()
                    .chain(work)
                    .chain(outcome)
                    .chain(cards)
                    .map(|message| id(message)),
            )
            .flatten()
            .collect(),
        TranscriptItem::CompletedWork {
            id: own,
            work,
            artifacts,
            final_message,
            ..
        } => std::iter::once(Some(own.clone()))
            .chain(final_message.iter().map(|message| id(message)))
            .chain(work.iter().chain(artifacts).map(|message| id(message)))
            .flatten()
            .collect(),
        TranscriptItem::Unknown(value) => id(value).into_iter().collect(),
    }
}
