//! What Settings > Cloud Boxes asks the app to do: open one agentbox setup step in a Main Hub
//! terminal, and start the "Set it up for me" agent session.

use crate::*;

/// The arguments a `runAgentboxTerminalCommand` message may carry to gxserver. gxserver validates
/// each one again; anything else in the message is dropped.
const AGENTBOX_TERMINAL_COMMAND_ARGUMENTS: [&str; 5] =
    ["provider", "agent", "host", "alias", "ssh"];

impl GhostexGpuiApp {
    /// CDXC:AgentBox 2026-11-25 WHY:
    /// Settings > Cloud Boxes sends only a step name (`install`, `login`, `prepare`, `agentLogin`, `remoteDockerAdd`, ...) and its arguments, and gxserver works out the command text (`agentboxStep`, server/src/shown_commands/, through agentbox's own `terminalCommand` check), so a page can never make the app type text of its choosing. The step needs a real terminal because agentbox's logins and wizards ask questions. Since 2026-11-25 it is Run in terminal's Main Hub terminal with the step typed in and not sent (user: "we should do the same thing please for all commands like this in the app so we can run them in main hub for the user"), replacing the active project's Commands panel terminal that ran it at once.
    /// SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/cloud_boxes.rs, apps/desktop/src/app/gx_store/create/shown_command_terminal.rs.
    pub(crate) fn run_agentbox_terminal_command(
        &mut self,
        message: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(command) = message
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
        else {
            return;
        };
        let mut args = serde_json::Map::new();
        args.insert("command".into(), command.into());
        for key in AGENTBOX_TERMINAL_COMMAND_ARGUMENTS {
            if let Some(value) = message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                args.insert(key.into(), value.into());
            }
        }
        // The step changes what is ready; the next launcher or picker reads it again.
        crate::app::gx_store::invalidate_agentbox_status();
        self.close_native_app_modal_from_bridge(cx);
        self.gx_store_open_shown_command_terminal(
            None,
            "agentboxStep".to_string(),
            Some(serde_json::Value::Object(args)),
            cx,
        );
    }

    /// "Set it up for me" on Settings > Cloud Boxes: an agent session in the Ghostex folder
    /// project (the one Help chats use) that installs and configures agentbox.
    pub(crate) fn run_agentbox_setup_chat(&mut self, cx: &mut gpui::Context<Self>) {
        let project_path = shared_settings::ghostex_storage_paths()
            .config_dir
            .to_string_lossy()
            .to_string();
        let dispatched = self.dispatch_gpui_os_integration_command_message(
            serde_json::json!({
                "action": "createAgentboxSetupChat",
                "projectPath": project_path,
            }),
            cx,
        );
        if !dispatched {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Couldn't start the setup agent",
                "The sidebar is not ready to start a session yet.",
                cx,
            );
        }
    }
}
