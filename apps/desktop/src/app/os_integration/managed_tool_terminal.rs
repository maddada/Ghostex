//! A Ghostex-managed tool's install that needs a password prompt where there is no desktop dialog
//! (Linux system tools on WSL or a headless session): a Main Hub terminal with the install typed in.

use crate::*;

impl GhostexGpuiApp {
    /// CDXC:ManagedTools 2026-11-25 WHY:
    /// Settings > Integrations > Tools installs Linux system tools with one password prompt; where polkit has no dialog to show (WSL, a headless session) gxserver answers with the `sudo` command instead, and the prompt has to be a real terminal. Since 2026-11-25 that terminal is Run in terminal's (user: "we should do the same thing please for all commands like this in the app so we can run them in main hub for the user"): a new Main Hub terminal with the command typed in and not sent, so the user presses Enter and types the password; it replaces the active project's Commands panel terminal that ran it at once. The page sends only the tool id and gxserver resolves the text (`systemToolsInstall`, server/src/shown_commands/), so a page can never make the app type text of its choosing.
    /// SEE-ALSO: server/src/managed_tools/system_tools.rs, apps/desktop/src/app/gx_store/create/shown_command_terminal.rs.
    pub(crate) fn run_managed_tool_terminal_command(
        &mut self,
        tool_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        // Only the Linux system tools install through a terminal.
        if tool_id != "systemTools" {
            return;
        }
        self.close_native_app_modal_from_bridge(cx);
        self.gx_store_open_shown_command_terminal(None, "systemToolsInstall".to_string(), None, cx);
    }
}
