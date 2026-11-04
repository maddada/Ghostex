//! The docked sidebar's own GPUI view, so a redraw that is not the sidebar's reuses its last frame.

use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, Render, Styled as _, Subscription,
    WeakEntity, Window, div,
};

use crate::GhostexGpuiApp;

/// CDXC:Sidebar 2026-09-21 WHY:
/// The sidebar was drawn inline by the window root, so every chat redraw (a streamed token, a transcript scroll tick, a composer keystroke) rebuilt and laid out the whole sidebar as well.
/// As a cached view it is reused on those frames. Its state and listeners stay on `GhostexGpuiApp`, and it observes that entity, so every app notify redraws it in the same frame and nothing the sidebar reads can go stale: a focus move still repaints its highlight with the list update that caused it.
/// Moving sidebar state into this view so store updates notify it alone was weighed and left out: measured after the chat views were cached, the root and sidebar renders together cost under two percent of the main thread.
pub(crate) struct NativeSidebarHost {
    app: WeakEntity<GhostexGpuiApp>,
    _app_changed: Subscription,
}

impl NativeSidebarHost {
    fn new(app: &Entity<GhostexGpuiApp>, cx: &mut Context<Self>) -> Self {
        Self {
            app: app.downgrade(),
            _app_changed: cx.observe(app, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for NativeSidebarHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.app.upgrade() {
            Some(app) => app.update(cx, |app, cx| app.render_native_sidebar(window, cx)),
            None => div().into_any_element(),
        }
    }
}

impl GhostexGpuiApp {
    pub(crate) fn render_docked_native_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.entity();
        let host = self
            .native_sidebar
            .host
            .get_or_insert_with(|| cx.new(|cx| NativeSidebarHost::new(&app, cx)))
            .clone();
        gpui::AnyView::from(host)
            .cached(gpui::StyleRefinement::default().size_full())
            .into_any_element()
    }
}
