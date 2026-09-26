//! The web build's sidebar command route. The desktop's `actions.rs` routes sidebar commands into desktop-only UI (native window prompts, titlebar popups) that a page does not have, so only the action type and the two entry points the drawing code calls are lifted from it (`extracted-items.txt`); where a command goes is this file's.
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

include!(concat!(env!("OUT_DIR"), "/native_sidebar_actions.rs"));

impl GhostexGpuiApp {
    pub(crate) fn dispatch_native_sidebar_ui(
        &mut self,
        command: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.web_run_sidebar_command(command, cx);
    }
}
