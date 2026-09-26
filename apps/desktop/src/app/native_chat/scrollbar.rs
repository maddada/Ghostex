//! The transcript's own scrollbar.
//!
//! gpui-component's `Scrollbar` already implements `ScrollbarHandle` for gpui's
//! `ListState`, so the chat list needs no hand-rolled thumb: the component reads
//! the list's offset and content size, drags it through
//! `set_offset_from_scrollbar`, and brackets the drag with the list's own
//! `scrollbar_drag_started` / `scrollbar_drag_ended`. It also already carries
//! the app's exact thumb colors (`apply_gpui_component_theme` in
//! app/helpers/titlebar.rs, shared with packages/components/ui/scrollbar-theme.css),
//! so only the 5px thickness from React's session-chat-scrollbar.css is set here.
//!
//! Its mouse handling is scoped to the thumb strip, so it never takes a click,
//! a drag or a selection away from the transcript underneath it. The strip is
//! the pane's last 5px, and every composer control is inset by the footer's own
//! 16px padding, so the column beside the box covers nothing interactive: the
//! same geometry React had, where the composer overlay centered a `max-w-3xl px-4`
//! card under a track pinned to the pane's right edge.

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, IntoElement as _, ParentElement as _, Styled as _, div, px, size};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};

/// Unscaled track and thumb width, the value in session-chat-scrollbar.css.
pub(super) const THICKNESS: f32 = 5.0;

impl NativeChatView {
    /// The thin thumb beside the transcript: it fades in while the list scrolls
    /// or the pointer is on it, fades out when the reader stops, and drags.
    ///
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// React's track was `top: 0; bottom: 0` of the message scroller, and the scroller spanned the whole
    /// chat pane because the composer was an overlay over it and the transcript kept its end clear
    /// with a bottom inset that tracked the box (use-session-chat-composer-inset.ts), so the viewport
    /// and the track kept their length whatever the box did. The native composer is a real sibling
    /// region below the list, so the list's viewport is what shrinks as the box grows, and a track
    /// bound to that viewport ended higher up the pane on every grow, collapse and expand: the thumb
    /// jumped. So the track is a column of its own reaching the bottom of the pane, measured once per
    /// frame, and the scroll size reported to the component is `track + the list's max offset`, which
    /// keeps `content - track` equal to the list's scrollable distance exactly. That was React's math
    /// (a constant viewport against a content height that carries the composer inset), and it leaves
    /// the thumb still while the box resizes.
    ///
    /// CDXC:SessionChat 2026-09-22 WHY:
    /// That scroll size is handed to the component as a closure, read when the bar lays out, not
    /// computed here while the pane builds its elements. Built here it was the previous frame's max
    /// offset: a streaming reply is spliced out and remeasured every frame, so the list's content
    /// height at build time lacked the row being remeasured while the offset the bar read after
    /// the list's layout included it, and the thumb jumped down and back on every streamed line
    /// while the reader sat at the end of the transcript.
    ///
    /// CDXC:SessionChat 2026-09-21 DECISION:
    /// "We should only show it in the gpui chat view when the user scrolls", not when new messages
    /// appear. The component's own rule shows the bar on any offset change, and the offset of a list
    /// following its tail moves with every streamed line, so the bar is shown from the list's scroll
    /// handler instead, which gpui calls for the reader's wheel scrolls only.
    pub(super) fn transcript_scrollbar(&self, p: &ChatAppearance) -> AnyElement {
        let track = self.scrollbar_track.get();
        let measured = track > px(0.0);
        let mut bar = Scrollbar::vertical(&self.list)
            .id("chat-transcript-scrollbar")
            .thickness(px(THICKNESS * p.scale))
            .mode(ScrollbarMode::Scrolling)
            .shown_by_host_scroll(self.transcript_scrolled_at);
        if measured {
            let list = self.list.clone();
            bar = bar
                .scroll_size_with(move || size(px(0.0), track + list.max_offset_for_scrollbar().y));
        }
        let next_track = self.scrollbar_track.clone();
        let pane = self.bounds.clone();
        div()
            .absolute()
            .top_0()
            .right_0()
            .w(px(THICKNESS * p.scale))
            // Before the first measurement the column is the transcript region alone, the placement
            // and the content size the component computes on its own.
            .map(|column| {
                if measured {
                    column.h(track)
                } else {
                    column.bottom_0()
                }
            })
            .child(bar)
            .child(
                // Visual-only and non-interactive: it reads where this column starts, which with the
                // pane's own rect gives the distance to the pane's bottom edge.
                gpui::canvas(
                    move |rect, window, cx| {
                        let height = pane.get().bottom() - rect.origin.y;
                        if height > px(0.0) && next_track.replace(height) != height {
                            cx.notify(window.current_view());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}
