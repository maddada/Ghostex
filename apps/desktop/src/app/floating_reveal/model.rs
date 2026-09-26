//! What floats, how wide it is, and the state the reveal keeps between polls.

use std::time::Instant;

use crate::app::consts::*;
use crate::*;

/// The width of the left-edge strip that arms the reveal, and the only width the workarea gives up
/// for it: the strip is a real sibling frame in the body row, not a layer over the view.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// Ten pixels because the user picked that number on 2026-09-09 ("narrow the reveal region from
/// 30px to 10px at the sidebar edge to avoid triggering it too easily") and this is the same
/// gesture with the same job. Screen 10 draws the strip six pixels wide, but it draws it tinted so
/// a reader can see it at all, which is a drawing aid rather than a measurement; a real user
/// decision about this exact width outranks it, and keeping the number means the gesture feels on
/// macOS exactly as it does today.
pub(crate) const FLOATING_REVEAL_EDGE_WIDTH: f32 = 10.0;

/// The visual line between the floating sidebar and the floating sessions column. It is painted
/// chrome with no id and no listener, so it has no hitbox: the floating panel is not a place to
/// resize the split.
pub(crate) const FLOATING_REVEAL_RAIL_WIDTH: f32 = 2.0;

/// CDXC:Sidebar 2026-09-21 DECISION:
/// User: the floating sessions column always has a set width. It is this constant, clamped to the
/// window, rather than the docked split's share of the window, which grew past 1000px on a wide
/// display.
pub(crate) const FLOATING_REVEAL_AGENTS_COLUMN_WIDTH: f32 = 520.0;

/// The corner radius of the floating panels: the sessions panel here and the Docs view's floating
/// files list (`native/macos/GpuiDocsDrawer.m` cuts the corners, see its CDXC:Sidebar decision).
pub(crate) const FLOATING_PANEL_CORNER_RADIUS: f32 = 12.0;

/// How long a reveal asked for by name (Reveal Active Session) waits for the pointer.
pub(crate) const FLOATING_REVEAL_REQUEST_GRACE_SECS: u64 = 5;

/// How long the panel takes to slide in, and to slide away again.
///
/// CDXC:Sidebar 2026-09-23 DECISION:
/// User: "please also make the animation when the sidebar/chat is hidden and i hover over the left side better", after asking that every panel animation respect the system "reduce animations" setting and follow the Panel animations setting ("none slow normal fast"). The floating panel now slides on the Panel animations duration and curve the docked panels use (`panelAnimationSpeed`, CSS `ease-out`), and macOS Reduce Motion snaps it. This supersedes the 2026-09-22 rule that the slide followed the Collapse animation speed setting and ignored Reduce Motion.
pub(crate) fn floating_reveal_slide_duration(reduce_motion: bool) -> std::time::Duration {
    if reduce_motion {
        return std::time::Duration::ZERO;
    }
    crate::app::panel_motion::panel_motion_duration()
}

/// The reveal's sweep while nothing is on screen: often enough to feel immediate on the edge,
/// rare enough to cost nothing.
pub(crate) const SIDEBAR_HOVER_REVEAL_IDLE_POLL: std::time::Duration =
    std::time::Duration::from_millis(60);

/// The sweep while a panel is out. AppKit runs the slide on its own 120Hz timer and only needs the
/// same 60ms; Windows and Linux step the slide from the sweep itself, so it runs at frame rate.
pub(crate) const SIDEBAR_HOVER_REVEAL_ACTIVE_POLL: std::time::Duration =
    if cfg!(target_os = "macos") {
        std::time::Duration::from_millis(60)
    } else {
        std::time::Duration::from_millis(16)
    };

/// Which panels the floating window carries this time.
///
/// CDXC:Sidebar 2026-09-21 DECISION:
/// User (screen 10, revised 2026-09-21): hovering the left edge floats back what is folded away
/// and it slides away when the pointer leaves. With the sidebar and the sessions column both
/// folded, the top half of the edge floats the sidebar and the bottom half floats the sessions
/// column; with only the sidebar folded the whole edge floats it. With the sidebar docked, its
/// own left 10px floats the folded sessions column beside it. This supersedes the 2026-09-20 rule
/// that one panel always carried the sidebar and that the reveal needed a collapsed sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FloatingRevealContent {
    pub(crate) sidebar: bool,
    pub(crate) agents_column: bool,
}

