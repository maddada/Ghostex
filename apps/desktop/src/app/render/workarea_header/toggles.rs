//! The header's three panel toggles: hide sidebar on the leading side, the command terminal and the
//! view panel on the trailing side.

use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use std::sync::atomic::{AtomicU32, Ordering};

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::*;

/// The panel toggles' width as last laid out, which is how much of the band's trailing edge they
/// claim (`workarea_header_toggles_footprint`).
static PANEL_TOGGLES_WIDTH: AtomicU32 = AtomicU32::new(0);

pub(crate) fn header_panel_toggle_button(
    id: &'static str,
    icon: &'static str,
    size_reduction: f32,
    enabled: bool,
    icon_color: Option<gpui::Hsla>,
    scale: f32,
) -> gpui::Stateful<gpui::Div> {
    // One shape on every OS, so the sidebar's Search row can reserve the same width for its two
    // leading toggles everywhere (native_sidebar/navigation.rs).
    div()
        .id(id)
        .relative()
        .flex()
        .flex_shrink_0()
        .h(px((TITLEBAR_CONTROL_HEIGHT - size_reduction) * scale))
        .items_center()
        .justify_center()
        .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING * scale))
        .rounded(px(TITLEBAR_BUTTON_RADIUS * scale))
        .cursor_default()
        // Windows: occlude the ancestor Drag hitbox so WM_NCHITTEST keeps the button in the client
        // area (see the CDXC:PlatformSupport note in action_buttons.rs).
        .when(cfg!(target_os = "windows"), |this| this.occlude())
        .when(enabled, |this| {
            this.hover(|this| this.bg(titlebar_button_hover_color()))
        })
        .child(
            div()
                .flex()
                .ml(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_LEFT_OFFSET * scale))
                .mt(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_TOP_OFFSET * scale))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon,
                    (TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE - size_reduction) * scale,
                    if enabled {
                        icon_color.unwrap_or_else(titlebar_active_text_color)
                    } else {
                        titlebar_disabled_text_color()
                    },
                )),
        )
}

/// Toggle sidebar and Toggle Agents Panel show in the header and, with an icon colour, in the
/// sidebar's top row, where their tooltip is the sidebar's frosted bubble like every other sidebar
/// tooltip (CDXC:Tooltips 2026-10-25 DECISION in native_docs/project_search_view.rs).
fn panel_toggle_tooltip(
    label: &gpui::SharedString,
    in_sidebar: bool,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    if in_sidebar {
        crate::app::native_sidebar::link_chips::sidebar_style_tooltip(label.to_string(), window, cx)
    } else {
        titlebar_tooltip(label.clone(), window, cx)
    }
}

impl GhostexGpuiApp {
    /// CDXC:Titlebar 2026-09-21 DECISION:
    /// User: the command terminal and view panel toggles never move from the top right of the app
    /// when the view panel opens or closes; with the panel open they sit right of its Expand
    /// button. So the pair is the last thing in whichever half of the band reaches the window's
    /// trailing edge: the header while the panel is closed, the view tab strip while it is open.
    pub(crate) fn render_workarea_panel_toggles(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center()
            .gap(px(2.0))
            .child(
                // Visual-only separator: a plain div with no id and no interactivity, so it
                // registers no hitbox and the band's drag area keeps the gap.
                div()
                    .w(px(1.0))
                    .h(px(16.0))
                    .mx(px(3.0))
                    .bg(titlebar_button_border_color()),
            )
            .child(self.render_workarea_header_command_terminal_toggle(cx))
            .child(self.render_workarea_header_view_panel_toggle(cx))
            .on_prepaint(|bounds, _window, _cx| {
                PANEL_TOGGLES_WIDTH.store(bounds.size.width.as_f32().to_bits(), Ordering::Relaxed);
            })
    }

    /// How far the collapsed header's other controls end from the band's trailing edge beyond
    /// where they end with the panel open: the toggles and the pinned gap before them.
    fn workarea_header_toggles_footprint(&self) -> f32 {
        f32::from_bits(PANEL_TOGGLES_WIDTH.load(Ordering::Relaxed)) + WORKAREA_HEADER_PINNED_GAP
    }

    /// CDXC:Titlebar 2026-09-24 DECISION:
    /// User: Start/Open/Commit must not jump to the right side when the side panel collapses. While the view panel slides, the header's controls end as far in from the band's trailing edge as the larger of the panel on screen (with its divider) and the toggles' footprint, which is exactly where they end at rest at either end, so they glide between the two instead of jumping when the toggles change halves of the band. Opening, the toggles are already in the strip and the header row ends at the panel, so this is extra trailing padding for the header's controls.
    pub(crate) fn workarea_header_opening_clearance(&self) -> f32 {
        let frame = self.panel_motion.view_panel.frame();
        if !frame.animating || !frame.opening || !self.workarea_header_hosts_view_tab_strip() {
            return 0.0;
        }
        (self.workarea_header_toggles_footprint() - frame.extent - WORKSPACE_SPLIT_HANDLE_THICKNESS)
            .max(0.0)
    }

    /// Closing, the header row already spans the band and ends in the toggles, so the rest of its
    /// controls are held off them by what is still on screen of the panel
    /// (`workarea_header_opening_clearance`).
    pub(crate) fn workarea_header_closing_clearance(&self) -> f32 {
        let frame = self.panel_motion.view_panel.frame();
        if !frame.animating || frame.opening || self.workarea_header_hosts_view_tab_strip() {
            return 0.0;
        }
        (frame.extent + WORKSPACE_SPLIT_HANDLE_THICKNESS - self.workarea_header_toggles_footprint())
            .max(0.0)
    }

