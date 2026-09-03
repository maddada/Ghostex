// C1 wave-3 extraction: the free-standing consts/statics (theme, geometry, and protocol tables) moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

pub(crate) const DEFAULT_BROWSER_URL: &str = "https://www.google.com";

pub(crate) const GPUI_WORKSPACE_SHELL_STATE_VERSION: u64 = 1;

pub(crate) static GPUI_APP_QUIT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// The daemon holds `/api/handoffSessionChatDraft` for up to 16s while the
/// agent CLI answers the Ctrl+G handshake; leave room for the round trip (and
/// an SSH tunnel) on top of that.
pub(crate) const GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT: Duration = Duration::from_secs(30);

/*
CDXC:SessionChat 2026-08-21:
The terminal view's "Queued: N" chip needs the queue size for the handful of
sessions actually on screen in terminal mode. gxserver publishes the same count
as `queuedPromptCount` on its presentation snapshot, which the sidebar runtime
already receives; when that field reaches Rust through the focus-state tab
sessions this read should be deleted rather than kept beside it. Until then the
poll stays deliberately narrow: Agents mode only, visible panes only, the active
tab only, chat-capable sessions only — never a per-session sweep of the project.
*/
pub(crate) const GPUI_SESSION_CHAT_QUEUE_COUNT_POLL_INTERVAL: Duration = Duration::from_secs(3);

pub(crate) const GPUI_SESSION_CHAT_QUEUE_COUNT_TIMEOUT: Duration = Duration::from_secs(10);

/*
CDXC:SessionChat 2026-08-24:
Every chat-mode session owns a full Chromium page, and those pages stay alive
behind whatever pane is on screen. A workspace with many chat sessions therefore
pays renderer RAM for surfaces nobody has looked at in a long time. A surface
that has been continuously hidden this long is destroyed; the very next
visibility reconcile rebuilds it through `ensure_agents_chat_surface`, and the
page restores its transcript from gxserver and its draft from storage, so the
eviction is invisible apart from a reload. The window is deliberately long
because the rebuild costs a page load the user waits on.
*/
pub(crate) const GPUI_AGENTS_CHAT_SURFACE_HIDDEN_EVICT_AFTER: Duration =
    Duration::from_secs(20 * 60);

pub(crate) const GPUI_AGENTS_CHAT_SURFACE_EVICT_POLL_INTERVAL: Duration = Duration::from_secs(60);

#[cfg(target_os = "linux")]
pub(crate) static GPUI_LINUX_WINDOW_ICON: std::sync::OnceLock<Arc<image::RgbaImage>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "macos")]
pub(crate) static GPUI_KEYBOARD_ROUTER_NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) static GPUI_GHOSTTY_WORKSPACE_BACKGROUND_RGB: AtomicU64 = AtomicU64::new(0x050505);

pub(crate) static GPUI_WORKSPACE_BACKGROUND_RGB: AtomicU64 = AtomicU64::new(0x050505);

pub(crate) static GPUI_TITLEBAR_BACKGROUND_RGB: AtomicU64 = AtomicU64::new(0x0e0e0e);

pub(crate) static GPUI_TITLEBAR_GRADIENT_LEFT_RGB: AtomicU64 = AtomicU64::new(0x0e0e0e);

pub(crate) static GPUI_TITLEBAR_GRADIENT_RIGHT_RGB: AtomicU64 = AtomicU64::new(0x0e0e0e);

pub(crate) static GPUI_TITLEBAR_FOREGROUND_RGB: AtomicU64 = AtomicU64::new(0xffffff);

pub(crate) const DEFAULT_SIDEBAR_WIDTH: f32 = 235.0;

pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 150.0;

pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 520.0;

pub(crate) const SIDEBAR_RESET_WIDTH: f32 = 235.0;

/// The native resize rail between the sidebar column and the Agents
/// workspace. It is painted with the workspace background so it reads as a
/// 5px black gap, mirrored on both sidebar sides:
/// `window edge | sidebar page | 5px rail | workspace` on the left and the
/// reverse on the right. The sidebar page's own CSS supplies the 5px
/// sidebar-colored gutters on both of its edges, so the native column adds
/// no padding of its own.
pub(crate) const SIDEBAR_DIVIDER_WIDTH: f32 = 5.0;

pub(crate) const SIDEBAR_DIVIDER_LINE_WIDTH: f32 = 1.0;

pub(crate) const SIDEBAR_DIVIDER_HOVER_LINE_WIDTH: f32 = 3.0;

/* A chat file link routed to Docs waits this long for the Manage surface. */
pub(crate) const PENDING_DOCS_FILE_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(120);

pub(crate) const PENDING_DOCS_FILE_OPEN_MAX_ATTEMPTS: u32 = 100;

/* A chat draft handed to the terminal waits this long for a paste-capable
terminal surface. After the attempts run out the record stays parked, where
the next focus handoff or a return to chat picks it up. */
pub(crate) const PENDING_TERMINAL_COMPOSER_INSERT_RETRY_INTERVAL: Duration =
    Duration::from_millis(120);

pub(crate) const PENDING_TERMINAL_COMPOSER_INSERT_MAX_ATTEMPTS: u32 = 50;

pub(crate) const SIDEBAR_DIVIDER_HOVER_DELAY: Duration = Duration::from_millis(50);

pub(crate) const SIDEBAR_DIVIDER_HOVER_FADE_DURATION: Duration = Duration::from_millis(180);

pub(crate) const COMMAND_ACTION_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) const CEF_DARK_PREPAINT_BACKGROUND_COLOR: u32 = 0xFF0E0E0E;

pub(crate) const CEF_SESSION_CHAT_DARK_PREPAINT_BACKGROUND_COLOR: u32 = 0xFF0D0D0D;

/* Find keeps the older near-black its own page paints; only the chat surface
moved. */
pub(crate) const CEF_FIND_PROMPTS_DARK_PREPAINT_BACKGROUND_COLOR: u32 = 0xFF111111;

pub(crate) const CEF_LIGHT_PREPAINT_BACKGROUND_COLOR: u32 = 0xFFFDFDFD;

/// Matches `ghostexEditorProtocolVersion` in apps/editor/macos DaemonSupport.swift.
pub(crate) const GHOSTEX_EDITOR_PROTOCOL_VERSION: u64 = 1;

pub(crate) const GHOSTEX_EDITOR_DAEMON_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) const WORKSPACE_MIN_WIDTH: f32 = 240.0;

pub(crate) const CEF_KEY_CONTEXT: &str = "GhostexGpuiCef";

pub(crate) const TITLEBAR_DROPDOWN_KEY_CONTEXT: &str = "GhostexGpuiTitlebarDropdown";

pub(crate) const GHOSTTY_MOUSE_ZERO_MODS: ghostty_kit::ffi::ghostty_input_mods_e = 0;

pub(crate) const GHOSTTY_MOUSE_SHIFT_MOD: ghostty_kit::ffi::ghostty_input_mods_e = 1;

pub(crate) const GHOSTTY_MOUSE_CTRL_MOD: ghostty_kit::ffi::ghostty_input_mods_e = 2;

pub(crate) const GHOSTTY_MOUSE_ALT_MOD: ghostty_kit::ffi::ghostty_input_mods_e = 4;

pub(crate) const GHOSTTY_MOUSE_SUPER_MOD: ghostty_kit::ffi::ghostty_input_mods_e = 8;

pub(crate) const GHOSTTY_TERMINAL_OBSERVED_SHORTCUT_MODS: ghostty_kit::ffi::ghostty_input_mods_e =
    GHOSTTY_MOUSE_SHIFT_MOD
        | GHOSTTY_MOUSE_CTRL_MOD
        | GHOSTTY_MOUSE_ALT_MOD
        | GHOSTTY_MOUSE_SUPER_MOD;

pub(crate) const GPUI_TERMINAL_ESCAPE_KEYCODE: u32 = 53;

pub(crate) const GPUI_TERMINAL_GHOSTTY_KEY_ACTION_PRESS: ghostty_kit::ffi::ghostty_input_action_e =
    1;

pub(crate) const GPUI_TERMINAL_GHOSTTY_KEY_ACTION_REPEAT: ghostty_kit::ffi::ghostty_input_action_e =
    2;

pub(crate) const GHOSTTY_SCROLL_PRECISION_MOD: ghostty_kit::ffi::ghostty_input_scroll_mods_t = 1;

pub(crate) const GHOSTTY_MOUSE_PRESSURE_STAGE_NONE: u32 = 0;

pub(crate) const GHOSTTY_MOUSE_PRESSURE_STAGE_NORMAL: u32 = 1;

pub(crate) const GHOSTTY_MOUSE_PRESSURE_STAGE_DEEP: u32 = 2;

pub(crate) const TITLEBAR_HEIGHT: f32 = 28.0;

pub(crate) const TITLEBAR_CONTROL_HEIGHT: f32 = TITLEBAR_HEIGHT - 1.0;

#[cfg(target_os = "macos")]
pub(crate) const TITLEBAR_PROJECT_LEFT: f32 = 88.0;

#[cfg(not(target_os = "macos"))]
pub(crate) const TITLEBAR_PROJECT_LEFT: f32 = 9.0;

pub(crate) const TITLEBAR_PROJECT_CONTEXT_DISABLED_REASON: &str =
    "Switch to a project to access this view";

