//! When the files list is docked, floating, peeking or hidden, and the slide between those.

use std::time::{Duration, Instant};

use gpui::{Context, Pixels, Point};

use super::render::{FLOATING_SIDEBAR_MAX_WIDTH, view_width};
use super::state::{DocsSlide, DocsTransient};
use crate::GhostexGpuiApp;

/// CDXC:Docs 2026-09-12 DECISION:
/// User: hovering the corner button peeks the files list; a short open delay stops the list flashing open when the cursor merely crosses the corner, and a short close grace stops a slight overshoot from collapsing it.
pub(crate) const PEEK_OPEN_DELAY: Duration = Duration::from_millis(150);
pub(crate) const PEEK_CLOSE_GRACE: Duration = Duration::from_millis(200);

/// Where the files list is this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DocsSidebarLayout {
    pub(crate) narrow: bool,
    /// No file is open, so the list is docked whatever the width or the pinned intent.
    pub(crate) forced: bool,
    pub(crate) docked: bool,
    /// Showing over the document (a drawer or a peek).
    pub(crate) overlay: bool,
}

impl DocsSidebarLayout {
    pub(crate) fn visible(self) -> bool {
        self.docked || self.overlay
    }
}

impl GhostexGpuiApp {
    /// CDXC:Docs 2026-09-12 DECISION:
    /// User: the files sidebar has one persisted intent, pinned or hidden, and the shell width alone decides whether a pinned sidebar is docked or, below the floating breakpoint, a closed drawer. Width never rewrites the intent, so growing the shell back past the breakpoint restores whatever the user chose. Below the breakpoint the sidebar never covers the document on load; it opens only as a transient drawer and closes when a file opens, on an outside click, or on Escape. Hovering the corner button peeks the list for as long as the cursor stays inside it; the button under the cursor then pins it (docks it when wide, holds the drawer open when narrow).
    ///
    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: when no file is open the files list must be showing, docked, even in a narrow pane where it would otherwise be a floating drawer, because an empty document area with no list is a dead end. The forced dock never rewrites the pinned intent, so opening a file returns the list to whatever the user chose for that width.
    pub(crate) fn native_docs_sidebar_layout(&self) -> DocsSidebarLayout {
        let state = &self.native_docs;
        let narrow = view_width() < FLOATING_SIDEBAR_MAX_WIDTH;
        // CDXC:Docs 2026-11-13 WHY: a selection naming a file that is not open (one closed while the open files were being restored) showed "Select a file" with neither the list nor its Show files button in a narrow pane, so what counts is whether a file is shown, not whether one is selected.
        let forced = state.active_document().is_none();
        let docked = (state.sidebar_pinned && !narrow) || forced;
        DocsSidebarLayout {
            narrow,
            forced,
            docked,
            overlay: !docked && state.transient.is_some(),
        }
    }

    fn native_docs_start_slide(&mut self, opening: bool) {
        let id = self.native_docs.slide.map_or(0, |slide| slide.id + 1);
        self.native_docs.slide = Some(DocsSlide {
            opening,
            started: Instant::now(),
            id,
        });
    }

    /// The docked list's frame this frame: pinning and unpinning tween its width like the app's
    /// own panels, with the list sliding at its full width inside the frame.
    ///
    /// CDXC:Docs 2026-09-25 DECISION:
    /// User: "the animation for the docs sidebar appearing/pinning/collapsing is all not matching the sessions sidebar (no animation when pinning/unpinning)". Pinning and unpinning the docked files list tween its width on the panels' own motion (`panel_motion.rs`: the Panel animations speed and curve, Reduce Motion snaps), the list anchored to the view's right edge like the side panel, and the document resizing with it as it does beside the sessions sidebar. The floating list (a drawer or a peek) slides its own window like the floating sessions sidebar. This supersedes the 2026-09-12 decision that only the panel moved and nothing reflowed.
    pub(crate) fn native_docs_sample_docked_motion(
        &mut self,
        layout: DocsSidebarLayout,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> crate::app::panel_motion::PanelFrame {
        let state = &mut self.native_docs;
        if state.docked_narrow != Some(layout.narrow) {
            if state.docked_narrow.is_some() {
                state.docked_motion.reset();
            }
            state.docked_narrow = Some(layout.narrow);
        }
        let frame = state.docked_motion.sample(
            layout.docked,
            super::render::SIDEBAR_WIDTH,
            0.0,
            cx.reduce_motion(),
            web_time::Instant::now(),
        );
        if frame.animating {
            window.request_animation_frame();
        }
        frame
    }

    /// The floating list's slide length: the panels' own duration, or none with Reduce Motion.
    pub(crate) fn native_docs_slide_duration() -> Duration {
        crate::app::floating_reveal::model::floating_reveal_slide_duration(
            crate::app::helpers::gpui_macos_reduce_motion_enabled(),
        )
    }

    /// How far the sliding panel is across its own width (0 = fully out, 1 = hidden), and whether
    /// a closing slide is still playing and so keeps the panel drawn.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn native_docs_slide_offset(&self) -> (f32, bool) {
        let Some(slide) = self.native_docs.slide else {
            return (0.0, false);
        };
        let duration = Self::native_docs_slide_duration().as_secs_f32();
        let progress = if duration <= 0.0 {
            1.0
        } else {
            (slide.started.elapsed().as_secs_f32() / duration).min(1.0)
        };
        let eased = crate::app::panel_motion::panel_motion_eased(progress);
        if slide.opening {
            (1.0 - eased, false)
        } else {
            (eased, progress < 1.0)
        }
    }

