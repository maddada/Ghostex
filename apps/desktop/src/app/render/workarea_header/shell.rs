//! The header row itself: its frame, its window-control duties, its drag behaviour, and the float
//! that replaces the hairline the old titlebar drew under itself.

use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;

use super::anchor::record_workarea_header_bottom_y;
use crate::app::claude_web::HeaderControl;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::render::window_drag_region::window_drag_region;
use crate::*;

impl GhostexGpuiApp {
    /// True while the header's own half of the band is too narrow for its labels, which is what the
    /// mockup's narrow chat column drops first. With a view open that half is the sessions column,
    /// so opening a view is usually enough to reach it.
    pub(crate) fn workarea_header_compact(&self, window: &Window) -> bool {
        self.workarea_header_row_width(window) < WORKAREA_HEADER_COMPACT_WIDTH
    }

    pub(crate) fn render_workarea_header_row(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:Titlebar 2026-09-20 DECISION:
        User: the header keeps only what belongs to the active project. The sidebar toggle stays a flat Tabler layout-sidebar glyph, the right controls are Git, Actions and Open In, and everything occasional (Ask Ghostex, Tips & Tricks, Resources, Dev servers, Extensions) is reached from one trailing menu button.
        This supersedes the 2026-06-22 rule that listed Tips and Resources as their own titlebar buttons; Settings, Keep Awake, the notification bell and the account usage meters live in sidebar chrome, not this strip.

        CDXC:Titlebar 2026-09-20 DECISION:
        User: the window has no titlebar row, and the view buttons are not in it. This header carries
        the project breadcrumb, Start/Open/Commit, the trailing ⋯ menu and the two panel toggles, with
        no line under it; which views are open is the view panel's own tab strip
        (render/view_tab_strip.rs), so the centred mode switcher and its compact dropdown are gone.
        This supersedes the 2026-06-14 full-width titlebar strip and the 2026-09-11 rule that kept
        the view buttons centred in this row.
        */
        let compact = self.workarea_header_compact(window);

        /*
        CDXC:Titlebar 2026-09-20 DECISION:
        User: "they just do nice fade from bottom mask thingy at the top". The header floats over
        the sessions column instead of sitting above it, so the transcript scrolls under it and
        fades out; it paints the colour of the surface beneath it, draws no bottom border, and there
        is no edge at all where it meets the content. This is the overlap the user approved, and it
        is scoped to the header over the content beneath it: the row is opaque and occludes the
        mouse, so the band it covers is the drag area it has always been, and the ramp below it
        (`render_workarea_header_content_fade`) carries no hitbox at all, so everything under the
        faded strip keeps every click, drag and scroll. Nothing here licenses another overlay. How
        the band is divided between this row and the view panel's tab strip is in band.rs.
        The 2026-09-21 follow-up extends the colour and the missing edge to every state of a GPUI
        chat, including the ones whose own chrome keeps the transcript below the row rather than
        under it (`agents_column_meets_gpui_chat`).
        2026-09-23: User: with window glass on, the header is see-through and the chat starts
        below it instead of scrolling under it, since an opaque row would read as a solid strip on
        the glass. Without glass everything above still holds.

        CDXC:Titlebar 2026-09-20 WHY:
        `occlude()` is what makes the float honest rather than a second input layer: without it the
        row's own hitbox would not stop a click on the empty drag area from also reaching the
        transcript painted underneath, and the same press would both drag the window and land in the
        chat. It blocks the mouse only where the header is actually drawn, and only while no drag is
        in flight (`workarea_header_blocks_mouse`).

        CDXC:Titlebar 2026-09-20 WHY:
        The row paints the chat's own background, not the workspace's, over a GPUI chat column: the
        fade below it ramps from this colour to transparent, so a colour the content beneath does
        not use turns that ramp into a crossfade between two surfaces instead of a fade-out of one,
        and where the chat starts below the row instead the same colour is what leaves no visible
        band. Over any other column the content below carries the workspace background, so that is
        what the row paints there.
        */
        let header_background = self.workarea_header_surface_color();
        let header = div()
            .id("ghostex-gpui-workarea-header")
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(header_background)
            .when(self.workarea_header_blocks_mouse(), |header| {
                header.occlude()
            })
            .text_color(titlebar_text_color())
            .font_family(crate::ui_fonts::UI_FONT)
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .on_prepaint(|bounds, _window, _cx| {
                record_workarea_header_bottom_y(bounds.bottom().as_f32());
            });

        let header = window_drag_region(header);
        // Diagnostics only (`log_window_drag`): every left press inside the header's rectangle as
        // the window saw it, before any element handles it. A press logged here with no
        // `gpui.windowDrag.press` after it means something above the header took it; no line at all
        // means it never reached this window.
        let split_panes = self.agents_workspace.rendered_leaf_order().len();
        let header = header.relative().child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    // The first held-button move the window gets inside the header per press, and
                    // the release, both before any element handles them.
                    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, _| {
                        if phase == gpui::DispatchPhase::Capture
                            && event.pressed_button == Some(MouseButton::Left)
                            && bounds.contains(&event.position)
                            && !HEADER_DRAG_SEEN.swap(true, std::sync::atomic::Ordering::Relaxed)
                        {
                            crate::app::render::window_drag_region::log_window_drag(
                                "headerDragSeen",
                                event.position,
                                serde_json::json!({}),
                            );
                        }
                    });
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, _| {
                        if phase == gpui::DispatchPhase::Capture
                            && event.button == MouseButton::Left
                            && bounds.contains(&event.position)
                        {
                            crate::app::render::window_drag_region::log_window_drag(
                                "headerReleaseSeen",
                                event.position,
                                serde_json::json!({ "clickCount": event.click_count }),
                            );
                        }
                    });
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, _, _| {
                        if phase == gpui::DispatchPhase::Capture {
                            HEADER_DRAG_SEEN.store(false, std::sync::atomic::Ordering::Relaxed);
                        }
                        if phase == gpui::DispatchPhase::Capture
                            && event.button == MouseButton::Left
                            && bounds.contains(&event.position)
                        {
                            crate::app::render::window_drag_region::log_window_drag(
                                "headerPressSeen",
                                event.position,
                                serde_json::json!({
                                    "panes": split_panes,
                                    "clickCount": event.click_count,
                                    "firstMouse": event.first_mouse,
                                }),
                            );
                        }
                    });
                },
            )
            .absolute()
            .inset_0(),
        );

        header
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .child(
                h_flex()
                    .id("ghostex-gpui-workarea-header-left")
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    // Claude is its own place: no breadcrumb of the project and session from
                    // before it (app/claude_web/header.rs).
                    .map(|this| {
                        if !self.header_control_shown(HeaderControl::Breadcrumb) {
                            this.child(self.render_claude_header_leading(window, cx))
                        } else {
                            this.child(self.render_workarea_header_breadcrumb(compact, cx))
                        }
                    }),
            )
            /*
            CDXC:Titlebar 2026-09-21 DECISION:
            User: the header's buttons must not disappear when the chat column is narrowed. The
            trailing half sizes to its controls and the breadcrumb takes whatever is left, the way
            a pane tab bar keeps its action cluster while its tabs scroll. This supersedes the even
            split between the two halves, which clipped the panel toggles at the trailing edge
            while the breadcrumb still held half the row.
            */
            .child(
                h_flex()
                    .id("ghostex-gpui-workarea-header-right")
                    .h_full()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .justify_end()
                    .child(self.render_workarea_header_actions(compact, window, cx)),
            )
    }
}

/// Diagnostics only: whether this press's first held-button move over the header was logged.
static HEADER_DRAG_SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