/*
CDXC:Titlebar 2026-08-20:
Source (Code) has no working remote runtime yet: code-server is launched on this
machine against a local path, so a machine-scoped remote project would open the
wrong tree. Disable the tab for remote projects through the same availability
contract the Quick/projectless reasons use, so the tab click, the compact mode
menu, hotkeys, and restored-mode coercion all refuse it in one place.
*/
pub(crate) const TITLEBAR_REMOTE_SOURCE_DISABLED_REASON: &str =
    "Code is currently disabled for remote projects";

#[cfg(target_os = "macos")]
pub(crate) const TITLEBAR_COMPACT_MODE_WIDTH_THRESHOLD: f32 = 1050.0;

#[cfg(not(target_os = "macos"))]
pub(crate) const TITLEBAR_COMPACT_MODE_WIDTH_THRESHOLD: f32 = 1330.0;

pub(crate) const TITLEBAR_BUTTON_WIDTH: f32 = 42.0;

pub(crate) const TITLEBAR_TOOLTIP_HEIGHT: f32 = 20.0;

pub(crate) const TITLEBAR_TOOLTIP_TEXT_SIZE: f32 = 12.0;

pub(crate) const TITLEBAR_TOOLTIP_LINE_HEIGHT: f32 = 18.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) const TITLEBAR_WINDOW_BUTTON_WIDTH: f32 = 46.0;

pub(crate) const TITLEBAR_SETTINGS_BUTTON_WIDTH: f32 = 45.0;

pub(crate) const TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH: f32 = 556.0;

pub(crate) const TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT: f32 = 650.0;

pub(crate) const TITLEBAR_PROJECT_LABEL_FALLBACK: &str = "Ghostex";

pub(crate) const GPUI_PROJECT_IS_QUICK_ENV: &str = "GHOSTEX_GPUI_PROJECT_IS_QUICK";

pub(crate) const GPUI_SIDEBAR_PROJECT_CONTEXT_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_PROJECT_CONTEXT_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.activeProjectContext";

pub(crate) const GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.nativeProjectPathAction";

pub(crate) const GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.titlebarGitMenuState";

pub(crate) const GPUI_TITLEBAR_GIT_MENU_ROW_LABEL_MAX_CHARS: usize = 80;

pub(crate) const GPUI_TITLEBAR_GIT_MENU_BRANCH_MAX_CHARS: usize = 200;

pub(crate) const GPUI_TITLEBAR_GIT_MENU_MAX_ROWS: usize = 16;

pub(crate) const GPUI_SIDEBAR_TITLEBAR_GIT_ACTION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_TITLEBAR_GIT_ACTION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.titlebarGitAction";

pub(crate) const GPUI_TITLEBAR_GIT_ACTION_REFRESH_SELECTOR: &str = "refresh";

pub(crate) const GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.commandAction";

pub(crate) const GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.commandRunEnd";

pub(crate) const GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.gxserverPresentationFocusState";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalFocus";

pub(crate) const GPUI_SIDEBAR_CREATE_PROJECT_AGENT_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_CREATE_PROJECT_AGENT_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.createProjectAgent";

pub(crate) const GPUI_SIDEBAR_CREATE_PROJECT_TERMINAL_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_CREATE_PROJECT_TERMINAL_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.createProjectTerminal";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalRenameCommand";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_TITLE_MAX_CHARS: usize = 120;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ENTER_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ENTER_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalEnter";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalLifecycleRequest";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalLifecycleResult";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_ID_MAX: u64 =
    9_007_199_254_740_991;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalBell";

#[cfg(target_os = "windows")]
pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_VERSION: u64 = 1;

#[cfg(target_os = "windows")]
pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalTitleChanged";

#[cfg(target_os = "windows")]
pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS: usize = 512;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalEscapePressed";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTerminalRuntimeAction";

pub(crate) const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.sessionCompletionSound";

pub(crate) const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MAX_CHARS: usize = 64;

pub(crate) const GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.openBrowserUrl";

pub(crate) const GPUI_SIDEBAR_BROWSER_TAB_FOCUS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.browserTabFocus";

pub(crate) const GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS: usize = 16 * 1024;

pub(crate) const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.projectBoardConversationRequest";

pub(crate) const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.projectBoardConversationResponse";

pub(crate) const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_PAYLOAD_MAX_CHARS: usize = 256 * 1024;

pub(crate) const GPUI_PROJECT_BOARD_INITIALIZE_BEADS_ACTION: &str = "initializeBeads";

pub(crate) const GPUI_PROJECT_BOARD_INITIALIZE_BEADS_COMMAND_ID: &str =
    "ghostex.gpui.projectBoard.initializeBeads";

pub(crate) const GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_ACTION: &str = "installOrUpdateBeads";

pub(crate) const GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_COMMAND_ID: &str =
    "ghostex.gpui.projectBoard.installOrUpdateBeads";

pub(crate) const GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION: &str = "runBeadsMigration";

pub(crate) const GPUI_PROJECT_BOARD_MIGRATE_BEADS_COMMAND_ID: &str =
    "ghostex.gpui.projectBoard.migrateBeads";

pub(crate) const GPUI_PROJECT_BOARD_ADOPT_BEADS_COMMAND_ID: &str =
    "ghostex.gpui.projectBoard.adoptBeads";

pub(crate) const GPUI_PROJECT_BOARD_ADOPT_BEADS_FAST_FORWARD_COMMAND_ID: &str =
    "ghostex.gpui.projectBoard.adoptBeadsFastForward";

pub(crate) const GPUI_PROJECT_BOARD_RECONCILE_BEADS_FORK_COMMAND_ID: &str =
    "ghostex.gpui.projectBoard.reconcileBeadsFork";

pub(crate) const GPUI_PROJECT_BOARD_COMMAND_COMPLETED_EVENT: &str =
    "ghostex-project-board-command-completed";

pub(crate) const GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.sessionStatusIndicators";

pub(crate) const GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.petOverlayState";

pub(crate) const GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.globalActions";

/*
CDXC:AgentLauncher 2026-08-01-16:00:
The tab strip draws at most this many Global Actions. The cap keeps the action
cluster from crowding out the tabs themselves on a narrow pane, and it bounds
the bridge payload the same way the status-indicator bridge bounds its rows.
Actions past the cap stay runnable from Settings and Ghostex Quick Access.
*/
pub(crate) const GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS: usize = 8;

pub(crate) const GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.statusPetActivation";

pub(crate) const GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.menuBarProjectActivation";

pub(crate) const GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.menuBarSessionActivation";

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceTabSessionSelected";

pub(crate) const GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.commandPaletteSessionFocus";

pub(crate) const GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.stashedPromptSessionJump";

pub(crate) const GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.commandPaletteRunSidebarCommand";

pub(crate) const GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.nativeAppShotCaptured";

pub(crate) const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.nativeAppShotPrompt";

pub(crate) const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION: u64 = 1;

pub(crate) const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.nativeAppShotPromptResult";

pub(crate) const GPUI_SIDEBAR_REMOTE_EVENT_NAME: &str = "ghostex-gpui-sidebar-remote-event";

pub(crate) const GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS: usize = 512;

pub(crate) const GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS: usize = 4096;

pub(crate) const GPUI_TITLEBAR_SELECTION_PROJECT_LIMIT: usize = 256;

pub(crate) const GPUI_TITLEBAR_OPEN_TARGET_SELECTIONS_SETTINGS_KEY: &str =
    "gpuiTitlebarOpenTargetByProject";

pub(crate) const GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY: &str =
    "gpuiTitlebarActionCommandByProject";

pub(crate) const GPUI_NATIVE_APP_SHOT_PROMPT_MAX_CHARS: usize = 24 * 1024;

pub(crate) const GPUI_SIDEBAR_VISIBLE_SESSION_IDS_MAX: usize = 64;

pub(crate) const GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX: usize = 128;

pub(crate) const GPUI_STATUS_INDICATOR_MAX_PROJECTS: usize = 32;

pub(crate) const GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT: usize = 16;

pub(crate) const GPUI_STATUS_INDICATOR_MAX_ACTIVITIES: usize = 3;

pub(crate) const GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS: usize = 120;

pub(crate) const GPUI_STATUS_INDICATOR_ICON_DATA_URL_MAX_CHARS: usize = 700_000;

pub(crate) const GPUI_SESSION_ATTENTION_NOTIFICATION_SESSION_COOLDOWN: Duration =
    Duration::from_secs(20);

pub(crate) const GPUI_SESSION_ATTENTION_NOTIFICATION_GLOBAL_WINDOW: Duration =
    Duration::from_secs(60);

pub(crate) const GPUI_SESSION_ATTENTION_NOTIFICATION_GLOBAL_LIMIT: usize = 8;

pub(crate) const GPUI_STATUS_PET_STACK_RIGHT_INSET: f32 = 18.0;

pub(crate) const GPUI_STATUS_PET_STACK_BOTTOM_INSET: f32 = 18.0;

pub(crate) const GPUI_STATUS_PET_STACK_WIDTH: f32 = 320.0;

pub(crate) const GPUI_PET_OVERLAY_ACTIVITY_CARD_WIDTH: f32 = 266.0;

pub(crate) const GPUI_PET_OVERLAY_AVATAR_WIDTH: f32 = 84.0;

pub(crate) const GPUI_PET_OVERLAY_AVATAR_HEIGHT: f32 = 91.0;

pub(crate) const GPUI_PET_OVERLAY_SPRITESHEET_COLUMNS: f32 = 8.0;

pub(crate) const GPUI_PET_OVERLAY_SPRITESHEET_ROWS: f32 = 9.0;

pub(crate) const GPUI_PET_OVERLAY_STATUS_BADGE_HEIGHT: f32 = 24.0;

