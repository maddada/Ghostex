//! Keeping a disclosure's header where the reader clicked it.
//!
//! CDXC:SessionChat 2026-09-26 DECISION:
//! User: when a collapsed area is expanded, keep the scroll anchored so that area's header stays at the same spot on screen, instead of sending the reader to the bottom of what opened; they read it from the top.
//! Two things moved the header. The toggles remeasured the whole list with gpui's proportional anchor, which keeps the top row's offset as a fraction of its height, so a long row partly scrolled off the top pushed the reader deep into what it grew; the remeasure now keeps pixels. And a transcript following its tail pins the bottom, so a body growing below the header pushed the header up and away; following pauses for the motion and is handed back afterwards, re-engaging only once the reader is at the bottom again. The one toggle whose revealed rows appear above it ("+N previous tool calls") scrolls by exactly what they grew, so it stays put too.

use super::state::NativeChatView;
use gpui::{FollowMode, Window, px};

impl NativeChatView {
    /// Every transcript disclosure calls this when the reader toggles it, before the open state
    /// flips. `key` is the disclosure's key, `opening` what the press does.
    pub(super) fn anchor_disclosure_toggle(&mut self, key: &str, opening: bool) {
        let mut motions = self.disclosure_motion.borrow_mut();
        if self.list.is_following_tail() && !motions.follow_paused {
            // After a layout the list already holds the real top of the tail it followed, so
            // stopping here leaves every row where it was.
            self.list.set_follow_mode(FollowMode::Normal);
            motions.follow_paused = true;
        }
        motions.awaiting = Some((key.to_string(), web_time::Instant::now()));
        if key.starts_with("tool-run:") {
            motions.grows_above = Some((key.to_string(), opening, opening.then_some(0.0)));
        }
        drop(motions);
        let count = self.list.item_count();
        self.list.remeasure_items(0..count);
    }

    /// Before the list lays out: scroll with rows easing in above a toggle, and hand tail
    /// following back once nothing is moving.
    pub(super) fn sync_disclosure_anchor(&mut self, window: &mut Window) {
        let mut motions = self.disclosure_motion.borrow_mut();
        if let Some((key, opening, applied)) = motions.grows_above.clone() {
            let awaiting = motions
                .awaiting
                .as_ref()
                .is_some_and(|(awaited, _)| awaited == &key);
            // Read before `frame`, which forgets the measurement when the motion settles.
            let natural = motions.natural(&key);
            match motions.frame(&key) {
                Some(frame) if natural > 0.0 => {
                    let base = applied.unwrap_or(natural);
                    let now = natural * frame.log;
                    if (now - base).abs() > 0.01 {
                        self.list.scroll_by(px(now - base));
                    }
                    motions.grows_above = Some((key, opening, Some(now)));
                }
                // Not measured yet, or pressed but not drawn yet: nothing has moved.
                Some(_) => {}
                None if awaiting => {}
                None => {
                    // Settled: land on the rows' final height.
                    if let Some(base) = applied {
                        let target = if opening { natural.max(base) } else { 0.0 };
                        if (target - base).abs() > 0.01 {
                            self.list.scroll_by(px(target - base));
                        }
                    }
                    motions.grows_above = None;
                }
            }
        }
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
                // Tail mode again without jumping: scrolling to the current top leaves it waiting
                // until the reader is back at the bottom, where the list re-engages on its own.
                let top = self.list.logical_scroll_top();
                self.list.set_follow_mode(FollowMode::Tail);
                self.list.scroll_to(top);
            }
        }
    }
}
