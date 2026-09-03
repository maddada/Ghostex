// C1 wave-3 re-cluster: the CommandTerminalActivity/TabStatus enums and the CommandTerminalSession model, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:SessionStatus 2026-06-27-05:07:
Command-pane tab status indicators now have live Action parity plus safe restored metadata: Action command tabs own run ids, session-state file stamping, status-file polling, run-start Working, idle completion, completion feedback, and exit cleanup, while non-Action/restored tabs remain enum/boolean status only. Status must not be inferred from shell titles, output, paths, command text, env, logs, or persisted shell JSON. Persistence keeps only the bounded Action selector needed to reclaim the same tab after restart; run ids, status-file paths, command text, stdout/stderr, terminal content, countdown labels, paths, tokens, and private titles remain runtime-only.

CDXC:CommandPane 2026-06-25-11:24:
The command-pane model still keeps an idle enum for persistence and sidebar indicators, but command tab chrome should match macOS by rendering indicators only for working, attention, and delayed-send states. Idle command tabs show no status indicator.

CDXC:SessionStatus 2026-06-25-13:18:
Command-tab status chrome is a trailing native slot, not a leading title prefix: the tab title reserves trailing space for active status, the indicator hides while hover close chrome is visible, and Delayed Send uses clock glyph chrome instead of a rounded dot.

CDXC:SessionSleep 2026-06-25-14:27:
Command-pane Sleep is a renderer lifecycle flag, not tab deletion. Persist only the sleeping boolean with safe command metadata so tabs remain in their command group while mounted command terminal body slots are withheld until explicit wake.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandTerminalActivity {
    Idle,
    Working,
    Attention,
}

impl Default for CommandTerminalActivity {
    fn default() -> Self {
        Self::Idle
    }
}

impl CommandTerminalActivity {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "working" => Some(Self::Working),
            "attention" => Some(Self::Attention),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandTerminalTabStatus {
    Idle,
    Working,
    Attention,
    DelayedSend,
}

impl CommandTerminalTabStatus {
    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
            Self::DelayedSend => "delayed-send",
        }
    }
}

pub(crate) fn command_terminal_tab_status(
    activity: CommandTerminalActivity,
    delayed_send_active: bool,
) -> CommandTerminalTabStatus {
    if delayed_send_active {
        CommandTerminalTabStatus::DelayedSend
    } else {
        match activity {
            CommandTerminalActivity::Idle => CommandTerminalTabStatus::Idle,
            CommandTerminalActivity::Working => CommandTerminalTabStatus::Working,
            CommandTerminalActivity::Attention => CommandTerminalTabStatus::Attention,
        }
    }
}

pub(crate) struct CommandTerminalSession {
    pub(crate) id: CommandSessionId,
    pub(crate) title: String,
    pub(crate) gxserver_session_key: Option<GpuiLocalWorkspaceSessionKey>,
    /*
    CDXC:RemoteMachines 2026-08-29:
    A remote project's terminal Action runs its command on the machine that
    owns the project, so its command tab attaches over SSH to a session on that
    machine's daemon instead of owning a local one. This is that session's
    identity: it is what lets the tab close the remote session when it closes,
    and reattach to it after a restart. It carries only the machine/project/
    session selectors, never SSH details, tokens, remote paths, or command
    text, and a tab has either this or `gxserver_session_key`, never both.
    */
    pub(crate) remote_action_session: Option<GpuiRemoteAttachSessionReference>,
    /// Runtime-only zmx persistence session name from gxserver attach
    /// metadata; see `TerminalSession::zmx_session_name`.
    pub(crate) zmx_session_name: Option<String>,
    pub(crate) activity: CommandTerminalActivity,
    pub(crate) delayed_send_active: bool,
    pub(crate) delayed_send_timer_owned: bool,
    pub(crate) close_after_done_armed: bool,
    pub(crate) is_sleeping: bool,
    pub(crate) action_close_terminal_on_exit: bool,
    pub(crate) action_command_id: Option<String>,
    pub(crate) action_play_completion_sound: bool,
    pub(crate) action_run_id: Option<String>,
    pub(crate) action_status_file_path: Option<PathBuf>,
}

impl CommandTerminalSession {
    pub(crate) fn placeholder(id: CommandSessionId, title: String) -> Self {
        Self {
            id,
            title,
            gxserver_session_key: None,
            remote_action_session: None,
            zmx_session_name: None,
            activity: CommandTerminalActivity::Idle,
            delayed_send_active: false,
            delayed_send_timer_owned: false,
            close_after_done_armed: false,
            is_sleeping: false,
            action_close_terminal_on_exit: false,
            action_command_id: None,
            action_play_completion_sound: false,
            action_run_id: None,
            action_status_file_path: None,
        }
    }

    pub(crate) fn with_activity(mut self, activity: CommandTerminalActivity) -> Self {
        self.activity = activity;
        self
    }

    pub(crate) fn with_delayed_send_active(mut self, delayed_send_active: bool) -> Self {
        self.delayed_send_active = delayed_send_active;
        self.delayed_send_timer_owned = false;
        self
    }

    pub(crate) fn set_delayed_send_active(&mut self, delayed_send_active: bool, timer_owned: bool) {
        self.delayed_send_active = delayed_send_active;
        self.delayed_send_timer_owned = delayed_send_active && timer_owned;
    }

    pub(crate) fn with_sleeping(mut self, is_sleeping: bool) -> Self {
        self.is_sleeping = is_sleeping;
        self
    }

    pub(crate) fn with_close_after_done_armed(mut self, close_after_done_armed: bool) -> Self {
        self.close_after_done_armed = close_after_done_armed;
        self
    }

    pub(crate) fn with_gxserver_session_key(
        mut self,
        key: Option<GpuiLocalWorkspaceSessionKey>,
    ) -> Self {
        self.gxserver_session_key = key;
        self
    }

    pub(crate) fn with_remote_action_session(
        mut self,
        reference: Option<GpuiRemoteAttachSessionReference>,
    ) -> Self {
        self.remote_action_session = reference;
        self
    }

    pub(crate) fn with_action_command_id(mut self, action_command_id: String) -> Self {
        self.action_command_id = Some(action_command_id);
        self
    }

    pub(crate) fn tab_status(&self) -> CommandTerminalTabStatus {
        /*
        CDXC:DelayedSend 2026-06-25-18:07:
        Native command-pane tab sync publishes Delayed Send countdown labels before it skips sleeping sessions for working/attention activity. Preserve the timer clock on sleeping command tabs, while sleeping tabs without a timer still render idle parked chrome.
        */
        if self.delayed_send_active {
            return CommandTerminalTabStatus::DelayedSend;
        }
        if self.is_sleeping {
            return CommandTerminalTabStatus::Idle;
        }
        command_terminal_tab_status(self.activity, false)
    }

    pub(crate) fn sidebar_hud_indicator_status(&self) -> &'static str {
        /*
        CDXC:SessionStatus 2026-06-27-06:30:
        Sidebar command-session HUD indicators mirror native terminal lifecycle, not Action completion feedback. GPUI has no persisted lifecycle error state for live local command tabs, so awake command tabs export running and sleeping tabs export idle without deriving error from Attention or exposing terminal-private data.
        */
        if self.is_sleeping { "idle" } else { "running" }
    }
}
