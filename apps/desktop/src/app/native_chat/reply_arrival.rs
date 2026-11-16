//! The agent's reply arriving: it fades in, and a long one stops the chat at its top.
//!
//! CDXC:SessionChat 2026-11-08 DECISION:
//! User: "please when the last message from the agent in a turn is coming in animate it in with a fade and please keep me at the top of it dont scroll me to the bottom of it because i want to read from the top".
//! The newest row of the transcript becoming an agent reply (prose with no tool calls, live or as a finished turn's final message) is an arrival: the reply fades in over the transcript's own fade, and a reader following the bottom is stopped at the reply's top when it is taller than what fits, held there while the rows above it fold, until they scroll themselves. A reply that fits keeps today's following. A reader scrolled up is never moved. A reply that turns out not to be the last (tool calls follow it while the turn runs) hands following back, so working output follows as before; the disclosure reveal (disclosure_reveal.rs) is untouched.
//! The reply is only known to be the turn's last once the turn ends, so the hold starts at the text and is let go if the turn carries on. With Claude the text first streams in as the `streaming` row (gx-chat-core `STREAMING_ID`) and becomes the saved row when the turn ends; that is one arrival. While it streams the chat follows it until it no longer fits, then stops at its top.

use super::state::NativeChatView;
use ghostex_gx_chat_core::TranscriptItem;
use ghostex_gx_chat_core::session::constants::STREAMING_ID;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, FollowMode, IntoElement as _, ParentElement as _, Styled as _, Window, div, px,
};
use serde_json::Value;
use std::{cell::Cell, rc::Rc, time::Duration};
use web_time::Instant;

/// The fade, the transcript's own arrival fade (transcript_reveal.rs).
const FADE: Duration = Duration::from_millis(250);
/// A reply not painted within this long after it arrived is not held (it never came into view).
const PLACE_TIMEOUT: Duration = Duration::from_millis(1500);
/// Moves smaller than this are layout rounding.
const TOLERANCE: f32 = 0.5;

/// Where the arriving reply last painted, in window pixels: its top and bottom.
type Extent = Rc<Cell<Option<(f32, f32)>>>;

#[derive(Default)]
pub(crate) struct ReplyArrival {
    /// The transcript was drawn with rows once, so a newest reply after that is an arrival, not
    /// the history loading.
    seeded: bool,
    /// The newest reply as of the last draw.
    newest: Option<String>,
    fade: Option<(String, Instant)>,
    hold: Option<ReplyHold>,
}

struct ReplyHold {
    id: String,
    extent: Extent,
    started: Instant,
    /// Where the reply's top is held, once it is too tall to follow. Until then the list follows
    /// and every paint checks again, since a streaming reply grows.
    at: Option<f32>,
}