    pub(crate) fn native_docs_slide_running(&self) -> bool {
        self.native_docs
            .slide
            .is_some_and(|slide| slide.started.elapsed() < Self::native_docs_slide_duration())
    }

    /// Shows the list: a drawer on a narrow view, docked and pinned on a wide one. Used by the
    /// corner button, the Pin button and Cmd+F.
    pub(crate) fn native_docs_show_sidebar(&mut self, cx: &mut Context<Self>) {
        let layout = self.native_docs_sidebar_layout();
        self.native_docs.peek_timer = None;
        if layout.narrow {
            if !layout.visible() {
                self.native_docs_start_slide(true);
            }
            self.native_docs.transient = Some(DocsTransient::Drawer);
        } else {
            self.native_docs.sidebar_pinned = true;
            self.native_docs.transient = None;
            self.native_docs_persist_sidebar_pinned(cx);
        }
        self.native_docs_notify(cx);
    }

    /// Hides the list and records the intent.
    pub(crate) fn native_docs_hide_sidebar(&mut self, cx: &mut Context<Self>) {
        let was_visible = self.native_docs_sidebar_layout().visible();
        self.native_docs.sidebar_pinned = false;
        self.native_docs.transient = None;
        self.native_docs.peek_timer = None;
        if was_visible && !self.native_docs_sidebar_layout().visible() {
            self.native_docs_start_slide(false);
        }
        self.native_docs_persist_sidebar_pinned(cx);
        self.native_docs_notify(cx);
    }

    /// Closes a drawer or a peek, leaving the pinned intent alone.
    pub(crate) fn native_docs_close_transient(&mut self, cx: &mut Context<Self>) {
        if self.native_docs.transient.take().is_none() {
            return;
        }
        self.native_docs.peek_timer = None;
        if !self.native_docs_sidebar_layout().visible() {
            self.native_docs_start_slide(false);
        }
        self.native_docs_notify(cx);
    }

    /// Opening a file closes a drawer (not a peek, which follows the pointer).
    pub(crate) fn native_docs_close_drawer(&mut self, cx: &mut Context<Self>) {
        if self.native_docs.transient == Some(DocsTransient::Drawer) {
            self.native_docs_close_transient(cx);
        }
    }

    /// Opens a peek after the open delay, unless the list is showing or just slid away.
    pub(crate) fn native_docs_schedule_peek(&mut self, cx: &mut Context<Self>) {
        let layout = self.native_docs_sidebar_layout();
        if layout.visible() || self.native_docs.peek_timer.is_some() {
            return;
        }
        if self
            .native_docs
            .slide
            .is_some_and(|slide| !slide.opening && self.native_docs_slide_running())
        {
            return;
        }
        let generation = self.native_docs.generation;
        self.native_docs.peek_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PEEK_OPEN_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.native_docs.peek_timer = None;
                if this.native_docs.generation != generation
                    || this.native_docs_sidebar_layout().visible()
                {
                    return;
                }
                this.native_docs.transient = Some(DocsTransient::Peek);
                this.native_docs_start_slide(true);
                this.native_docs_notify(cx);
            });
        }));
    }

    /// Cancels a peek that has not opened yet.
    pub(crate) fn native_docs_cancel_peek_open(&mut self) {
        if self.native_docs.transient.is_none() {
            self.native_docs.peek_timer = None;
        }
    }

    /// The pointer moved over the Docs view: a peek stays open while the pointer is over the list
    /// and closes after the grace once it leaves.
    ///
    /// CDXC:Docs 2026-10-09 DECISION:
    /// User: "pls disable hovering on the right side of files making the floating files appear its annoying only hovering on the button should do that". Only hovering (or clicking) the Show files button peeks the floating files list; the Docs view's right edge reveals nothing. This supersedes the 2026-09-12 decisions that the last 10px along that edge revealed the list.
    pub(crate) fn native_docs_pointer_moved(
        &mut self,
        position: Point<Pixels>,
        sidebar_left: Option<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs.transient == Some(DocsTransient::Peek) {
            let inside = sidebar_left.is_some_and(|left| position.x >= left);
            if inside {
                self.native_docs.peek_timer = None;
            } else {
                self.native_docs_leave_peek(cx);
            }
        }
    }

    /// The pointer left a peek: it closes after the grace unless the pointer comes back.
    pub(crate) fn native_docs_leave_peek(&mut self, cx: &mut Context<Self>) {
        if self.native_docs.transient != Some(DocsTransient::Peek)
            || self.native_docs.peek_timer.is_some()
        {
            return;
        }
        let generation = self.native_docs.generation;
        self.native_docs.peek_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PEEK_CLOSE_GRACE).await;
            let _ = this.update(cx, |this, cx| {
                this.native_docs.peek_timer = None;
                if this.native_docs.generation == generation
                    && this.native_docs.transient == Some(DocsTransient::Peek)
                {
                    this.native_docs_close_transient(cx);
                }
            });
        }));
    }

    /// The Pin button on a wide peek: dock it. On a narrow peek the list stays a drawer.
    pub(crate) fn native_docs_pin_peek(&mut self, cx: &mut Context<Self>) {
        if self.native_docs_sidebar_layout().narrow {
            self.native_docs.transient = Some(DocsTransient::Drawer);
            self.native_docs_notify(cx);
        } else {
            self.native_docs.transient = None;
            self.native_docs.slide = None;
            self.native_docs.sidebar_pinned = true;
            self.native_docs_persist_sidebar_pinned(cx);
            self.native_docs_notify(cx);
        }
    }
}