pub(crate) const GPUI_PET_OVERLAY_ANIMATION_TICK: Duration = Duration::from_millis(100);

pub(crate) const SIDEBAR_FOCUS_BORDER_HANDOFF_TIMEOUT: Duration = Duration::from_millis(350);

pub(crate) const GPUI_PET_OVERLAY_IDLE_SPEED_MULTIPLIER: u64 = 6;

pub(crate) const GPUI_LOCAL_APP_SHOT_SESSION_MAP_MAX: usize = 64;

pub(crate) const GPUI_REMOTE_MACHINE_ID_MAX_CHARS: usize = 80;

pub(crate) const TITLEBAR_ICON_INFO: &str = "titlebar/info-circle.svg";

pub(crate) const TITLEBAR_ICON_DEVICE_DESKTOP: &str = "titlebar/device-desktop.svg";

pub(crate) const TITLEBAR_ICON_EXTENSIONS: &str = "titlebar/puzzle.svg";

pub(crate) const TITLEBAR_ICON_GIT_COMMIT: &str = "titlebar/git-commit.svg";

pub(crate) const TITLEBAR_ICON_PLAYER_PLAY: &str = "titlebar/player-play.svg";

pub(crate) const TITLEBAR_ICON_FOLDER_OPEN: &str = "titlebar/folder-open.svg";

pub(crate) const TITLEBAR_ICON_VSCODE: &str = "titlebar/vscode.svg";

pub(crate) const TITLEBAR_ICON_CHEVRON_LEFT: &str = "titlebar/chevron-left.svg";

pub(crate) const TITLEBAR_ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_SIDEBAR: &str = "titlebar/layout-sidebar.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_SIDEBAR_LEFT_COLLAPSE: &str =
    "titlebar/layout-sidebar-left-collapse.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_SIDEBAR_LEFT_EXPAND: &str =
    "titlebar/layout-sidebar-left-expand.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_BOARD_SPLIT: &str = "titlebar/layout-board-split.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_SPLIT_VERTICAL: &str = "titlebar/layout-split-vertical.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_SINGLE_PANE: &str = "titlebar/layout-single-pane.svg";

pub(crate) const TITLEBAR_ICON_LAYOUT_SIDEBAR_RIGHT: &str = "titlebar/layout-sidebar-right.svg";

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) const TITLEBAR_ICON_WINDOW_MINIMIZE: &str = "titlebar/window-minimize.svg";

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) const TITLEBAR_ICON_WINDOW_MAXIMIZE: &str = "titlebar/window-maximize.svg";

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) const TITLEBAR_ICON_WINDOW_RESTORE: &str = "titlebar/window-restore.svg";

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) const TITLEBAR_ICON_WINDOW_CLOSE: &str = "titlebar/xmark.svg";

pub(crate) const TITLEBAR_ICON_SETTINGS: &str = "titlebar/settings.svg";

pub(crate) const TITLEBAR_ICON_BOX: &str = "titlebar/box.svg";

pub(crate) const TITLEBAR_ICON_CODE: &str = "titlebar/code.svg";

pub(crate) const TITLEBAR_ICON_GIT_COMPARE: &str = "titlebar/git-compare.svg";

pub(crate) const TITLEBAR_ICON_UPLOAD: &str = "titlebar/upload.svg";

pub(crate) const TITLEBAR_ICON_STACK_PUSH: &str = "titlebar/stack-push.svg";

pub(crate) const TITLEBAR_ICON_ROCKET: &str = "titlebar/rocket.svg";

pub(crate) const TITLEBAR_ICON_GIT_PULL_REQUEST: &str = "titlebar/git-pull-request.svg";

pub(crate) const TITLEBAR_ICON_DOWNLOAD: &str = "titlebar/download.svg";

pub(crate) const TITLEBAR_TIPS_READ_STORAGE_KEY: &str = "ghostex.titlebar.tips.readIds";

pub(crate) const TITLEBAR_TIP_IDS: &[&str] = &[
    "command-palette-all-actions",
    "customize-sidebar-layout-and-tools",
    "sleep-idle-sessions-from-resources",
    "run-same-project-in-a-worktree",
    "use-ghostex-computer-use-skill",
    "use-ghostex-browser-use-skill",
    "use-ghostex-embedded-browser-use-skill",
    "schedule-recurring-agent-work",
    "continue-session-from-mobile-app",
    "find-session-by-prompt-text",
    "star-prompts-you-want-again",
    "add-todos-to-kanban-page",
];

// Both platform backends probe quietly at launch and every 15 minutes. The
// titlebar is the shared availability surface; platform-specific UI is only
// entered after the user clicks it.
pub(crate) const GPUI_UPDATE_AVAILABILITY_PROBE_INTERVAL: Duration = Duration::from_secs(15 * 60);

pub(crate) const TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE: f32 = 18.0;

pub(crate) const TITLEBAR_SIDEBAR_COLLAPSE_ICON_LEFT_OFFSET: f32 = 1.5;

pub(crate) const TITLEBAR_SIDEBAR_COLLAPSE_ICON_TOP_OFFSET: f32 = 0.5;

pub(crate) const TITLEBAR_UPDATE_ICON_SIZE: f32 = 14.0;

pub(crate) const TITLEBAR_UPDATE_PROGRESS_RING_SIZE: f32 = 16.0;

pub(crate) const TITLEBAR_UPDATE_PROGRESS_RING_RADIUS: f32 = 5.5;

pub(crate) const TITLEBAR_UPDATE_PROGRESS_RING_STROKE: f32 = 2.0;

pub(crate) const FIND_BAR_HEIGHT: f32 = 35.0;

/// Height of the tab bar's "Queued: N" chip. Shorter than the tab bar itself so
/// the chip reads as an inset control and never drives the bar's own height.
pub(crate) const TERMINAL_QUEUED_PROMPTS_CHIP_HEIGHT: f32 = 20.0;

pub(crate) const FIND_BAR_NAV_BUTTON_WIDTH: f32 = 42.0;

pub(crate) const FIND_BAR_CLOSE_BUTTON_WIDTH: f32 = 41.0;

pub(crate) const BROWSER_TOOLBAR_HEIGHT: f32 = 35.0;

pub(crate) const BROWSER_TOOLBAR_BUTTON_WIDTH: f32 = TITLEBAR_BUTTON_WIDTH;

pub(crate) const BROWSER_TOOLBAR_BUTTON_ICON_SIZE: f32 = 16.0;

pub(crate) const BROWSER_TOOLBAR_HORIZONTAL_PADDING: f32 = 12.0;

pub(crate) const BROWSER_TOOLBAR_ITEM_GAP: f32 = 0.0;

pub(crate) const BROWSER_TOOLBAR_ADDRESS_GAP: f32 = 18.0;

pub(crate) const BROWSER_TOOLBAR_ADDRESS_RIGHT_GAP: f32 = 14.0;

pub(crate) const BROWSER_ADDRESS_MINIMUM_WIDTH: f32 = 180.0;

pub(crate) const BROWSER_ADDRESS_HEIGHT: f32 = 20.0;

pub(crate) const BROWSER_TAB_BAR_HEIGHT: f32 = 28.0;

pub(crate) const BROWSER_TAB_MAX_WIDTH: f32 = 175.0;

pub(crate) const BROWSER_TAB_MIN_WIDTH: f32 = 170.0;

pub(crate) const BROWSER_TAB_GAP: f32 = 2.0;

pub(crate) const BROWSER_TAB_ICON_SIZE: f32 = 14.0;

pub(crate) const BROWSER_FAVICON_HTTP_URL_MAX_CHARS: usize = 2048;

pub(crate) const BROWSER_FAVICON_HTTP_REDIRECT_LIMIT: u32 = 3;

pub(crate) const BROWSER_FAVICON_DATA_URL_MAX_CHARS: usize = 96 * 1024;

pub(crate) const BROWSER_FAVICON_IMAGE_MAX_BYTES: usize = 64 * 1024;

pub(crate) const BROWSER_FAVICON_IMAGE_MAX_DIMENSION: u32 = 512;

pub(crate) const BROWSER_FAVICON_IMAGE_MAX_PIXELS: u64 = 512 * 512;

pub(crate) const BROWSER_FAVICON_IMAGE_MAX_FRAMES: usize = 16;

pub(crate) const BROWSER_TAB_FAVICON_COLORS: [u32; 8] = [
    0x3ed2a4, 0xffc857, 0xff7a7a, 0x8ba7ff, 0xb877ff, 0x52d6ff, 0xf28cc6, 0x9bd45a,
];

pub(crate) const BROWSER_TAB_CLOSE_SIZE: f32 = 20.0;

pub(crate) const BROWSER_TAB_ACTION_BUTTON_SIZE: f32 = 42.0;

pub(crate) const BROWSER_TAB_ACTION_CLUSTER_WIDTH: f32 = 84.0;

pub(crate) const BROWSER_HISTORY_MAX_ENTRIES: usize = 50;

pub(crate) const BROWSER_HISTORY_MENU_MAX_ROWS: usize = 8;

pub(crate) const BROWSER_ADDRESS_ONLY_CEF_URL: &str = "about:blank";

pub(crate) const BROWSER_ZOOM_EPSILON: f64 = 0.001;

pub(crate) const TERMINAL_FONT_ZOOM_STEP: f32 = 1.0;

pub(crate) const BROWSER_PROFILE_DEFAULT_ID: u64 = 1;

pub(crate) const BROWSER_PROFILE_FIRST_GENERATED_ID: u64 = 2;

pub(crate) const BROWSER_PROFILE_MAX_PROFILES: usize = 32;

