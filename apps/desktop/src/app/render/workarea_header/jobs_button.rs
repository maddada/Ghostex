//! The header's jobs button: shown while a long job runs or a failed one waits, just left of the
//! voice button; it opens the jobs popover (window/jobs_popover.rs). The decision record is in
//! window/jobs_popover_view.rs (CDXC:BackgroundJobs).

use std::cell::Cell;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, InteractiveElement as _, IntoElement, MouseButton, Pixels,
    StatefulInteractiveElement as _, Styled as _,
};
use gpui_component::ElementExt as _;

use crate::app::helpers::*;
use crate::app::window::jobs_popover_view::{jobs_button, jobs_button_text};
use crate::*;

thread_local! {
    /// Where the button was last painted, for its popover to hang from.
    static ANCHOR: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

impl GhostexGpuiApp {
    pub(crate) fn render_workarea_header_jobs_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let rows = self.background_jobs.rows();
        let (label, tooltip, failed) = jobs_button_text(&rows)?;
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Jobs);
        Some(
            jobs_button(
                label.clone(),
                failed,
                titlebar_active_text_color(),
                titlebar_popup_menu_destructive(),
                TITLEBAR_CONTROL_HEIGHT,
                TITLEBAR_BUTTON_RADIUS,
            )
            .role(gpui::Role::Button)
            .aria_label(label)
            .ml(gpui::px(4.0))
            .cursor_default()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .when(open, |this| this.bg(titlebar_button_hover_color()))
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let open = this.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Jobs);
                    let anchor = ANCHOR.with(Cell::get);
                    this.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::Jobs,
                        !open,
                        anchor,
                        window,
                        cx,
                    );
                }),
            )
            .on_prepaint(|bounds, _, _| ANCHOR.with(|anchor| anchor.set(Some(bounds))))
            .into_any_element(),
        )
    }
}
