//! The reveal and hide motion of the window's collapsible panels: the sidebar, the view panel and
//! the command pane, bottom or right. One tween drives all of them so they open and close alike.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};

use crate::GhostexGpuiApp;

/// CDXC:Workarea 2026-09-24 DECISION:
/// User: "add animations for collapsing and expanding the right/left/bottom sidebars and panels", with reveal and hide "always consistently" the same, then: the Agents Panel animates too, the right side panel is "anchored to the right most side of the window", the bottom command pane's terminal must not show "squeezed then expanding" (it stays hidden and fades in near the end of the opening), and "do same thing for side panel please": the view panel's frame also slides open empty and its view (a native view, the view picker, a sleeping card) fades in near the end, while a web page, which cannot fade with GPUI, fades in right after the slide settles. Then, of the view picker's cards: "i want them to fade in / out when expanding/collapsing this and no jumpiness please", so the picker also stays in a closing panel and fades out over the first half of the slide, and opening content fades in over the last half (it was the last 30%). The system "reduce animations" setting is respected, and there are settings for "none slow normal fast". Every panel's width or height tweens on CSS `ease-out` (`cubic-bezier(0, 0, 0.58, 1)`) over the Panel animations duration (Slow 320ms, Normal 200ms, Fast 120ms), opening and closing alike, and a toggle made mid-flight reverses from the size on screen. Off, or macOS Reduce Motion, snaps. After "when we're animating open/close for the sidebar it's flickering because we're hiding then showing the cef right? because if i just move the cef's drag handle there's no issue", a web page the slide only pushes resizes live like a divider drag and is never hidden; only the view panel's own slide hides its page.
///
/// CDXC:Workarea 2026-09-23 WHY:
/// The panel's content keeps the larger of its two sizes for the whole tween and the animating frame clips it, so the panel slides rather than reflowing. Terminals whose bounds move during the tween hold their grid until it settles (`set_grid_resize_held`), and a terminal inside the opening panel (whose bounds never move) resizes once on its second frame. Web pages follow the divider drag instead: a page the slide only pushes (the sidebar, the Agents Panel or the command pane moving beside the view panel) is moved and resized live every frame, because hiding it for the slide and fading it back made the page blink, and dragging the divider already resizes Chromium every frame without trouble, which disproved the per-frame cost worry. Only while the view panel itself slides is its page, which would poke outside the panel's clip, made transparent until the slide settles (`CefBrowser::set_motion_hidden`), because GPUI cannot clip an AppKit child view.
///
/// The duration in milliseconds; 0 is Off. Set from `panelAnimationSpeed` whenever settings load.
static PANEL_MOTION_DURATION_MS: AtomicU16 = AtomicU16::new(200);

/// Reads `panelAnimationSpeed` ("off", "slow", "normal", "fast"); anything else is Normal.
pub(crate) fn refresh_panel_motion_speed(object: &serde_json::Map<String, serde_json::Value>) {
    let millis = match object
        .get("panelAnimationSpeed")
        .and_then(serde_json::Value::as_str)
    {
        Some("off") => 0,
        Some("slow") => 320,
        Some("fast") => 120,
        _ => 200,
    };
    PANEL_MOTION_DURATION_MS.store(millis, Ordering::Relaxed);
}

pub(crate) fn panel_motion_duration() -> Duration {
    Duration::from_millis(u64::from(PANEL_MOTION_DURATION_MS.load(Ordering::Relaxed)))
}

/// CSS `ease-out`.
const PANEL_MOTION_EASING: [f32; 4] = [0.0, 0.0, 0.58, 1.0];

/// `progress` (0 to 1) on the panels' curve, for another surface that slides like them.
pub(crate) fn panel_motion_eased(progress: f32) -> f32 {
    bezier(progress.clamp(0.0, 1.0), PANEL_MOTION_EASING)
}