pub(crate) const BROWSER_PROFILE_DEFAULT_CEF_ID: &str = "default";

pub(crate) const APP_MODAL_HOST_CEF_PROFILE_ID: &str = "app-modal";

pub(crate) const APP_MODAL_HOST_ID: &str = "ghostex-gpui-app-modal-host";

/*
CDXC:Onboarding 2026-08-13:
YouTube rejects an iframe embedded by the file:// modal host because that
request cannot carry the HTTP referrer identity required by the player. Keep
the tutorial in the normal native child window, but load the watch page as its
top-level CEF document so playback follows the same working path as Browser.
*/
pub(crate) const GHOSTEX_TUTORIAL_VIDEO_URL: &str = "https://www.youtube.com/watch?v=APdP-j5n4Mw";

/// CDXC:Onboarding 2026-08-18: the watch page reports
/// main-frame load-end before its player has installed keyboard shortcuts, so
/// the host-side fullscreen key press waits this long after that edge.
pub(crate) const GPUI_TUTORIAL_VIDEO_FULLSCREEN_KEY_DELAY: Duration = Duration::from_millis(1500);

pub(crate) const TITLEBAR_TIPS_PANEL_CEF_PROFILE_ID: &str = "titlebar-tips-panel";

pub(crate) const TITLEBAR_TIPS_PANEL_ID: &str = "ghostex-gpui-titlebar-tips-panel";

pub(crate) const TITLEBAR_RESOURCES_PANEL_CEF_PROFILE_ID: &str = "titlebar-resources-panel";

pub(crate) const TITLEBAR_RESOURCES_PANEL_ID: &str = "ghostex-gpui-titlebar-resources-panel";

pub(crate) const TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH: f32 = 656.0;

pub(crate) const GPUI_TITLEBAR_NATIVE_PROCESS_REQUEST_ID_MAX_CHARS: usize = 256;

pub(crate) const GPUI_TITLEBAR_NATIVE_PROCESS_REJECTED_EXIT_CODE: i32 = 126;

pub(crate) const APP_MODAL_HOST_WINDOW_WIDTH: f32 = 1080.0;

pub(crate) const APP_MODAL_HOST_WINDOW_HEIGHT: f32 = 760.0;

pub(crate) const APP_MODAL_HOST_GIT_COMMIT_WINDOW_WIDTH: f32 = 1078.0;

pub(crate) const APP_MODAL_HOST_GIT_COMMIT_WINDOW_HEIGHT: f32 = 758.0;

pub(crate) const APP_MODAL_HOST_COMMAND_PALETTE_WINDOW_WIDTH: f32 = 654.0 + 19.0 + 19.0;

pub(crate) const APP_MODAL_HOST_COMPACT_WINDOW_WIDTH: f32 = 760.0;

pub(crate) const APP_MODAL_HOST_SIDEBAR_SPACE_EDITOR_WINDOW_HEIGHT: f32 = 380.0;

pub(crate) const APP_MODAL_HOST_PREVIOUS_SESSIONS_WINDOW_HEIGHT: f32 = 680.0;

pub(crate) const APP_MODAL_HOST_DELAYED_SEND_WINDOW_WIDTH: f32 = 470.0;

/*
 * Session Automations must begin taller than its intrinsic React form so the
 * fixed-window scroll cap cannot clip the one-shot fit-height measurement.
 * The measured dialog immediately shrinks this bootstrap frame to an exact fit.
 */
pub(crate) const APP_MODAL_HOST_DELAYED_SEND_WINDOW_HEIGHT: f32 = 565.0;

pub(crate) const APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH: f32 = 570.0;

pub(crate) const APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_WIDTH: f32 = 560.0;

pub(crate) const APP_MODAL_HOST_MISSING_PROJECT_FOLDER_WINDOW_HEIGHT: f32 = 360.0;

/*
 * CDXC:TranscriptExport 2026-08-20:
 * The export result dialog is a compact confirmation: a path, an agent select,
 * and three buttons. It opens on the Rename Session width and lets the one-shot
 * `contentHeightMeasured` fit shrink the frame to whatever it actually rendered.
 */
pub(crate) const APP_MODAL_HOST_EXPORT_TRANSCRIPT_RESULT_WINDOW_WIDTH: f32 = 570.0;

pub(crate) const APP_MODAL_HOST_EXPORT_TRANSCRIPT_RESULT_WINDOW_HEIGHT: f32 = 420.0;

/*
 * CDXC:AppModal 2026-07-26-07:20:
 * The measured Rename Session dialog is 431px tall, so the historic 428px frame
 * cropped its bottom edge by a few pixels.
 *
 * CDXC:AppModal 2026-07-28:
 * Compact modal-host dialogs now report their rendered height once per open
 * and the child window fits to it, so this constant is only the pre-measure
 * frame. It matches the measured default Rename Session dialog instead of the
 * 600px worst-case tall state; long pasted text scrolls inside the dialog via
 * the fixed-window stylesheet caps.
 */
pub(crate) const APP_MODAL_HOST_RENAME_SESSION_WINDOW_HEIGHT: f32 = 440.0;

/*
 * CDXC:AppModal 2026-07-28:
 * Bounds for the one-shot `contentHeightMeasured` window fit. The floor keeps
 * a broken measurement from collapsing the window below a usable dialog; the
 * ceiling keeps it inside the smallest supported display height.
 */
pub(crate) const APP_MODAL_HOST_FIT_CONTENT_MIN_WINDOW_HEIGHT: f32 = 200.0;

pub(crate) const APP_MODAL_HOST_FIT_CONTENT_MAX_WINDOW_HEIGHT: f32 = 850.0;

/*
 * CDXC:AppModal 2026-07-26-07:20:
 * Every app-modal child window is a fitted, non-resizable frame. The compact
 * form and list modals below were measured against the real modal-host React
 * dialogs (rendered from the shipped bundle at the candidate window size), so
 * each window is the dialog's own footprint plus a small margin instead of the
 * shared 1080x760 Settings frame that left short forms stranded in dead space.
 * Measured dialog footprints: Add Worktree 619, Clone Repository 633, Delete
 * Worktree 505 (CSS-capped at 560), Portless Setup 290, Remote Setup 311,
 * Browser Access 544, Rename Session 431. Remote Project sizes to its
 * stylesheet width and scrolls.
 */
pub(crate) const APP_MODAL_HOST_WORKTREE_WINDOW_WIDTH: f32 = 640.0;

pub(crate) const APP_MODAL_HOST_WORKTREE_WINDOW_HEIGHT: f32 = 640.0;

pub(crate) const APP_MODAL_HOST_DELETE_WORKTREE_WINDOW_HEIGHT: f32 = 600.0;

pub(crate) const APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_WIDTH: f32 = 640.0;

pub(crate) const APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_HEIGHT: f32 = 340.0;

pub(crate) const APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_WIDTH: f32 = 560.0;

pub(crate) const APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_HEIGHT: f32 = 380.0;

/*
CDXC:RemotePairing 2026-09-03:
Remote Setup opens on a frame tall enough for both sections; the one-shot
fit-height pass then sizes the child window down to what React rendered.
*/
pub(crate) const APP_MODAL_HOST_REMOTE_SETUP_WINDOW_WIDTH: f32 = 560.0;

pub(crate) const APP_MODAL_HOST_REMOTE_SETUP_WINDOW_HEIGHT: f32 = 760.0;

pub(crate) const APP_MODAL_HOST_REMOTE_PROJECT_PICKER_WINDOW_WIDTH: f32 = 720.0;

pub(crate) const APP_MODAL_HOST_REMOTE_PROJECT_PICKER_WINDOW_HEIGHT: f32 = 640.0;

pub(crate) const APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_WIDTH: f32 = 640.0;

pub(crate) const APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_HEIGHT: f32 = 560.0;

/*
 * CDXC:AddProject 2026-07-30:
 * The shared add-project dialog is a top-anchored command dialog whose browse
 * list scrolls inside itself, so it takes a fixed command-palette-style frame
 * instead of a one-shot height fit: the dialog changes height on every step
 * (machines, sources, browse list, clone destination) and a fitted frame would
 * freeze it at whichever step opened first.
 *
 * CDXC:AddProject 2026-07-31:
 * The dialog now fills this window edge to edge (search field at the top,
 * shortcut footer on the bottom edge, list scrolling in between), so the frame
 * is sized to the tallest common step instead of the old 520px box that left a
 * dead strip under the dialog's own footer. 460px fits the six-row Sources step
 * with a little slack and still gives the browse listing ~300px of scroller.
 */
pub(crate) const APP_MODAL_HOST_ADD_PROJECT_WINDOW_WIDTH: f32 = 640.0;

pub(crate) const APP_MODAL_HOST_ADD_PROJECT_WINDOW_HEIGHT: f32 = 460.0;

pub(crate) const APP_MODAL_HOST_READY_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) const BROWSER_ICON_CHEVRON_RIGHT: &str = "titlebar/chevron-right.svg";

pub(crate) const TITLEBAR_POPUP_COMPACT_WIDTH: f32 = 240.0;

pub(crate) const TITLEBAR_POPUP_GIT_WIDTH: f32 = 300.0;

pub(crate) const TITLEBAR_POPUP_EXTENSIONS_WIDTH: f32 = 340.0;

pub(crate) const TITLEBAR_POPUP_TIPS_WIDTH: f32 = 556.0;

pub(crate) const TITLEBAR_POPUP_RESOURCES_WIDTH: f32 = 656.0;

