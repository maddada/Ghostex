//! Which window a click, a command or an app-wide change reaches when several workspace windows
//! are open.

use gpui::{Context, Window};

use super::registry::{
    lead_workspace_window, other_workspace_window_apps, workspace_window_showing_session,
};
use crate::*;

impl GhostexGpuiApp {
    /// When this window is not the lead, runs `f` on the lead's app in the lead's window on the
    /// next turn and returns true: the app-wide work (the updater, Keep Awake) lives there.
    pub(crate) fn forward_to_lead_window(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        if self.is_lead_window() {
            return false;
        }
        let Some((handle, lead)) = lead_workspace_window(cx) else {
            return false;
        };
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = lead.update(cx, |app, cx| f(app, window, cx));
            });
        });
        true
    }

    /// Runs `f` on every other open workspace window's app on the next turn, for a change every
    /// window shows (an update, waking from sleep, Reduce Motion).
    pub(crate) fn update_other_workspace_windows(
        &self,
        cx: &mut Context<Self>,
        f: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let others = other_workspace_window_apps(cx.entity_id());
        if others.is_empty() {
            return;
        }
        cx.defer(move |cx| {
            for other in others {
                other.update(cx, |app, cx| f(app, cx));
            }
        });
    }

    /// A notification banner or a menu bar session row, which reaches the lead: the window that
    /// already shows the session (`row_id`, the sidebar row id) takes it, the one last active
    /// first, else this window, whose focus then goes on to the window that shows the session's
    /// workspace (session_routing.rs). `activate` focuses the session in the window chosen.
    pub(crate) fn activate_session_in_its_window(
        &mut self,
        row_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        activate: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let caller = (cx.entity_id(), self.gx_store_shows_session_row(row_id));
        if let Some((handle, app)) = workspace_window_showing_session(row_id, caller, cx)
            && app.entity_id() != cx.entity_id()
        {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    window.activate_window();
                    let _ = app.update(cx, |app, cx| activate(app, cx));
                });
            });
            return;
        }
        window.activate_window();
        activate(self, cx);
    }
}