/// The share of a tween over which the panel's content fades: in at the end of an opening, out at
/// the start of a closing.
const PANEL_CONTENT_FADE_SHARE: f32 = 0.5;

/// How long a web page takes to fade back in once a slide has settled: the same span the panel's
/// GPUI content fades over at the end of an opening, run after it because the page cannot show
/// until it no longer pokes outside the sliding frame.
pub(crate) fn panel_content_fade_duration() -> Duration {
    panel_motion_duration().mul_f32(PANEL_CONTENT_FADE_SHARE)
}

/// Whether the view panel itself is sliding open or shut this frame. Its page is the only one a
/// slide clips; every other slide just pushes pages, which then resize live.
static VIEW_PANEL_SLIDING: AtomicBool = AtomicBool::new(false);

pub(crate) fn view_panel_sliding() -> bool {
    VIEW_PANEL_SLIDING.load(Ordering::Relaxed)
}

/// Whether the view panel is shut and would slide if it opened now: a page shown at this point is
/// the panel opening, and it starts transparent so its last frame never flashes before the slide's
/// first paint takes it over (`CefSurface::set_visible`).
static VIEW_PANEL_WOULD_SLIDE_OPEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn view_panel_would_slide_open() -> bool {
    VIEW_PANEL_WOULD_SLIDE_OPEN.load(Ordering::Relaxed)
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `progress`: solve x(u) = progress, then read y(u).
fn bezier(progress: f32, [x1, y1, x2, y2]: [f32; 4]) -> f32 {
    let sample = |t: f32, a: f32, b: f32| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..20 {
        let mid = (low + high) / 2.0;
        if sample(mid, x1, x2) < progress {
            low = mid;
        } else {
            high = mid;
        }
    }
    sample((low + high) / 2.0, y1, y2)
}

#[derive(Clone, Copy, Debug)]
struct PanelTween {
    from: f32,
    started: web_time::Instant,
    duration: Duration,
}

/// One frame of a panel: how much of it is on screen, and how big its content is laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PanelFrame {
    /// The panel's width or height on screen this frame.
    pub(crate) extent: f32,
    /// The size the content is laid out at: the larger end of the tween, so it never reflows.
    pub(crate) content_extent: f32,
    pub(crate) animating: bool,
    /// Whether the running tween is an opening one.
    pub(crate) opening: bool,
    /// How far through its duration the running tween is, 0 to 1.
    pub(crate) progress: f32,
}

impl PanelFrame {
    fn settled(extent: f32) -> Self {
        Self {
            extent,
            content_extent: extent,
            animating: false,
            opening: false,
            progress: 1.0,
        }
    }

    /// The opacity of an opening panel's content: hidden for the first half of the slide and faded
    /// in over the rest, so it is never seen being revealed at a size it is about to leave.
    pub(crate) fn opening_content_opacity(self) -> f32 {
        if !self.animating || !self.opening {
            return 1.0;
        }
        ((self.progress - (1.0 - PANEL_CONTENT_FADE_SHARE)) / PANEL_CONTENT_FADE_SHARE)
            .clamp(0.0, 1.0)
    }

    /// The opacity of a closing panel's content that is still drawn (the view picker): faded out
    /// over the first half of the slide, so the frame finishes shutting empty.
    pub(crate) fn closing_content_opacity(self) -> f32 {
        if !self.animating || self.opening {
            return 1.0;
        }
        (1.0 - self.progress / PANEL_CONTENT_FADE_SHARE).clamp(0.0, 1.0)
    }
}

/// The motion of one panel. `sample` is called once per frame with the panel's state; a change of
/// open state starts a tween from whatever is on screen.
#[derive(Debug, Default)]
pub(crate) struct PanelMotion {
    open: Option<bool>,
    /// The panel's size when it was last open, so a close tweens from it.
    open_extent: f32,
    /// The size it collapses to (0 for most panels; the command strip's height for the bottom dock).
    closed_extent: f32,
    tween: Option<PanelTween>,
    frame: PanelFrame,
}

