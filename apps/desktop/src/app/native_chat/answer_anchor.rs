//! Keeping a one-click answer where the reader clicked it.
//!
//! CDXC:SessionChat 2026-11-05 DECISION:
//! User: "when i answer one of these the scroll jumps please fix!!" An answer is sent as a user message, so a transcript following its tail pinned its bottom while the "You answered" card (and then the agent's reply) arrived below, sliding the clicked pill up the screen; readers answer several pills in a row from the same spot. A click on a one-click answer or its Change, and a send or Skip all from a questions form in the transcript, now stops tail following and holds the row under the pointer at its screen position while rows above, below or inside it change height. The hold ends when the reader moves the list themselves (wheel, scrollbar, Scroll to bottom, a jump to a question, a search match, the minimap) or sends from the composer; a composer send follows the tail again when the list was following at the first click, so typing a message keeps today's behaviour.

use super::state::NativeChatView;
use gpui::{AnyElement, FollowMode, IntoElement as _, Styled as _, Window, px};
use serde_json::Value;

/// Moves smaller than this are layout rounding, not a jump.
const HOLD_TOLERANCE: f32 = 0.5;

#[derive(Default)]
pub(super) struct AnswerHold {
    /// Set from the click until the reader moves the list.
    active: bool,
    /// Whether the transcript followed its tail at the first click of the hold.
    resume: bool,
    /// The held point, once the first frame after the click has read where the pointer was.
    point: Option<HeldPoint>,
    /// The click has not been placed on a row yet.
    placing: bool,
}

#[derive(Clone)]
struct HeldPoint {
    /// The identities of the row under the pointer (`row_ids` in pagination.rs).
    row: Vec<String>,
    /// How far below the row's top the pointer was.
    in_row: f32,
    /// Where on screen (window coordinates) that point stays.
    screen: f32,
}

/// The form a transcript action answers in place, when it is one that holds the transcript.
pub(super) fn holding_form(action: &Value) -> Option<&str> {
    match action["type"].as_str()? {
        "inlineAnswer" | "inlineAnswerChange" | "formSend" | "formSkipAll" => {
            action["form"].as_str()
        }
        // The reader's own words from the Reply popover: by its form, or its tag alone.
        "inlineAnswerText" => action["form"].as_str().or(action["tag"].as_str()),
        _ => None,
    }
}

/// How far to scroll so the held point is back at `want`, or `None` while it is there.
fn hold_correction(want: f32, now: f32) -> Option<f32> {
    let moved = now - want;
    (moved.abs() > HOLD_TOLERANCE).then_some(moved)
}

/// The row whose painted span (`index`, top, bottom) holds `y`.
fn row_at(rows: &[(usize, f32, f32)], y: f32) -> Option<usize> {
    rows.iter()
        .find(|(_, top, bottom)| *top <= y && y < *bottom)
        .map(|(index, _, _)| *index)
}

impl NativeChatView {
    /// Called for every action before it goes to the core: an in-place answer starts (or renews) the
    /// hold, and stops tail following without moving a row.
    pub(super) fn hold_for_answer(&mut self, action: &Value, cx: &mut gpui::Context<Self>) {
        let Some(form) = holding_form(action) else {
            return;
        };
        // A form answered in the Resources modal is not under the reader's pointer in the transcript.
        if self.snapshot["resourceModal"]["id"].as_str() == Some(form) {
            return;
        }
        let following = self.list.is_following_tail();
        {
            let mut hold = self.answer_hold.borrow_mut();
            if !hold.active {
                hold.resume = following || self.transcript_focus_follow.get() == Some(true);
            }
            hold.active = true;
            hold.placing = true;
            hold.point = None;
        }
        if following {
            // After a layout the list already holds the real top of the tail it followed, so
            // stopping here leaves every row where it was (disclosure_anchor.rs).
            self.list.set_follow_mode(FollowMode::Normal);
        }
        cx.notify();
    }

    pub(super) fn answer_hold_active(&self) -> bool {
        self.answer_hold.borrow().active
    }

    /// The reader moved the list on purpose: the held row is free to move again, and a list that is
    /// not following waits for the reader to reach the bottom, as after any scroll up.
    pub(super) fn release_answer_hold(&self) {
        self.release_reply_hold();
        if !std::mem::take(&mut *self.answer_hold.borrow_mut()).active {
            return;
        }
        self.resume_tail_waiting();
    }

    /// The composer is sending: it follows the tail again when the list was following at the click.
    pub(super) fn end_answer_hold_for_send(&self) {
        let hold = std::mem::take(&mut *self.answer_hold.borrow_mut());
        if !hold.active {
            return;
        }
        if hold.resume && !self.held_by_focus_or_disclosure() {
            self.list.set_follow_mode(FollowMode::Tail);
        } else {
            self.resume_tail_waiting();
        }
    }

    /// When the focused field hold ends while an answer holds the list, the answer keeps it still and
    /// takes over whether the composer's next send follows the tail.
    pub(super) fn answer_hold_takes_focus_resume(&self, resume: bool) -> bool {
        let mut hold = self.answer_hold.borrow_mut();
        if !hold.active {
            return false;
        }
        hold.resume |= resume;
        true
    }

