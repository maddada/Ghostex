use super::*;

/*
CDXC:Terminal 2026-06-27-02:27:
Programmatic Return delivery is allowed only through exact mounted Ghostty surfaces that already passed their target-specific owner checks. Reuse the native macOS Return key tuple for command Delayed Send and mapped Agents rename commands instead of writing newline text or using the currently focused terminal as fallback.
*/
pub(crate) const GPUI_TERMINAL_RETURN_KEYCODE: u32 = 36;

pub(crate) const GPUI_TERMINAL_KEYPAD_ENTER_KEYCODE: u32 = 76;

pub(crate) const GPUI_TERMINAL_RETURN_TEXT: &str = "\r";

pub(crate) const GPUI_TERMINAL_RETURN_UNSHIFTED_CODEPOINT: u32 = 13;

pub(crate) const COMMAND_PANE_DELAYED_SEND_RETURN_KEYCODE: u32 = GPUI_TERMINAL_RETURN_KEYCODE;

pub(crate) const COMMAND_PANE_DELAYED_SEND_RETURN_TEXT: &str = GPUI_TERMINAL_RETURN_TEXT;

pub(crate) const COMMAND_PANE_DELAYED_SEND_RETURN_UNSHIFTED_CODEPOINT: u32 =
    GPUI_TERMINAL_RETURN_UNSHIFTED_CODEPOINT;

pub(crate) const COMMAND_PANE_GHOSTTY_KEY_ACTION_PRESS: ghostty_kit::ffi::ghostty_input_action_e =
    1;

pub(crate) const COMMAND_PANE_CLOSE_AFTER_DONE_DELAY: Duration = Duration::from_secs(3 * 60);

/*
CDXC:FocusMode 2026-06-25-14:56:
The GPUI command pane should honor the shared native default for Sleep Focused Session. GPUI key strings use `alt` for macOS Option, so keep this constant aligned with the shared `alt+shift+s` default.
*/
pub(crate) const SLEEP_FOCUSED_SESSION_DEFAULT_KEY: &str = "alt-shift-s";

pub(crate) const PROJECT_EDITOR_COMPANION_WIDTH_RATIO: f32 = 0.32;

pub(crate) const PROJECT_EDITOR_COMPANION_MIN_WIDTH: f32 = 280.0;

pub(crate) const PROJECT_EDITOR_COMPANION_SPLIT_RATIO: f32 = 0.5;

pub(crate) const PROJECT_EDITOR_AWAKE_MODE_CAP: usize = 3;

