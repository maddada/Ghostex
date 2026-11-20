//! The left-edge hot zone that arms the reveal.

#[cfg(not(target_os = "macos"))]
use gpui::InteractiveElement as _;
#[cfg(not(target_os = "macos"))]
use gpui::IntoElement;
#[cfg(not(target_os = "macos"))]
use gpui::MouseMoveEvent;
#[cfg(not(target_os = "macos"))]
use gpui::ParentElement as _;
#[cfg(not(target_os = "macos"))]
use gpui::StatefulInteractiveElement as _;
#[cfg(not(target_os = "macos"))]
use gpui::Styled as _;
#[cfg(not(target_os = "macos"))]
use gpui::div;
#[cfg(not(target_os = "macos"))]
use gpui::prelude::FluentBuilder as _;
#[cfg(not(target_os = "macos"))]
use gpui::px;

#[cfg(not(target_os = "macos"))]
use super::model::FLOATING_REVEAL_EDGE_WIDTH;
use super::model::FloatingRevealContent;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-23 DECISION:
    /// User: "let's please not show the px bar that i need to hover on, make it transparent and
    /// overlayed 10px on top of whatever we show there", with no icons, and (asked whether a hover-only
    /// zone was preferred) chose a real transparent layer on every platform that also takes the clicks
    /// in those 10px. So while the sidebar is collapsed the zone takes no layout: the workarea keeps
    /// the full width and the zone lies over its left 10px. While the sessions column is folded too it
    /// stays two equal halves, the top floating the sidebar and the bottom the sessions column. This
    /// supersedes the 2026-09-20 rule that the hot zone was a sibling strip and never an overlay, and
    /// the 2026-09-21/23 icons that marked it.
    pub(crate) fn floating_reveal_edge_want(&self, top_half: bool) -> FloatingRevealContent {
        let split = self.floating_reveal_available().agents_column;
        FloatingRevealContent {
            sidebar: top_half || !split,
            agents_column: !top_half && split,
        }
    }

    /// CDXC:Sidebar 2026-09-23 WHY:
    /// Browser, Docs and Kanban pages are native CEF views above everything GPUI paints, so a GPUI
    /// overlay would never see the pointer over a page that reaches the window's left edge (an
    /// expanded view always does). On macOS the zone is therefore a native view kept on top of those
    /// (`GhostexGpuiRevealEdgeSync` in native/macos/GpuiSidebarReveal.m) and this element is only
    /// drawn on Windows and Linux.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn render_floating_reveal_edge_strip(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let split = self.floating_reveal_available().agents_column;
        div()
            .id("ghostex-gpui-floating-reveal-edge")
            .absolute()
            .top_0()
            // Right of the rail, which keeps the window's left edge (app/sidebar_rail/).
            .left(px(self.sidebar_rail_width()))
            .flex()
            .flex_col()
            .w(px(FLOATING_REVEAL_EDGE_WIDTH))
            .h_full()
            .occlude()
            .on_hover(cx.listener(|this, hovered: &bool, _window, _cx| {
                if !*hovered {
                    this.disarm_floating_reveal_edge();
                }
            }))
            .child(Self::render_floating_reveal_edge_zone(
                "ghostex-gpui-floating-reveal-edge-sidebar",
                true,
                cx,
            ))
            .when(split, |this| {
                this.child(Self::render_floating_reveal_edge_zone(
                    "ghostex-gpui-floating-reveal-edge-sessions",
                    false,
                    cx,
                ))
            })
    }

    /// One zone of the strip. Hover alone is not enough: while the panel covers the strip the main
    /// window stops seeing the pointer, so the strip can be left believing it is still hovered.
    /// Movement is what arms the reveal, and the panel clears the flag when it closes, so coming
    /// back to the edge is a fresh gesture rather than an instant re-open.
    #[cfg(not(target_os = "macos"))]
    fn render_floating_reveal_edge_zone(
        id: &'static str,
        top_half: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .flex_1()
            .min_h_0()
            .w_full()
            .on_mouse_move(
                cx.listener(move |this, _event: &MouseMoveEvent, _window, _cx| {
                    let want = this.floating_reveal_edge_want(top_half);
                    this.arm_floating_reveal_from_strip(want);
                }),
            )
    }
}
