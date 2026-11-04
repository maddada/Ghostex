//! The sidebar's extension buttons in the browser: extensions are hosted by the desktop app, so
//! this build lists none (apps/desktop/src/app/claude_web/sidebar_button.rs is the desktop's
//! answer).

use gpui::{Context, Window};

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::navigation::SidebarExtensionButton;

impl GhostexGpuiApp {
    pub(crate) fn sidebar_extension_buttons(&self) -> Vec<SidebarExtensionButton> {
        Vec::new()
    }

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
