//! The Windows and Linux host.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! Neither backend has AppKit's `addChildWindow:` or its panel clipping, and GPUI exposes exactly
//! one cross-platform geometry call on an open window, `Window::resize`. So the slide is the same
//! shape AppKit runs, driven from here: the popup is resized from a sliver to its full width while
//! the panels stay pinned to its right edge. The pointer is read through `is_window_hovered`, which
//! is the only reading that works on Wayland, where no client may ask where the pointer is.

use std::time::Duration;
use std::time::Instant;

use gpui::Pixels;
use gpui::Point;

use super::model::*;
use crate::*;

/// How long the pointer may stay off the panel before it slides away, matching the AppKit host's
/// own 0.2s grace.
const FLOATING_REVEAL_DISMISS_DELAY_MS: u64 = 200;

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-23 WHY:
    /// Chat menus, pickers and previews own separate windows. Moving focus or the pointer into a child opened by the floating chat must keep its source panel alive, just as a sidebar menu does.
    pub(super) fn floating_reveal_chat_child_active(&self, cx: &gpui::App) -> bool {
        let Some(panel) = self.floating_reveal.panel.as_ref() else {
            return false;
        };
        self.native_chat_views.values().any(|chat| {
            chat.read(cx).active_child_window_source(cx) == Some(panel.window.window_id())
        })
    }

    pub(super) fn sync_floating_reveal_host(
        &mut self,
        requested: bool,
        keep_under_pointer: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.floating_reveal.panel.is_none() {
            // A drag that happens to cross the strip is not a reveal gesture. AppKit refuses the
            // same way, from `NSEvent.pressedMouseButtons`; here the app's own drag flags say it,
            // and `workarea_header_blocks_mouse` is already the phrase for "no drag in flight".
            let armed = self.floating_reveal.edge_hovered && self.workarea_header_blocks_mouse();
            if requested || keep_under_pointer || armed {
                self.open_floating_reveal(keep_under_pointer, cx);
            }
            return;
        }
        // The main window moved out from under the panel, and only AppKit can move a window after
        // it is open. Take the panel away rather than leave it stranded on the old spot.
        if self
            .floating_reveal
            .panel
            .as_ref()
            .is_some_and(|panel| self.floating_reveal_frame(1.0).origin != panel.anchor)
        {
            self.close_floating_reveal(cx);
            return;
        }
        let now = Instant::now();
        let hovered = self
            .floating_reveal
            .panel
            .as_ref()
            .and_then(|panel| {
                panel
                    .window
                    .update(cx, |_, window, _| window.is_window_hovered())
                    .ok()
            })
            .unwrap_or(false);
        let mut inside = hovered
            || self.floating_reveal_chat_child_active(cx)
            || self.floating_reveal.edge_hovered
            // The rail, left of the panel, keeps it out as the window's left edge does on macOS
            // (`sidebar_rail_reveal_hover`).
            || self.sidebar_rail.reveal_half.is_some()
            // Beside a docked sidebar the pointer that opened the panel is still over the sidebar.
            || (self.floating_reveal.sidebar_hovered && !self.sidebar_collapsed)
            // A menu or a rename field opened from the floating sidebar owns the pointer for as
            // long as it is up; taking the panel away under it would close both.
            || self.native_sidebar.menu.is_some()
            || self.native_sidebar.name_editor.is_some();
        if let Some(until) = self.floating_reveal.requested_until {
            if hovered || now >= until {
                self.floating_reveal.requested_until = None;
            } else {
                inside = true;
            }
        }
        if inside {
            self.floating_reveal.outside_since = None;
            self.set_floating_reveal_slide_target(1.0);
        } else {
            let since = *self.floating_reveal.outside_since.get_or_insert(now);
            if now.duration_since(since) >= Duration::from_millis(FLOATING_REVEAL_DISMISS_DELAY_MS)
            {
                if self.floating_reveal.slide.target != 0.0 {
                    self.dismiss_floating_reveal_chat_windows(cx);
                }
                self.set_floating_reveal_slide_target(0.0);
            }
        }
        self.step_floating_reveal_slide(cx);
    }

    pub(super) fn attach_floating_reveal_panel(
        &mut self,
        window: gpui::WindowHandle<gpui_component::Root>,
        _native_view: Option<*mut std::ffi::c_void>,
        anchor: Point<Pixels>,
        content: FloatingRevealContent,
        width: f32,
        sticky: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.floating_reveal.panel = Some(FloatingRevealPanel {
            window,
            anchor,
            content,
            width,
        });
        // A links card opened from the docked sidebar does not stay up over the floating one.
        if content.sidebar {
            self.close_session_links_card(cx);
        }
        self.floating_reveal.outside_since = None;
        self.floating_reveal.slide = FloatingRevealSlide {
            // A panel kept under the pointer by a layout change never slides; only the hover
            // reveal does.
            progress: if sticky { 1.0 } else { 0.0 },
            from: if sticky { 1.0 } else { 0.0 },
            target: 1.0,
            started: Instant::now(),
        };
        true
    }

    pub(super) fn dispose_floating_reveal_host(&self, _panel: &FloatingRevealPanel) {}

    fn set_floating_reveal_slide_target(&mut self, target: f32) {
        if self.floating_reveal.slide.target == target {
            return;
        }
        self.floating_reveal.slide.from = self.floating_reveal.slide.progress;
        self.floating_reveal.slide.target = target;
        self.floating_reveal.slide.started = Instant::now();
    }

    fn step_floating_reveal_slide(&mut self, cx: &mut gpui::Context<Self>) {
        let slide = self.floating_reveal.slide;
        let elapsed = slide.started.elapsed().as_secs_f32();
        let duration = floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f32();
        let t = if duration <= 0.0 {
            1.0
        } else {
            (elapsed / duration).clamp(0.0, 1.0)
        };
        let remaining = 1.0 - t;
        let eased = 1.0 - remaining * remaining * remaining;
        let progress = slide.from + (slide.target - slide.from) * eased;
        self.floating_reveal.slide.progress = progress;
        // The panel covers the strip once it is fully out, so the main window stops seeing the
        // pointer there and the strip's hover would stay stuck on, holding the panel open forever.
        // The strip's job ends when the slide does; from here the panel's own hover is the truth.
        if slide.target == 1.0 && progress >= 0.999 {
            self.floating_reveal.edge_hovered = false;
        }
        if slide.target == 0.0 && progress <= 0.001 {
            self.close_floating_reveal(cx);
            return;
        }
        let Some((handle, width)) = self
            .floating_reveal
            .panel
            .as_ref()
            .map(|panel| (panel.window, panel.width))
        else {
            return;
        };
        let frame = self.floating_reveal_frame(width * progress);
        let _ = handle.update(cx, |_, window, _| window.resize(frame.size));
    }
}