/// The open panel: its window, and what it was opened to show.
pub(crate) struct FloatingRevealPanel {
    pub(crate) window: gpui::WindowHandle<gpui_component::Root>,
    #[cfg(target_os = "macos")]
    pub(crate) native_view: *mut std::ffi::c_void,
    /// The panel's top-left in screen coordinates when it opened. GPUI can resize a window on every
    /// backend but can only move one on AppKit, so the other two close the panel rather than leave
    /// it behind when the main window is dragged somewhere else.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) anchor: gpui::Point<gpui::Pixels>,
    pub(crate) content: FloatingRevealContent,
    pub(crate) width: f32,
}

/// The slide, on the two backends that have to run it themselves.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// AppKit animates the panel by resizing the child window while the content inside stays
/// right-aligned at full width, so the page slides in without reflowing. Windows and Linux have the
/// same shape available (`Window::resize` is the one cross-platform geometry call GPUI exposes) and
/// none of the alternatives: a full-width window sliding its content would leave a transparent
/// interactive region over the workarea, which is exactly the overlay the layout rules forbid.
#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FloatingRevealSlide {
    pub(crate) progress: f32,
    pub(crate) target: f32,
    pub(crate) from: f32,
    pub(crate) started: Instant,
}

#[cfg(not(target_os = "macos"))]
impl Default for FloatingRevealSlide {
    fn default() -> Self {
        Self {
            progress: 0.0,
            target: 1.0,
            from: 0.0,
            started: Instant::now(),
        }
    }
}

#[derive(Default)]
pub(crate) struct FloatingRevealState {
    pub(crate) panel: Option<FloatingRevealPanel>,
    /// The left-edge strip has the pointer. Set by the strip's own mouse-move, cleared when the
    /// pointer leaves it and whenever the panel closes, so a panel the pointer walked away from
    /// cannot re-open under a stale hover.
    pub(crate) edge_hovered: bool,
    /// A reveal asked for by name holds the panel open until the pointer visits it or this passes.
    pub(crate) requested_until: Option<Instant>,
    /// What the gesture that armed or requested the reveal asked for. Cleared when the panel
    /// closes.
    pub(crate) want: FloatingRevealContent,
    /// The docked sidebar has the pointer, which keeps a panel open beside it.
    #[cfg(not(target_os = "macos"))]
    pub(crate) sidebar_hovered: bool,
    /// When the pointer left the panel, for the dismissal delay.
    #[cfg(not(target_os = "macos"))]
    pub(crate) outside_since: Option<Instant>,
    #[cfg(not(target_os = "macos"))]
    pub(crate) slide: FloatingRevealSlide,
}

impl GhostexGpuiApp {
    /// The reveal exists while something is folded away: the collapsed sidebar, or the sessions
    /// column an expanded view has taken the workarea from.
    pub(crate) fn floating_reveal_eligible(&self) -> bool {
        let content = self.floating_reveal_available();
        content.sidebar || content.agents_column
    }

    /// CDXC:Sidebar 2026-09-21 DECISION:
    /// User: with the sidebar shown, the floating sessions pane always opens to the right of the
    /// sidebar, whether a hover on the sidebar's left edge or a session click asked for it. This
    /// supersedes the same-day arrangement that floated it over the sidebar for the hover.
    pub(crate) fn floating_reveal_left_inset(&self) -> f32 {
        if self.sidebar_collapsed {
            0.0
        } else {
            self.sidebar_width + SIDEBAR_DIVIDER_WIDTH
        }
    }

    /// The strip is the collapsed sidebar's place in the body row. Docked, the sidebar's own left
    /// 10px is the trigger instead (`handle_floating_reveal_sidebar_edge_move`).
    pub(crate) fn floating_reveal_edge_strip_visible(&self) -> bool {
        self.sidebar_collapsed
    }

