//! Showing what a disclosure opened when it opens near the bottom of the visible transcript.
//!
//! CDXC:SessionChat 2026-11-08 DECISION:
//! User (Windows, a closed "4 tool calls" group as the last row above the composer): "when im in this case and tool calls is at the bottom then i click to expand it we need to scroll to the bottom of the last tool call please, right now we don't scroll in this case to reveal what was expanded which is wrong".
//! Opening a disclosure used to pause tail following and keep its header still (`disclosure_anchor.rs`, 2026-09-26), so a fold at the bottom opened entirely below the fold. Now a reader at the bottom (or within the scroll pill's edge threshold of it) keeps following the tail while it opens: the chat grows upward and the end of what opened stays in view, and following new output carries on. A reader scrolled up is not moved to the bottom: the chat scrolls only as far as the end of what opened needs, and never so far that the header they clicked leaves the top. Closing keeps the 2026-09-26 behaviour.
//! This file is the scroll arithmetic; `disclosure_anchor.rs` applies it to the list and `disclosure_motion.rs` measures the opened body.

/// What opening or closing a disclosure does to the transcript's scroll position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToggleScroll {
    /// The reader was at the bottom and is opening: keep following the tail, so the opened body
    /// grows the chat upward and its end stays above the composer.
    FollowBottom,
    /// The reader had scrolled up and is opening: hold the header still, then scroll only as far as
    /// the opened body's end needs ([`reveal_distance`]).
    Reveal,
    /// Closing, or a focused field or clicked answer holds the list: keep the header where it is.
    Hold,
}

/// Decide [`ToggleScroll`] for a toggle. `following` is whether the list follows its tail,
/// `distance_to_end` how far the viewport's bottom is above the transcript's end, `edge` how close
/// to the end still counts as the bottom, `held` whether something else holds the list still.
pub(super) fn toggle_scroll(
    opening: bool,
    following: bool,
    distance_to_end: f32,
    edge: f32,
    held: bool,
) -> ToggleScroll {
    if !opening || held {
        ToggleScroll::Hold
    } else if following || distance_to_end <= edge {
        ToggleScroll::FollowBottom
    } else {
        ToggleScroll::Reveal
    }
}