impl PanelMotion {
    pub(crate) fn frame(&self) -> PanelFrame {
        self.frame
    }

    /// Drops any running tween, for a layout change the panel should not animate (a dock moving to
    /// the other side, a maximised view).
    pub(crate) fn reset(&mut self) {
        self.tween = None;
        self.open = None;
        self.frame = PanelFrame::default();
    }

    pub(crate) fn sample(
        &mut self,
        open: bool,
        open_extent: f32,
        closed_extent: f32,
        reduce_motion: bool,
        now: web_time::Instant,
    ) -> PanelFrame {
        if open {
            self.open_extent = open_extent;
        }
        self.closed_extent = closed_extent;
        let target = if open {
            self.open_extent
        } else {
            self.closed_extent
        };
        match self.open {
            Some(previous) if previous != open && !reduce_motion => {
                let from = if self.frame.animating || self.frame.extent > 0.0 {
                    self.frame.extent
                } else if previous {
                    self.open_extent
                } else {
                    self.closed_extent
                };
                let duration = panel_motion_duration();
                if !duration.is_zero() {
                    self.tween = Some(PanelTween {
                        from,
                        started: now,
                        duration,
                    });
                } else {
                    self.tween = None;
                }
            }
            _ => {}
        }
        self.open = Some(open);
        if reduce_motion {
            self.tween = None;
        }
        let frame = match self.tween {
            Some(tween) => {
                let progress = now.saturating_duration_since(tween.started).as_secs_f32()
                    / tween.duration.as_secs_f32();
                if progress >= 1.0 {
                    self.tween = None;
                    PanelFrame::settled(target)
                } else {
                    let eased = bezier(progress.clamp(0.0, 1.0), PANEL_MOTION_EASING);
                    PanelFrame {
                        extent: tween.from + (target - tween.from) * eased,
                        content_extent: tween.from.max(target),
                        animating: true,
                        opening: open,
                        progress: progress.clamp(0.0, 1.0),
                    }
                }
            }
            None => PanelFrame::settled(target),
        };
        self.frame = frame;
        frame
    }
}

/// The three panels' motion, sampled once at the top of each root render so the body and the
/// header band read the same frame.
#[derive(Debug, Default)]
pub(crate) struct PanelMotions {
    pub(crate) sidebar: PanelMotion,
    pub(crate) view_panel: PanelMotion,
    /// The Agents Panel (the sessions column beside an open view), folded away by the Agents Panel
    /// toggle and Expand.
    pub(crate) agents_column: PanelMotion,
    pub(crate) command_pane: PanelMotion,
    /// The command pane's dock side the last frame, so a dock move resets instead of animating.
    command_pane_side: Option<crate::app::model::GpuiCommandPaneSide>,
    /// The project on screen the last frame. The view panel and the command pane belong to the
    /// project, so switching projects shows the new one's panels as they are rather than sliding.
    project_id: Option<String>,
    /// Whether the view panel showed the picker the last frame it was open. The picker is GPUI's
    /// own drawing, so a closing slide keeps it in the frame to fade it out; a view's native page
    /// is gone with the view.
    pub(crate) view_panel_showed_picker: bool,
    was_animating: bool,
}

impl GhostexGpuiApp {
    /// Samples every panel's tween for this frame and keeps frames coming while one runs. Called
    /// at the top of the root render, before anything reads a panel's size.
    pub(crate) fn sample_panel_motion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = web_time::Instant::now();
        let reduce_motion = cx.reduce_motion();

        if self.panel_motion.project_id != self.agents_workspace_project_id {
            self.panel_motion
                .project_id
                .clone_from(&self.agents_workspace_project_id);
            self.panel_motion.view_panel.reset();
            self.panel_motion.agents_column.reset();
            self.panel_motion.command_pane.reset();
        }

