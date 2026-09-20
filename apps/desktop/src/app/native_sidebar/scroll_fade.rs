//! The ramp that fades the session list out at its bottom edge, in place of the hairline the
//! Commands row used to draw above itself.

use gpui::{IntoElement, Styled as _, div, px};

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::app::helpers::*;

impl GhostexGpuiApp {
    /*
    CDXC:Sidebar 2026-09-20 DECISION:
    User: the session list fades out at its bottom end only. The matching ramp at the top is gone,
    so nothing shades the list where it meets the project header, and there is no rule under the
    Search row, above the usage strip or above the Commands row. The ramp is painted over the list
    by the wrapper that owns it, never by the rows around it, so the fade always ends exactly where
    the list does even as the strip above the Commands row appears and disappears. This supersedes
    the 2026-09-19 rule that framed the list with a hairline at each end and the earlier 2026-09-20
    rule that faded both ends; the decision that both rows are one pixel taller is unchanged and
    lives in navigation.rs.

    CDXC:Sidebar 2026-09-20 WHY:
    The ramp is a bare `div()` with a background: no id, no listener, no hover style, no cursor and
    no group, which is what keeps GPUI from giving it a hitbox at all, so the rows underneath keep
    every click, drag, hover and scroll they had. That is the same rule the work area header's fade
    follows (render/workarea_header/overlap.rs).

    CDXC:Sidebar 2026-09-20 WHY:
    The sidebar's fill is a vertical gradient, so the ramp starts from the stop nearest its own
    edge, the gradient's bottom colour. One flat colour would have left a visible band there
    whenever custom chrome is on, and in light mode the two stops are the same value anyway.
    */
    pub(crate) fn render_native_sidebar_list_fade(
        &self,
        appearance: &SidebarAppearance,
    ) -> impl IntoElement {
        let color = sidebar_chrome_gradient_bottom_color();
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(SIDEBAR_LIST_BOTTOM_FADE_HEIGHT * appearance.scale))
            .bg(gpui::linear_gradient(
                180.0,
                gpui::linear_color_stop(color.opacity(0.0), 0.0),
                gpui::linear_color_stop(color, 1.0),
            ))
    }
}