    /// `in_sidebar` is the sidebar's look while its top row hosts the button; the header passes
    /// `None`.
    pub(crate) fn render_sidebar_collapse_button(
        &self,
        in_sidebar: Option<&SidebarAppearance>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:Titlebar 2026-06-22-19:39:
        The visible sidebar toggle should match the macOS React titlebar's current flat layout-sidebar icon. Do not render the old blue circular chevron visual.

        CDXC:Sidebar 2026-06-26-10:04:
        The GPUI header sidebar button toggles the same in-shell collapsed chrome state as Cmd+B and the shared command-palette action. Collapse hides the sidebar and divider siblings without writing sidebarWidth, so the user's expanded width is restored on the next toggle.
        */
        header_panel_toggle_button(
            "ghostex-gpui-sidebar-collapse",
            TITLEBAR_ICON_LAYOUT_SIDEBAR,
            0.0,
            true,
            in_sidebar.map(|appearance| appearance.chrome_icon),
            in_sidebar.map_or(1.0, |appearance| appearance.scale),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                // With Claude on, showing the sidebar leaves Claude (app/claude_web/takeover.rs).
                this.toggle_sidebar_from_user(cx);
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, {
            let tooltip = titlebar_tooltip_label("Toggle sidebar", "toggleSidebarCollapsed");
            let in_sidebar = in_sidebar.is_some();
            move |window, cx| panel_toggle_tooltip(&tooltip, in_sidebar, window, cx)
        })
    }

    /// CDXC:Workarea 2026-09-23 DECISION:
    /// User: "a new button with a chat icon to the top left that allows just toggling the agents
    /// area", called the Agents Panel, right of the Toggle sidebar button, and the Expand side
    /// panel and Expand side panel fully buttons stay "in sync with the current state as much as
    /// possible". There is one state for all three, not a fourth flag: `view_panel_maximized` (the
    /// Agents Panel is folded away) and `sidebar_collapsed`. It flips only the panel, so a fully
    /// expanded view keeps its hidden sidebar and Expand fully simply stops reading as on; the
    /// Toggle sidebar button beside it owns the sidebar. User: the button never shows as active
    /// while the panel is shown, the way Toggle sidebar never does (supersedes the 2026-09-22 lit
    /// state). With the side panel closed the Agents Panel is the whole workarea and there is
    /// nothing to fold it behind, so the button is disabled the way Expand is, and says so; the
    /// "Open a view" picker counts as open (see `view_panel_maximized`).
    pub(crate) fn render_workarea_header_agents_toggle(
        &self,
        in_sidebar: Option<&SidebarAppearance>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let enabled = self.view_panel_open();
        let tooltip = if enabled {
            titlebar_tooltip_label("Toggle Agents Panel", "expandViewPanel")
        } else {
            "Open a view to hide the Agents Panel".into()
        };
        header_panel_toggle_button(
            "ghostex-gpui-workarea-header-agents-toggle",
            TITLEBAR_ICON_MESSAGE,
            0.0,
            enabled,
            in_sidebar.map(|appearance| appearance.chrome_icon),
            in_sidebar.map_or(1.0, |appearance| appearance.scale),
        )
        .when(enabled, |this| {
            this.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_view_panel_maximized(cx);
                }),
            )
        })
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, {
            let in_sidebar = in_sidebar.is_some();
            move |window, cx| panel_toggle_tooltip(&tooltip, in_sidebar, window, cx)
        })
    }

    /// CDXC:CommandPane 2026-09-20 DECISION:
    /// User: the header carries a command-terminal toggle beside the view-panel toggle, so the
    /// command pane can be opened and closed without reaching for its collapsed strip.
    pub(crate) fn render_workarea_header_command_terminal_toggle(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let expanded = self.command_pane.is_expanded();
        let tooltip = titlebar_tooltip_label("Toggle bottom panel", "openCommandsPanel");
        header_panel_toggle_button(
            "ghostex-gpui-workarea-header-command-terminal-toggle",
            TITLEBAR_ICON_PANEL_BOTTOM,
            0.0,
            true,
            None,
            1.0,
        )
        .when(expanded, |this| this.bg(titlebar_active_segment_color()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.handle_command_pane_control_action(
                    CommandPaneControlAction::ToggleExpanded,
                    None,
                    window,
                    cx,
                );
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
            titlebar_tooltip(tooltip.clone(), window, cx)
        })
    }

    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User (screen 02): the header's view-panel toggle opens the tab this project last had open,
    /// the picker when it has no tabs, and closes the panel while it is open. It is never disabled,
    /// because the picker is always something to show. This supersedes the earlier rule that it
    /// opened "the first view its context offers" and went dead when there was none.
    pub(crate) fn render_workarea_header_view_panel_toggle(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let open = self.view_panel_open();
        let tooltip = titlebar_tooltip_label("Toggle side panel", "toggleViewPanel");
        header_panel_toggle_button(
            "ghostex-gpui-workarea-header-view-panel-toggle",
            TITLEBAR_ICON_PANEL_RIGHT,
            0.0,
            true,
            None,
            1.0,
        )
        .when(open, |this| this.bg(titlebar_active_segment_color()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.toggle_view_panel(window, cx);
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
            titlebar_tooltip(tooltip.clone(), window, cx)
        })
    }
}