        let sidebar_open = crate::app::model::gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        let sidebar_extent = self.sidebar_width + crate::app::consts::SIDEBAR_DIVIDER_WIDTH;
        self.panel_motion
            .sidebar
            .sample(sidebar_open, sidebar_extent, 0.0, reduce_motion, now);

        if self.view_panel_maximized() {
            self.panel_motion.view_panel.reset();
        } else {
            let open = self.view_panel_open();
            if open {
                self.panel_motion.view_panel_showed_picker = self.view_picker_open();
            }
            let extent = if open {
                self.view_panel_open_width(window)
            } else {
                0.0
            };
            self.panel_motion
                .view_panel
                .sample(open, extent, 0.0, reduce_motion, now);
        }

        // The Agents Panel folds only beside an open panel (a view or the picker); without one it is
        // the whole workarea.
        if self.view_panel_open() {
            let open = !self.view_panel_maximized();
            let extent = self.workarea_header_row_width_at_rest(window);
            self.panel_motion
                .agents_column
                .sample(open, extent, 0.0, reduce_motion, now);
        } else {
            self.panel_motion.agents_column.reset();
        }

        let (side, open, extent, closed) = self.command_pane_motion_state(window);
        if self.panel_motion.command_pane_side != side {
            self.panel_motion.command_pane.reset();
            self.panel_motion.command_pane_side = side;
        }
        if side.is_some() {
            self.panel_motion
                .command_pane
                .sample(open, extent, closed, reduce_motion, now);
        }

        let animating = self.panel_motion.sidebar.frame().animating
            || self.panel_motion.view_panel.frame().animating
            || self.panel_motion.agents_column.frame().animating
            || self.panel_motion.command_pane.frame().animating;
        VIEW_PANEL_SLIDING.store(
            self.panel_motion.view_panel.frame().animating,
            Ordering::Relaxed,
        );
        VIEW_PANEL_WOULD_SLIDE_OPEN.store(
            !self.view_panel_open() && !reduce_motion && !panel_motion_duration().is_zero(),
            Ordering::Relaxed,
        );
        crate::terminal_element::set_grid_resize_held(animating);
        if animating {
            window.request_animation_frame();
        } else if self.panel_motion.was_animating {
            // Views whose bounds did not change on the settling frame are reused from the cache and
            // would keep a held terminal grid or a transparent page; one full refresh lets every
            // one of them take its final size.
            cx.defer_in(window, |_, window, _| window.refresh());
        }
        self.panel_motion.was_animating = animating;
    }
}

impl GhostexGpuiApp {
    /// The view panel's width while it is open: the band's width less the header row's share of the
    /// split and the rail between them, the same numbers the band and the body lay out from.
    fn view_panel_open_width(&self, window: &Window) -> f32 {
        let band_width = (crate::app::model::command_pane_workspace_width(
            window,
            self.sidebar_width,
            self.sidebar_collapsed,
        ) - self.workarea_header_trailing_dock_reserve_at_rest(window))
        .max(0.0);
        (band_width
            - crate::app::consts::WORKSPACE_SPLIT_HANDLE_THICKNESS
            - self.workarea_header_row_width_at_rest(window))
        .max(0.0)
    }

