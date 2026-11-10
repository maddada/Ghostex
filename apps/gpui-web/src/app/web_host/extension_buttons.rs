//! The Claude and Grok Bot buttons in the browser: extensions are hosted by the desktop app, so
//! this build has none (apps/desktop/src/app/claude_web/header_buttons.rs is the desktop's
//! answer).

use gpui::{Context, Window};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn toggle_sidebar_extension_view(
        &mut self,
        _extension_id: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(crate) fn close_sidebar_extension_view(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }
}
