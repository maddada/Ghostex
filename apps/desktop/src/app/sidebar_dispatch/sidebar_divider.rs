//! The sidebar resize divider, drag, width reset, collapse, hover reveal polling and the command pane side.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use futures::channel::oneshot;

use crate::app::floating_reveal::model::SIDEBAR_HOVER_REVEAL_ACTIVE_POLL;
use crate::app::floating_reveal::model::SIDEBAR_HOVER_REVEAL_IDLE_POLL;

use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::resize_rail::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_sidebar_resize_divider(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("ghostex-gpui-sidebar-resize-divider")
            .relative()
            .flex_shrink_0()
            .w(px(SIDEBAR_DIVIDER_WIDTH))
            .h_full()
            .map(|this| {
                // With the work area frame on, the divider is part of the gutter and draws nothing
                // of its own: the body row (or, under glass, the gutter ring) paints there, and the
                // card's border is the only edge. It stays the grab target.
                if work_area_frame_active() {
                    return this;
                }
                this
                    // The body row sits 1px under the titlebar so panes can own
                    // their top edge; carry the titlebar hairline across the divider.
                    .border_t_1()
                    .border_color(glass_divider(titlebar_button_border_color()))
                    // Under glass the divider sits on the sidebar's tint (nothing tints the window
                    // beneath it), with its faint line laid over that.
                    .bg(if window_glass_active() {
                        sidebar_glass_tint()
                    } else {
                        sidebar_divider_line_color()
                    })
                    .when(window_glass_active(), |this| {
                        this.child(div().absolute().inset_0().bg(sidebar_divider_line_color()))
                    })
            })
            // The workspace beside the sidebar is often a CEF page (Browser, Docs), so the
            // whole grab strip lies over the native sidebar.
            .child(resize_rail_deferred_strip(
                resize_rail_grab_strip(
                    "ghostex-gpui-sidebar-resize-grab-strip",
                    WorkspaceSplitAxis::Horizontal,
                    ResizeRailGrabSide::Leading,
                )
                .on_hover(cx.listener(|this, hovered, _, cx| {
                    this.set_sidebar_divider_hovering(*hovered, cx);
                }))
                .on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
                    this.set_sidebar_divider_hovering(true, cx);
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.handle_sidebar_divider_mouse_down(event, window, cx);
                    }),
                )
                .when(self.sidebar_divider_hover_visible, |this| {
                    this.child(resize_rail_hover_line(
                        "ghostex-gpui-sidebar-resize-divider-hover-line",
                        WorkspaceSplitAxis::Horizontal,
                        ResizeRailGrabSide::Leading,
                    ))
                }),
            ))
    }

    pub(crate) fn set_sidebar_divider_hovering(
        &mut self,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.sidebar_divider_hovering == hovered {
            if !hovered && self.sidebar_divider_hover_visible {
                self.sidebar_divider_hover_visible = false;
                cx.notify();
            }
            return;
        }

        self.sidebar_divider_hover_epoch = self.sidebar_divider_hover_epoch.wrapping_add(1);
        self.sidebar_divider_hovering = hovered;

        if !hovered {
            self.sidebar_divider_hover_visible = false;
            cx.notify();
            return;
        }

        self.sidebar_divider_hover_visible = false;
        let epoch = self.sidebar_divider_hover_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SIDEBAR_DIVIDER_HOVER_DELAY)
                .await;

            let _ = this.update(cx, |this, cx| {
                if this.sidebar_divider_hover_epoch == epoch && this.sidebar_divider_hovering {
                    this.sidebar_divider_hover_visible = true;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn handle_sidebar_root_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.handle_sidebar_drag_move(event, window, cx);

        let hovering =
            self.sidebar_drag.is_some() || self.sidebar_divider_contains_mouse_position(event);
        self.set_sidebar_divider_hovering(hovering, cx);
    }

    pub(crate) fn sidebar_divider_contains_mouse_position(&self, event: &MouseMoveEvent) -> bool {
        self.sidebar_divider_contains_position(event.position)
    }

    pub(crate) fn sidebar_divider_contains_position(&self, position: gpui::Point<Pixels>) -> bool {
        if !gpui_sidebar_chrome_visible(self.sidebar_collapsed) {
            return false;
        }
        let x = position.x.as_f32();
        let (start_x, end_x) = gpui_sidebar_divider_x_bounds(self.sidebar_width);

        // CDXC:Sidebar 2026-09-20 WHY:
        // The divider used to start below the titlebar row. That row is gone, so the body row and
        // its divider own the window from its top edge down; the header is a child of the
        // workspace column to the divider's right and never overlaps this band.
        x >= start_x && x <= end_x
    }

    pub(crate) fn handle_sidebar_divider_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        if event.click_count >= 2 {
            self.reset_sidebar_width(window);
            cx.notify();
            return;
        }

        self.sidebar_drag = Some(SidebarDragState {
            start_x: event.position.x.as_f32(),
            start_width: self.sidebar_width,
        });
        self.set_sidebar_divider_hovering(true, cx);
        // The grab strip stops blocking the mouse once a drag is active, which needs a new frame even
        // when the hover state above was already set.
        cx.notify();
    }

    pub(crate) fn handle_sidebar_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.sidebar_drag else {
            return;
        };

        if !event.dragging() {
            self.finish_sidebar_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        let max_width = current_sidebar_max_width(window, self.active_mode);
        let delta = event.position.x.as_f32() - drag.start_x;
        let next_width = clamp_sidebar_width(drag.start_width + delta, max_width);
        if (next_width - self.sidebar_width).abs() >= 0.5 {
            self.sidebar_width = next_width;
            cx.notify();
        }
    }

    pub(crate) fn handle_sidebar_drag_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.sidebar_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_sidebar_drag(cx);
    }

    pub(crate) fn finish_sidebar_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sidebar_drag.take().is_none() {
            return;
        }
        self.clear_sidebar_divider_hover_state();
        self.persist_sidebar_width_if_lead_window();
        cx.notify();
    }

    pub(crate) fn reset_sidebar_width(&mut self, window: &Window) {
        let max_width = current_sidebar_max_width(window, self.active_mode);
        let reset_width = read_sidebar_default_width_setting().unwrap_or(SIDEBAR_RESET_WIDTH);
        self.sidebar_width = clamp_sidebar_width(reset_width, max_width);
        self.cancel_sidebar_divider_interaction_state();
        self.persist_sidebar_width_if_lead_window();
    }

    /// The saved width is the one the next launch opens with, which is the lead window's; a New
    /// Window keeps its own width to itself (app/workspace_windows/).
    fn persist_sidebar_width_if_lead_window(&self) {
        if self.is_lead_window() {
            persist_sidebar_width_setting(self.sidebar_width);
        }
    }

    pub(crate) fn cancel_sidebar_divider_interaction_state(&mut self) {
        self.sidebar_drag = None;
        self.clear_sidebar_divider_hover_state();
    }

    pub(crate) fn clear_sidebar_divider_hover_state(&mut self) {
        self.sidebar_divider_hovering = false;
        self.sidebar_divider_hover_visible = false;
        self.sidebar_divider_hover_epoch = self.sidebar_divider_hover_epoch.wrapping_add(1);
    }

    pub(crate) fn toggle_gpui_sidebar_collapsed(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:Sidebar 2026-06-26-10:04:
        GPUI Cmd+B and the shared `toggleSidebarCollapsed` action collapse only shell chrome state. Preserve `sidebar_width` and cancel divider interaction state so expanding restores the user's resized sidebar without writing a zero-width setting or leaving stale hover/drag chrome active.
        */
        self.sidebar_collapsed = gpui_next_sidebar_collapsed_state(self.sidebar_collapsed);
        self.cancel_sidebar_divider_interaction_state();
        self.update_sidebar_cef_surface_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn update_sidebar_cef_surface_visibility(&mut self, cx: &mut gpui::Context<Self>) {
        self.update_sidebar_reveal(false, false, cx);
    }

    /// CDXC:Sidebar 2026-09-20 DECISION:
    /// User (ruling 7B, screen 10): edge-hover floating is on all three platforms, not macOS only.
    /// The gesture, its panel and its dismissal therefore live in `app/floating_reveal/`, which is
    /// shared; only the host that owns the child window is per platform. This supersedes the
    /// macOS-only reveal this function used to forward to.
    pub(crate) fn update_sidebar_reveal(
        &mut self,
        requested: bool,
        keep_under_pointer: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.update_floating_reveal(requested, keep_under_pointer, cx);
    }

    /// The gesture's clock. Pointer-leave is the only thing no element can report (the panel covers
    /// the strip that armed it), so one sweep owns the whole reveal. It tightens to a frame while a
    /// panel is on screen, because the backends that animate the slide themselves step it here.
    ///
    /// CDXC:Sidebar 2026-10-23 WHY:
    /// The sweep used to wake the app every 60ms for as long as the window existed, although with
    /// nothing folded away there is no edge to watch, and a minimized or covered window has no
    /// pointer over it. It now parks after a sweep that finds neither a panel or armed edge nor
    /// something folded away in a visible window, and wakes when the app changes (folding the
    /// sidebar or maximizing a view always notifies it) or the window is shown again. The sweep
    /// that parks has already hidden the macOS edge zone and closed any panel.
    pub(crate) fn start_sidebar_hover_reveal_polling(
        &self,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Only the two observers own the slot a parked sweep waits on, so when the app and its
        // window are gone the slot goes with them and the parked sweep ends.
        let waker: Rc<RefCell<Option<oneshot::Sender<()>>>> = Rc::default();
        let parked_slot = Rc::downgrade(&waker);
        let window_visible = Rc::new(Cell::new(window.is_visible()));
        cx.observe_self({
            let waker = waker.clone();
            let window_visible = window_visible.clone();
            move |this, _cx| {
                if window_visible.get() && this.floating_reveal_sweep_needed() {
                    wake_floating_reveal_sweep(&waker);
                }
            }
        })
        .detach();
        window
            .observe_window_visibility(move |visibility, _window, _cx| {
                window_visible.set(visibility.is_visible());
                if visibility.is_visible() {
                    wake_floating_reveal_sweep(&waker);
                }
            })
            .detach();
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let mut interval = Some(SIDEBAR_HOVER_REVEAL_IDLE_POLL);
            loop {
                match interval {
                    Some(interval) => cx.background_executor().timer(interval).await,
                    None => {
                        let Some(slot) = parked_slot.upgrade() else {
                            break;
                        };
                        let (sender, receiver) = oneshot::channel();
                        *slot.borrow_mut() = Some(sender);
                        drop(slot);
                        // Canceled when the observers holding the slot are dropped with the app
                        // or its window, which the update below then reports.
                        let _ = receiver.await;
                    }
                }
                let result = window.update(cx, |_, window, cx| {
                    this.update(cx, |this, cx| {
                        if this.poll_floating_reveal(window, cx) {
                            Some(SIDEBAR_HOVER_REVEAL_ACTIVE_POLL)
                        } else if window.is_visible() && this.floating_reveal_eligible() {
                            Some(SIDEBAR_HOVER_REVEAL_IDLE_POLL)
                        } else {
                            None
                        }
                    })
                });
                match result {
                    Ok(Ok(next)) => interval = next,
                    _ => break,
                }
            }
        })
        .detach();
    }

    /// Whether the reveal's sweep has anything to watch: something folded away that the edge
    /// floats back, or a panel, an armed edge or a reveal asked for by name still pending.
    fn floating_reveal_sweep_needed(&self) -> bool {
        self.floating_reveal_eligible()
            || self.floating_reveal.panel.is_some()
            || self.floating_reveal.edge_hovered
            || self.floating_reveal.requested_until.is_some()
    }

    pub(crate) fn apply_gpui_command_pane_side_from_saved_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        // Settings persists commandsPanelSide through the patch path; a save
        // whose side differs from the live placement re-docks the pane on the
        // next render instead of waiting for relaunch. Any in-flight rail drag
        // belongs to the old axis, so drop it rather than let it keep resizing.
        let saved_side = gpui_command_pane_side_from_shared_settings(settings_snapshot);
        if saved_side == self.command_pane_side {
            return;
        }
        self.command_pane_side = saved_side;
        self.command_pane.resize_drag = None;
        self.clear_command_resize_hover_state();
    }
}

/// Resumes a parked reveal sweep; a sweep that is running has nothing stored here.
fn wake_floating_reveal_sweep(waker: &RefCell<Option<oneshot::Sender<()>>>) {
    if let Some(sender) = waker.borrow_mut().take() {
        let _ = sender.send(());
    }
}