    /// The command pane's dock side (none while it floats, which does not animate), whether it is
    /// open, its size open (its rail included), and the size it collapses to.
    fn command_pane_motion_state(
        &self,
        window: &Window,
    ) -> (
        Option<crate::app::model::GpuiCommandPaneSide>,
        bool,
        f32,
        f32,
    ) {
        use crate::app::consts::{
            COMMAND_PANE_SPLIT_HANDLE_THICKNESS, WORKSPACE_SPLIT_HANDLE_THICKNESS,
        };
        use crate::app::model::{CommandPaneWorkspaceLayoutPlan as Plan, GpuiCommandPaneSide};
        let side = self.command_pane_side;
        let (open, extent, closed) = match self.command_pane_layout_plan(window) {
            Plan::Floating { .. } => return (None, false, 0.0, 0.0),
            Plan::Pinned { panel_height } => {
                (true, panel_height + WORKSPACE_SPLIT_HANDLE_THICKNESS, 0.0)
            }
            Plan::PinnedRight { panel_width } => {
                (true, panel_width + COMMAND_PANE_SPLIT_HANDLE_THICKNESS, 0.0)
            }
            Plan::Collapsed { bottom_reservation } => (
                false,
                0.0,
                if side == GpuiCommandPaneSide::Bottom {
                    bottom_reservation.height
                } else {
                    0.0
                },
            ),
            Plan::Hidden => (false, 0.0, 0.0),
        };
        (Some(side), open, extent, closed)
    }

    /// This frame's command pane layout, from the same inputs its renderer and the header band use.
    pub(crate) fn command_pane_layout_plan(
        &self,
        window: &Window,
    ) -> crate::app::model::CommandPaneWorkspaceLayoutPlan {
        let workspace_width = crate::app::model::command_pane_workspace_width(
            window,
            self.sidebar_width,
            self.sidebar_collapsed,
        );
        crate::app::model::command_pane_workspace_layout_plan(
            self.command_pane.mode,
            self.command_pane.has_panel_sessions(),
            crate::app::model::command_pane_content_height(window),
            self.command_pane.height_ratio,
            self.command_pane_side,
            workspace_width,
            self.command_pane.width_ratio,
        )
    }
}

/// Clips `content`, laid out at the frame's content size, to the frame's size along the x axis.
/// `anchor_trailing` keeps the content's right edge in place (a panel on the right).
pub(crate) fn clip_panel_horizontally(
    frame: PanelFrame,
    anchor_trailing: bool,
    content: AnyElement,
) -> gpui::Div {
    let inner = div()
        .absolute()
        .top_0()
        .h_full()
        .w(px(frame.content_extent))
        .flex()
        .child(content);
    div()
        .relative()
        .flex_none()
        .h_full()
        .w(px(frame.extent.max(0.0)))
        .overflow_hidden()
        .child(if anchor_trailing {
            inner.right_0()
        } else {
            inner.left_0()
        })
}

/// Clips `content`, laid out at the frame's content size, to the frame's size along the y axis,
/// its top edge in place.
pub(crate) fn clip_panel_vertically(frame: PanelFrame, content: AnyElement) -> gpui::Div {
    div()
        .relative()
        .flex_none()
        .w_full()
        .h(px(frame.extent.max(0.0)))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(px(frame.content_extent))
                .flex()
                .flex_col()
                .child(content),
        )
}

/// A closing panel whose content is already gone: its frame, in the colour it was, sliding shut.
/// `rail_last` puts its rail on the far side (a panel on the left, whose rail faces right).
pub(crate) fn closing_panel_ghost(
    frame: PanelFrame,
    vertical: bool,
    rail_last: bool,
    rail: gpui::Hsla,
    rail_thickness: f32,
    fill: gpui::Hsla,
) -> AnyElement {
    let extent = frame.extent.max(0.0);
    if vertical {
        gpui_component::v_flex()
            .flex_none()
            .w_full()
            .h(px(extent))
            .overflow_hidden()
            .child(div().flex_none().w_full().h(px(rail_thickness)).bg(rail))
            .child(div().flex_1().w_full().bg(fill))
            .into_any_element()
    } else {
        let rail = div().flex_none().h_full().w(px(rail_thickness)).bg(rail);
        let body = div().flex_1().h_full().bg(fill);
        gpui_component::h_flex()
            .flex_none()
            .h_full()
            .w(px(extent))
            .overflow_hidden()
            .items_stretch()
            .map(|this| {
                if rail_last {
                    this.child(body).child(rail)
                } else {
                    this.child(rail).child(body)
                }
            })
            .into_any_element()
    }
}
