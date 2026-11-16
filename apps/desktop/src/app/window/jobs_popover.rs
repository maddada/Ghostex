//! The jobs popover's window content: the app's jobs (app/background_jobs/) drawn by
//! jobs_popover_view.rs, with Cancel, Retry and Dismiss sent back to the app. The window takes
//! the popover's own height as jobs come and go.

use std::rc::Rc;

use gpui::{IntoElement, ParentElement as _, Render, Styled as _, Window, div, px};
use gpui_component::ElementExt as _;

use super::jobs_popover_view::{
    self as view, JOBS_POPOVER_WIDTH, JobsAction, JobsHandler, JobsModel,
};
use crate::app::helpers::*;
use crate::*;

/// The tallest the popover grows before its window stops following it.
pub(crate) const JOBS_POPOVER_MAX_HEIGHT: f32 = 520.0;

/// The first frame's height for `jobs` rows; the panel then sizes its window to what it drew.
pub(crate) fn jobs_popover_height(jobs: usize) -> f32 {
    (38.0 + 96.0 * jobs.max(1) as f32).min(JOBS_POPOVER_MAX_HEIGHT)
}

pub(crate) struct JobsPopoverPanel {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    /// Held until its window has the popover's height, then shown (CDXC:AppModal 2026-11-13 in
    /// popup_frame.rs).
    hold: crate::app::window::popup_frame::PopupHoldState,
}

impl JobsPopoverPanel {
    pub(crate) fn new(main_app: gpui::WeakEntity<GhostexGpuiApp>) -> Self {
        Self {
            main_app,
            hold: Default::default(),
        }
    }

    fn act(&mut self, action: JobsAction, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| match action {
            JobsAction::Cancel(id) => app.background_job_cancel(id, cx),
            JobsAction::Retry(id) => app.background_job_retry(id, cx),
            JobsAction::Dismiss(id) => app.background_job_dismiss(id, cx),
        });
    }
}

impl Render for JobsPopoverPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let rows = self
            .main_app
            .upgrade()
            .map(|app| app.read(cx).background_jobs.rows())
            .unwrap_or_default();
        let light = chrome_uses_light_appearance();
        let glass = window_glass_active();
        let model = JobsModel {
            rows,
            light,
            glass,
            // The header dropdowns' own surface, as the Share popover takes it (collab_share.rs).
            surface: Some(popup_menu_frame()),
        };
        let on: JobsHandler<Self> =
            Rc::new(|panel: &mut Self, action, _window, cx| panel.act(action, cx));
        let popover = view::render_jobs_popover(&model, &on, cx);
        let hold = self.hold.clone();
        let main_app = self.main_app.clone();
        let measured = div().w(px(JOBS_POPOVER_WIDTH)).child(popover).on_prepaint(
            move |bounds, window, cx| {
                let height = bounds
                    .size
                    .height
                    .as_f32()
                    .clamp(60.0, JOBS_POPOVER_MAX_HEIGHT);
                crate::app::window::popup_frame::fit_popup_window_to_content(
                    height,
                    true,
                    &hold,
                    window,
                    cx,
                    move |cx| {
                        let _ = main_app.update(cx, |app, cx| {
                            app.reveal_titlebar_popup(GpuiTitlebarPopupKind::Jobs, cx)
                        });
                    },
                );
            },
        );
        crate::app::window::popup_frame::popup_open_frame_held(
            div().size_full(),
            measured,
            self.hold.get() == crate::app::window::popup_frame::PopupHold::Held,
            false,
            cx.reduce_motion(),
        )
    }
}
