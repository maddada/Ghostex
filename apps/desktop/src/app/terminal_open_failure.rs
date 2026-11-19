//! What the app says when a terminal the user opened could not be attached: the reason in plain
//! words, and for a terminal of a project whose folder is missing, the mark's popover.
//!
//! CDXC:AppModal 2026-11-19 DECISION:
//! User, of "Couldn't open this terminal / Ghostex couldn't reconnect to it" shown when they opened
//! their shortpoint project, whose folder had been renamed outside Ghostex: "no idea why i saw this,
//! this dialog is super unclear from first glance". Every refusal now names its reason in the title
//! and the line: a terminal whose folder is gone names the folder; a terminal that no longer
//! exists, a Ghostex service that does not answer, a terminal that did not start and a call that
//! took too long each get their own title. The raw error stays behind Copy details
//! (CDXC:Notices 2026-11-06 in modals/modal_messages_and_toasts.rs).
//!
//! CDXC:AppModal 2026-11-19 WHY: a failed open never opens the Missing Project Folder dialog. It
//! did for a few hours (9a3853b53), and a chat that retries its terminal reopened that dialog
//! again and again, which is what the user could not get rid of on their shortpoint project.
//!
//! CDXC:Projects 2026-11-20 DECISION: User (Q4242, Q4244): no pop-up for a missing project folder.
//! A terminal of a project whose header wears the slashed computer opens no notice: its chat says
//! the folder is missing (server/src/project_folder_problem.rs), and a click on the session opens
//! the mark's popover under the project's header (native_sidebar/folder_mark.rs); a retry opens
//! nothing. A terminal whose own folder is gone inside a project that is still there (a removed
//! worktree folder) keeps the notice that names it.
//! SEE-ALSO: apps/desktop/src/bin/native_modal_demo/notice_modal.rs (the `terminal-*` preview
//! states, same words).

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
    /// notice, `show_local_workspace_account_refusal_toast`). `opened_by_user`: a click in the
    /// sidebar asked for it, not a retry.
    pub(crate) fn show_local_workspace_terminal_open_failure(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        error: &GpuiGxserverRpcError,
        opened_by_user: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        // A session of a project whose folder is missing: the project's header wears the mark and
        // the session's chat says so, so no notice. A click opens the mark's popover; a retry
        // opens nothing.
        if error.code.as_deref() == Some(RESTORE_BLOCKED_MISSING_FOLDER)
            && self.local_project_wears_folder_mark(&key.project_id)
        {
            if opened_by_user {
                self.open_folder_popover_for_local_project(&key.project_id, cx);
            }
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
}
