//! Keeping a disclosure's header where the reader clicked it.
//!
//! CDXC:SessionChat 2026-09-26 DECISION:
//! User: when a collapsed area is expanded, keep the scroll anchored so that area's header stays at the same spot on screen, instead of sending the reader to the bottom of what opened; they read it from the top.
//! Two things moved the header. The toggles remeasured the whole list with gpui's proportional anchor, which keeps the top row's offset as a fraction of its height, so a long row partly scrolled off the top pushed the reader deep into what it grew; the remeasure now keeps pixels. And a transcript following its tail pins the bottom, so a body growing below the header pushed the header up and away; following pauses for the motion and is handed back afterwards, re-engaging only once the reader is at the bottom again.
//! CDXC:SessionChat 2026-11-08 DECISION: "+N previous tool calls" used to be the one toggle whose revealed rows appeared above it, so it scrolled by what they grew; now it opens to the top of the group and its rows grow below it like every other disclosure, so that special case is gone.

use super::state::NativeChatView;
use gpui::{FollowMode, Window};

impl NativeChatView {
    /// Every transcript disclosure calls this when the reader toggles it, before the open state
    /// flips. `key` is the disclosure's key.
    pub(super) fn anchor_disclosure_toggle(&mut self, key: &str, _opening: bool) {
        let mut motions = self.disclosure_motion.borrow_mut();
        if self.list.is_following_tail() && !motions.follow_paused {
            // After a layout the list already holds the real top of the tail it followed, so
            // stopping here leaves every row where it was.
            self.list.set_follow_mode(FollowMode::Normal);
            motions.follow_paused = true;
        }
        motions.awaiting = Some((key.to_string(), web_time::Instant::now()));
        drop(motions);
        let count = self.list.item_count();
        self.list.remeasure_items(0..count);
    }

    /// Before the list lays out: hand tail following back once nothing is moving.
    pub(super) fn sync_disclosure_anchor(&mut self, window: &mut Window) {
        let mut motions = self.disclosure_motion.borrow_mut();
        // A toggled row that is never drawn again (it scrolled away, the transcript changed) must
        // not hold following off for good.
        if motions
            .awaiting
            .as_ref()
            .is_some_and(|(_, pressed)| pressed.elapsed().as_millis() > 1000)
        {
            motions.awaiting = None;
        }
        if motions.follow_paused {
            if motions.running() || motions.awaiting.is_some() {
                window.request_animation_frame();
            } else {
                motions.follow_paused = false;
                drop(motions);
                // An answer clicked meanwhile keeps the list still (answer_anchor.rs).
                if self.answer_hold_active() {
                    return;
                }
                // Tail mode again without jumping: scrolling to the current top leaves it waiting
                // until the reader is back at the bottom, where the list re-engages on its own. A
                // reader still at the bottom keeps following, so rows arriving with this frame
                // are not left below the fold.
                let top = self.list.logical_scroll_top();
                let at_bottom = !super::scroll_bottom::transcript_left_bottom(&self.list);
                self.list.set_follow_mode(FollowMode::Tail);
                if !at_bottom {
                    self.list.scroll_to(top);
                }
            }
        }
    }
}