pub(crate) const PROJECT_EDITOR_AUTO_SLEEP_POLICY_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) const GPUI_NATIVE_TITLEBAR_TIPS: &[GpuiNativeTitlebarTip] = &[
    GpuiNativeTitlebarTip {
        body: "Search for project actions, pane splits and moves, session controls, settings shortcuts, and other Ghostex actions.",
        icon_path: COMMAND_ICON_COMMAND,
        id: "command-palette-all-actions",
        title: "Press Cmd Shift P anywhere to open Ghostex Quick Access",
    },
    GpuiNativeTitlebarTip {
        body: "Open Settings to customize sidebar presets, visible details, agents, actions, project tools, and workspace open targets.",
        icon_path: TITLEBAR_ICON_LAYOUT_SIDEBAR_LEFT_EXPAND,
        id: "customize-sidebar-layout-and-tools",
        title: "Customize the sidebar",
    },
    GpuiNativeTitlebarTip {
        body: "The Resources menu can sleep inactive terminal sessions while keeping them restorable in the sidebar.",
        icon_path: COMMAND_ICON_MOON,
        id: "sleep-idle-sessions-from-resources",
        title: "Sleep idle sessions from Resources",
    },
    GpuiNativeTitlebarTip {
        body: "Click Add Worktree on a project header so a second agent can work on a branch without touching the main checkout.",
        icon_path: TITLEBAR_ICON_LAYOUT_SIDEBAR_LEFT_EXPAND,
        id: "run-same-project-in-a-worktree",
        title: "Run the same project in a worktree",
    },
    GpuiNativeTitlebarTip {
        body: "Configure Ghostex Computer Use in Settings, then ask agents to use /ghostex-computer-use for native macOS app control.",
        icon_path: TITLEBAR_ICON_DEVICE_DESKTOP,
        id: "use-ghostex-computer-use-skill",
        title: "Use /ghostex-computer-use for desktop control",
    },
    GpuiNativeTitlebarTip {
        body: "Configure Ghostex Browser Use in Settings, then ask agents to use /ghostex-browser-use for supported external browser pages through Cua Driver.",
        icon_path: BROWSER_ICON_WORLD,
        id: "use-ghostex-browser-use-skill",
        title: "Use /ghostex-browser-use for browser pages",
    },
    GpuiNativeTitlebarTip {
        body: "Configure Ghostex Embedded Browser Use in Settings, then ask agents to use /ghostex-embedded-browser-use for page inspection, console logs, screenshots, and clicks in Ghostex panes.",
        icon_path: BROWSER_ICON_WORLD,
        id: "use-ghostex-embedded-browser-use-skill",
        title: "Use /ghostex-embedded-browser-use for Ghostex panes",
    },
    GpuiNativeTitlebarTip {
        body: "Open the Automate tab to run agents on a schedule without sitting in the session.",
        icon_path: COMMAND_ICON_COMMAND,
        id: "schedule-recurring-agent-work",
        title: "Schedule recurring agent work",
    },
    GpuiNativeTitlebarTip {
        body: "Open More Options in the top right of the sidebar, click \"Mobile\", then attach the Mobile app to a running agent session.",
        icon_path: TITLEBAR_ICON_DEVICE_DESKTOP,
        id: "continue-session-from-mobile-app",
        title: "Continue a session from the Mobile app",
    },
    GpuiNativeTitlebarTip {
        body: "Open More Options in the top right of the sidebar, click \"Search by Prompt\", then type any words you remember from the prompt.",
        icon_path: BROWSER_ICON_SEARCH,
        id: "find-session-by-prompt-text",
        title: "Find any session from prompt text",
    },
    GpuiNativeTitlebarTip {
        body: "In Search by Prompt, favorite a prompt so it stays at the top the next time you search.",
        icon_path: BROWSER_ICON_SEARCH,
        id: "star-prompts-you-want-again",
        title: "Star prompts you want again",
    },
    GpuiNativeTitlebarTip {
        body: "Then you can easily ask agents to \"work on beads with high priority from the kanban board\"",
        icon_path: COMMAND_ICON_COMMAND,
        id: "add-todos-to-kanban-page",
        title: "Add all your Todos in the Kanban page",
    },
];

/*
CDXC:Navigation 2026-07-29:
Rapid sidebar clicking across projects used to stack one complete project
switch per click. Each switch parks the outgoing Agents model and destroys the
whole process-local runtime graph (terminal runtimes, Ghostty/engine surfaces,
command pane, browser surfaces), and every superseded switch also
throws away the attach round trip it had already started. Project switches are
therefore coalesced leading-edge + trailing-debounce: the first request runs
immediately so a single click never gets slower, and requests that arrive while
that switch is still settling collapse into one trailing replay of the latest
authoritative request per bridge kind. Requests targeting the project that is
already active are never coalesced; those are intra-project session focus
changes and stay instant.
*/
pub(crate) const GPUI_PROJECT_SWITCH_SETTLE_WINDOW: Duration = Duration::from_millis(350);

pub(crate) const WORKSPACE_RENAME_COMMAND_MOUNT_RETRY_LIMIT: usize = 80;

pub(crate) const WORKSPACE_RENAME_COMMAND_MOUNT_RETRY_INTERVAL: Duration =
    Duration::from_millis(100);

// macOS AUTO_SUBMIT_STAGED_RENAME_DELAY_MS parity (native/sidebar/native-sidebar.tsx).
pub(crate) const WORKSPACE_RENAME_COMMAND_SUBMIT_DELAY: Duration = Duration::from_millis(1_000);

/*
CDXC:SessionTitles 2026-08-26:
gxserver's measured clear-burst law, mirrored for the local rename command
(`build_agent_tui_clear_input` in server/src/session_chat_send.rs): kill toward
the start (Ctrl+U) 2 * (lines + slack) - 1 times, then the same count toward the
end (Ctrl+K). One Ctrl+U kills exactly ONE logical line, which is why the single
kill this path used to send left a multi-line draft in the composer with the
rename glued onto its remains. The rename command is one logical line, so with
gxserver's 8-line slack the count is 2 * (1 + 8) - 1 = 17 — the same overshoot
bias gxserver takes, because the draft's real line count is unknowable from
here.
*/
pub(crate) const WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS: usize = 17;

