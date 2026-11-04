//! Open and close plumbing for the native Send Bug / Feedback pop-up, shared by the desktop and the GPUI
//! web build (symlinked there).
//! SEE-ALSO: apps/desktop/src/app/window/feedback_modal/ (the pop-up and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs and apps/gpui-web/src/app/web_host/modals.rs (the two window openers).
use crate::app::gx_store::{read_preference_value, read_primary_agent_launcher_id};
use crate::app::window::*;
use crate::*;
use ghostex_gx_core::{MachineId, SessionKey};

impl GhostexGpuiApp {
    /// Opens the pop-up for the `feedback` modal kind, with the GitHub username the last feedback
    /// named and the default agent for new sessions. It needs nothing from the open message: the
    /// pop-up drafts, sends and starts the feedback agent through gxserver itself, and tells the app
    /// when it closed or which agent session to open. `collect_with_agent` starts it with "Collect
    /// more data with an agent" on, as the chat's Copy Report does.
    pub(crate) fn open_gpui_feedback_modal(
        &mut self,
        collect_with_agent: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let config = FeedbackModalConfig {
            app: if cfg!(target_family = "wasm") {
                "web"
            } else {
                "desktop"
            },
            palette: self.gpui_native_dialog_palette(),
            github_username: read_preference_value(FEEDBACK_GITHUB_USERNAME_STORAGE_KEY)
                .ok()
                .flatten()
                .unwrap_or_default(),
            agent_id: read_primary_agent_launcher_id(),
            agent_machine: self.feedback_agent_machine(),
            #[cfg(not(target_family = "wasm"))]
            offline_diagnostics: Some(feedback_offline_diagnostics),
            #[cfg(target_family = "wasm")]
            offline_diagnostics: None,
            scrub_context: feedback_scrub_context(),
            collect_with_agent,
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| match command {
            FeedbackModalCommand::Closed => {
                app.release_native_app_modal_window(GpuiAppModalKind::Feedback, cx)
            }
            FeedbackModalCommand::OpenSession {
                remote_machine_id: Some(machine_id),
                project_id,
                session_id,
                agent_id,
            } => {
                app.release_native_app_modal_window(GpuiAppModalKind::Feedback, cx);
                // As a new agent session on a remote project opens (gx_store/create/agent.rs).
                let chat = app.gx_store_preferred_interface(&agent_id) == "chat";
                let session = SessionKey::remote(machine_id, project_id, session_id);
                let payload = ghostex_gx_core::open_remote_session_terminal(
                    &session.to_sidebar_session_id(),
                    true,
                    chat.then_some("chat"),
                );
                app.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
            }
            FeedbackModalCommand::OpenSession {
                remote_machine_id: None,
                project_id,
                session_id,
                agent_id,
            } => {
                app.release_native_app_modal_window(GpuiAppModalKind::Feedback, cx);
                let interface = app.gx_store_preferred_interface(&agent_id);
                app.gx_store_focus_created_session(
                    &project_id,
                    &session_id,
                    true,
                    Some(&interface),
                    cx,
                );
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::Feedback,
            FEEDBACK_MODAL_WIDTH,
            FEEDBACK_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiFeedbackModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// The remote machine the session being reported runs on: the focused session's, else the
    /// active project's; `None` when that is this computer (CDXC:Feedback 2026-10-18 in
    /// window/feedback_modal/state.rs).
    fn feedback_agent_machine(&self) -> Option<FeedbackAgentMachine> {
        let focus = self.gx_store.core.focus();
        let machine = focus
            .focused_session
            .as_ref()
            .map(|session| &session.machine)
            .or_else(|| {
                focus
                    .active_project
                    .as_ref()
                    .map(|project| &project.machine)
            })?;
        let MachineId::Remote(machine_id) = machine else {
            return None;
        };
        let name = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
            .and_then(|machines| {
                machines.iter().find(|machine| {
                    machine.get("id").and_then(serde_json::Value::as_str)
                        == Some(machine_id.as_str())
                })
            })
            .and_then(|machine| machine.get("name").and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("The remote machine")
            .to_string();
        let hud = self.gx_store_launch_hud();
        Some(FeedbackAgentMachine {
            machine_id: machine_id.clone(),
            name,
            target: self.gpui_remote_gxserver_request_target(machine_id),
            agent_id: read_primary_agent_launcher_id()
                .map(|agent_id| ghostex_gx_core::remote_launch_agent_id(hud.as_deref(), &agent_id)),
        })
    }
}

/// This computer's gxserver startup diagnostics for the browser issue Send Bug / Feedback opens
/// while gxserver does not answer: a fresh health probe and the startup report the "gxserver
/// failed to start" notice copies.
#[cfg(not(target_family = "wasm"))]
fn feedback_offline_diagnostics() -> String {
    let (_, probe) = crate::app::helpers::gpui_probe_local_gxserver_health_with_diagnostics();
    crate::app::helpers::gpui_gxserver_startup_failure_report(&[format!(
        "Health check when the report was made: {probe}"
    )])
}

/// What this computer knows is private: its home folder, user and computer names, and the names,
/// hosts and users of the saved remote machines (CDXC:Feedback 2026-10-22 DECISION in
/// window/feedback_modal/offline_issue.rs).
fn feedback_scrub_context() -> FeedbackScrubContext {
    let variable = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let home_dirs = ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(variable)
        .collect();
    let mut private_words: Vec<String> = [
        "USER",
        "USERNAME",
        "LOGNAME",
        "COMPUTERNAME",
        "USERDOMAIN",
        "HOSTNAME",
    ]
    .into_iter()
    .filter_map(variable)
    .collect();
    #[cfg(all(unix, not(target_family = "wasm")))]
    {
        let mut name = [0u8; 256];
        // SAFETY: the buffer is writable for its whole length and stays alive for the call.
        if unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } == 0 {
            let end = name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name.len());
            let host = String::from_utf8_lossy(&name[..end]).trim().to_string();
            if let Some(short) = host.split('.').next().filter(|short| *short != host) {
                private_words.push(short.to_string());
            }
            private_words.push(host);
        }
    }
    if let Some(machines) = shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
    {
        for machine in machines {
            for field in ["name", "sshHost", "sshUser", "easyConnectAddress"] {
                if let Some(value) = machine.get(field).and_then(serde_json::Value::as_str) {
                    private_words.push(value.to_string());
                }
            }
        }
    }
    FeedbackScrubContext {
        home_dirs,
        private_words,
    }
}
