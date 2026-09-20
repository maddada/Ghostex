//! The header's three panel toggles: hide sidebar on the leading side, the command terminal and the
//! view panel on the trailing side.

use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

pub(crate) fn header_panel_toggle_button(
    id: &'static str,
    icon: &'static str,
    size_reduction: f32,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let button = div()
        .id(id)
        .relative()
        .flex()
        .flex_shrink_0()
        .h(px(TITLEBAR_CONTROL_HEIGHT - size_reduction))
        .items_center()
        .justify_center()
        .rounded(px(TITLEBAR_BUTTON_RADIUS))
        .cursor_default()
        .when(enabled, |this| {
            this.hover(|this| this.bg(titlebar_button_hover_color()))
        });
    #[cfg(target_os = "macos")]
    let button = button.px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING)).child(
        div()
            .flex()
            .ml(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_LEFT_OFFSET))
            .mt(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_TOP_OFFSET))
            .items_center()
            .justify_center()
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE - size_reduction,
                if enabled {
                    titlebar_active_text_color()
                } else {
                    titlebar_disabled_text_color()
                },
            )),
    );
    #[cfg(not(target_os = "macos"))]
    let button = button
        .w(px(TITLEBAR_BUTTON_WIDTH - size_reduction))
        .border_r_1()
        .border_color(titlebar_button_border_color())
        .child(titlebar_svg_icon(
            icon,
            TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE - size_reduction,
            if enabled {
                titlebar_icon_color()
            } else {
                titlebar_disabled_text_color()
            },
        ));
    button
}

impl GhostexGpuiApp {
    pub(crate) fn render_sidebar_collapse_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:Titlebar 2026-06-22-19:39:
        The visible sidebar toggle should match the macOS React titlebar's current flat layout-sidebar icon. Do not render the old blue circular chevron visual.

        CDXC:Sidebar 2026-06-26-10:04:
        The GPUI header sidebar button toggles the same in-shell collapsed chrome state as Cmd+B and the shared command-palette action. Collapse hides the sidebar and divider siblings without writing sidebarWidth, so the user's expanded width is restored on the next toggle.

        Windows and Linux do not have traffic lights to clear. Their collapse
        control uses the same full-height 42px segmented frame as the other
        toggles, mirrored with a trailing divider.
        */
        header_panel_toggle_button(
            "ghostex-gpui-sidebar-collapse",
            TITLEBAR_ICON_LAYOUT_SIDEBAR,
            0.0,
            true,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.toggle_gpui_sidebar_collapsed(cx);
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, {
            let tooltip = titlebar_tooltip_label("Hide sidebar", "toggleSidebarCollapsed");
            move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx)
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
        let tooltip = if expanded {
            titlebar_tooltip_label("Hide command terminal", "openCommandsPanel")
        } else {
            titlebar_tooltip_label("Show command terminal", "openCommandsPanel")
        };
        header_panel_toggle_button(
            "ghostex-gpui-workarea-header-command-terminal-toggle",
            TITLEBAR_ICON_LAYOUT_SPLIT_VERTICAL,
            0.0,
            true,
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
        let tooltip = if open {
            titlebar_tooltip_label("Close the view panel", "toggleViewPanel")
        } else {
            titlebar_tooltip_label("Open the view panel", "toggleViewPanel")
        };
        header_panel_toggle_button(
            "ghostex-gpui-workarea-header-view-panel-toggle",
            TITLEBAR_ICON_LAYOUT_COLUMNS,
            0.0,
            true,
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