/// How far to scroll down so an opened body's end shows above the visible bottom, for a reader who
/// had scrolled up. `view_top`/`view_bottom` bound what the reader sees (window pixels), `top` is
/// the opened part's top and `end` its bottom as last painted, and `room` how much must stay
/// visible above `top`: the header's height when `top` is the body under it, 0 when `top` is the
/// header itself. Never negative: content already in view never moves, and a header already at
/// the top stops it.
pub(super) fn reveal_distance(
    view_top: f32,
    view_bottom: f32,
    top: f32,
    end: f32,
    room: f32,
) -> f32 {
    let hidden = end - view_bottom;
    if hidden <= 0.0 {
        return 0.0;
    }
    let spare = (top - room - view_top).max(0.0);
    hidden.min(spare)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transcript as a column of rows with a viewport over it, the way gpui's list lays it out:
    /// following the tail pins the content's end to the viewport's bottom on every layout.
    struct Transcript {
        rows: Vec<f32>,
        viewport: f32,
        scroll_top: f32,
        following: bool,
    }

    impl Transcript {
        fn content(&self) -> f32 {
            self.rows.iter().sum()
        }
        fn layout(&mut self) {
            let max = (self.content() - self.viewport).max(0.0);
            if self.following {
                self.scroll_top = max;
            }
            self.scroll_top = self.scroll_top.clamp(0.0, max);
        }
        fn distance_to_end(&self) -> f32 {
            (self.content() - self.viewport - self.scroll_top).max(0.0)
        }
        /// Row `ix`'s top and bottom in viewport coordinates.
        fn row(&self, ix: usize) -> (f32, f32) {
            let top: f32 = self.rows[..ix].iter().sum::<f32>() - self.scroll_top;
            (top, top + self.rows[ix])
        }
        /// Toggle row `ix` open to `opened` pixels (header + body), over a few eased frames,
        /// applying the scroll rule the way `disclosure_anchor.rs` does.
        fn open(&mut self, ix: usize, header: f32, opened: f32) {
            let rule = toggle_scroll(true, self.following, self.distance_to_end(), 10.0, false);
            // Following resumes (or carries on) for a reader at the bottom; anyone else holds.
            self.following = rule == ToggleScroll::FollowBottom;
            for step in 1..=10 {
                self.rows[ix] = header + (opened - header) * step as f32 / 10.0;
                self.layout();
                if rule == ToggleScroll::Reveal {
                    let (top, end) = self.row(ix);
                    self.scroll_top +=
                        reveal_distance(0.0, self.viewport, top + header, end, header);
                    self.layout();
                }
            }
        }
    }

    #[test]
    fn opening_at_the_bottom_follows_the_tail() {
        assert_eq!(
            toggle_scroll(true, true, 0.0, 10.0, false),
            ToggleScroll::FollowBottom
        );
        // A few pixels short of the end still counts as the bottom.
        assert_eq!(
            toggle_scroll(true, false, 6.0, 10.0, false),
            ToggleScroll::FollowBottom
        );
        assert_eq!(
            toggle_scroll(true, false, 40.0, 10.0, false),
            ToggleScroll::Reveal
        );
        // Closing, and a held list, keep the header still as before.
        assert_eq!(
            toggle_scroll(false, true, 0.0, 10.0, false),
            ToggleScroll::Hold
        );
        assert_eq!(
            toggle_scroll(true, true, 0.0, 10.0, true),
            ToggleScroll::Hold
        );
    }

    #[test]
    fn a_group_opened_at_the_bottom_shows_the_end_of_its_last_call() {
        // The user's case: the closed "4 tool calls" group is the last row.
        let mut chat = Transcript {
            rows: vec![400.0, 300.0, 24.0],
            viewport: 500.0,
            scroll_top: 0.0,
            following: true,
        };
        chat.layout();
        chat.open(2, 24.0, 260.0);
        let (_, end) = chat.row(2);
        assert!(
            end <= chat.viewport + 0.01,
            "end {end} is below the viewport"
        );
        assert!(chat.following, "following new output carries on");
        assert!(chat.distance_to_end() < 0.01);
    }

    #[test]
    fn a_group_opened_at_the_bottom_with_rows_after_it_still_shows_its_end() {
        // A running tool row follows the group; both were in view at the bottom.
        let mut chat = Transcript {
            rows: vec![600.0, 24.0, 60.0],
            viewport: 500.0,
            scroll_top: 0.0,
            following: false,
        };
        chat.layout();
        // A few pixels above the end.
        chat.scroll_top = chat.content() - chat.viewport - 5.0;
        chat.open(1, 24.0, 300.0);
        let (_, end) = chat.row(1);
        assert!(
            end <= chat.viewport + 0.01,
            "end {end} is below the viewport"
        );
        assert!(chat.following);
    }

    #[test]
    fn a_reader_scrolled_up_is_moved_only_as_far_as_the_opened_end_needs() {
        let mut chat = Transcript {
            rows: vec![300.0, 24.0, 900.0],
            viewport: 500.0,
            scroll_top: 0.0,
            following: false,
        };
        chat.layout();
        // The group's header sits at 300..324; opened it ends at 524, 24 below the viewport.
        chat.open(1, 24.0, 224.0);
        let (top, end) = chat.row(1);
        assert!(
            (end - chat.viewport).abs() < 0.01,
            "end {end} lands on the bottom edge"
        );
        assert!(
            (top - 276.0).abs() < 0.01,
            "moved by exactly what was hidden, header at {top}"
        );
        assert!(!chat.following, "not sent to the bottom");
    }

    #[test]
    fn a_reader_scrolled_up_keeps_the_clicked_header_on_screen() {
        let mut chat = Transcript {
            rows: vec![300.0, 24.0, 900.0],
            viewport: 500.0,
            scroll_top: 0.0,
            following: false,
        };
        chat.layout();
        // Opened far taller than the viewport: the header stops at the top and stays there.
        chat.open(1, 24.0, 2000.0);
        let (top, _) = chat.row(1);
        assert!(
            top.abs() < 0.01,
            "header top {top} left the screen or never reached the top"
        );
    }

    #[test]
    fn content_already_in_view_never_moves() {
        let mut chat = Transcript {
            rows: vec![100.0, 24.0, 900.0],
            viewport: 500.0,
            scroll_top: 0.0,
            following: false,
        };
        chat.layout();
        chat.open(1, 24.0, 200.0);
        assert_eq!(chat.scroll_top, 0.0);
        assert_eq!(reveal_distance(0.0, 500.0, 100.0, 480.0, 24.0), 0.0);
        // A header already above the top never pulls the view further down.
        assert_eq!(reveal_distance(0.0, 500.0, -50.0, 900.0, 24.0), 0.0);
    }
}