pub(crate) const TITLEBAR_POPUP_MENU_MAX_HEIGHT: f32 = 420.0;

pub(crate) const TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT: f32 = 650.0;

pub(crate) const TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH: f32 = 2.0;

pub(crate) const TITLEBAR_POPUP_MENU_GAP: f32 = 6.0;

pub(crate) const TITLEBAR_POPUP_MENU_ROW_HEIGHT: f32 = 30.0;

pub(crate) const TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE: f32 = 13.0;

pub(crate) const TITLEBAR_POPUP_MENU_ROW_ICON_SIZE: f32 = 16.0;

pub(crate) const TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT: f32 = 22.0;

pub(crate) const TITLEBAR_POPUP_ACTION_ROW_HEIGHT: f32 = 44.0;

pub(crate) const TITLEBAR_POPUP_EXTENSION_ROW_HEIGHT: f32 = 40.0;

pub(crate) const TITLEBAR_EXTENSION_POPUP_DEFAULT_WIDTH: f32 = 380.0;

pub(crate) const TITLEBAR_EXTENSION_POPUP_DEFAULT_HEIGHT: f32 = 560.0;

pub(crate) const TITLEBAR_POPUP_READING_HEADER_HEIGHT: f32 = 34.0;

pub(crate) const TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE: f32 = 12.0;

pub(crate) const TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE: f32 = 16.0;

pub(crate) const TITLEBAR_POPUP_VERTICAL_OFFSET: f32 = 6.0;

/*
CDXC:Titlebar 2026-07-09:
The titlebar popup NSPanels are sized before opening, so their height math
must mirror gpui-component PopupMenu layout exactly or the last menu rows get
clipped: the popover root adds a 1px border on each side, the items column
adds 4px vertical padding on each side (10px chrome total), adjacent items
are separated by a 2px column gap, separators render as a 2px border plus
2px vertical margins (6px), and every item row is at least 26px tall. All
menus keep the full 10px vertical chrome; zeroing the bottom item padding
makes the last row sit flush on the window edge and read as clipped.
*/
pub(crate) const TITLEBAR_POPUP_MENU_VERTICAL_CHROME: f32 = 10.0;

pub(crate) const TITLEBAR_POPUP_MENU_ITEM_GAP: f32 = 2.0;

pub(crate) const TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT: f32 = 6.0;

pub(crate) const TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT: f32 = 26.0;

pub(crate) const TITLEBAR_POPUP_MENU_BORDER_CHROME: f32 = 2.0;

pub(crate) const TITLEBAR_POPUP_ACTION_PREVIEW_TEXT_SIZE: f32 = 11.0;

pub(crate) const GPUI_TITLEBAR_TIPS_READ_IDS_SETTINGS_KEY: &str = "gpuiTitlebarTipsReadIds";

pub(crate) const TITLEBAR_ACTION_UNCONFIGURED_PREVIEW: &str = "Set the command";

pub(crate) const TITLEBAR_TIPS_TOOLTIP: &str = "Tips";

pub(crate) const TITLEBAR_RESOURCES_TOOLTIP: &str = "Resources Monitor";

pub(crate) const TITLEBAR_EXTENSIONS_TOOLTIP: &str = "Extensions";

pub(crate) const TITLEBAR_GIT_TOOLTIP: &str = "Git actions";

pub(crate) const TITLEBAR_ACTIONS_TOOLTIP: &str = "Quick Actions. Right click for more options";

pub(crate) const TITLEBAR_OPEN_TARGETS_TOOLTIP: &str =
    "Open in an app. Right click for more options";

pub(crate) const TITLEBAR_UPDATE_AVAILABLE_TOOLTIP: &str =
    "Update Ghostex! All your sessions will continue running.";

pub(crate) const BROWSER_ICON_RELOAD: &str = "titlebar/reload.svg";

pub(crate) const BROWSER_ICON_HOME: &str = "titlebar/home.svg";

pub(crate) const BROWSER_ICON_STOP: &str = "titlebar/xmark.svg";

pub(crate) const BROWSER_ICON_SEARCH: &str = "titlebar/search.svg";

pub(crate) const BROWSER_ICON_HISTORY: &str = "titlebar/history.svg";

pub(crate) const COMMAND_ICON_PLUS: &str = "titlebar/plus.svg";

pub(crate) const COMMAND_ICON_CLOCK: &str = "titlebar/clock.svg";

pub(crate) const COMMAND_ICON_COMMAND: &str = "titlebar/command.svg";

// Floating command panes are temporarily disabled because unpinning is not
// reliable yet. Keep the floating layout and state transitions available so
// they can be restored by flipping this flag once that behavior is revisited.
pub(crate) const COMMAND_PANE_FLOATING_MODE_ENABLED: bool = false;

pub(crate) const COMMAND_ICON_PIN: &str = "titlebar/pin.svg";

pub(crate) const COMMAND_ICON_PIN_SLASH: &str = "titlebar/pin-slash.svg";

pub(crate) const COMMAND_ICON_CHEVRON_UP: &str = "titlebar/chevron-up.svg";

pub(crate) const COMMAND_ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";

pub(crate) const COMMAND_ICON_CHEVRON_LEFT: &str = "titlebar/chevron-left.svg";

pub(crate) const COMMAND_ICON_CHEVRON_RIGHT: &str = "titlebar/chevron-right.svg";

pub(crate) const COMMAND_ICON_MOON: &str = "titlebar/moon.svg";

pub(crate) const COMMAND_ICON_XMARK: &str = "titlebar/xmark.svg";

pub(crate) const BROWSER_ICON_LOCK_FILLED: &str = "titlebar/lock-filled.svg";

pub(crate) const BROWSER_ICON_WORLD: &str = "titlebar/world.svg";

pub(crate) const BROWSER_ICON_TOOLS: &str = "titlebar/tools.svg";

/*
GPUI Tips uses the same gpui-component PopupMenu child-window path as the
other titlebar dropdowns. The legacy React titlebar host remains available to
the macOS app, while these first-party URLs stay bounded before they enter a
GPUI Browser pane.
*/
pub(crate) const GHOSTEX_CHANGELOG_URL: &str = "https://github.com/maddada/ghostex/releases";

pub(crate) const GHOSTEX_DOCS_URL: &str = "https://ghostex.dev/docs";

pub(crate) const BROWSER_ICON_POINTER: &str = "titlebar/pointer.svg";

pub(crate) const BROWSER_ICON_USER_CIRCLE: &str = "titlebar/user-circle.svg";

pub(crate) const BROWSER_ICON_MICROPHONE: &str = "titlebar/microphone.svg";

pub(crate) const BROWSER_ICON_CAMERA: &str = "titlebar/camera.svg";

/// Microphone/camera prompt row, sized like the toolbar it sits under.
pub(crate) const BROWSER_MEDIA_PERMISSION_BAR_HEIGHT: f32 = 36.0;

pub(crate) const BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL: &str = "Agentation";

pub(crate) const BROWSER_FEEDBACK_TOOL_UNAVAILABLE_TOOLTIP: &str =
    "This site disallows using this tool";

pub(crate) const BROWSER_FEEDBACK_AGENTATION_PACKAGE_MODULE_URL: &str =
    "https://esm.sh/agentation@3.0.2?bundle&deps=react@18.2.0,react-dom@18.2.0";

pub(crate) const BROWSER_FEEDBACK_AGENTATION_REACT_MODULE_URL: &str = "https://esm.sh/react@18.2.0";

pub(crate) const BROWSER_FEEDBACK_AGENTATION_REACT_DOM_CLIENT_MODULE_URL: &str =
    "https://esm.sh/react-dom@18.2.0/client?deps=react@18.2.0";

pub(crate) const PROJECT_EDITOR_COMPANION_RESTORE_ICON: &str = "titlebar/chevron-right.svg";

