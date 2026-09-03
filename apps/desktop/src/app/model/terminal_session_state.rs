// C1 wave-3 re-cluster: TerminalSession, AgentsWorkspaceSessionKind, TerminalSessionPresentationState, and AgentTerminalActivity/TabStatus, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:Terminal 2026-06-22-23:50:
Failed Agents terminal startup is a first-class presentation state so the tab survives launch failure with clear retry UI. The durable shell state stores only the safe `startup-failed` slug; runtime ids, launch errors, command text, cwd/path, env, process ids, stdout/stderr, and terminal content remain runtime-only or absent.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalSessionPresentationState {
    Running,
    Sleeping,
    Mounting,
    StartupFailed,
    RestoredUnmounted,
    PoppedOutPlaceholder,
}

impl TerminalSessionPresentationState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "sleeping" => Some(Self::Sleeping),
            "mounting" => Some(Self::Mounting),
            "startup-failed" => Some(Self::StartupFailed),
            "restored-unmounted" => Some(Self::RestoredUnmounted),
            "popped-out-placeholder" => Some(Self::PoppedOutPlaceholder),
            _ => None,
        }
    }

    pub(crate) fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    pub(crate) fn activation_pending_state(self) -> Option<Self> {
        match self {
            Self::Sleeping
            | Self::StartupFailed
            | Self::RestoredUnmounted
            | Self::PoppedOutPlaceholder => Some(Self::Mounting),
            Self::Running | Self::Mounting => None,
        }
    }

    pub(crate) fn counts_as_focus_mode_visible(self) -> bool {
        !matches!(self, Self::Sleeping)
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Mounting => "mounting",
            Self::StartupFailed => "startup-failed",
            Self::RestoredUnmounted => "restored-unmounted",
            Self::PoppedOutPlaceholder => "popped-out-placeholder",
        }
    }

    pub(crate) fn tab_badge_label(self) -> Option<&'static str> {
        match self {
            Self::Running => None,
            Self::Sleeping => None,
            Self::Mounting => Some("MNT"),
            Self::StartupFailed => Some("ERR"),
            Self::RestoredUnmounted => Some("RST"),
            Self::PoppedOutPlaceholder => Some("OUT"),
        }
    }

    pub(crate) fn placeholder_label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Sleeping => "Sleeping",
            Self::Mounting => "Mounting",
            Self::StartupFailed => "Startup failed",
            Self::RestoredUnmounted => "Restored",
            Self::PoppedOutPlaceholder => "Popped out",
        }
    }

    pub(crate) fn placeholder_title(self) -> &'static str {
        match self {
            Self::Running => "Terminal",
            Self::Sleeping => "Sleeping terminal",
            Self::Mounting => "Mounting terminal",
            Self::StartupFailed => "Terminal startup failed",
            Self::RestoredUnmounted => "Restored terminal",
            Self::PoppedOutPlaceholder => "Terminal popped out",
        }
    }

    /*
    CDXC:Terminal 2026-06-24-07:35:
    Terminal presentation-state copy is user-visible terminal copy, even though this source helper still represents the current shell body. Sleeping and mounting messages must remain source-only, non-launching, and private-detail-free while accurately describing pending wake/materialization state.
    */
    pub(crate) fn placeholder_message(self) -> &'static str {
        match self {
            Self::Running => "",
            Self::Sleeping => {
                "This tab stays sleeping when selected. Activate this terminal to enter wake startup pending until the runtime is ready."
            }
            Self::Mounting => {
                "Startup or materialization is pending until the terminal runtime and surface are ready. Clicking only focuses this pending terminal and does not mark it running."
            }
            Self::StartupFailed => {
                "The terminal runtime did not start. Retry keeps this tab in place and returns it to startup pending without storing private launch details."
            }
            Self::RestoredUnmounted => {
                "This restored tab remains in the workspace tree while its runtime surface is unmounted. Activate it to enter materialization pending."
            }
            Self::PoppedOutPlaceholder => {
                "The session is represented in this pane while its active surface belongs to a popped-out window. Activate it to enter reattach pending."
            }
        }
    }

    pub(crate) fn placeholder_action_label(self) -> &'static str {
        match self {
            Self::Running => "",
            Self::Sleeping => "Wake",
            Self::Mounting => "Pending startup",
            Self::StartupFailed => "Retry",
            Self::RestoredUnmounted => "Materialize",
            Self::PoppedOutPlaceholder => "Reattach",
        }
    }
}