    fn held_by_focus_or_disclosure(&self) -> bool {
        self.transcript_focus_follow.get().is_some()
            || self.disclosure_motion.borrow().follow_paused
    }

    /// Tail mode that is not following: it engages again once a layout reaches the end.
    fn resume_tail_waiting(&self) {
        if self.list.is_following_tail() || self.held_by_focus_or_disclosure() {
            return;
        }
        let top = self.list.logical_scroll_top();
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to(top);
    }

    /// Paints nothing; placed after the transcript list, inside the cached transcript view, so it
    /// reads the layout that view just made. The first frame after a click places the hold on the
    /// row under the pointer; later frames scroll the list back by however far that point moved.
    pub(super) fn answer_hold_sync(&self) -> AnyElement {
        let hold = self.answer_hold.clone();
        let list = self.list.clone();
        let (placing, held) = {
            let hold = hold.borrow();
            (hold.active && hold.placing, hold.point.clone())
        };
        // The rows' identities are read here, where the transcript is, for the paint below.
        let row_ids: Vec<Vec<String>> = if placing {
            self.items.iter().map(super::pagination::row_ids).collect()
        } else {
            Vec::new()
        };
        let held_ix = held.as_ref().and_then(|point| {
            let key = point.row.first()?;
            self.items
                .iter()
                .position(|row| super::pagination::row_ids(row).first() == Some(key))
        });
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window: &mut Window, _| {
                let mut hold = hold.borrow_mut();
                if !hold.active {
                    return;
                }
                if list.is_scrollbar_dragging() {
                    hold.point = None;
                    hold.placing = false;
                    return;
                }
                if hold.placing {
                    hold.placing = false;
                    let pointer = window.mouse_position();
                    if !list.viewport_bounds().contains(&pointer) {
                        // Sent from the keyboard: following stays off, and a focused form field
                        // holds itself (scroll_bottom.rs).
                        return;
                    }
                    let viewport_bottom = list.viewport_bounds().bottom();
                    let rows: Vec<(usize, f32, f32)> = (list.logical_scroll_top().item_ix
                        ..list.item_count())
                        .map_while(|index| {
                            let bounds = list.bounds_for_item(index)?;
                            (bounds.top() < viewport_bottom)
                                .then(|| (index, bounds.top().as_f32(), bounds.bottom().as_f32()))
                        })
                        .collect();
                    let y = pointer.y.as_f32();
                    let Some(index) = row_at(&rows, y) else {
                        return;
                    };
                    let Some(row) = row_ids.get(index).filter(|ids| !ids.is_empty()) else {
                        return;
                    };
                    let top = rows
                        .iter()
                        .find(|(at, _, _)| *at == index)
                        .map_or(y, |row| row.1);
                    hold.point = Some(HeldPoint {
                        row: row.clone(),
                        in_row: y - top,
                        screen: y,
                    });
                    return;
                }
                let (Some(point), Some(index)) = (held.as_ref(), held_ix) else {
                    return;
                };
                let Some(bounds) = list.bounds_for_item(index) else {
                    return;
                };
                let now = bounds.top().as_f32() + point.in_row;
                if let Some(moved) = hold_correction(point.screen, now) {
                    list.scroll_by(px(moved));
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn in_place_answers_hold_the_transcript() {
        for kind in [
            "inlineAnswer",
            "inlineAnswerChange",
            "inlineAnswerText",
            "formSend",
            "formSkipAll",
        ] {
            assert_eq!(
                holding_form(&json!({"type": kind, "form": "f1", "choice": 0})),
                Some("f1"),
                "{kind}"
            );
        }
    }

    #[test]
    fn composer_sends_and_other_actions_do_not_hold() {
        for action in [
            json!({"type": "send", "text": "hi"}),
            json!({"type": "queue", "text": "hi"}),
            json!({"type": "formChoice", "form": "f1", "question": 0, "choice": 1}),
            json!({"type": "formText", "form": "f1", "question": 0, "text": "x"}),
            json!({"type": "inlineAnswer"}),
        ] {
            assert_eq!(holding_form(&action), None, "{action}");
        }
    }

    #[test]
    fn a_held_point_that_moved_scrolls_back_by_the_move() {
        assert_eq!(hold_correction(300.0, 300.0), None);
        assert_eq!(hold_correction(300.0, 300.4), None);
        // The "You answered" card pushed nothing above it, but a row above grew by 48px.
        assert_eq!(hold_correction(300.0, 348.0), Some(48.0));
        // A finished turn regrouped above the clicked row and shrank.
        assert_eq!(hold_correction(300.0, 180.0), Some(-120.0));
    }

    #[test]
    fn the_pointer_picks_the_row_it_is_over() {
        let rows = [(4, -120.0, 40.0), (5, 40.0, 400.0), (6, 400.0, 520.0)];
        assert_eq!(row_at(&rows, 0.0), Some(4));
        assert_eq!(row_at(&rows, 40.0), Some(5));
        assert_eq!(row_at(&rows, 399.9), Some(5));
        assert_eq!(row_at(&rows, 450.0), Some(6));
        assert_eq!(row_at(&rows, 600.0), None);
    }
}
