//! Which actions a sidebar row offers.
//!
//! SEE-ALSO: packages/core-ui/session-card-capabilities.ts, the one resolver both the React card
//! menu and the native row menu read, so the two menus cannot disagree. Every rule below is that
//! file's, including the CDXC decisions it records (a draft hides Fork and Full Reload; Sleep
//! Below targets running rows only; a remote row must opt in to the host-timer actions).

use crate::sidebar_view::view::SessionRow;

use super::text::{js_trim, transcript_agent};

/// The per-row answers the session menu and the hover strip are built from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionCapabilities {
    pub(crate) can_close_after_done: bool,
    pub(crate) can_copy_attach_command: bool,
    pub(crate) can_copy_resume_command: bool,
    pub(crate) can_delayed_send: bool,
    pub(crate) can_export_transcript: bool,
    pub(crate) can_fork_session: bool,
    pub(crate) can_full_reload_session: bool,
    pub(crate) can_generate_session_title: bool,
    pub(crate) can_open_session_note: bool,
    pub(crate) can_pin_session: bool,
    pub(crate) can_rename_session: bool,
    pub(crate) can_sleep_session: bool,
    pub(crate) can_split_session_right: bool,
    pub(crate) can_tag_session: bool,
    pub(crate) is_browser_session: bool,
}

impl SessionCapabilities {
    /// `getSidebarSessionContextMenuEligibility`. `is_remote_session` is the group's remote machine
    /// context; `workspace_focus_bridge` is the host's `postWorkspaceTerminalFocus`, which the web
    /// app does not have.
    ///
    /// CDXC:ContextMenus 2026-09-26 DECISION:
    /// User: Copy Resume and Copy Attach appear whenever Show debug UI controls is on; the separate Show command copy actions switch is gone. They stay hidden otherwise because they expose raw shell commands.
    pub(crate) fn resolve(
        row: &SessionRow,
        is_remote_session: bool,
        debugging_mode: bool,
        workspace_focus_bridge: bool,
    ) -> Self {
        let facts = &row.menu_facts;
        let is_browser = row.is_browser;
        // A `SessionRow` only ever exists for a row the list draws, so `hasSession` and
        // `isConcreteSessionRow` (which excludes the project list's More row, a thing this list
        // does not have) are both true here.
        let terminal_action = !is_browser;
        Self {
            can_close_after_done: terminal_action
                && (!is_remote_session || facts.can_toggle_close_after_done),
            can_copy_attach_command: debugging_mode
                && terminal_action
                && facts
                    .session_persistence_provider
                    .as_deref()
                    .is_some_and(|provider| !provider.is_empty())
                && facts
                    .session_persistence_name
                    .as_deref()
                    .is_some_and(|name| !name.is_empty()),
            can_copy_resume_command: debugging_mode
                && terminal_action
                && supports_resume_command_copy(row),
            can_delayed_send: terminal_action
                && (!is_remote_session || facts.can_schedule_delayed_send),
            can_export_transcript: terminal_action
                && !row.is_draft
                && transcript_agent(facts.agent_name.as_deref(), row.agent_icon.as_deref())
                    .is_some(),
            can_fork_session: terminal_action && !row.is_draft && supports_fork(row),
            can_split_session_right: terminal_action && !row.is_draft && workspace_focus_bridge,
            can_full_reload_session: terminal_action
                && !row.is_draft
                && supports_full_reload_menu_action(row, is_remote_session),
            can_generate_session_title: terminal_action
                && supports_generated_name(row)
                && facts
                    .first_user_message
                    .as_deref()
                    .is_some_and(|message| !js_trim(message).is_empty()),
            can_open_session_note: terminal_action
                && facts
                    .agent_session_id
                    .as_deref()
                    .is_some_and(|id| !js_trim(id).is_empty()),
            can_pin_session: true,
            can_rename_session: terminal_action,
            // `canSleepSidebarSession || canWakeSidebarSession`: a row that is neither running nor
            // sleeping (a stopped one kept visible by a pin or a tag) offers neither.
            can_sleep_session: row.lifecycle_state == "running"
                || row.lifecycle_state == "sleeping",
            can_tag_session: terminal_action,
            is_browser_session: is_browser,
        }
    }
}

/// `canSleepSidebarSession`.
pub(crate) fn can_sleep(row: &SessionRow) -> bool {
    row.lifecycle_state == "running"
}

/// `canWakeSidebarSession`.
pub(crate) fn can_wake(row: &SessionRow) -> bool {
    row.lifecycle_state == "sleeping"
}

/// `supportsResumeCommandCopy`.
fn supports_resume_command_copy(row: &SessionRow) -> bool {
    let icon = row.agent_icon.as_deref().unwrap_or_default();
    matches!(
        icon,
        "codex"
            | "claude"
            | "copilot"
            | "gemini"
            | "opencode"
            | "pi"
            | "cursor-cli"
            | "antigravity-cli"
    ) || (row.menu_facts.agent_name.as_deref() == Some("zcode")
        && row
            .menu_facts
            .agent_session_id
            .as_deref()
            .is_some_and(|id| !id.is_empty()))
}

/// `supportsFork`.
fn supports_fork(row: &SessionRow) -> bool {
    matches!(
        row.agent_icon.as_deref().unwrap_or_default(),
        "codex" | "claude" | "pi"
    )
}

/// `supportsGeneratedName`.
fn supports_generated_name(row: &SessionRow) -> bool {
    matches!(
        row.agent_icon.as_deref().unwrap_or_default(),
        "codex" | "claude" | "pi" | "antigravity-cli"
    )
}

/// `supportsFullReloadMenuAction`: a remote row reloads only when it is a terminal row.
fn supports_full_reload_menu_action(row: &SessionRow, is_remote_session: bool) -> bool {
    if is_remote_session {
        return row.session_kind.as_deref() == Some("terminal");
    }
    supports_full_reload(row)
}

/// `supportsFullReload`.
pub(crate) fn supports_full_reload(row: &SessionRow) -> bool {
    let icon = row.agent_icon.as_deref().unwrap_or_default();
    if icon == "antigravity-cli" {
        return row
            .menu_facts
            .agent_session_id
            .as_deref()
            .is_some_and(|id| !js_trim(id).is_empty());
    }
    matches!(icon, "codex" | "claude" | "opencode" | "pi" | "cursor-cli")
}
