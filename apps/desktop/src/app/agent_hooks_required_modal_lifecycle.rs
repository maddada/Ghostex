//! Open, close, and sidebar bridge plumbing for the native Install Hooks prompt.
//! SEE-ALSO: apps/desktop/src/app/window/agent_hooks_required_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), the `confirmAgentHookLaunch` arm in apps/desktop/src/app/delayed_send.rs (the React host's route, mirrored here).
use crate::app::window::*;
use crate::*;

/// The launch the prompt decides about: the `open` message's ids, posted back
/// with the user's answer as `confirmAgentHookLaunch`.
struct GpuiAgentHookLaunch {
    agent_id: String,
    hook_agent_id: String,
    group_id: Option<String>,
    account_id: Option<String>,
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `agentHooksRequired` modal kind. Refuses payloads the React host would
    /// have refused: `agentId`, `agentName` and `hookAgentId` must be non-empty.
    pub(crate) fn open_gpui_agent_hooks_required_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let (Some(agent_id), Some(agent_name), Some(hook_agent_id)) =
            (text("agentId"), text("agentName"), text("hookAgentId"))
        else {
            return;
        };
        let launch = GpuiAgentHookLaunch {
            agent_id,
            hook_agent_id: hook_agent_id.clone(),
            group_id: text("groupId"),
            account_id: message
                .get("accountId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        };
        let config = AgentHooksRequiredModalConfig {
            agent_name,
            hook_agent_id,
            palette: self.gpui_native_modal_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_agent_hooks_required_modal_command(&launch, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::AgentHooksRequired,
            AGENT_HOOKS_REQUIRED_MODAL_WIDTH,
            AGENT_HOOKS_REQUIRED_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiAgentHooksRequiredModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_agent_hooks_required_modal_command(
        &mut self,
        launch: &GpuiAgentHookLaunch,
        command: AgentHooksRequiredModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let install_hooks = match command {
            AgentHooksRequiredModalCommand::Install => Some(true),
            AgentHooksRequiredModalCommand::Skip => Some(false),
            AgentHooksRequiredModalCommand::Close => None,
        };
        if let Some(install_hooks) = install_hooks {
            self.dispatch_gpui_confirm_agent_hook_launch(launch, install_hooks, cx);
        }
        self.release_native_app_modal_window(GpuiAppModalKind::AgentHooksRequired, cx);
    }

    /// The `confirmAgentHookLaunch` route of the React host: the same bounded
    /// ids the delayed_send arm accepts, handed to the Rust store (gx_store/create/agent.rs).
    fn dispatch_gpui_confirm_agent_hook_launch(
        &mut self,
        launch: &GpuiAgentHookLaunch,
        install_hooks: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        fn bounded(value: Option<&str>, max_len: usize) -> Option<&str> {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.len() <= max_len)
        }
        let Some(agent_id) = bounded(Some(&launch.agent_id), 128) else {
            return;
        };
        let Some(hook_agent_id) = bounded(Some(&launch.hook_agent_id), 128) else {
            return;
        };
        let mut message = serde_json::json!({
            "agentId": agent_id,
            "hookAgentId": hook_agent_id,
            "installHooks": install_hooks,
            "type": "confirmAgentHookLaunch",
        });
        if let Some(group_id) = bounded(launch.group_id.as_deref(), 512) {
            message["groupId"] = serde_json::json!(group_id);
        }
        if let Some(account_id) = bounded(launch.account_id.as_deref(), 256) {
            message["accountId"] = serde_json::json!(account_id);
        }
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }
}
