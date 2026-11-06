//! The pictures an agent opened from disk: a row-sized thumbnail at the right of a tool row that
//! read one, and the one-row strip of the same tiles a closed fold keeps of them, which scrolls
//! sideways when it does not fit.
//!
//! CDXC:SessionChat 2026-11-02 SEE-ALSO:
//! The user's decision and every rule (which calls read a picture, which pictures a turn keeps, a
//! picture the user already showed) are in packages/gx-chat-core/src/transcript/read_images.rs;
//! this only lays the tiles out. The phone draws the same tiles in
//! apps/mobile/app/src/chat/native/transcript/Images.tsx (`ReadImages`).

use super::pills_strip::{PILLS_FADE, edge_fade};
use super::tool_run::{ROW_PAD_X, TILE};
use super::{appearance::ChatAppearance, images::VISUAL, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, DispatchPhase, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, ScrollEvent, SharedString, StatefulInteractiveElement as _, Styled as _,
    canvas, div, point, px,
};
use serde_json::Value;

/// Where a strip under a closed fold of a message's calls starts: under a tool row's label.
pub(super) const FOLDED_CALLS_INSET: f32 = ROW_PAD_X + TILE + 8.0;

/// The side of every read-picture tile, unscaled: a little taller than a tool row's glyph tile, so
/// it sits inside the row beside its duration without making the row taller than a picture-less
/// one by more than a few points.
///
/// CDXC:SessionChat 2026-11-06 DECISION:
/// User, on a Read of a screenshot that showed no picture: "when agent is reading a pic please show thumbnail of it next to this row in the chat transcript". One tile for every picture an agent read: this square, on the row's right before its duration (`tool_run.rs`, `tool_card.rs`), and the same square in the strip a closed fold keeps (`read_picture_strip`), superseding the 48px tiles under the row. A click opens the picture viewer and never toggles the row. Later that day the user had a closed fold's strip leave out a picture their own message already shows; the open row's tile keeps it (CDXC:SessionChat 2026-11-06 in read_images.rs, `is_user_shown`).
pub(super) const READ_TILE: f32 = 28.0;

impl NativeChatView {
    /// The pictures one tool row read, as `READ_TILE` squares side by side for the row's right
    /// end, each opening the picture viewer on its picture; a file that has gone draws the
    /// missing-picture square. `key` names the row among the transcript's pictures.
    pub(super) fn read_pictures(
        &mut self,
        key: &str,
        images: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let images = images
            .as_array()
            .filter(|images| !images.is_empty())?
            .clone();
        let s = p.scale;
        let mut row = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(VISUAL.user_row_gap * s));
        for index in 0..images.len() {
            // The user's square placeholder for a picture that cannot be read, not a named chip.
            row = row.child(self.image_tile_at(key, &images, index, true, READ_TILE, p, cx));
        }
        Some(row.into_any_element())
    }

    /// The pictures a closed fold keeps in view: one row of thumbnails that scrolls sideways when
    /// it does not fit, fading into `edge` (the colour behind it) at an edge that hides more, as
    /// the chat box's pills do (`edge_fade`, pills_strip.rs).
    ///
    /// CDXC:SessionChat 2026-11-02 WHY:
    /// The user asked for the side panel's tab strip behaviour (a vertical wheel turn moves it sideways), which GPUI gives an x-only scroller on the bubble pass (`overflow_x_scroll`, view_tab_strip.rs). Inside the transcript that pass comes after the transcript list has already scrolled the page by the same wheel (CDXC:SessionChat 2026-09-21 on `nested_scroll`), so the strip scrolls itself on the capture pass and stops the event, and lets it through once it is at its end. A mouse wheel's vertical turn and a trackpad's sideways swipe move it; a trackpad's vertical swipe still scrolls the page, so reading past a strip never gets caught in it.
    pub(super) fn read_picture_strip(
        &mut self,
        key: &str,
        images: &Value,
        inset: f32,
        edge: Hsla,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let images = images
            .as_array()
            .filter(|images| !images.is_empty())?
            .clone();
        let s = p.scale;
        let mut row = div()
            .flex()
            .flex_shrink_0()
            .gap(px(VISUAL.user_row_gap * s))
            .pl(px(inset * s));
        for index in 0..images.len() {
            row = row.child(self.image_tile_at(key, &images, index, true, READ_TILE, p, cx));
        }
        let scroll_key = SharedString::from(format!("read-strip:{key}"));
        let handle = self
            .nested_scrolls
            .borrow_mut()
            .entry(scroll_key.clone())
            .or_default()
            .clone();
        let fades = |handle: &gpui::ScrollHandle| {
            let offset = -f32::from(handle.offset().x);
            let max = f32::from(handle.max_offset().x);
            (offset > 0.5, max - offset > 0.5)
        };
        let shown = fades(&handle);
        let chat = cx.entity_id();
        let wheel = handle.clone();
        Some(
            div()
                .relative()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .id(scroll_key)
                        .w_full()
                        .flex()
                        .overflow_x_scroll()
                        .track_scroll(&handle)
                        .child(row),
                )
                .when(shown.0, |strip| {
                    strip.child(edge_fade(true, edge, PILLS_FADE * s))
                })
                .when(shown.1, |strip| {
                    strip.child(edge_fade(false, edge, PILLS_FADE * s))
                })
                .child(
                    canvas(
                        |bounds, window, _| {
                            window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                        },
                        move |_, hitbox, window, cx| {
                            // The fades follow what this layout measured.
                            if fades(&wheel) != shown {
                                cx.notify(chat);
                            }
                            let handle = wheel.clone();
                            window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                                if phase != DispatchPhase::Capture
                                    || !hitbox.should_handle_scroll(window)
                                {
                                    return;
                                }
                                let delta = event.delta.pixel_delta(window.line_height());
                                let step = if delta.x != px(0.0) || event.delta.precise() {
                                    delta.x
                                } else {
                                    delta.y
                                };
                                let offset = handle.offset();
                                let next = (offset.x + step).clamp(-handle.max_offset().x, px(0.0));
                                if next == offset.x {
                                    return;
                                }
                                handle.set_offset(point(next, offset.y));
                                cx.stop_propagation();
                                window.refresh();
                            });
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element(),
        )
    }
}