/*
CDXC:Workarea 2026-06-22-06:24:
GPUI workspace chrome should match the macOS workspace shell constants: terminal tab bars are 36px high, workspace tabs stay in the 170-175px macOS width band, command titlebars and collapsed strips are 26px high, and divider/resize rails remain real layout siblings around 5px with 1px visual separators.

CDXC:Workarea 2026-06-22-06:24:
The command panel stores an in-memory height ratio, but its default and double-click reset derive from the shared command-pane default-height setting when that fits within the 5%-90% available-content clamp. Project-editor companions default to roughly 32% of the editor area with a practical minimum and persist resize/reset mutations through the GPUI shell state.

CDXC:CommandPane 2026-06-25-11:29:
GPUI command-pane initial height, missing persisted height, and double-click reset must honor the same Settings.commandsPanelDefaultHeightPx value as the macOS app. Keep the Rust side on the shared 125px default and 40px-600px setting clamp so changing the Workspace setting affects future opens/resets without rewriting explicit persisted ratios.

CDXC:Terminal 2026-06-22-23:33:
Full-width secondary terminal creation is a distinct Agents shell action from pane-local Split Below. It wraps the entire existing Agents workspace tree as the top branch, appends a selected Mounting terminal in a bottom row, and keeps startup honest without fake Running state, libghostty mount, process launch, command text, stdout/stderr, or terminal content.

CDXC:CommandPane 2026-06-22-11:15:
The far-right Agents pane overflow is a NativeMenu scoped to the clicked pane id. It exposes pane/layout actions only, omits per-tab close commands, dispatches GPUI actions through the root render tree, and reuses the existing placeholder-only shell mutations without overlapping GPUI panels, hidden hit regions, libghostty mounts, terminal processes, command text, or terminal content.

CDXC:CommandPane 2026-06-22-13:17:
Merge All Tabs is Agents-workspace-only parity. The pane menu and Ctrl+Shift+M flatten only the Agents split tree into one clicked or focused tab group, preserve terminal placeholder ids and presentation states, select the target pane's active session when possible, clear Focus mode because split geometry is removed, and never merge command-pane, Browser, Source, Kanban, Manage, project-editor, libghostty, process, command text, or terminal content state.

CDXC:ContextMenus 2026-06-22-11:19:
Agents tab right-click context menus are separate from the far-right pane overflow: they are NativeMenus scoped to the clicked pane/session ids, expose only Select Tab and Close Tab, and keep pane/layout actions out of per-tab menus.

CDXC:ContextMenus 2026-06-22-11:27:
Browser tab right-click context menus are NativeMenus scoped to the clicked Browser pane id and Browser tab id. They expose only Select Tab and Close Tab, reuse the existing Browser selection and close semantics, and must not include pane split/layout, toolbar, history, project-editor, overlay, hidden hit-region, or hit-test-routing behavior.

CDXC:Browser 2026-06-22-13:46:
The far-right Browser pane overflow is a NativeMenu scoped to the clicked Browser pane id. It exposes pane/layout actions only, omits per-tab close/select plus toolbar/history/profile/devtools commands, and creates address-only Browser tabs through the existing Browser sync path so split panes do not load new CEF surfaces.

CDXC:Browser 2026-08-12:
The Browser pane overflow starts with Open in External Browser, followed by a
separator before its pane/layout actions. The typed action resolves the clicked
pane's active tab at dispatch time and sends only a validated HTTP(S) URL to the
existing OS-default-browser opener.

    CDXC:ContextMenus 2026-06-22-11:31:
    Command-pane tab right-click context menus are NativeMenus scoped to the clicked command group and command session. They expose scoped close rows while tab selection and collapsed-strip expansion stay on left-click tab activation.

    CDXC:ContextMenus 2026-06-27-05:07:
    Command-tab context menus must stay outside Action process ownership: run-start metadata, status-file polling, completion feedback, and exit cleanup own live Action status, while menu actions may only select/sleep/close the scoped tab without inspecting command text, output, paths, env, logs, status-file contents, Browser/CEF state, overlays, hidden hit regions, or hit-test routing.

    CDXC:ContextMenus 2026-06-25-14:13:
    AppKit command tabs receive panel actions in their titlebar action model, and `primaryTabContextMenuActions` keeps only per-session actions that are present in that model before Sleep/Close scopes. Keep GPUI command-tab context menus tab-scoped and leave Pin/Unpin plus Minimize on the fixed command-panel action buttons.

    CDXC:ContextMenus 2026-06-27-01:49:
    Native command-panel sessions expose only fixed panel action payloads, so Swift `primaryTabContextMenuActions` produces no Rename Session, Delayed Send, or Close After Done block for command-tab right-click. GPUI command-tab menus must start with eligible Focus only, then scoped Sleep/Close rows, while focused command-palette and modal actions keep their separate routes.

        CDXC:Browser 2026-06-22-11:38:
        Browser History is an OS-owned NativeMenu opened from the toolbar History button, not Back/Forward dropdown chrome or an in-layout GPUI panel. The menu derives labels from sanitized URL history through the existing URL display helper, carries only the target history index in a typed action, and creates a new loaded Browser tab only after the user selects a row.

        CDXC:CommandPane 2026-06-22-12:09:
        Browser and command tab bars need the same sticky edge affordance as Agents when the active tab is clipped by horizontal overflow. Render the affordance as fixed-width, visible, non-interactive sibling chrome between the scrollable tab strip and the fixed control cluster; do not use overlays, hidden hit regions, hit-test routing, or synthetic coordinate routing.

        CDXC:CommandPane 2026-06-25-13:30:
        Command tabs no longer use the permanent workspace-style edge reveal. Native command chrome only shows a conditional active-tab proxy when horizontal overflow clips the active command tab.

        CDXC:FocusMode 2026-06-22-23:33:
        Cmd+T and Cmd+N parity places keyboard-created and clicked-pane new terminal/browser tabs immediately after the active tab in the target split pane. Cmd+T is shell-focus scoped: focused Agents panes in Agents mode add Mounting startup placeholders, while focused command panes add command placeholders to the focused command group. Cmd+N switches and wakes Browser because existing Browser popup and toolbar commands already select Browser through the Browser sync path.

        CDXC:FocusMode 2026-06-25-16:05:
        Cmd+D and Cmd+Shift+D are focused terminal split hotkeys in the placeholder shell. Agents focus in Agents mode creates Mounting startup placeholders to the right or below the focused Agents pane; command-pane focus must match native by treating both hotkeys as horizontal command splits beside the focused command group.

        CDXC:CommandPane 2026-06-22-13:05:
        Agents workspace tabs can be dragged into an expanded command-pane group body as a placeholder transfer. The command pane gets a command-only placeholder session with the same visible title, the Agents tab is removed only when that move would not empty the final root Agents leaf, command drops keep center grouping plus left/right horizontal splits, and no libghostty mount, real process, command text, stdout/stderr, overlay, hidden hit region, or hit-test routing is introduced.

        CDXC:CommandPane 2026-06-22-16:18:
        Agents workspace tabs can also be dragged to an expanded command-pane tab-strip boundary or end target. This creates a command-only placeholder tab with the visible Agents title at the exact requested command tab index, focuses/expands that command group, then removes the Agents source only after insertion succeeds; it never transfers libghostty state, terminal content, command text, process state, Source/Kanban/Automate/Manage surfaces, CEF state, overlays, hidden hit regions, or native/root hit-test routing.
        */
pub(crate) const WORKSPACE_TAB_BAR_HEIGHT: f32 = 36.0;

pub(crate) const WORKSPACE_TAB_WIDTH: f32 = 172.0;

pub(crate) const WORKSPACE_TAB_ICON_WIDTH: f32 = 14.0;

pub(crate) const WORKSPACE_TAB_AGENT_ICON_SIZE: f32 = 14.0;

pub(crate) const WORKSPACE_TAB_CLOSE_SIZE: f32 = 20.0;

pub(crate) const WORKSPACE_TAB_CLOSE_TRAILING_PADDING: f32 = 4.0;

pub(crate) const WORKSPACE_TAB_CLOSE_TOP_OFFSET: f32 =
    (WORKSPACE_TAB_BAR_HEIGHT - WORKSPACE_TAB_CLOSE_SIZE) / 2.0;

pub(crate) const WORKSPACE_TAB_CLOSE_ICON_SIZE: f32 = 10.0;

pub(crate) const WORKSPACE_TAB_STATUS_INDICATOR_SIZE: f32 = 7.0;

pub(crate) const WORKSPACE_TAB_STATUS_INDICATOR_TRAILING_PADDING: f32 = 10.0;

pub(crate) const WORKSPACE_TAB_SLEEP_ICON_SIZE: f32 = WORKSPACE_TAB_AGENT_ICON_SIZE;

pub(crate) const WORKSPACE_TAB_SLEEP_SVG_SIZE: f32 = 11.0;

pub(crate) const WORKSPACE_TAB_SLEEP_ICON_TRAILING_PADDING: f32 = 11.0;

pub(crate) const WORKSPACE_TAB_STATUS_TITLE_GAP: f32 = 4.0;

pub(crate) const WORKSPACE_TAB_STATUS_TITLE_RESERVED_WIDTH: f32 =
    WORKSPACE_TAB_STATUS_INDICATOR_SIZE
        + WORKSPACE_TAB_STATUS_INDICATOR_TRAILING_PADDING
        + WORKSPACE_TAB_STATUS_TITLE_GAP;

pub(crate) const WORKSPACE_TAB_SLEEP_TITLE_RESERVED_WIDTH: f32 = WORKSPACE_TAB_SLEEP_ICON_SIZE
    + WORKSPACE_TAB_SLEEP_ICON_TRAILING_PADDING
    + WORKSPACE_TAB_STATUS_TITLE_GAP;

pub(crate) const WORKSPACE_TAB_ACTION_BUTTON_WIDTH: f32 = 42.0;

pub(crate) const WORKSPACE_TAB_ACTION_BUTTON_HEIGHT: f32 = 34.0;

pub(crate) const WORKSPACE_TAB_ACTION_ICON_SIZE: f32 = 15.0;

pub(crate) const WORKSPACE_TAB_SELECTED_WHITE_OVERLAY_ALPHA: f32 = 0.13;

pub(crate) const WORKSPACE_TAB_INACTIVE_WHITE_OVERLAY_ALPHA: f32 = 0.06;

pub(crate) const WORKSPACE_SPLIT_HANDLE_THICKNESS: f32 = 5.0;

pub(crate) const WORKSPACE_SPLIT_SEPARATOR_THICKNESS: f32 = 1.0;

pub(crate) const WORKSPACE_BOTTOM_ROW_TOP_RATIO: f32 = 0.72;

pub(crate) const PANE_RESIZE_MINIMUM_WIDTH: f32 = 220.0;

pub(crate) const PANE_RESIZE_MINIMUM_HEIGHT: f32 = 160.0;

pub(crate) const WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH: f32 = 460.0;

pub(crate) const SPATIAL_FOCUS_HALF_PLANE_TOLERANCE: f32 = 2.0;

pub(crate) const WORKSPACE_DROP_EDGE_BAND_FRACTION: f32 = 0.24;

pub(crate) const AGENTS_SPLIT_DROP_PREVIEW_FRACTION: f32 = 0.5;

