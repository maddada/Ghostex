//! Claude's page in the sessions column's place while Claude is on (claude_web/takeover.rs): the
//! whole work area with the side panel closed, the left column beside it when it is open.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::px;
use gpui::relative;
use gpui_component::v_flex;

use super::view::{claude_web_enabled, claude_web_view_id};
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::render::agents_workspace_layout::AgentsWorkspaceLayout;
use crate::app::render::resize_rail::*;
use crate::*;

impl GhostexGpuiApp {
    /// The work area's main column: the sessions, or Claude's page while Claude is on.
    pub(crate) fn render_workarea_primary_column(
        &mut self,
        layout: AgentsWorkspaceLayout,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if self.claude_on() {
            if claude_web_enabled()
                && self.titlebar_mode_available(TitlebarMode::Extension(claude_web_view_id()))
            {
                return self.render_claude_column(layout, window, cx);
            }
            // Claude was switched off in Settings: the layout it replaced comes back.
            self.turn_claude_off(cx);
        }
        self.render_agents_workspace(layout, window, cx)
    }

    fn render_claude_column(
        &self,
        layout: AgentsWorkspaceLayout,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = claude_web_view_id();
        let mode = TitlebarMode::Extension(id);
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Extension(id);
        let page = if !self.project_editor_shell.is_mode_awake(mode) {
            // After a relaunch the page sleeps until clicked, like every web view
            // (CDXC:Browser 2026-09-19).
            self.render_project_editor_sleeping_placeholder(mode, cx)
        } else if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key)
        {
            self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx)
        } else {
            let signature = self
                .web_runtime_placeholder_signature(mode)
                .unwrap_or_else(|| self.extension_view_placeholder_signature(id));
            self.render_project_editor_placeholder(signature, cx)
        };
        let outer_rail_edges = self.main_workspace_outer_rail_edges(window);
        let rail_edges = match layout {
            // The split divider is the column's right-hand rail.
            AgentsWorkspaceLayout::Column { .. } => RailFacingEdges {
                right: true,
                ..outer_rail_edges
            },
            _ => outer_rail_edges,
        };
        // The page is a native child view that paints over GPUI, so it starts below the floating
        // header like any column that is not the GPUI chat (render/workarea_header/overlap.rs).
        let column = v_flex()
            .id("ghostex-gpui-claude-column")
            .pt(px(self.workarea_header_column_top_inset(false)))
            .min_h_0()
            .overflow_hidden()
            .bg(workspace_nested_background());
        let column = match layout {
            AgentsWorkspaceLayout::FullWidth | AgentsWorkspaceLayout::Floating => {
                column.flex_1().min_w_0()
            }
            AgentsWorkspaceLayout::Column { split_ratio } => column
                .flex_grow(split_ratio)
                .flex_shrink_1()
                .flex_basis(relative(0.0))
                .h_full()
                .min_w(px(WORKAREA_AGENTS_COLUMN_MIN_WIDTH)),
        };
        let pane = v_flex()
            .flex_1()
            .w_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            // The account bar is a sibling row above the page, never over it.
            .child(self.render_claude_web_account_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .child(page),
            );
        column
            .child(rail_aware_pane_border(
                pane,
                rail_edges,
                workspace_pane_border_color_for_state(
                    self.project_editor_surface_border_state(mode, window),
                ),
                workspace_pane_border_color(),
                None,
            ))
            .into_any_element()
    }
}
