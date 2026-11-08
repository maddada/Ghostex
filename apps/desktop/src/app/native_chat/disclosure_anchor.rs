//! Keeping a disclosure's header where the reader clicked it.
//!
//! CDXC:SessionChat 2026-09-26 DECISION:
//! User: when a collapsed area is expanded, keep the scroll anchored so that area's header stays at the same spot on screen, instead of sending the reader to the bottom of what opened; they read it from the top.
//! Two things moved the header. The toggles remeasured the whole list with gpui's proportional anchor, which keeps the top row's offset as a fraction of its height, so a long row partly scrolled off the top pushed the reader deep into what it grew; the remeasure now keeps pixels. And a transcript following its tail pins the bottom, so a body growing below the header pushed the header up and away; following pauses for the motion and is handed back afterwards, re-engaging only once the reader is at the bottom again.
//! CDXC:SessionChat 2026-11-08 DECISION: opening a disclosure at the bottom of the chat no longer pauses following, and one opened by a reader scrolled up scrolls just enough to show its end (disclosure_reveal.rs); closing still holds the header.
//! CDXC:SessionChat 2026-11-08 DECISION: "+N previous tool calls" used to be the one toggle whose revealed rows appeared above it, so it scrolled by what they grew; now it opens to the top of the group and its rows grow below it like every other disclosure, so that special case is gone.

use super::disclosure_motion::Reveal;
use super::disclosure_reveal::{ToggleScroll, reveal_distance, toggle_scroll};
use super::state::NativeChatView;
use gpui::{FollowMode, Window, px};

/// The header row's height, unscaled, kept in view above an opened body when it is revealed.
const HEADER_ROOM: f32 = 28.0;

impl NativeChatView {
    /// Every transcript disclosure calls this when the reader toggles it, before the open state
    /// flips. `key` is the disclosure's key, `opening` what the press does.
    pub(super) fn anchor_disclosure_toggle(&mut self, key: &str, opening: bool) {
        let scale = super::appearance::ChatAppearance::current(&self.snapshot).scale;
        // A focused field or a clicked answer holds the list, and a subagent's transcript is
        // another list than the one this scrolls.
        let held = self.answer_hold_active()
            || self.transcript_focus_follow.get().is_some()
            || self.snapshot["subagent"].is_object();
        let distance = super::scroll_bottom::transcript_distance_to_end(&self.list).as_f32();
        let rule = toggle_scroll(
            opening,
            self.list.is_following_tail(),
            distance,
            super::scroll_bottom::edge_threshold() * scale,
            held,
        );
        let mut motions = self.disclosure_motion.borrow_mut();
        motions.reveal = None;
        match rule {
            // Opening at the bottom: keep (or take back) following, so what opens grows the chat
            // upward with its end in view (disclosure_reveal.rs).
            ToggleScroll::FollowBottom => {
                if !self.list.is_following_tail() {
                    self.list.set_follow_mode(FollowMode::Tail);
                }
                motions.follow_paused = false;
            }
            ToggleScroll::Reveal | ToggleScroll::Hold => {
                if self.list.is_following_tail() && !motions.follow_paused {
                    // After a layout the list already holds the real top of the tail it followed,
                    // so stopping here leaves every row where it was.
                    self.list.set_follow_mode(FollowMode::Normal);
                    motions.follow_paused = true;
                }
                if rule == ToggleScroll::Reveal {
                    motions.reveal = Some(Reveal {
                        key: key.to_string(),
                        extent: Default::default(),
                        room: HEADER_ROOM * scale,
                        pressed: web_time::Instant::now(),
                        settled: false,
                    });
                }
            }
        }
        motions.awaiting = Some((key.to_string(), web_time::Instant::now()));
        drop(motions);
        let count = self.list.item_count();
        self.list.remeasure_items(0..count);
    }

    /// Before the list lays out: scroll the end of a disclosure opened by a reader who had
    /// scrolled up into view as it grows (disclosure_reveal.rs).
    fn sync_disclosure_reveal(&mut self, window: &mut Window) {
        let mut motions = self.disclosure_motion.borrow_mut();
        let Some(reveal) = motions.reveal.as_ref() else {
            return;
        };
        let key = reveal.key.clone();
        if let Some(part) = reveal.extent.take() {
            let view = self.list.viewport_bounds();
            let view_top = view.top().as_f32();
            let view_bottom = view.bottom().as_f32() - self.transcript_inset;
            // Only a part painted in the transcript moves it: a card beside the list or over the
            // composer opens where it is.
            let in_list = part.center_x >= view.left().as_f32()
                && part.center_x <= view.right().as_f32()
                && part.top < view_bottom
                && part.bottom > view_top;
            if in_list {
                let room = if part.header_inside { 0.0 } else { reveal.room };
                let distance = reveal_distance(view_top, view_bottom, part.top, part.bottom, room);
                if distance > 0.5 {
                    self.list.scroll_by(px(distance));
                }
            }
        }
        let moving = motions.is_moving(&key)
            || motions
                .awaiting
                .as_ref()
                .is_some_and(|(awaited, _)| awaited == &key);
        let Some(reveal) = motions.reveal.as_mut() else {
            return;
        };
        // A row that is never drawn again (it scrolled away, the transcript changed) must not keep
        // scrolling for good.
        let done = reveal.pressed.elapsed().as_millis() > 1500 || (!moving && reveal.settled);
        if done {
            motions.reveal = None;
            return;
        }
        // Once the motion ends, one more paint lands on the settled height.
        reveal.settled = !moving;
        window.request_animation_frame();
    }

    /// Before the list lays out: reveal what a scrolled-up reader opened, and hand tail following
    /// back once nothing is moving.
    pub(super) fn sync_disclosure_anchor(&mut self, window: &mut Window) {
        self.sync_disclosure_reveal(window);
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