pub(crate) const COMMAND_PANE_BODY_DROP_EDGE_BAND_FRACTION: f32 = 0.24;

pub(crate) const COMMAND_PANE_TAB_BAR_HEIGHT: f32 = 26.0;

pub(crate) const COMMAND_PANE_STRIP_HEIGHT: f32 = 26.0;

/*
CDXC:CommandPane 2026-09-03:
The collapsed strip's left inset equals the expanded leaf's 1px side edge plus its 2px group border, so tabs sit at the same x whether the panel is minimized or not. The edge width is the part of that inset painted as the left border line.
*/
pub(crate) const COMMAND_PANE_COLLAPSED_STRIP_LEFT_MARGIN: f32 = 3.0;

pub(crate) const COMMAND_PANE_COLLAPSED_STRIP_LEFT_EDGE_WIDTH: f32 = 1.0;

pub(crate) const COMMAND_PANE_COLLAPSED_STRIP_RIGHT_MARGIN: f32 = 0.0;

/*
CDXC:CommandPane 2026-06-25-18:07:
Native floating command panels reserve the collapsed-strip footprint, then inset the expanded floating panel by 25px from the workspace edges above that footprint. Keep the inset in normal absolute layout so the floating panel does not become a full-width pinned panel clone.

CDXC:CommandPane 2026-06-25-18:19:
The reserved floating bottom footprint is plain command-panel chrome, not the interactive collapsed tab strip.
*/
pub(crate) const COMMAND_PANE_FLOATING_MARGIN: f32 = 25.0;

/*
CDXC:CommandPane 2026-06-25-18:14:
Native command-panel chrome owns the full panel frame. Keep the trailing command content inset by one logical pixel while the leading pane edge stays flush with the workspace pane boundary.
*/
pub(crate) const COMMAND_PANE_OUTER_CONTENT_RIGHT_INSET: f32 = 1.0;

/*
CDXC:CommandPane 2026-06-25-13:30:
Native command tab strips do not render a permanent decorative edge reveal. Their overflow affordance is the conditional 30px Show Active Tab proxy when the active tab is clipped below 60px visible, using a 12px reveal scroll margin.
*/
pub(crate) const COMMAND_PANE_STICKY_ACTIVE_TAB_BUTTON_SIZE: f32 = 30.0;

pub(crate) const COMMAND_PANE_STICKY_ACTIVE_TAB_ICON_SIZE: f32 = 11.0;

pub(crate) const COMMAND_PANE_ACTIVE_TAB_REVEAL_SCROLL_MARGIN: f32 = 12.0;

pub(crate) const COMMAND_PANE_ACTIVE_TAB_REVEAL_MINIMUM_VISIBLE_WIDTH: f32 = 60.0;

/*
CDXC:CommandPane 2026-06-25-13:45:
Native command tab strips keep direct horizontal scrolling native, ignore precise vertical trackpad gestures, and amplify non-precision vertical wheel ticks by 18x with a 96px minimum so mouse wheels traverse overflowing command tabs quickly.
*/
pub(crate) const COMMAND_PANE_VERTICAL_WHEEL_TAB_SCROLL_MULTIPLIER: f32 = 18.0;

pub(crate) const COMMAND_PANE_MINIMUM_DISCRETE_VERTICAL_WHEEL_TAB_SCROLL_DELTA: f32 = 96.0;

/*
CDXC:CommandPane 2026-06-25-13:50:
Native pane titlebars reserve empty command tab chrome for double-click New Terminal when a real tab/add/control was not hit. Use the native 34px preferred target width, keep the 24px minimum as an asserted contract, and route only double-clicks through terminal creation.
*/
pub(crate) const COMMAND_PANE_EMPTY_TITLEBAR_DOUBLE_CLICK_TARGET_WIDTH: f32 = 34.0;

/*
CDXC:CommandPane 2026-06-25-18:46:
Native command titlebars hide the inline New Terminal button when it would squeeze the tab viewport below the compact double-click target. Preserve at least the 56px native empty-titlebar viewport before spending 26px on the command add button.
*/
pub(crate) const COMMAND_PANE_MINIMUM_VISIBLE_TAB_VIEWPORT_WIDTH_WITH_DOUBLE_CLICK_TARGET: f32 =
    56.0;

pub(crate) const COMMAND_PANE_TAB_ADD_BUTTON_GAP: f32 = 0.0;

/*
CDXC:CommandPane 2026-06-25-13:32:
Native command tabs fit equally inside the available command tab viewport, clamped from 72px to 160px, and the collapsed command-panel bar uses the same command-role tab sizing as expanded command titlebars. Do not keep separate fixed expanded/collapsed tab widths.
*/
pub(crate) const COMMAND_PANE_TAB_MIN_WIDTH: f32 = 72.0;

pub(crate) const COMMAND_PANE_TAB_MAX_WIDTH: f32 = 160.0;

pub(crate) const COMMAND_PANE_TAB_END_DROP_TARGET_WIDTH: f32 =
    COMMAND_PANE_EMPTY_TITLEBAR_DOUBLE_CLICK_TARGET_WIDTH;

/*
CDXC:CommandPane 2026-06-25-13:25:
Native command tabs keep the compact old command typography for both active and inactive tabs: 11pt semibold text with stable light color. Do not reuse workspace-style inactive dimming or active-state font-weight changes for command chrome.
*/
pub(crate) const COMMAND_PANE_TAB_TITLE_FONT_SIZE: f32 = 11.0;

/*
CDXC:CommandPane 2026-06-25-13:11:
Native command tabs reveal the inline close affordance only while the owning tab is hovered. Keep the close frame at the native 20px size, 4px from the trailing edge, flat-cornered, and out of tab flex layout so hover does not remeasure the title.
*/
pub(crate) const COMMAND_PANE_TAB_CLOSE_SIZE: f32 = 20.0;

pub(crate) const COMMAND_PANE_TAB_CLOSE_TRAILING_PADDING: f32 = 4.0;

pub(crate) const COMMAND_PANE_TAB_CLOSE_TOP_OFFSET: f32 =
    (COMMAND_PANE_TAB_BAR_HEIGHT - COMMAND_PANE_TAB_CLOSE_SIZE) / 2.0;

pub(crate) const COMMAND_PANE_TAB_CLOSE_CORNER_RADIUS: f32 = 0.0;

/*
CDXC:CommandPane 2026-06-25-14:01:
Native command-tab close chrome uses the same stable #0e0e0e icon-button background and #cfcfcf stroked X as command tab-bar buttons. Render an icon inside the hover-only 20px frame instead of a lowercase text x with hover-only background.
*/
pub(crate) const COMMAND_PANE_TAB_CLOSE_ICON_SIZE: f32 = 10.0;

/*
CDXC:CommandPane 2026-06-25-14:17:
Native command tab separators are explicit 1px white/10% trailing fills on command tabs that have a following command tab. Do not rely on a right border because the last tab must not draw separator chrome.
*/
pub(crate) const COMMAND_PANE_TAB_SEPARATOR_WIDTH: f32 = 1.0;

/*
CDXC:CommandPane 2026-06-25-14:36:
Native command tabs use the AppKit pane-tab compositing base (#050608) with white overlays: 13% for active command tabs and 6% for inactive command tabs. Hover reveals close chrome only; it does not brighten the tab fill.

CDXC:SessionSleep 2026-06-25-14:39:
Inactive sleeping command tabs use the native parked-tab visual treatment: keep selected sleeping tabs visually selected, but reduce inactive sleeping tab fill to a 3.2% white overlay and dim its title by 48%.
*/
pub(crate) const COMMAND_PANE_TAB_BACKGROUND_BASE_RED: u8 = 0x05;

pub(crate) const COMMAND_PANE_TAB_BACKGROUND_BASE_GREEN: u8 = 0x06;

pub(crate) const COMMAND_PANE_TAB_BACKGROUND_BASE_BLUE: u8 = 0x08;

pub(crate) const COMMAND_PANE_TAB_ACTIVE_OVERLAY_ALPHA: f32 = 0.13;

pub(crate) const COMMAND_PANE_TAB_INACTIVE_OVERLAY_ALPHA: f32 = 0.06;

pub(crate) const COMMAND_PANE_TAB_SLEEPING_INACTIVE_OVERLAY_ALPHA: f32 = 0.032;

pub(crate) const COMMAND_PANE_TAB_TITLE_SLEEPING_INACTIVE_ALPHA_MULTIPLIER: f32 = 0.48;

/*
CDXC:SessionStatus 2026-06-25-13:18:
Native command tabs reserve a trailing status slot for working, attention, and Delayed Send. Working/attention render as 8px circular fills 9px from the trailing edge; Delayed Send renders a 14px clock centered on that slot; all status chrome is hidden while hover close chrome is visible, but title reservation stays stable.
*/
pub(crate) const COMMAND_PANE_TAB_STATUS_INDICATOR_SIZE: f32 = 8.0;

pub(crate) const COMMAND_PANE_TAB_STATUS_INDICATOR_TRAILING_PADDING: f32 = 9.0;

pub(crate) const COMMAND_PANE_TAB_STATUS_TITLE_GAP: f32 = 4.0;

pub(crate) const COMMAND_PANE_TAB_STATUS_TITLE_RESERVED_WIDTH: f32 =
    COMMAND_PANE_TAB_STATUS_INDICATOR_SIZE
        + COMMAND_PANE_TAB_STATUS_INDICATOR_TRAILING_PADDING
        + COMMAND_PANE_TAB_STATUS_TITLE_GAP;

