use super::state::{NativeChatConfig, NativeChatView};
use crate::*;
use serde_json::json;

impl GhostexGpuiApp {
    /// CDXC:AgentLauncher 2026-09-23 WHY:
    /// Closing the picker left the main window's input handoff until its next frame, losing the first characters typed after clicking an agent. The picker has already released its window here, so activate the main window and focus the staged input before returning from the launch command.
    pub(crate) fn focus_staged_chat_after_picker(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(window) = self.main_window_handle else {
            return;
        };
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return;
        };
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return;
        };
        if !view.read(cx).config.session_id.is_empty() {
            return;
        }
        let _ = window.update(cx, |_, window, cx| {
            window.activate_window();
            self.reclaim_gpui_root_for_native_chat_composer(window);
            view.update(cx, |view, cx| {
                view.focus_requested = true;
                view.ensure_input(window, cx);
            });
        });
    }

    /// CDXC:AgentLauncher 2026-09-23 DECISION:
    /// User: sidebar and Cmd+Shift+T launches must accept typing instantly, with preparation in the background, and the initial layout must match the settled chat as closely as possible. This extends the 2026-09-22 instant composer decision: use the normal placeholder, toolbar and agent controls from the first frame, and carry the known identity into the controller so boot cannot briefly erase them.
    pub(crate) fn stage_native_chat_launch(
        &mut self,
        shell_session_id: TerminalSessionId,
        project_id: &str,
        agent_id: &str,
        name: &str,
        icon: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let agent = match icon {
            Some("cursor-cli") => "cursor",
            Some("antigravity-cli") => "antigravity",
            Some("hermes-agent") => "hermes",
            Some("grok-build") => "grok",
            Some(icon) => icon,
            None => agent_id,
        };
        let config = NativeChatConfig {
            machine_id: crate::app::gx_chat::LOCAL_MACHINE_ID.into(),
            project_id: project_id.into(),
            session_id: String::new(),
            sidebar_session_id: String::new(),
            shell_session_id,
            client_id: format!("native-desktop-{}", std::process::id()),
            remote: None,
            app: Some(cx.weak_entity()),
            parent_native_view: self.parent_ns_view,
            initial_snapshot: None,
            initial_presentation: None,
        };
        let view = self.insert_native_chat(config, None, cx);
        view.update(cx, |view, cx| {
            view.snapshot = std::sync::Arc::new(json!({
                "status": "starting",
                "agent": agent,
                "sessionAgentId": agent_id,
                "composerPlaceholder": ghostex_gx_chat_core::composer::policy::DESKTOP_COMPOSER_PLACEHOLDER,
                "composerActions": {"summary": true, "note": false, "stash": true, "attach": true, "terminal": true},
                "optionLabels": {
                    "showModel": true,
                    "agentIcon": icon,
                    "model": if icon == Some("zcode") { Some(name) } else { None },
                    "modelDisplay": if icon == Some("zcode") { Some(name) } else { None }
                },
                "contextMeter": matches!(icon, Some("codex" | "claude")).then(|| json!({
                    "tooltip": "Context usage not yet reported",
                    "statusLineReserved": view.status_line_reserved,
                    "starred": []
                })),
                "newSessionWelcome": {
                    "agentName": name,
                    "icon": icon,
                    "showTitle": true,
                    "title": format!("What should we build with {name}?")
                },
                "loadingStage": null
            }));
            view.focus_requested = true;
            cx.notify();
        });
    }
}

impl NativeChatView {
    pub(crate) fn bind_launched_session(
        &mut self,
        mut config: NativeChatConfig,
        cx: &mut gpui::Context<Self>,
    ) {
        let presentation = config.initial_presentation.get_or_insert_with(|| json!({}));
        for field in ["agent", "sessionAgentId"] {
            if presentation[field].is_null() {
                presentation[field] = self.snapshot[field].clone();
            }
        }
        self.config = config;
        self.runtime = Some(Self::start_runtime(&self.config, cx));
        cx.notify();
    }

    pub(super) fn retain_launch_welcome(&self, snapshot: &mut serde_json::Value) {
        if self.items.is_empty()
            && self.snapshot["newSessionWelcome"].is_object()
            && (snapshot["status"] == "loading" || snapshot["status"].is_null())
            && snapshot["error"].is_null()
        {
            snapshot["status"] = json!("starting");
            snapshot["loadingStage"] = serde_json::Value::Null;
            snapshot["newSessionWelcome"] = self.snapshot["newSessionWelcome"].clone();
        }
    }
}