    /// The sidebar's left 10px arms the reveal of a folded sessions column while the sidebar is
    /// docked. The sidebar reads its own pointer position; nothing is layered over its rows.
    pub(crate) fn handle_floating_reveal_sidebar_edge_move(&mut self, x: f32) {
        let armed = !self.sidebar_collapsed
            && self.floating_reveal_available().agents_column
            && x < FLOATING_REVEAL_EDGE_WIDTH;
        if armed && self.floating_reveal.panel.is_none() {
            self.floating_reveal.want = FloatingRevealContent {
                sidebar: false,
                agents_column: true,
            };
        }
        self.floating_reveal.edge_hovered = armed;
    }

    /// The edge strip has the pointer, in the half that asks for `want`.
    pub(crate) fn arm_floating_reveal_from_strip(&mut self, want: FloatingRevealContent) {
        if self.floating_reveal.panel.is_none() {
            self.floating_reveal.want = want;
        }
        self.floating_reveal.edge_hovered = true;
    }

    /// The pointer left a trigger without opening anything, so nothing is asked for any more.
    pub(crate) fn disarm_floating_reveal_edge(&mut self) {
        self.floating_reveal.edge_hovered = false;
        if self.floating_reveal.panel.is_none() && self.floating_reveal.requested_until.is_none() {
            self.floating_reveal.want = FloatingRevealContent::default();
        }
    }

    /// CDXC:Sidebar 2026-09-21 DECISION:
    /// User: clicking a session row in the sidebar, and creating a new agent, open the floating
    /// sessions pane while an expanded view has folded the sessions column away, with the sidebar
    /// collapsed or shown, and the pane takes the keyboard so typing lands in the terminal or chat
    /// right away. It is a reveal asked for by name, so it waits the usual grace for the pointer
    /// and slides away if the pointer never comes. A floating sidebar that is already out stays.
    pub(crate) fn reveal_floating_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.floating_reveal_available().agents_column {
            return;
        }
        let keeps_sidebar = self
            .floating_reveal
            .panel
            .as_ref()
            .is_some_and(|panel| panel.content.sidebar);
        self.floating_reveal.want = FloatingRevealContent {
            sidebar: keeps_sidebar,
            agents_column: true,
        };
        self.update_floating_reveal(true, false, cx);
        self.schedule_floating_reveal_focus(cx);
    }

    /// What is folded away and could float back.
    pub(crate) fn floating_reveal_available(&self) -> FloatingRevealContent {
        FloatingRevealContent {
            sidebar: self.sidebar_collapsed,
            agents_column: self.view_panel_maximized(),
        }
    }

    /// What the panel carries: what the gesture asked for, of what is folded away.
    pub(crate) fn floating_reveal_content(&self) -> FloatingRevealContent {
        let available = self.floating_reveal_available();
        let want = self.floating_reveal.want;
        FloatingRevealContent {
            sidebar: want.sidebar && available.sidebar,
            agents_column: want.agents_column && available.agents_column,
        }
    }

    pub(crate) fn floating_reveal_width_for(&self, content: FloatingRevealContent) -> f32 {
        let span = (self.main_window_bounds.size.width.as_f32()
            - self.floating_reveal_left_inset())
        .max(1.0);
        let sidebar = if content.sidebar {
            self.sidebar_width
        } else {
            0.0
        };
        let rail = if content.sidebar && content.agents_column {
            FLOATING_REVEAL_RAIL_WIDTH
        } else {
            0.0
        };
        let column = if content.agents_column {
            FLOATING_REVEAL_AGENTS_COLUMN_WIDTH
        } else {
            0.0
        };
        (sidebar + rail + column).clamp(1.0, span)
    }

    /// True while the open panel is the only place the Agents workspace is rendered. Every terminal,
    /// chat and focus gate reads `agents_workspace_visible()`, which folds this in, so a session
    /// whose pane moved into the panel counts as on screen rather than as hidden.
    pub(crate) fn floating_reveal_hosts_agents_column(&self) -> bool {
        self.floating_reveal
            .panel
            .as_ref()
            .is_some_and(|panel| panel.content.agents_column)
    }
}
