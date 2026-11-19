//! What the app says when a terminal the user opened could not be attached: the reason in plain
//! words, and for a missing project folder the dialog that can fix it.
//!
//! CDXC:AppModal 2026-11-19 DECISION:
//! User, of "Couldn't open this terminal / Ghostex couldn't reconnect to it" shown when they opened
//! their shortpoint project, whose folder had been renamed outside Ghostex: "no idea why i saw this,
//! this dialog is super unclear from first glance". Every refusal now names its reason in the title
//! and the line. A terminal whose project folder is gone opens the Missing Project Folder dialog
//! (Locate Folder… moves the project and its terminals, `relocate_project_moving_sessions` in
//! gxserver); a terminal whose own folder is gone while its project's is there names the folder;
//! a terminal that no longer exists, a Ghostex service that does not answer, a terminal that did
//! not start and a call that took too long each get their own title. The raw error stays behind
//! Copy details (CDXC:Notices 2026-11-06 in modals/modal_messages_and_toasts.rs).
//! SEE-ALSO: apps/desktop/src/bin/native_modal_demo/notice_modal.rs (the `terminal-*` preview
//! states, same words).

use std::path::Path;

use ghostex_gx_core::ProjectKey;

use crate::app::helpers::*;
use crate::*;

/// The title and line of a terminal-open failure that has no better place to send the user.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TerminalOpenFailureWords {
    pub(crate) title: &'static str,
    pub(crate) message: String,
}

/// The words for `error`, with `folder` the terminal's missing folder when gxserver named one.
pub(crate) fn terminal_open_failure_words(
    code: Option<&str>,
    message: &str,
    folder: Option<&str>,
) -> TerminalOpenFailureWords {
    if code == Some(RESTORE_BLOCKED_MISSING_FOLDER) {
        return TerminalOpenFailureWords {
            title: "This terminal's folder is missing",
            message: match folder {
                Some(folder) => format!(
                    "{folder} no longer exists, so the terminal can't start there. Put the folder back, then open the terminal again."
                ),
                None => "Its folder no longer exists, so the terminal can't start there. Put the folder back, then open the terminal again.".to_string(),
            },
        };
    }
    if code == Some("notFound") {
        return TerminalOpenFailureWords {
            title: "This terminal no longer exists",
            message: "It was closed or removed. Pick another one in the sidebar.".to_string(),
        };
    }
    if gpui_command_terminal_attach_startup_not_ready(message)
        || gxserver_transport_failure_is_unreachable(message)
    {
        return TerminalOpenFailureWords {
            title: "Ghostex isn't responding",
            message: "Its background service didn't answer. Wait a moment and open the terminal again, or restart Ghostex.".to_string(),
        };
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return TerminalOpenFailureWords {
            title: "Terminal took too long to open",
            message: "Ghostex didn't answer in time. Open the terminal again.".to_string(),
        };
    }
    if message.starts_with("gxserver did not confirm") {
        return TerminalOpenFailureWords {
            title: "The terminal didn't start",
            message: "Ghostex couldn't start it. Open it again, or restart it from the sidebar."
                .to_string(),
        };
    }
    TerminalOpenFailureWords {
        title: "Couldn't open this terminal",
        message: "Ghostex couldn't reconnect to it. Try again, or restart it from the sidebar."
            .to_string(),
    }
}

impl GhostexGpuiApp {
    /// Tells the user why the terminal of `key` did not open (an account refusal has its own
    /// notice, `show_local_workspace_account_refusal_toast`).
    pub(crate) fn show_local_workspace_terminal_open_failure(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        error: &GpuiGxserverRpcError,
        cx: &mut gpui::Context<Self>,
    ) {
        if error.code.as_deref() == Some(RESTORE_BLOCKED_MISSING_FOLDER)
            && self.terminal_folder_is_in_missing_project_folder(
                &key.project_id,
                error.missing_folder.as_deref(),
            )
            && self.gx_store_present_missing_project_folder(&key.project_id, cx)
        {
            return;
        }
        let words = terminal_open_failure_words(
            error.code.as_deref(),
            &error.message,
            error.missing_folder.as_deref(),
        );
        self.dispatch_gpui_workspace_action_toast_with_details(
            "warning",
            words.title,
            &words.message,
            error.message.as_str(),
            cx,
        );
    }

    /// Whether the project's own folder is gone and the terminal's missing folder is that folder or
    /// one inside it, so locating the project's folder is what brings the terminal back.
    fn terminal_folder_is_in_missing_project_folder(
        &self,
        project_id: &str,
        folder: Option<&str>,
    ) -> bool {
        let Some(project_path) = self
            .gx_store
            .core
            .presentation()
            .project(&ProjectKey::local(project_id))
            .and_then(|project| project.path.clone())
            .filter(|path| !path.trim().is_empty())
        else {
            return false;
        };
        let project_path = Path::new(project_path.trim());
        !project_path.is_dir()
            && folder.is_none_or(|folder| Path::new(folder).starts_with(project_path))
    }
}
