//! The error texts a renderer command can answer with.

/// Why a renderer command failed. [`Self::message`] is the text the CLI prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererCommandError {
    InvalidTitle,
    InvalidSettingsPatch,
    InvalidSettingsTab,
    NoMatchingProject,
    /// CDXC:Sessions 2026-10-25 SEE-ALSO: `ghostex create-session --group-id` waits for a new session's sidebar row by matching this message's text (`SESSION_NOT_LISTED_MESSAGE` in server/src/ghostex_cli/session_groups.rs); change both together.
    NoMatchingSession,
    /// The app part that performs the command is not there (no runtime to focus through yet).
    BridgeUnavailable,
    Unsupported,
    /// Any failure the old runtime did not name, such as an address over the length limit.
    Failed,
    /// `ghostex edit --wait`: the app has no editor-closed signal to wait for.
    WaitUnsupported,
    /// `move-project` with a direction other than `up` or `down`.
    InvalidDirection,
    /// `ghostex open` / `edit` with no path.
    NoPaths,
    /// A group verb naming a group the project does not hold.
    NoMatchingSessionGroup,
    /// `create-session-group` in a project that already has as many groups as the sidebar allows.
    SessionGroupLimit,
    /// `move-session-to-group` into a group gxserver's copy cannot hold any more members of.
    SessionGroupFull,
    /// A group verb before the app has read its stored session groups.
    SessionGroupsLoading,
}

impl RendererCommandError {
    /// The text the CLI prints.
    ///
    /// The first seven are the old runtime's `safeGpuiRendererCommandErrorMessage` list; anything
    /// it did not list (the URL length check among them) was answered as "Renderer command failed."
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidTitle => "Invalid renderer command title.",
            Self::InvalidSettingsPatch => "Invalid settings patch.",
            Self::InvalidSettingsTab => "Invalid settings tab.",
            Self::NoMatchingProject => "No matching project was found.",
            Self::NoMatchingSession => "No matching session was found.",
            Self::BridgeUnavailable => "Renderer command bridge unavailable.",
            Self::Unsupported => "Unsupported renderer command.",
            Self::Failed => "Renderer command failed.",
            Self::WaitUnsupported => {
                "Ghostex cannot wait for a file to close; run the command without --wait."
            }
            Self::InvalidDirection => "The direction must be up or down.",
            Self::NoPaths => "No path to open was given.",
            Self::NoMatchingSessionGroup => "No matching session group was found.",
            Self::SessionGroupLimit => "Group limit reached for this project.",
            Self::SessionGroupFull => "That session group is full.",
            Self::SessionGroupsLoading => {
                "Ghostex is still loading its session groups; try again in a moment."
            }
        }
    }
}