/// Ctrl+U — kill toward the start of the composer line.
pub(crate) const AGENT_TUI_CLEAR_INPUT_LINE: &str = "\u{15}";

/// Ctrl+K — kill toward the end of the composer line.
pub(crate) const AGENT_TUI_CLEAR_INPUT_FORWARD: &str = "\u{b}";

/*
CDXC:Workarea 2026-06-29-00:02:
Source, Kanban, Automate, and Manage no longer keep sidebar readiness/proof stores beside the direct runtime gates. Source placeholder copy comes from the app-owned code-server launch state, while real Source/Kanban/Automate/Manage replacement is authorized only by `project_workarea_runtime_url_for_slot` plus an owned normal-layout CEF surface.

CDXC:Workarea 2026-06-29-00:15:
Owned Source/Kanban/Automate/Manage CEF surfaces must also match the current direct runtime URL identity before reuse or visibility. A valid URL for a different active project is not authority to keep a stale slot-owned surface alive.
*/
pub(crate) const SOURCE_CODE_SERVER_EDITOR_HOST: &str = "127.0.0.1";

/*
CDXC:CodeEditor 2026-06-28-04:05:
GPUI Source must not bind the macOS app's 3775 listener or share its code-server
profile. The macOS header click lag was caused by a GPUI-owned 3775 listener, so
GPUI owns a separate localhost port and storage name while keeping all project
URLs derived from the strict in-memory sidebar snapshot.
*/
pub(crate) const SOURCE_CODE_SERVER_EDITOR_PORT: u16 = 3777;

pub(crate) const SOURCE_CODE_SERVER_COMPONENT_NAME: &str = "code-server";

pub(crate) const SOURCE_CODE_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "codeViewTabHidden";

pub(crate) const BROWSER_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "browserViewTabHidden";

pub(crate) const KANBAN_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "kanbanViewTabHidden";

pub(crate) const AUTOMATE_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "automateViewTabHidden";

pub(crate) const DOCS_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "docsViewTabHidden";

pub(crate) const TIPS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "tipsAndTricksTitlebarButtonHidden";

pub(crate) const NOTIFICATIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "notificationsTitlebarButtonHidden";

pub(crate) const HELP_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str = "helpTitlebarButtonHidden";

pub(crate) const RESOURCES_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "resourcesTitlebarButtonHidden";

pub(crate) const DEV_SERVERS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "devServersTitlebarButtonHidden";

pub(crate) const EXTENSIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "extensionsTitlebarButtonHidden";

pub(crate) const GIT_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "gitActionsTitlebarButtonHidden";

pub(crate) const QUICK_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "quickActionsTitlebarButtonHidden";

pub(crate) const OPEN_IN_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str = "openInTitlebarButtonHidden";

pub(crate) const SOURCE_CODE_SERVER_INSTALL_PROMPT: &str = "The VS Code IDE component is a 150mb optional install (one-time).\nWould you like to install it?";

/// CDXC:CodeEditor 2026-09-13 SEE-ALSO:
/// Match .dependencies/code-server/.node-version and its package.json engines: on-demand installation and Windows/WSL launches validate the bundled runtime against this major.
pub(crate) const SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR: u64 = 24;

pub(crate) const SOURCE_CODE_SERVER_LOADING_PLACEHOLDER_DELAY: Duration = Duration::from_secs(3);

pub(crate) const SOURCE_CODE_SERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(7);

pub(crate) const SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) const SOURCE_CODE_SERVER_HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub(crate) const SOURCE_CODE_SERVER_REMOTE_PORT: u16 = 3777;

pub(crate) const SOURCE_CODE_SERVER_TUNNEL_PORT_MIN: u16 = 43_000;

pub(crate) const SOURCE_CODE_SERVER_TUNNEL_PORT_MAX: u16 = 43_999;

pub(crate) const SOURCE_CODE_SERVER_TUNNEL_ATTEMPTS: usize = 24;

pub(crate) const COMMAND_PANE_GROUP_FOCUSED_BORDER_WIDTH: u8 = 1;

pub(crate) const COMMAND_PANE_GROUP_INACTIVE_BORDER_WIDTH: u8 = 2;