/// An agent reply's id: an assistant row with prose and no tool calls of its own.
fn reply_id(message: &Value) -> Option<String> {
    let tools = message["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty())
        || message["toolCard"].is_object();
    (message["role"] == "assistant"
        && !tools
        && message["text"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()))
    .then(|| message["id"].as_str().map(str::to_string))
    .flatten()
}

/// The reply the transcript ends with, if it ends with one.
fn newest_reply(items: &[TranscriptItem]) -> Option<String> {
    match items.last()? {
        TranscriptItem::Message { message } => reply_id(message),
        TranscriptItem::CompletedWork { final_message, .. }
        | TranscriptItem::Summary { final_message, .. } => {
            final_message.as_deref().and_then(reply_id)
        }
        _ => None,
    }
}

/// Where to hold a reply that arrived at the bottom: `None` while it fits between `view_top` and
/// `view_bottom` with `margin` above it (following carries on), else the screen line its top stops
/// at.
fn hold_line(view_top: f32, view_bottom: f32, top: f32, bottom: f32, margin: f32) -> Option<f32> {
    (bottom - top > view_bottom - view_top - margin).then_some(view_top + margin)
}

impl NativeChatView {
    /// Before the list lays out: notice a reply arriving as the transcript's newest row.
    pub(super) fn sync_reply_arrival(&mut self, cx: &gpui::App) {
        let newest = newest_reply(&self.items);
        let mut arrival = self.reply_arrival.borrow_mut();
        if !arrival.seeded {
            if !self.items.is_empty() {
                arrival.seeded = true;
                arrival.newest = newest;
            }
            return;
        }
        if newest == arrival.newest {
            return;
        }
        // The streaming reply settling into its saved row is the same reply: no second fade, and
        // the hold carries over.
        if arrival.newest.as_deref() == Some(STREAMING_ID)
            && let Some(id) = newest.as_ref()
        {
            arrival.newest = Some(id.clone());
            if let Some(hold) = arrival.hold.as_mut().filter(|hold| hold.id == STREAMING_ID) {
                hold.id = id.clone();
                // The settled row is drawn anew; its first paint measures it again.
                hold.extent = Rc::default();
                hold.started = Instant::now();
            }
            if let Some(fade) = arrival.fade.as_mut().filter(|fade| fade.0 == STREAMING_ID) {
                fade.0 = id.clone();
            }
            return;
        }
        arrival.newest = newest.clone();
        // The held reply is not the newest any more (the turn carried on, or a new one started):
        // working output follows as it always did.
        let left = arrival.hold.take().is_some_and(|hold| hold.at.is_some());
        let Some(id) = newest else {
            drop(arrival);
            if left {
                self.list.set_follow_mode(FollowMode::Tail);
            }
            return;
        };
        if !cx.reduce_motion() {
            arrival.fade = Some((id.clone(), Instant::now()));
        }
        let held_elsewhere = self.answer_hold_active()
            || self.transcript_focus_follow.get().is_some()
            || self.disclosure_motion.borrow().follow_paused;
        if (self.list.is_following_tail() || left) && !held_elsewhere {
            // The list keeps following for the reply's first paint, which it needs to be measured
            // (a row below the view is laid out but not painted); that frame is the first of its
            // fade, so its end showing for it is not seen. The paint decides whether to hold.
            if !self.list.is_following_tail() {
                // A held reply gave way to this one: follow it as working output does.
                self.list.set_follow_mode(FollowMode::Tail);
            }
            arrival.hold = Some(ReplyHold {
                id,
                extent: Rc::default(),
                started: Instant::now(),
                at: None,
            });
        }
    }

    /// A long reply holds the list at its top (following is off for it).
    pub(super) fn reply_hold_placed(&self) -> bool {
        self.reply_arrival
            .borrow()
            .hold
            .as_ref()
            .is_some_and(|hold| hold.at.is_some())
    }

    /// The reader moved the list themselves: the reply is theirs to scroll.
    pub(super) fn release_reply_hold(&self) {
        let placed = self.reply_arrival.borrow_mut().hold.take();
        if placed.is_some() && !self.list.is_following_tail() {
            // Tail mode that waits for the reader to reach the bottom again.
            let top = self.list.logical_scroll_top();
            self.list.set_follow_mode(FollowMode::Tail);
            self.list.scroll_to(top);
        }
    }

    /// The reply `id` as drawn: fading in while it arrives, and measured while it is held.
    pub(super) fn reply_arrival_row(
        &self,
        id: &str,
        row: AnyElement,
        window: &mut Window,
    ) -> AnyElement {
        let mut arrival = self.reply_arrival.borrow_mut();
        let opacity = match &arrival.fade {
            Some((fading, started)) if fading == id => {
                let progress = started.elapsed().as_secs_f32() / FADE.as_secs_f32();
                if progress >= 1.0 {
                    arrival.fade = None;
                    None
                } else {
                    window.request_animation_frame();
                    Some(gpui::ease_in_out(progress.max(0.0)))
                }
            }
            _ => None,
        };
        let extent = arrival
            .hold
            .as_ref()
            .filter(|hold| hold.id == id)
            .map(|hold| hold.extent.clone());
        if opacity.is_none() && extent.is_none() {
            return row;
        }
        div()
            .relative()
            .w_full()
            .min_w_0()
            .when_some(opacity, |wrap, opacity| wrap.opacity(opacity))
            .child(row)
            .when_some(extent, |wrap, extent| {
                wrap.child(
                    gpui::canvas(
                        move |bounds, _, _| {
                            extent.set(Some((bounds.top().as_f32(), bounds.bottom().as_f32())))
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
            .into_any_element()
    }

    /// Paints nothing; placed after the transcript list so it reads the layout just made. The
    /// reply's first paint decides: one that fits follows the tail as before, a taller one stops
    /// with its top near the top of the view; later paints keep that top where it stopped while
    /// rows above it fold or grow.
    pub(super) fn reply_hold_sync(&self) -> AnyElement {
        let arrival = self.reply_arrival.clone();
        let list = self.list.clone();
        let inset = self.transcript_inset;
        let margin = super::scroll_bottom::question_jump_margin()
            * super::appearance::ChatAppearance::current(&self.snapshot).scale;
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window: &mut Window, _| {
                let mut arrival = arrival.borrow_mut();
                let Some(hold) = arrival.hold.as_mut() else {
                    return;
                };
                if list.is_scrollbar_dragging() {
                    // A placed hold turned following off; the drag hands the list back to Tail
                    // mode where it is, so dragging to the end (or reaching it later) follows
                    // again. Left in Normal mode it never followed again until the down pill
                    // (GitHub issue #615).
                    let placed = arrival.hold.take().is_some_and(|hold| hold.at.is_some());
                    if placed && !list.is_following_tail() {
                        let top = list.logical_scroll_top();
                        list.set_follow_mode(FollowMode::Tail);
                        list.scroll_to(top);
                    }
                    return;
                }
                let Some((top, bottom)) = hold.extent.get() else {
                    if hold.started.elapsed() > PLACE_TIMEOUT {
                        arrival.hold = None;
                        list.set_follow_mode(FollowMode::Tail);
                    } else {
                        window.request_animation_frame();
                    }
                    return;
                };
                let view = list.viewport_bounds();
                let view_top = view.top().as_f32();
                let view_bottom = view.bottom().as_f32() - inset;
                let at = match hold.at {
                    Some(at) => at,
                    None => match hold_line(view_top, view_bottom, top, bottom, margin) {
                        Some(at) => {
                            hold.at = Some(at);
                            list.set_follow_mode(FollowMode::Normal);
                            at
                        }
                        None => {
                            // It fits: follow the tail, as before, and look again at the next
                            // paint in case it grows past the view.
                            if !list.is_following_tail() {
                                list.set_follow_mode(FollowMode::Tail);
                                window.request_animation_frame();
                            }
                            return;
                        }
                    },
                };
                let moved = top - at;
                if moved.abs() > TOLERANCE {
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