pub(crate) const COMMAND_PANE_TAB_STATUS_INDICATOR_TOP_OFFSET: f32 =
    (COMMAND_PANE_TAB_BAR_HEIGHT - COMMAND_PANE_TAB_STATUS_INDICATOR_SIZE) / 2.0;

pub(crate) const COMMAND_PANE_TAB_DELAYED_SEND_ICON_SIZE: f32 = 14.0;

pub(crate) const COMMAND_PANE_TAB_DELAYED_SEND_ICON_TRAILING_PADDING: f32 =
    COMMAND_PANE_TAB_STATUS_INDICATOR_TRAILING_PADDING
        - ((COMMAND_PANE_TAB_DELAYED_SEND_ICON_SIZE - COMMAND_PANE_TAB_STATUS_INDICATOR_SIZE)
            / 2.0);

pub(crate) const COMMAND_PANE_TAB_DELAYED_SEND_ICON_TOP_OFFSET: f32 =
    ((COMMAND_PANE_TAB_BAR_HEIGHT - COMMAND_PANE_TAB_DELAYED_SEND_ICON_SIZE) / 2.0) - 1.0;

pub(crate) const COMMAND_PANE_TAB_TITLE_TRAILING_PADDING: f32 = 8.0;

/*
CDXC:CommandPane 2026-06-25-12:29:
Native command-panel action buttons use the full 26px command titlebar height as their button frame. Keep GPUI fixed command-panel controls square to the tab-bar height so Pin/Unpin and Minimize/Expand occupy the same normal-layout region as macOS.
*/
pub(crate) const COMMAND_PANE_CONTROL_BUTTON_SIZE: f32 = COMMAND_PANE_TAB_BAR_HEIGHT;

pub(crate) const COMMAND_PANE_CONTROL_ICON_SIZE: f32 = 14.0;

/*
CDXC:CommandPane 2026-06-25-13:47:
Command-panel action buttons are contiguous full-height 26px frames with no inter-button gap, no wrapper left border, and flat corners. Expanded and collapsed titlebars keep the visibility frame flush trailing, and both chevrons use the same asymmetric icon padding so their horizontal placement matches.
*/
pub(crate) const COMMAND_PANE_CONTROL_BUTTON_GAP: f32 = 0.0;

pub(crate) const COMMAND_PANE_CONTROL_CORNER_RADIUS: f32 = 0.0;

pub(crate) const COMMAND_PANE_CONTROL_EXPANDED_TRAILING_PADDING: f32 = 0.0;

pub(crate) const COMMAND_PANE_CONTROL_COLLAPSED_TRAILING_PADDING: f32 = 0.0;

pub(crate) const COMMAND_PANE_VISIBILITY_ICON_LEADING_PADDING: f32 = 4.0;

pub(crate) const COMMAND_PANE_SPLIT_HANDLE_THICKNESS: f32 = 5.0;

pub(crate) const COMMAND_PANE_DEFAULT_SESSION_TITLE: &str = "Command Terminal";

pub(crate) const COMMAND_PANE_DEFAULT_HEIGHT_PX: f32 = 125.0;

pub(crate) const COMMAND_PANE_MIN_DEFAULT_HEIGHT_PX: f32 = 40.0;

pub(crate) const COMMAND_PANE_MAX_DEFAULT_HEIGHT_PX: f32 = 600.0;

pub(crate) const COMMAND_PANE_MIN_HEIGHT_RATIO: f32 = 0.05;

pub(crate) const COMMAND_PANE_MAX_HEIGHT_RATIO: f32 = 0.90;

/*
CDXC:CommandPane 2026-08-16:
A right-docked command pane sizes by a workspace-width ratio instead of the
bottom pane's height ratio. The default is a constant fraction rather than a
second Settings value; the vertical resize rail and its double-click reset are
the only ways to change it, and the ratio persists per project next to
`heightRatio` so switching sides keeps both remembered sizes.
*/
pub(crate) const COMMAND_PANE_DEFAULT_WIDTH_RATIO: f32 = 0.38;

pub(crate) const COMMAND_PANE_MIN_WIDTH_RATIO: f32 = 0.10;

pub(crate) const COMMAND_PANE_MAX_WIDTH_RATIO: f32 = 0.90;

/*
CDXC:SessionSleep 2026-06-25-14:49:
Sleeping command-pane bodies should mirror native AppKit placeholders: black body, centered medium 13px wake text, and the exact "Press Any Key to Wake" affordance only when click-to-wake placeholders are enabled.

CDXC:SessionSleep 2026-06-27-00:22:
Native `SleepingPanePlaceholderContentView` measures the wake label from the exact command body: max width is body width minus 8, max height is body height minus 16, nonpositive max dimensions hide the label, text wraps by character, width is ceil(measured width)+8 clamped to the max, height is ceil(measured height) with an 18px minimum clamped to the max, and the frame is centered in the body.

CDXC:SessionSleep 2026-06-27-00:22:
The GPUI wake label must be paint-only body chrome inside the normal command body canvas so it cannot own hit testing, keyboard routing, persistence, logs, or fallback geometry. Existing body click/key wake handlers remain the only wake behavior.
*/
pub(crate) const COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL: &str = "Press Any Key to Wake";

pub(crate) const COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_FONT_SIZE: f32 = 13.0;

pub(crate) const COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT: f32 = 18.0;

pub(crate) const COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_HORIZONTAL_PADDING: f32 = 4.0;

pub(crate) const COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_VERTICAL_PADDING: f32 = 8.0;

pub(crate) const GPUI_KEEP_AWAKE_POWER_CHECK_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) const GPUI_KEEP_AWAKE_LID_SLEEP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

pub(crate) const GPUI_KEEP_AWAKE_WORKING_SESSION_GRACE: Duration = Duration::from_secs(20 * 60);

/*
CDXC:DelayedSend 2026-06-25-15:42:
Native command terminals show an active Delayed Send countdown as a centered terminal-body badge, not only as tab chrome. Match the AppKit badge typography, color, padding, and minimum size inside the existing command body element without adding an interactive overlay.

CDXC:DelayedSend 2026-06-25-19:13:
Native `delayedSendLabelFrame` hides the terminal-body badge when the exact body is 48px wide or smaller, or 32px tall or smaller, then centers the fitted badge while clamping it to body width minus 32px and body height minus 24px. GPUI must keep that rule as exact-body geometry evidence and must not infer visibility from cached command-group bounds.

CDXC:DelayedSend 2026-06-27-00:07:
The rendered GPUI body badge must use the same exact body bounds as native during paint, not a flex overlay with minimum body dimensions. Keep the badge as private-data-safe canvas chrome inside the command body so it cannot own input or route hit testing.
*/
pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_FONT_SIZE: f32 = 23.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_LINE_HEIGHT: f32 = 30.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_FONT_FAMILY: &str = "SF Mono";

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_HORIZONTAL_PADDING: f32 = 30.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_TOTAL_HORIZONTAL_PADDING: f32 =
    COMMAND_PANE_DELAYED_SEND_BADGE_HORIZONTAL_PADDING * 2.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_MIN_HEIGHT: f32 = 58.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_CORNER_RADIUS: f32 = 12.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_MIN_BODY_WIDTH: f32 = 48.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_MIN_BODY_HEIGHT: f32 = 32.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_BODY_WIDTH_CLAMP_INSET: f32 = 32.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_BADGE_BODY_HEIGHT_CLAMP_INSET: f32 = 24.0;

pub(crate) const COMMAND_PANE_DELAYED_SEND_MIN_DELAY_MS: u64 = 60_000;

pub(crate) const COMMAND_PANE_DELAYED_SEND_MAX_DELAY_MS: u64 = 2_147_483_647;

pub(crate) const COMMAND_PANE_DELAYED_SEND_RESTORE_FIRE_GRACE_MS: u64 = 2_000;

pub(crate) const COMMAND_PANE_DELAYED_SEND_PERSIST_INTERVAL: Duration = Duration::from_secs(60);

pub(crate) const GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION: Duration =
    Duration::from_secs(10);

pub(crate) const GPUI_AGENTS_SEND_WHEN_STOPPED_POLL_INTERVAL: Duration = Duration::from_millis(250);

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

pub(crate) const PROJECT_EDITOR_COMPANION_RESTORE_RAIL_WIDTH: f32 = 32.0;

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

pub(crate) const SOURCE_CODE_SERVER_EDITOR_ORIGIN: &str = "http://127.0.0.1:3777";

pub(crate) const SOURCE_CODE_SERVER_COMPONENT_NAME: &str = "code-server";

pub(crate) const SOURCE_CODE_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "codeViewTabHidden";

pub(crate) const BROWSER_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "browserViewTabHidden";

pub(crate) const KANBAN_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "kanbanViewTabHidden";

pub(crate) const AUTOMATE_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "automateViewTabHidden";

pub(crate) const DOCS_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "docsViewTabHidden";

pub(crate) const TIPS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "tipsAndTricksTitlebarButtonHidden";

pub(crate) const RESOURCES_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "resourcesTitlebarButtonHidden";

pub(crate) const EXTENSIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "extensionsTitlebarButtonHidden";

pub(crate) const GIT_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "gitActionsTitlebarButtonHidden";

pub(crate) const QUICK_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str =
    "quickActionsTitlebarButtonHidden";

pub(crate) const OPEN_IN_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY: &str = "openInTitlebarButtonHidden";

pub(crate) const SOURCE_CODE_SERVER_INSTALL_PROMPT: &str = "The VS Code IDE component is a 150mb optional install (one-time).\nWould you like to install it?";

pub(crate) const SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR: u64 = 22;

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