/*
CDXC:SessionStatus 2026-06-22-16:27:
Agents running-tab dots must use semantic placeholder state from the macOS sidebar vocabulary instead of arbitrary session-id colors. Persist only the safe shell metadata: idle/working/attention activity plus a Delayed Send boolean; never persist deadlines, labels, command text, paths, stdout/stderr, terminal content, tokens, or user-owned titles.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentTerminalActivity {
    Idle,
    Working,
    Attention,
}

impl Default for AgentTerminalActivity {
    fn default() -> Self {
        Self::Idle
    }
}

impl AgentTerminalActivity {
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
pub(crate) enum AgentTerminalTabStatus {
    Idle,
    Working,
    Attention,
    DelayedSend,
}

impl AgentTerminalTabStatus {
    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
            Self::DelayedSend => "delayed-send",
        }
    }
}

pub(crate) fn agent_terminal_tab_status(
    activity: AgentTerminalActivity,
    delayed_send_active: bool,
) -> AgentTerminalTabStatus {
    if delayed_send_active {
        AgentTerminalTabStatus::DelayedSend
    } else {
        match activity {
            AgentTerminalActivity::Idle => AgentTerminalTabStatus::Idle,
            AgentTerminalActivity::Working => AgentTerminalTabStatus::Working,
            AgentTerminalActivity::Attention => AgentTerminalTabStatus::Attention,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TerminalSession {
    pub(crate) id: TerminalSessionId,
    pub(crate) title: String,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) startup_eligible_when_mounting: bool,
    /*
    CDXC:SessionTitles 2026-08-04:
    The generating-title flag belongs to the reconciled runtime tab session
    that owns the visible pane. Keeping it only in the separate sidebar focus
    snapshot made Windows render depend on a reverse project/session mapping
    that can be absent while a WSL terminal is attaching. This flag is
    presentation-only and must not enter shell-state persistence.
    */
    pub(crate) is_generating_first_prompt_title: bool,
    pub(crate) activity: AgentTerminalActivity,
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) kind: AgentsWorkspaceSessionKind,
    pub(crate) delayed_send_active: bool,
    /*
    CDXC:Zmx 2026-07-06:
    Runtime-only zmx persistence session name captured from gxserver attach
    metadata, mirroring macOS `session.sessionPersistenceName`. Terminal-content
    clicks use it for zmx's conditional grid-size refresh. Never persisted to
    shell-state JSON and never derived from titles, paths, or renderer text.
    */
    pub(crate) zmx_session_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentsWorkspaceSessionKind {
    #[default]
    Terminal,
}

impl AgentsWorkspaceSessionKind {
    pub(crate) fn from_sidebar_kind(kind: &str) -> Option<Self> {
        match kind {
            "agent" | "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    pub(crate) fn shell_state_slug(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
        }
    }
}

impl TerminalSession {
    pub(crate) fn placeholder(
        id: TerminalSessionId,
        title: String,
        presentation_state: TerminalSessionPresentationState,
    ) -> Self {
        Self {
            id,
            title,
            startup_eligible_when_mounting: presentation_state
                == TerminalSessionPresentationState::Mounting,
            presentation_state,
            is_generating_first_prompt_title: false,
            activity: AgentTerminalActivity::Idle,
            agent_icon: None,
            kind: AgentsWorkspaceSessionKind::Terminal,
            delayed_send_active: false,
            zmx_session_name: None,
        }
    }

    pub(crate) fn with_activity(mut self, activity: AgentTerminalActivity) -> Self {
        self.activity = activity;
        self
    }

    pub(crate) fn with_agent_icon(mut self, agent_icon: Option<&'static str>) -> Self {
        self.agent_icon = agent_icon;
        self
    }

    pub(crate) fn with_kind(mut self, kind: AgentsWorkspaceSessionKind) -> Self {
        self.kind = kind;
        self
    }

    pub(crate) fn with_delayed_send_active(mut self, delayed_send_active: bool) -> Self {
        self.delayed_send_active = delayed_send_active;
        self
    }

    pub(crate) fn can_enter_startup_pipeline(&self) -> bool {
        self.presentation_state == TerminalSessionPresentationState::Mounting
            && self.startup_eligible_when_mounting
    }

    pub(crate) fn set_presentation_state(
        &mut self,
        presentation_state: TerminalSessionPresentationState,
    ) {
        self.set_presentation_state_with_startup_eligibility(
            presentation_state,
            presentation_state == TerminalSessionPresentationState::Mounting,
        );
    }

    pub(crate) fn set_presentation_state_with_startup_eligibility(
        &mut self,
        presentation_state: TerminalSessionPresentationState,
        startup_eligible_when_mounting: bool,
    ) {
        /*
        CDXC:Terminal 2026-06-23-18:00:
        `Mounting` can mean a true new/retry/materialize startup or a wake/reattach placeholder waiting for an exact parked owner move. Keep that startup eligibility runtime-only and out of shell-state JSON so restored shell-state `mounting` after restart, sleeping wake, and popped-out reattach cannot accidentally create a duplicate Ghostty process, while new terminals, explicit restored-unmounted activation, and in-process failed-startup retry remain eligible.
        */
        self.presentation_state = presentation_state;
        self.startup_eligible_when_mounting = presentation_state
            == TerminalSessionPresentationState::Mounting
            && startup_eligible_when_mounting;
    }

    pub(crate) fn tab_status(&self) -> AgentTerminalTabStatus {
        agent_terminal_tab_status(self.activity, self.delayed_send_active)
    }
}
