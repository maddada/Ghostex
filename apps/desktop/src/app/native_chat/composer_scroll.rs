use super::state::NativeChatView;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement, Styled, div, px,
};
use serde_json::json;

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-23 SEE-ALSO: `sessionChatKeepComposerExpanded` in `packages/shared/ghostex-settings/types.ts` holds the user's decision; a missing key reads as its default, off.
    pub(super) fn composer_collapse_eligible(&self) -> bool {
        !keep_composer_expanded() && self.composer_collapse_allowed()
    }

    /// Everything but the scroll gesture's own setting that stops the box collapsing.
    fn composer_collapse_allowed(&self) -> bool {
        self.maximized_window.is_none()
            && !(self.snapshot["questionCard"]["visible"] == true
                && self.snapshot["prompt"]["kind"] == "question")
            && self.snapshot["composerCollapseEligible"] == true
    }

    /// The pane is too short for the full box (ported from React's
    /// `sessionChatComposerHeightConstrained`; the threshold is in composer-animation.json).
    fn composer_height_constrained(&self) -> bool {
        let scale = super::appearance::ChatAppearance::current(&self.snapshot).scale;
        let height = f32::from(self.bounds.get().size.height) / scale.max(0.01);
        height > 0.0 && height < super::composer_animation::METRICS.constrained_pane_height_px
    }

    /// CDXC:SessionChat 2026-09-23 WHY:
    /// A short pane's box first stayed open for as long as the text field held focus, but the field keeps focus (and its caret) when the user clicks the transcript, and selecting a session hands it focus without any click, so the box of the focused short pane was almost always open. It now opens only on something the user does to the box itself: a press inside it, or typing into it. A press anywhere else in the chat, the field losing focus, or another pane taking focus closes it again.
    pub(super) fn composer_collapsed(&self) -> bool {
        if self.composer_height_constrained() {
            return self.composer_collapse_allowed() && !self.short_pane_composer_open;
        }
        self.composer_collapse_eligible() && self.snapshot["composerCollapsed"] == true
    }

    /// This frame's chat box tween. `render` reads it before it builds the transcript, whose end
    /// inset comes from the same frame, and `render_composer` reads it again to paint the box.
    pub(super) fn composer_frame(
        &mut self,
        cx: &Context<Self>,
    ) -> super::composer_animation::ComposerFrame {
        // With the box kept expanded while scrolling, the only collapse left is a short pane's,
        // and it changes shape at once rather than animating.
        let reduce_motion = cx.reduce_motion() || keep_composer_expanded();
        self.composer_animation
            .set_collapsed(self.composer_collapsed(), reduce_motion);
        self.composer_animation.advance(reduce_motion)
    }

    pub(super) fn scrollable_transcript(
        &self,
        transcript: impl IntoElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let chat = cx.weak_entity();
        let p = super::appearance::ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        let scale = p.scale;
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            // React's transcript menu trigger wrapped the whole message list, minimap, scrollbar and
            // scroll button included; pills and links stop the press first (transcript_menu.rs).
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.transcript_secondary_press(event, window, cx)
                }),
            )
            .child(self.minimap_row(transcript.into_any_element(), cx))
            // Under window glass there is no solid colour to fade into; the ramp would paint a
            // dark band above the composer instead.
            .when(
                !crate::app::helpers::window_glass_active_for(self.main_window),
                |this: gpui::Div| {
                    this.child(
                        // React masked the viewport's last rows into the composer band
                        // (`--scroll-fade-mask` on `[data-slot='message-scroller-viewport']` in chat.css,
                        // deleted on 2026-09-25).
                        // GPUI cannot mask a scrolling list, so the same shape is painted: a plain div with
                        // no id and no interactivity, which registers no hitbox and takes no input.
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(px(24.0 * p.scale))
                            .bg(gpui::linear_gradient(
                                180.0,
                                gpui::linear_color_stop(p.background.opacity(0.0), 0.0),
                                gpui::linear_color_stop(p.background, 1.0),
                            )),
                    )
                },
            )
            .child(self.transcript_scrollbar(&p))
            .child(self.scroll_bottom_button(cx))
            .child(
                gpui::canvas(
                    |bounds, _, _| bounds,
                    move |_, bounds, window, _| {
                        let chat = chat.clone();
                        window.on_mouse_event(
                            move |event: &gpui::ScrollWheelEvent, phase, _, cx| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !bounds.contains(&event.position)
                                    || event.modifiers.control
                                {
                                    return;
                                }
                                let _ = chat.update(cx, |chat, cx| {
                                    let delta = event.delta.pixel_delta(px(16.0)).y.as_f32();
                                    if delta == 0.0 {
                                        return;
                                    }
                                    let offset =
                                        -chat.list.scroll_px_offset_for_scrollbar().y.as_f32();
                                    // The list's end padding is the band a collapsed box left
                                    // behind (`ComposerFrame::transcript_inset`), not transcript:
                                    // the gesture measures to the last row's end, so a collapsed
                                    // box expands again as soon as the rows are all in view.
                                    let maximum = (chat.list.max_offset_for_scrollbar().y.as_f32()
                                        - chat.composer_animation.current_transcript_inset())
                                    .max(0.0);
                                    let end_distance = (maximum - offset).max(0.0);
                                    // CDXC:SessionChat 2026-09-23 WHY: A wheel in either direction that leaves an expanded box's list inside the band the collapsed box would only fill with the held inset is not a collapse gesture. Collapsing there uncovers no rows. Upward, a small wobble at the edge flipped the box on every reversal; downward, a box that had just expanded one band short of the real end collapsed again on the next 24px toward the end and re-expanded at once. Supersedes the 2026-09-21 upward-only guard.
                                    if !chat.composer_collapsed()
                                        && (end_distance + delta).max(0.0)
                                            < chat.composer_animation.collapse_travel(scale)
                                    {
                                        return;
                                    }
                                    chat.invoke(
                                        json!({"type":"composerScroll", "delta":delta,
                            "distanceToEnd":end_distance,
                            "canScroll":if delta > 0.0 { offset > 0.0 } else { end_distance > 0.0 },
                            "eligible":chat.composer_collapse_eligible()}),
                                        cx,
                                    );
                                });
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}

impl NativeChatView {
    /// Every press inside the chat, seen before anything else handles it: inside the box opens a
    /// short pane's box, anywhere else closes it.
    pub(super) fn note_short_pane_composer_press(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let open = self.composer_bounds.get().contains(&position);
        if self.short_pane_composer_open != open {
            self.short_pane_composer_open = open;
            cx.notify();
        }
    }
}

/// `sessionChatKeepComposerExpanded`; a missing key reads as its default, off.
fn keep_composer_expanded() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("sessionChatKeepComposerExpanded")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}
