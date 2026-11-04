//! The previous and next arrows at the viewer's sides, shown while the pointer is near that side
//! and there is a picture that way, and the "3 / 6" counter under the picture.
//!
//! CDXC:SessionChat 2026-10-22 DECISION:
//! User: "left and right arrows that appear when the pointer comes near the viewer's left or right edge", alongside the Left and Right keys, move to the previous or next picture. Asked for the floating page window's pictures; the viewer is the transcript's own, so a message with several pictures gets the same arrows.
//!
//! CDXC:SessionChat 2026-10-24 SEE-ALSO:
//! The list the arrows step through is the whole transcript's pictures (gx-chat-core transcript/pictures.rs, with the user's words). An arrow fades in as the pointer nears its side, fades out as it leaves, and is not drawn at the first or last picture; the phone's viewer (apps/mobile/app/src/chat/native/cards/ImageViewer.tsx) shows the same buttons and counter.

use super::super::appearance::ChatAppearance;
use super::window::ImageViewerWindow;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, DispatchPhase, InteractiveElement as _,
    IntoElement, MouseMoveEvent, ParentElement as _, Pixels, StatefulInteractiveElement as _,
    Styled as _, canvas, div, px, svg,
};
use gpui_component::tooltip::Tooltip;
use std::time::Duration;

/// How close to a side the pointer has to come for that side's arrow to show.
const EDGE_REACH: f32 = 120.0;
const ARROW_SIZE: f32 = 44.0;
const ARROW_INSET: f32 = 16.0;
const ARROW_FADE: Duration = Duration::from_millis(160);

impl ImageViewerWindow {
    /// The side the pointer is near: -1 left, 1 right, 0 neither.
    fn note_pointer(&mut self, x: Pixels, width: Pixels, cx: &mut Context<Self>) {
        let edge = if x < px(EDGE_REACH) {
            -1
        } else if x > width - px(EDGE_REACH) {
            1
        } else {
            0
        };
        if edge == self.edge {
            return;
        }
        // The arrow the pointer just left fades out; the one it reached fades in.
        self.fading = (self.edge != 0).then_some(self.edge);
        self.edge = edge;
        self.arrow_generation += 1;
        let generation = self.arrow_generation;
        if self.fading.is_some() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(ARROW_FADE).await;
                let _ = this.update(cx, |this, cx| {
                    if this.arrow_generation == generation {
                        this.fading = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// The two arrows and the window-wide pointer listener that shows them; nothing when there is
    /// only one picture.
    pub(super) fn step_arrows(
        &self,
        index: usize,
        count: usize,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if count < 2 {
            return None;
        }
        let view = cx.weak_entity();
        // A canvas registers no hitbox, so it hears every move without taking input from anything.
        let listener = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let width = window.viewport_size().width;
                        let _ = view.update(cx, |this, cx| {
                            this.note_pointer(event.position.x, width, cx)
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();
        // The sidebar tooltip bubble's fill: these float over the picture (CDXC:Tooltips 2026-10-25
        // on `in_window_bubble`).
        let surface = crate::app::native_sidebar::link_chips::in_window_bubble(p.window_glass).fill;
        let surface_hover =
            surface.blend(crate::app::helpers::titlebar_popup_menu_foreground().opacity(0.1));
        let generation = self.arrow_generation;
        let arrow = |delta: isize, showing: bool, cx: &Context<Self>| {
            let (id, label, icon) = if delta < 0 {
                (
                    "chat-image-previous",
                    "Previous image (←)",
                    "titlebar/chevron-left.svg",
                )
            } else {
                (
                    "chat-image-next",
                    "Next image (→)",
                    "titlebar/chevron-right.svg",
                )
            };
            let button = div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(ARROW_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(surface)
                .when(p.light, |this| this.border_1().border_color(p.border))
                .chat_cursor_pointer()
                .hover(move |style| style.bg(surface_hover))
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                // The surround closes the viewer on a press; the arrow keeps it.
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.chat
                        .update(cx, |chat, cx| chat.step_image_viewer(delta, cx));
                }))
                .child(
                    svg()
                        .path(icon)
                        .size(px(22.0))
                        .text_color(crate::app::helpers::titlebar_popup_menu_foreground()),
                );
            // A plain column takes no input, so only the button itself is in the way.
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .when(delta < 0, |this| this.left(px(ARROW_INSET)))
                .when(delta > 0, |this| this.right(px(ARROW_INSET)))
                .flex()
                .items_center()
                .child(button.with_animation(
                    (
                        if showing {
                            "chat-image-arrow-in"
                        } else {
                            "chat-image-arrow-out"
                        },
                        generation,
                    ),
                    Animation::new(ARROW_FADE).with_easing(gpui::ease_out_quint()),
                    move |button, delta| button.opacity(if showing { delta } else { 1.0 - delta }),
                ))
        };
        // Nothing to step to past either end, so that side shows no arrow at all.
        let reachable = |side: isize| {
            if side < 0 {
                index > 0
            } else {
                index + 1 < count
            }
        };
        let drawn = |side: isize| -> Option<bool> {
            if !reachable(side) {
                None
            } else if self.edge == side {
                Some(true)
            } else if self.fading == Some(side) {
                Some(false)
            } else {
                None
            }
        };
        let counter = div()
            .absolute()
            .bottom(px(16.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .px(px(10.0))
                    .py(px(3.0))
                    .rounded_full()
                    .bg(surface)
                    .when(p.light, |this| this.border_1().border_color(p.border))
                    .text_size(px(12.0))
                    .text_color(crate::app::helpers::titlebar_popup_menu_foreground())
                    .child(format!("{} / {count}", index + 1)),
            );
        Some(
            div()
                .absolute()
                .size_full()
                .child(listener)
                .child(counter)
                .when_some(drawn(-1), |this, showing| {
                    this.child(arrow(-1, showing, cx))
                })
                .when_some(drawn(1), |this, showing| this.child(arrow(1, showing, cx)))
                .into_any_element(),
        )
    }
}
