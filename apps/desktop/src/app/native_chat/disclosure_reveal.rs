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
