use std::{
    collections::hash_map::DefaultHasher,
    env,
    ffi::OsString,
    fs,
    hash::Hasher as _,
    io,
    path::{Path, PathBuf},
    process,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value};

pub const PROJECT_EDITOR_AUTO_SLEEP_DEFAULT_IDLE_MINUTES: f64 = 5.0;
pub const PROJECT_EDITOR_AUTO_SLEEP_MAX_IDLE_MINUTES: f64 = 300.0;
pub const DEFAULT_TERMINAL_FONT_SIZE: f32 = 13.0;
pub const MIN_TERMINAL_FONT_SIZE: f32 = 8.0;
pub const MAX_TERMINAL_FONT_SIZE: f32 = 32.0;
pub const DEFAULT_AGENT_ACCEPT_ALL_ENABLED: bool = true;
pub const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
pub const MAX_DEFAULT_PROMPT_AGENT_ID_LEN: usize = 120;
pub const DEFAULT_DEFAULT_EDITOR_COMMAND: &str = "code";
pub const DEFAULT_KEEP_AWAKE_DURATION_MINUTES: SharedKeepAwakeDurationMinutes =
    SharedKeepAwakeDurationMinutes::UntilTurnedOff;
pub const DEFAULT_KEEP_AWAKE_ALLOW_DISPLAY_SLEEP: bool = false;
pub const DEFAULT_KEEP_AWAKE_ACTIVATE_ON_EXTERNAL_DISPLAY: bool = false;
pub const DEFAULT_KEEP_AWAKE_ACTIVATE_ON_LAUNCH: bool = false;
pub const DEFAULT_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT: f64 = 20.0;
pub const DEFAULT_KEEP_AWAKE_DEACTIVATE_BELOW_BATTERY_THRESHOLD: bool = false;
pub const DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_LOW_POWER_MODE: bool = false;
pub const DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_USER_SWITCH: bool = false;
pub const DEFAULT_KEEP_AWAKE_PREVENT_LID_SLEEP: bool = false;
pub const DEFAULT_KEEP_AWAKE_WHILE_WORKING_SESSIONS: bool = false;
pub const DEFAULT_HIDE_KEEP_AWAKE_TITLEBAR_CONTROL: bool = false;
pub const DEFAULT_APP_SHOTS_ENABLED: bool = false;
pub const DEFAULT_APP_SHOTS_HOTKEY: SharedAppShotsHotkey = SharedAppShotsHotkey::BothCommand;
const MIN_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT: f64 = 10.0;
const MAX_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT: f64 = 90.0;
const MAX_CUSTOM_DEFAULT_EDITOR_COMMAND_CHARS: usize = 240;
const DEFAULT_TERMINAL_CURSOR_STYLE: &str = "bar";
const DEFAULT_TERMINAL_FONT_FAMILY: &str = "JetBrains Mono";
const DEFAULT_TERMINAL_FONT_WEIGHT: f64 = 300.0;
const NORMAL_TERMINAL_FONT_WEIGHT: f64 = 400.0;
const DEFAULT_TERMINAL_GHOSTTY_THEME: &str = "GitHub Dark";
const DEFAULT_TERMINAL_BACKGROUND_COLOR: &str = "#000000";
const DEFAULT_TERMINAL_BACKGROUND_IMAGE: &str = "";
const DEFAULT_TERMINAL_BACKGROUND_IMAGE_OPACITY: f64 = 1.0;
const DEFAULT_TERMINAL_BACKGROUND_IMAGE_FIT: &str = "cover";
const DEFAULT_TERMINAL_LETTER_SPACING: f64 = 0.0;
const DEFAULT_TERMINAL_LINE_HEIGHT: f64 = 1.2;
const DEFAULT_TERMINAL_CURSOR_STYLE_BLINK: bool = true;
const DEFAULT_TERMINAL_SCROLLBACK_LIMIT_MB: f64 = 15.0;
const DEFAULT_TERMINAL_COPY_ON_SELECT: &str = "false";
const DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE: &str = "false";
const DEFAULT_TERMINAL_CLIPBOARD_TRIM_TRAILING_SPACES: bool = true;
const DEFAULT_TERMINAL_CLIPBOARD_PASTE_PROTECTION: bool = true;
const DEFAULT_TERMINAL_PASTE_PREVIEWABLE_IMAGES: bool = true;
const DEFAULT_TERMINAL_MOUSE_HIDE_WHILE_TYPING: bool = false;
const DEFAULT_TERMINAL_SCROLLBAR: &str = "system";
const DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_DISCRETE: f64 = 1.0;
const DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_PRECISION: f64 = 1.0;
const DEFAULT_TERMINAL_SCROLL_TO_BOTTOM_WHEN_TYPING: bool = true;
const DEFAULT_WEB_LINKS_OPEN_IN_APP: bool = true;
const DEFAULT_TERMINAL_PANE_PADDING_PX: f64 = 0.0;
const MIN_TERMINAL_PANE_PADDING_PX: f64 = 0.0;
const MAX_TERMINAL_PANE_PADDING_PX: f64 = 64.0;
const MIN_TERMINAL_FONT_WEIGHT: f64 = 100.0;
const MAX_TERMINAL_FONT_WEIGHT: f64 = 900.0;
const MIN_TERMINAL_LINE_HEIGHT: f64 = 0.8;
const MAX_TERMINAL_LINE_HEIGHT: f64 = 2.0;
const MIN_TERMINAL_LETTER_SPACING: f64 = -2.0;
const MAX_TERMINAL_LETTER_SPACING: f64 = 8.0;
const MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER: f64 = 0.25;
const MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER: f64 = 8.0;
const MIN_GHOSTTY_SCROLLBACK_LIMIT_MB: f64 = 1.0;
const MAX_GHOSTTY_SCROLLBACK_LIMIT_MB: f64 = 200.0;
const GHOSTTY_THEME_UNMANAGED_SENTINEL: &str = "__ghostex_ghostty_theme_unmanaged__";
const GHOSTTY_CONFIG_DEFAULT_RELATIVE_PATH: &str =
    "Library/Application Support/com.mitchellh.ghostty/config.ghostty";
const GHOSTTY_CONFIG_CANDIDATE_RELATIVE_PATHS: &[&str] = &[
    "Library/Application Support/com.mitchellh.ghostty/config.ghostty",
    "Library/Application Support/com.ghostty.org/config.ghostty",
    "Library/Application Support/Ghostty/config.ghostty",
    "Library/Application Support/com.mitchellh.ghostty/config",
    "Library/Application Support/com.ghostty.org/config",
    "Library/Application Support/Ghostty/config",
];
const GHOSTEX_GHOSTTY_CONFIG_BLOCK_START: &str = "# BEGIN Ghostex managed terminal settings";
const GHOSTEX_GHOSTTY_CONFIG_BLOCK_END: &str = "# END Ghostex managed terminal settings";
const GHOSTTY_THEME_MANAGED_COLOR_KEYS: &[&str] = &[
    "background",
    "foreground",
    "palette",
    "selection-background",
    "selection-foreground",
    "cursor-color",
    "cursor-text",
];
const GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES: &[&str] = &[
    "# Applied by Ghostex:",
    "theme = GitHub Dark",
    "background = #000000",
    "foreground = #ffffff",
    "palette = 6=#39c5cf",
    "selection-background = #07284f",
    "cursor-style = bar",
    "cursor-color = #FFFFFF",
    "cursor-style-blink = true",
    "",
    "unfocused-split-opacity = 1",
    "split-divider-color = #8f8f8f",
    "mouse-shift-capture = false",
    "keybind = super+e=toggle_command_palette",
    "macos-option-as-alt = true",
    "shell-integration-features = ssh-env,ssh-terminfo",
    "",
    "font-family = \"JetBrains Mono\"",
    "font-size = 13",
    "adjust-cell-height = 20%",
    "adjust-cell-width = 0",
    "scrollback-limit = 15000000",
    "clipboard-trim-trailing-spaces = true",
    "clipboard-paste-protection = true",
    "copy-on-select = false",
    "confirm-close-surface = false",
    "mouse-hide-while-typing = false",
    "scrollbar = system",
    "mouse-scroll-multiplier = precision:1,discrete:1",
    "font-variation = wght=300",
];

static SHARED_SIDEBAR_SETTINGS_SERVICE: OnceLock<Mutex<SharedSidebarSettingsService>> =
    OnceLock::new();
static GHOSTEX_STORAGE_PATHS: OnceLock<ghostex_paths::GhostexPaths> = OnceLock::new();

/*
CDXC:GPUISettingsService 2026-06-24-10:50:
GPUI must read and persist the shared sidebar settings JSON through the central XDG/GHOSTEX_HOME path resolver. Keep this module as the single GPUI path/read/write contract so Settings UI parity handles `updateSettings` and `sidebarSide` without introducing a second settings store.

CDXC:GPUISettingsService 2026-06-24-10:50:
Rust should parse only the GPUI runtime fields it consumes today: debuggingMode, showBetaFeatures, sidebarDefaultWidthPx, sidebarSide, project-editor auto-sleep fields, legacy external-IDE command fields, and the supported embedded Ghostty surface font-size field. The raw JSON object is preserved for whole-object writes, but this service intentionally does not duplicate the full TypeScript `ghostexSettings` schema.

CDXC:GPUISettingsService 2026-06-24-10:50:
GPUI `updateSettings` handling needs a production write path: accept only JSON object payloads, create the shared state directory, write through an adjacent temp file then rename, skip byte-identical writes, and maintain a monotonic in-memory revision/hash/snapshot signal without logging paths, project names, URLs, commands, environment values, tokens, stdout/stderr, or user-owned content.

CDXC:GPUISettingsService 2026-06-24-11:14:
The real React app-modal host now saves through GPUI, so the service exposes immutable snapshot object reads and a central object write entrypoint. Keep validation at this boundary: only object-shaped Settings payloads may persist, and callers must use the returned snapshot for post-save hydration instead of re-reading the settings file ad hoc.

CDXC:GPUITerminalSettings 2026-06-27-10:10:
The GPUI surface FFI contract only accepts `terminalFontSize` through `ghostty_surface_config_s`; normalize it into `font_size` and keep every other terminal Settings key out of the surface request.

CDXC:GPUITerminalSettings 2026-06-27-10:10:
Font family, theme, cursor, scrollback, clipboard, and mouse settings are Ghostty config-file-backed for future or recreated surfaces. `terminalPastePreviewableImages` is runtime-only and must not be included in config-file change detection or config writes.

CDXC:GPUITerminalSettings 2026-06-27-10:22:
GPUI image paste preview is runtime-only app behavior and defaults on for parity with `ghostex-settings.ts`. Snapshot access must accept only strict JSON booleans for `terminalPastePreviewableImages`, with missing or malformed values resolving to true.

CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-11:39:
GPUI Settings matches macOS for gxserver-owned agent launch policy: `agentAcceptAllEnabled` and `defaultPromptAgentId` remain in shared Settings only as a synchronous render cache. Parse them with the same default/normalization semantics as the TypeScript settings schema so GPUI can compare saves and reconcile gxserver canonical responses without duplicating the full settings model.

CDXC:GPUISettingsGhosttyConfig 2026-06-24-12:24:
GPUI Settings owns a bounded Ghostty config-file writer, not an arbitrary path bridge. Select only the same Application Support config candidates used by macOS, create the preferred `com.mitchellh.ghostty/config.ghostty` file when none exist, replace only Ghostex's marked managed block, and never accept config paths from React or shared Settings JSON.

CDXC:GPUISettingsGhosttyConfig 2026-06-24-12:24:
GPUI can write Ghostty's config file for external Ghostty reloads and future/recreated embedded surfaces, but the current GPUI GhosttyKit wrapper exposes no safe app config reload/update FFI. Do not claim live embedded terminal reload, do not drop running surfaces as a fallback, and surface file write/open failures explicitly without creating a second config file.
*/

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedSettingsAutoSleepTarget {
    CodeEditor,
    Browser,
    ProjectEditor,
}

/*
CDXC:GPUISettingsSidebarSide 2026-06-26-23:35:
GPUI sidebar layout must use the same persisted `sidebarSide` value as macOS and SidebarApp `moveSidebar`: only `left` and `right` are accepted, and missing or malformed values render as left.

CDXC:GPUISettingsSidebarSide 2026-06-26-23:35:
Keep sidebar side in the shared native-sidebar settings object instead of creating a GPUI-only store. Writer helpers must update only `sidebarSide` through the shared object write path so unrelated Settings fields survive native side changes.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedSidebarSide {
    Left,
    Right,
}

impl SharedSidebarSide {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value {
            Some("right") => Self::Right,
            _ => Self::Left,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    pub fn write_to_settings_object(self, object: &mut Map<String, Value>) {
        object.insert(
            "sidebarSide".to_string(),
            Value::String(self.as_str().to_string()),
        );
    }
}

/*
CDXC:GPUISettingsCommandPaneSide 2026-08-16:
The command pane docks below the workspace by default; `commandsPanelSide` may
move it to a right-hand column. Only `bottom` and `right` are accepted, and
missing or malformed values render as bottom. Read-only here: the Settings
modal owns the write path through the shared settings object.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedCommandPaneSide {
    Bottom,
    Right,
}

impl SharedCommandPaneSide {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value {
            Some("right") => Self::Right,
            _ => Self::Bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedTerminalGhosttySurfaceConfig {
    font_size: f32,
}

impl SharedTerminalGhosttySurfaceConfig {
    pub fn font_size(self) -> f32 {
        self.font_size
    }
}

/// How closing a live terminal surface should be confirmed, mirroring the
/// Ghostty `confirm-close-surface` values the shared Settings schema stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedTerminalConfirmCloseSurface {
    /// Confirm only when the cursor is not sitting at a shell prompt.
    True,
    /// Never confirm.
    False,
    /// Always confirm while the process is alive.
    Always,
}

impl SharedTerminalConfirmCloseSurface {
    fn from_normalized(value: &str) -> Self {
        match value {
            "false" => Self::False,
            "always" => Self::Always,
            _ => Self::True,
        }
    }
}

/*
CDXC:GPUITerminalGpuiEngine 2026-07-04:
The GPUI-composited terminal engine (libghostty-vt + TerminalElement) is the
single terminal pipeline on every OS for Agents, command-pane, companion,
restored, and newly launched terminals. The macOS GhosttyKit implementation
remains compiled for now but is not selected at runtime. The composited engine
consumes the shared terminal typography/scrollback/close-confirm settings on
every platform.
*/
#[derive(Clone, Debug, PartialEq)]
pub struct SharedGpuiTerminalEngineSettings {
    pub enabled: bool,
    pub clipboard_trim_trailing_spaces: bool,
    pub copy_on_select: bool,
    pub selection_clipboard_enabled: bool,
    pub cursor_style: String,
    pub cursor_style_blink: bool,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: f32,
    pub ghostty_theme: String,
    pub terminal_background_rgb: Option<[u8; 3]>,
    pub background_image_path: String,
    pub background_image_opacity: f32,
    pub background_image_fit: String,
    pub letter_spacing: f32,
    pub line_height: f32,
    pub mouse_hide_while_typing: bool,
    pub mouse_scroll_multiplier_discrete: f32,
    pub mouse_scroll_multiplier_precision: f32,
    pub scrollbar_visible: bool,
    pub scrollback_limit_bytes: u64,
    pub scroll_to_bottom_when_typing: bool,
    pub confirm_close_surface: SharedTerminalConfirmCloseSurface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedGxserverAgentSettings {
    pub agent_accept_all_enabled: bool,
    pub default_prompt_agent_id: String,
}

impl SharedGxserverAgentSettings {
    pub fn new(agent_accept_all_enabled: bool, default_prompt_agent_id: &str) -> Self {
        Self {
            agent_accept_all_enabled,
            default_prompt_agent_id: normalize_default_prompt_agent_id(Some(
                default_prompt_agent_id,
            )),
        }
    }

    pub fn write_to_settings_object(&self, object: &mut Map<String, Value>) {
        object.insert(
            "agentAcceptAllEnabled".to_string(),
            Value::Bool(self.agent_accept_all_enabled),
        );
        object.insert(
            "defaultPromptAgentId".to_string(),
            Value::String(self.default_prompt_agent_id.clone()),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedAppShotsHotkey {
    BothCommand,
    BothShift,
    BothOption,
    DoubleLeftShift,
    DoubleLeftOption,
}

impl SharedAppShotsHotkey {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value {
            Some("both-shift") => Self::BothShift,
            Some("both-option") => Self::BothOption,
            Some("double-left-shift") => Self::DoubleLeftShift,
            Some("double-left-option") => Self::DoubleLeftOption,
            _ => DEFAULT_APP_SHOTS_HOTKEY,
        }
    }

    pub fn native_code(self) -> i32 {
        match self {
            Self::BothCommand => 0,
            Self::DoubleLeftShift => 1,
            Self::DoubleLeftOption => 2,
            Self::BothShift => 3,
            Self::BothOption => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedAppShotsSettings {
    pub enabled: bool,
    pub hotkey: SharedAppShotsHotkey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedKeepAwakeDurationMinutes {
    UntilTurnedOff,
    TwoHours,
    FiveHours,
}

impl SharedKeepAwakeDurationMinutes {
    pub fn from_minutes(minutes: u64) -> Option<Self> {
        match minutes {
            0 => Some(Self::UntilTurnedOff),
            120 => Some(Self::TwoHours),
            300 => Some(Self::FiveHours),
            _ => None,
        }
    }

    pub fn minutes(self) -> u64 {
        match self {
            Self::UntilTurnedOff => 0,
            Self::TwoHours => 120,
            Self::FiveHours => 300,
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::UntilTurnedOff => "Until turned off",
            Self::TwoHours => "For 2 hours",
            Self::FiveHours => "For 5 hours",
        }
    }
}

pub const KEEP_AWAKE_DURATION_OPTIONS: &[SharedKeepAwakeDurationMinutes] = &[
    SharedKeepAwakeDurationMinutes::UntilTurnedOff,
    SharedKeepAwakeDurationMinutes::TwoHours,
    SharedKeepAwakeDurationMinutes::FiveHours,
];

/// Which built-in buttons the Agents tab strip action cluster draws. Global
/// Actions render alongside whichever of these the user kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedTabStripBuiltInButtons {
    pub show_new_browser: bool,
    pub show_new_chat: bool,
    pub show_new_terminal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedKeepAwakeTitlebarSettings {
    pub feature_enabled: bool,
    pub hide_titlebar_control: bool,
    pub activate_on_external_display: bool,
    pub activate_on_launch: bool,
    pub battery_threshold_percent: f64,
    pub deactivate_below_battery_threshold: bool,
    pub deactivate_on_low_power_mode: bool,
    pub deactivate_on_user_switch: bool,
    pub default_duration_minutes: SharedKeepAwakeDurationMinutes,
    pub allow_display_sleep: bool,
    pub prevent_lid_sleep: bool,
    pub while_working_sessions: bool,
}

impl SharedKeepAwakeTitlebarSettings {
    pub fn titlebar_control_visible(self) -> bool {
        self.feature_enabled && !self.hide_titlebar_control
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedDefaultEditorCommand {
    Code,
    CodeInsiders,
    Codium,
    Cursor,
    Windsurf,
    Zed,
    Zeditor,
    Subl,
    Other,
}

impl SharedDefaultEditorCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::CodeInsiders => "code-insiders",
            Self::Codium => "codium",
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::Zed => "zed",
            Self::Zeditor => "zeditor",
            Self::Subl => "subl",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedDefaultEditorSettings {
    default_editor_command: SharedDefaultEditorCommand,
    editor_command: String,
}

impl SharedDefaultEditorSettings {
    pub fn default_editor_command(&self) -> SharedDefaultEditorCommand {
        self.default_editor_command
    }

    pub fn editor_command(&self) -> &str {
        &self.editor_command
    }
}

pub fn apply_recommended_ghostty_visible_settings(object: &mut Map<String, Value>) {
    insert_string(object, "terminalCursorStyle", "bar");
    insert_string(object, "terminalFontFamily", "JetBrains Mono");
    insert_number(object, "terminalFontSize", 13.0);
    insert_number(object, "terminalFontWeight", 400.0);
    insert_number(object, "terminalLetterSpacing", 0.0);
    insert_number(object, "terminalLineHeight", 1.2);
    insert_number(object, "terminalMouseScrollMultiplierDiscrete", 1.0);
    insert_number(object, "terminalMouseScrollMultiplierPrecision", 1.0);
}

pub fn reset_ghostty_visible_settings_to_defaults(object: &mut Map<String, Value>) {
    insert_string(object, "terminalCursorStyle", "bar");
    insert_string(object, "terminalFontFamily", "JetBrains Mono");
    insert_number(object, "terminalFontSize", 13.0);
    insert_number(object, "terminalFontWeight", 300.0);
    insert_number(object, "terminalLetterSpacing", 0.0);
    insert_number(object, "terminalLineHeight", 1.2);
    insert_number(object, "terminalMouseScrollMultiplierDiscrete", 1.0);
    insert_number(object, "terminalMouseScrollMultiplierPrecision", 1.0);
    insert_bool(object, "terminalScrollToBottomWhenTyping", true);
}

#[derive(Clone, Debug, PartialEq)]
pub struct SharedSidebarSettingsSnapshot {
    revision: u64,
    content_hash: u64,
    // Arc keeps snapshot clones cheap: hot callers (per-frame surface sync,
    // per-log scenario gating) clone the snapshot on every read, so the
    // settings object must not be deep-copied each time.
    object: Arc<Map<String, Value>>,
}

impl SharedSidebarSettingsSnapshot {
    pub fn empty() -> Self {
        Self {
            revision: 0,
            content_hash: hash_bytes(&[]),
            object: Arc::new(Map::new()),
        }
    }

    pub fn from_object(object: Map<String, Value>) -> Self {
        let content_hash = hash_settings_object(&object);
        Self {
            revision: 0,
            content_hash,
            object: Arc::new(object),
        }
    }

    fn with_signal(object: Map<String, Value>, revision: u64, content_hash: u64) -> Self {
        Self {
            revision,
            content_hash,
            object: Arc::new(object),
        }
    }

    #[allow(dead_code)]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[allow(dead_code)]
    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    pub fn object(&self) -> &Map<String, Value> {
        &self.object
    }

    pub fn debugging_mode(&self) -> bool {
        strict_bool_field(&self.object, "debuggingMode") == Some(true)
    }

    /*
    CDXC:GlobalActions 2026-08-01-16:00:
    Which built-in buttons the Agents tab strip draws. Hiding is opt-in per
    button, so a settings file written before this feature keeps every control.
    The pane overflow button has no toggle: it is the only route to the
    remaining pane actions, and hiding it would strand them.
    */
    pub fn tab_strip_built_in_buttons(&self) -> SharedTabStripBuiltInButtons {
        SharedTabStripBuiltInButtons {
            show_new_browser: strict_bool_field(&self.object, "hideTabStripNewBrowserButton")
                != Some(true),
            show_new_chat: strict_bool_field(&self.object, "hideTabStripNewChatButton")
                != Some(true),
            show_new_terminal: strict_bool_field(&self.object, "hideTabStripNewTerminalButton")
                != Some(true),
        }
    }

    pub fn show_beta_features(&self) -> bool {
        strict_bool_field(&self.object, "showBetaFeatures") == Some(true)
    }

    pub fn keep_awake_titlebar_settings(&self) -> SharedKeepAwakeTitlebarSettings {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
        GPUI consumes only the shared Keep Awake fields needed for the titlebar runtime. Match the TypeScript Settings defaults exactly: beta is strict boolean true only, hide and allow-display-sleep are strict booleans with false defaults, and duration normalizes only to 0, 120, or 300 minutes with 0 as the default.

        CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
        GPUI Keep Awake automation consumes the advanced shared Settings fields with the same strict boolean defaults and battery-threshold clamp as `ghostex-settings.ts`. Keep the parsed snapshot narrow and runtime-owned: renderer payloads do not supply commands, paths, shell text, probe output, or persisted Keep Awake state.
        */
        SharedKeepAwakeTitlebarSettings {
            feature_enabled: self.show_beta_features(),
            hide_titlebar_control: strict_bool_field(&self.object, "hideKeepAwakeTitlebarControl")
                .unwrap_or(DEFAULT_HIDE_KEEP_AWAKE_TITLEBAR_CONTROL),
            activate_on_external_display: strict_bool_field(
                &self.object,
                "keepAwakeActivateOnExternalDisplay",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_ACTIVATE_ON_EXTERNAL_DISPLAY),
            activate_on_launch: strict_bool_field(&self.object, "keepAwakeActivateOnLaunch")
                .unwrap_or(DEFAULT_KEEP_AWAKE_ACTIVATE_ON_LAUNCH),
            battery_threshold_percent: normalize_keep_awake_battery_threshold_percent(
                self.object.get("keepAwakeBatteryThresholdPercent"),
            ),
            deactivate_below_battery_threshold: strict_bool_field(
                &self.object,
                "keepAwakeDeactivateBelowBatteryThreshold",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_DEACTIVATE_BELOW_BATTERY_THRESHOLD),
            deactivate_on_low_power_mode: strict_bool_field(
                &self.object,
                "keepAwakeDeactivateOnLowPowerMode",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_LOW_POWER_MODE),
            deactivate_on_user_switch: strict_bool_field(
                &self.object,
                "keepAwakeDeactivateOnUserSwitch",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_USER_SWITCH),
            default_duration_minutes: normalize_keep_awake_duration_minutes(
                self.object.get("keepAwakeDefaultDurationMinutes"),
            ),
            allow_display_sleep: strict_bool_field(&self.object, "keepAwakeAllowDisplaySleep")
                .unwrap_or(DEFAULT_KEEP_AWAKE_ALLOW_DISPLAY_SLEEP),
            prevent_lid_sleep: strict_bool_field(&self.object, "keepAwakePreventLidSleep")
                .unwrap_or(DEFAULT_KEEP_AWAKE_PREVENT_LID_SLEEP),
            while_working_sessions: strict_bool_field(
                &self.object,
                "keepAwakeWhileWorkingSessions",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_WHILE_WORKING_SESSIONS),
        }
    }

    pub fn gxserver_agent_settings(&self) -> SharedGxserverAgentSettings {
        SharedGxserverAgentSettings {
            agent_accept_all_enabled: strict_bool_field(&self.object, "agentAcceptAllEnabled")
                .unwrap_or(DEFAULT_AGENT_ACCEPT_ALL_ENABLED),
            default_prompt_agent_id: normalize_default_prompt_agent_id(
                self.object
                    .get("defaultPromptAgentId")
                    .and_then(Value::as_str),
            ),
        }
    }

    pub fn app_shots_settings(&self) -> SharedAppShotsSettings {
        /*
        CDXC:GPUIAppShots 2026-06-25-23:07:
        GPUI App Shots is disabled unless the shared Settings toggle is explicitly true, and the native hotkey monitor must normalize unsupported saved values back to the macOS default `both-command`. Rust consumes only these two fields so screenshot capture and modifier handling stay native-owned while the React modal remains reused.

        CDXC:GPUIAppShots 2026-06-29-01:29:
        Shared App Shots hotkey parsing must forward both-Shift and both-Option values to the macOS monitor so GPUI honors the same expanded modifier-only capture choices as the reused Settings modal.
        */
        SharedAppShotsSettings {
            enabled: strict_bool_field(&self.object, "appShotsEnabled")
                .unwrap_or(DEFAULT_APP_SHOTS_ENABLED),
            hotkey: SharedAppShotsHotkey::from_settings_value(
                self.object.get("appShotsHotkey").and_then(Value::as_str),
            ),
        }
    }

    pub fn external_editor_settings(&self) -> SharedDefaultEditorSettings {
        /*
        Generic external project actions still read legacy saved editor choices
        without exposing them in Settings. Agents Hub does not use this path;
        it opens catalog-validated files in the owned Source workbench.
        */
        let default_editor_command = normalize_default_editor_command(
            self.object
                .get("defaultEditorCommand")
                .and_then(Value::as_str),
        );
        let custom_editor_command = normalize_custom_default_editor_command(
            self.object
                .get("customDefaultEditorCommand")
                .and_then(Value::as_str),
        );
        let editor_command = if default_editor_command == SharedDefaultEditorCommand::Other {
            if custom_editor_command.is_empty() {
                DEFAULT_DEFAULT_EDITOR_COMMAND.to_string()
            } else {
                custom_editor_command
            }
        } else {
            default_editor_command.as_str().to_string()
        };

        SharedDefaultEditorSettings {
            default_editor_command,
            editor_command,
        }
    }

    pub fn sidebar_default_width_px(&self) -> Option<f32> {
        self.object
            .get("sidebarDefaultWidthPx")
            .and_then(json_value_to_f32)
    }

    pub fn sidebar_side(&self) -> SharedSidebarSide {
        SharedSidebarSide::from_settings_value(
            self.object.get("sidebarSide").and_then(Value::as_str),
        )
    }

    pub fn command_pane_side(&self) -> SharedCommandPaneSide {
        SharedCommandPaneSide::from_settings_value(
            self.object.get("commandsPanelSide").and_then(Value::as_str),
        )
    }

    pub fn terminal_ghostty_surface_config(&self) -> SharedTerminalGhosttySurfaceConfig {
        SharedTerminalGhosttySurfaceConfig {
            font_size: normalize_terminal_font_size(
                self.object
                    .get("terminalFontSize")
                    .and_then(json_number_value_to_f32),
            ),
        }
    }

    pub fn gpui_terminal_engine_settings(&self) -> SharedGpuiTerminalEngineSettings {
        let font_weight = read_finite_number_field(
            &self.object,
            "terminalFontWeight",
            DEFAULT_TERMINAL_FONT_WEIGHT,
        )
        .clamp(MIN_TERMINAL_FONT_WEIGHT, MAX_TERMINAL_FONT_WEIGHT);
        let scrollback_limit_mb = read_finite_number_field(
            &self.object,
            "terminalScrollbackLimitMb",
            DEFAULT_TERMINAL_SCROLLBACK_LIMIT_MB,
        )
        .clamp(
            MIN_GHOSTTY_SCROLLBACK_LIMIT_MB,
            MAX_GHOSTTY_SCROLLBACK_LIMIT_MB,
        );
        SharedGpuiTerminalEngineSettings {
            // CDXC:GPUITerminalGpuiEngine 2026-07-11: the composited
            // libghostty-vt + TerminalElement pipeline is the single terminal
            // renderer on every OS and in every lifecycle state. Keep the
            // GhosttyKit implementation compiled on macOS for now, but do not
            // expose a setting that can select it at runtime.
            enabled: true,
            clipboard_trim_trailing_spaces: read_bool_field(
                &self.object,
                "terminalClipboardTrimTrailingSpaces",
                DEFAULT_TERMINAL_CLIPBOARD_TRIM_TRAILING_SPACES,
            ),
            copy_on_select: normalize_ghostty_copy_on_select(read_string_field(
                &self.object,
                "terminalCopyOnSelect",
                DEFAULT_TERMINAL_COPY_ON_SELECT,
            )) == "clipboard",
            selection_clipboard_enabled: normalize_ghostty_copy_on_select(read_string_field(
                &self.object,
                "terminalCopyOnSelect",
                DEFAULT_TERMINAL_COPY_ON_SELECT,
            )) != "false",
            cursor_style: normalize_terminal_cursor_style(read_string_field(
                &self.object,
                "terminalCursorStyle",
                DEFAULT_TERMINAL_CURSOR_STYLE,
            )),
            cursor_style_blink: read_bool_field(
                &self.object,
                "terminalCursorStyleBlink",
                DEFAULT_TERMINAL_CURSOR_STYLE_BLINK,
            ),
            font_family: normalize_ghostty_font_family(read_string_field(
                &self.object,
                "terminalFontFamily",
                DEFAULT_TERMINAL_FONT_FAMILY,
            )),
            font_size: normalize_terminal_font_size(
                self.object
                    .get("terminalFontSize")
                    .and_then(json_number_value_to_f32),
            ),
            font_weight: font_weight as f32,
            ghostty_theme: normalize_ghostty_theme(read_string_field(
                &self.object,
                "terminalGhosttyTheme",
                DEFAULT_TERMINAL_GHOSTTY_THEME,
            )),
            terminal_background_rgb: normalize_terminal_background_rgb(read_string_field(
                &self.object,
                "workspaceBackgroundColor",
                DEFAULT_TERMINAL_BACKGROUND_COLOR,
            )),
            background_image_path: read_string_field(
                &self.object,
                "terminalBackgroundImage",
                DEFAULT_TERMINAL_BACKGROUND_IMAGE,
            )
            .trim()
            .to_string(),
            background_image_opacity: read_finite_number_field(
                &self.object,
                "terminalBackgroundImageOpacity",
                DEFAULT_TERMINAL_BACKGROUND_IMAGE_OPACITY,
            )
            .clamp(0.0, 1.0) as f32,
            background_image_fit: normalize_terminal_background_image_fit(read_string_field(
                &self.object,
                "terminalBackgroundImageFit",
                DEFAULT_TERMINAL_BACKGROUND_IMAGE_FIT,
            )),
            letter_spacing: read_finite_number_field(
                &self.object,
                "terminalLetterSpacing",
                DEFAULT_TERMINAL_LETTER_SPACING,
            )
            .clamp(MIN_TERMINAL_LETTER_SPACING, MAX_TERMINAL_LETTER_SPACING)
                as f32,
            line_height: read_finite_number_field(
                &self.object,
                "terminalLineHeight",
                DEFAULT_TERMINAL_LINE_HEIGHT,
            )
            .clamp(MIN_TERMINAL_LINE_HEIGHT, MAX_TERMINAL_LINE_HEIGHT)
                as f32,
            mouse_hide_while_typing: read_bool_field(
                &self.object,
                "terminalMouseHideWhileTyping",
                DEFAULT_TERMINAL_MOUSE_HIDE_WHILE_TYPING,
            ),
            mouse_scroll_multiplier_discrete: read_finite_number_field(
                &self.object,
                "terminalMouseScrollMultiplierDiscrete",
                DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_DISCRETE,
            )
            .clamp(
                MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
                MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
            ) as f32,
            mouse_scroll_multiplier_precision: read_finite_number_field(
                &self.object,
                "terminalMouseScrollMultiplierPrecision",
                DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_PRECISION,
            )
            .clamp(
                MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
                MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
            ) as f32,
            scrollbar_visible: normalize_ghostty_scrollbar(read_string_field(
                &self.object,
                "terminalScrollbar",
                DEFAULT_TERMINAL_SCROLLBAR,
            )) != "never",
            scrollback_limit_bytes: (scrollback_limit_mb * 1_000_000.0).round().max(1.0) as u64,
            scroll_to_bottom_when_typing: read_bool_field(
                &self.object,
                "terminalScrollToBottomWhenTyping",
                DEFAULT_TERMINAL_SCROLL_TO_BOTTOM_WHEN_TYPING,
            ),
            confirm_close_surface: SharedTerminalConfirmCloseSurface::from_normalized(
                &normalize_ghostty_confirm_close_surface(read_string_field(
                    &self.object,
                    "terminalConfirmCloseSurface",
                    DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE,
                )),
            ),
        }
    }

    pub fn terminal_paste_previewable_images(&self) -> bool {
        strict_bool_field(&self.object, "terminalPastePreviewableImages")
            .unwrap_or(DEFAULT_TERMINAL_PASTE_PREVIEWABLE_IMAGES)
    }

    pub fn terminal_clipboard_paste_protection(&self) -> bool {
        strict_bool_field(&self.object, "terminalClipboardPasteProtection")
            .unwrap_or(DEFAULT_TERMINAL_CLIPBOARD_PASTE_PROTECTION)
    }

    /*
    CDXC:WebLinkOpenTarget 2026-08-19:
    Command-clicked terminal links, session chat links, and detected dev-server
    rows share one destination. The settings file is written by the sidebar, so
    an install that has not saved settings since the merge still carries the two
    legacy keys: read them in the same precedence the TypeScript normalizer
    uses, or every one of those users would silently jump to whichever default
    this accessor happened to pick.
    */
    pub fn web_links_open_in_app(&self) -> bool {
        web_links_open_in_app_from_object(&self.object)
    }

    pub fn terminal_pane_padding_px(&self) -> (f32, f32) {
        let read_padding = |key| {
            read_finite_number_field(&self.object, key, DEFAULT_TERMINAL_PANE_PADDING_PX)
                .clamp(MIN_TERMINAL_PANE_PADDING_PX, MAX_TERMINAL_PANE_PADDING_PX)
                as f32
        };
        (
            read_padding("terminalPaneHorizontalPaddingPx"),
            read_padding("terminalPaneVerticalPaddingPx"),
        )
    }

    pub fn show_session_id_in_terminal_panes(&self) -> bool {
        strict_bool_field(&self.object, "showSessionIdInTerminalPanes").unwrap_or(false)
    }

    pub fn auto_sleep_duration(&self, target: SharedSettingsAutoSleepTarget) -> Option<Duration> {
        let (enabled_key, minutes_key) = match target {
            SharedSettingsAutoSleepTarget::CodeEditor => (
                "autoSleepCodeEditorEnabled",
                "autoSleepCodeEditorIdleMinutes",
            ),
            SharedSettingsAutoSleepTarget::Browser => (
                "autoSleepBrowserSessionsEnabled",
                "autoSleepBrowserIdleMinutes",
            ),
            SharedSettingsAutoSleepTarget::ProjectEditor => (
                "autoSleepProjectEditorEnabled",
                "autoSleepProjectEditorIdleMinutes",
            ),
        };

        let enabled = self
            .object
            .get(enabled_key)
            .and_then(json_value_to_bool)
            .unwrap_or(true);
        if !enabled {
            return None;
        }

        let minutes = normalize_project_editor_auto_sleep_idle_minutes(
            self.object.get(minutes_key).and_then(json_value_to_f32),
        );
        Some(Duration::from_secs_f64(minutes * 60.0))
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct SharedSidebarSettingsWriteResult {
    pub status: SharedSidebarSettingsWriteStatus,
    pub snapshot: SharedSidebarSettingsSnapshot,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedSidebarSettingsWriteStatus {
    Changed,
    Unchanged,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum SharedSidebarSettingsWriteError {
    MalformedJson,
    ExpectedObject,
    Io(io::Error),
    Serialize(serde_json::Error),
}

impl From<io::Error> for SharedSidebarSettingsWriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for SharedSidebarSettingsWriteError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialize(error)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum SharedGhosttyConfigFileError {
    HomeUnavailable,
    Io(io::Error),
}

impl From<io::Error> for SharedGhosttyConfigFileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedGhosttyConfigFileWriteStatus {
    Changed,
    Unchanged,
}

#[derive(Clone, Debug, PartialEq)]
struct SharedGhosttyTerminalConfigValues {
    adjust_cell_height_percent: f64,
    adjust_cell_width: f64,
    clipboard_paste_protection: bool,
    clipboard_trim_trailing_spaces: bool,
    confirm_close_surface: String,
    copy_on_select: String,
    cursor_style: String,
    cursor_style_blink: bool,
    font_family: String,
    font_size: f64,
    font_variation_weight: Option<i64>,
    ghostty_theme: String,
    mouse_hide_while_typing: bool,
    mouse_scroll_multiplier_discrete: f64,
    mouse_scroll_multiplier_precision: f64,
    scrollback_limit_bytes: i64,
    scrollbar: String,
}

impl SharedGhosttyTerminalConfigValues {
    fn from_settings_object(object: &Map<String, Value>) -> Self {
        let line_height =
            read_finite_number_field(object, "terminalLineHeight", DEFAULT_TERMINAL_LINE_HEIGHT)
                .clamp(MIN_TERMINAL_LINE_HEIGHT, MAX_TERMINAL_LINE_HEIGHT);
        let letter_spacing = read_finite_number_field(
            object,
            "terminalLetterSpacing",
            DEFAULT_TERMINAL_LETTER_SPACING,
        )
        .clamp(MIN_TERMINAL_LETTER_SPACING, MAX_TERMINAL_LETTER_SPACING);
        let font_weight =
            read_finite_number_field(object, "terminalFontWeight", DEFAULT_TERMINAL_FONT_WEIGHT)
                .clamp(MIN_TERMINAL_FONT_WEIGHT, MAX_TERMINAL_FONT_WEIGHT);
        let scrollback_limit_mb = read_finite_number_field(
            object,
            "terminalScrollbackLimitMb",
            DEFAULT_TERMINAL_SCROLLBACK_LIMIT_MB,
        )
        .clamp(
            MIN_GHOSTTY_SCROLLBACK_LIMIT_MB,
            MAX_GHOSTTY_SCROLLBACK_LIMIT_MB,
        );

        Self {
            adjust_cell_height_percent: line_height - 1.0,
            adjust_cell_width: letter_spacing,
            clipboard_paste_protection: read_bool_field(
                object,
                "terminalClipboardPasteProtection",
                DEFAULT_TERMINAL_CLIPBOARD_PASTE_PROTECTION,
            ),
            clipboard_trim_trailing_spaces: read_bool_field(
                object,
                "terminalClipboardTrimTrailingSpaces",
                DEFAULT_TERMINAL_CLIPBOARD_TRIM_TRAILING_SPACES,
            ),
            confirm_close_surface: normalize_ghostty_confirm_close_surface(read_string_field(
                object,
                "terminalConfirmCloseSurface",
                DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE,
            )),
            copy_on_select: normalize_ghostty_copy_on_select(read_string_field(
                object,
                "terminalCopyOnSelect",
                DEFAULT_TERMINAL_COPY_ON_SELECT,
            )),
            cursor_style: normalize_terminal_cursor_style(read_string_field(
                object,
                "terminalCursorStyle",
                DEFAULT_TERMINAL_CURSOR_STYLE,
            )),
            cursor_style_blink: read_bool_field(
                object,
                "terminalCursorStyleBlink",
                DEFAULT_TERMINAL_CURSOR_STYLE_BLINK,
            ),
            font_family: normalize_ghostty_font_family(read_string_field(
                object,
                "terminalFontFamily",
                DEFAULT_TERMINAL_FONT_FAMILY,
            )),
            font_size: f64::from(normalize_terminal_font_size(
                object
                    .get("terminalFontSize")
                    .and_then(json_number_value_to_f32),
            )),
            font_variation_weight: if font_weight == NORMAL_TERMINAL_FONT_WEIGHT {
                None
            } else {
                Some(font_weight.round() as i64)
            },
            ghostty_theme: normalize_ghostty_theme(read_string_field(
                object,
                "terminalGhosttyTheme",
                DEFAULT_TERMINAL_GHOSTTY_THEME,
            )),
            mouse_hide_while_typing: read_bool_field(
                object,
                "terminalMouseHideWhileTyping",
                DEFAULT_TERMINAL_MOUSE_HIDE_WHILE_TYPING,
            ),
            mouse_scroll_multiplier_discrete: read_finite_number_field(
                object,
                "terminalMouseScrollMultiplierDiscrete",
                DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_DISCRETE,
            )
            .clamp(
                MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
                MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
            ),
            mouse_scroll_multiplier_precision: read_finite_number_field(
                object,
                "terminalMouseScrollMultiplierPrecision",
                DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_PRECISION,
            )
            .clamp(
                MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
                MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
            ),
            scrollback_limit_bytes: (scrollback_limit_mb * 1_000_000.0).round() as i64,
            scrollbar: normalize_ghostty_scrollbar(read_string_field(
                object,
                "terminalScrollbar",
                DEFAULT_TERMINAL_SCROLLBAR,
            )),
        }
    }

    fn managed_config_line_entries(&self) -> Vec<(&'static str, String)> {
        let mut lines = vec![
            (
                "font-size",
                format!("font-size = {}", format_ghostty_number(self.font_size)),
            ),
            (
                "adjust-cell-height",
                format!(
                    "adjust-cell-height = {}",
                    format_ghostty_percent(self.adjust_cell_height_percent)
                ),
            ),
            (
                "adjust-cell-width",
                format!(
                    "adjust-cell-width = {}",
                    format_ghostty_number(self.adjust_cell_width)
                ),
            ),
            ("background", "background = #000000".to_string()),
            ("foreground", "foreground = #ffffff".to_string()),
            ("palette", "palette = 6=#39c5cf".to_string()),
            (
                "selection-background",
                "selection-background = #07284f".to_string(),
            ),
            (
                "cursor-style",
                format!("cursor-style = {}", self.cursor_style),
            ),
            ("cursor-color", "cursor-color = #FFFFFF".to_string()),
            (
                "unfocused-split-opacity",
                "unfocused-split-opacity = 1".to_string(),
            ),
            (
                "split-divider-color",
                "split-divider-color = #8f8f8f".to_string(),
            ),
            (
                "mouse-shift-capture",
                "mouse-shift-capture = false".to_string(),
            ),
            (
                "keybind",
                "keybind = super+e=toggle_command_palette".to_string(),
            ),
            (
                "macos-option-as-alt",
                "macos-option-as-alt = true".to_string(),
            ),
            (
                "shell-integration-features",
                "shell-integration-features = ssh-env,ssh-terminfo".to_string(),
            ),
            (
                "scrollback-limit",
                format!("scrollback-limit = {}", self.scrollback_limit_bytes.max(1)),
            ),
            (
                "cursor-style-blink",
                format!(
                    "cursor-style-blink = {}",
                    format_ghostty_bool(self.cursor_style_blink)
                ),
            ),
            (
                "clipboard-trim-trailing-spaces",
                format!(
                    "clipboard-trim-trailing-spaces = {}",
                    format_ghostty_bool(self.clipboard_trim_trailing_spaces)
                ),
            ),
            (
                "clipboard-paste-protection",
                format!(
                    "clipboard-paste-protection = {}",
                    format_ghostty_bool(self.clipboard_paste_protection)
                ),
            ),
            (
                "copy-on-select",
                format!("copy-on-select = {}", self.copy_on_select),
            ),
            (
                "confirm-close-surface",
                format!("confirm-close-surface = {}", self.confirm_close_surface),
            ),
            (
                "mouse-hide-while-typing",
                format!(
                    "mouse-hide-while-typing = {}",
                    format_ghostty_bool(self.mouse_hide_while_typing)
                ),
            ),
            ("scrollbar", format!("scrollbar = {}", self.scrollbar)),
            (
                "mouse-scroll-multiplier",
                format!(
                    "mouse-scroll-multiplier = precision:{},discrete:{}",
                    format_ghostty_number(self.mouse_scroll_multiplier_precision),
                    format_ghostty_number(self.mouse_scroll_multiplier_discrete)
                ),
            ),
        ];
        if !self.font_family.is_empty() {
            lines.insert(
                0,
                (
                    "font-family",
                    format!("font-family = {}", format_ghostty_string(&self.font_family)),
                ),
            );
        }
        if let Some(font_variation_weight) = self.font_variation_weight {
            lines.push((
                "font-variation",
                format!("font-variation = wght={font_variation_weight}"),
            ));
        }
        if !self.ghostty_theme.is_empty() {
            lines.push((
                "theme",
                format!("theme = {}", format_ghostty_string(&self.ghostty_theme)),
            ));
        }
        lines
    }

    fn managed_config_lines_for_keys(&self, keys: &[&str]) -> Vec<String> {
        self.managed_config_line_entries()
            .into_iter()
            .filter(|(key, _)| keys.contains(key))
            .map(|(_, line)| line)
            .collect()
    }

    #[allow(dead_code)] // no caller: the managed ghostty config is written from the settings modal path instead
    fn managed_config_lines(&self) -> Vec<String> {
        self.managed_config_line_entries()
            .into_iter()
            .map(|(_, line)| line)
            .collect()
    }
}

/// Change key for the settings file used to skip re-reading it: mtime plus
/// length from `fs::metadata`. A stat is orders of magnitude cheaper than the
/// read+parse it replaces, which matters because `read_snapshot` runs on hot
/// paths (per-frame surface-host sync and per-call support-log scenario
/// gating) under the global service mutex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SharedSettingsFileIdentity {
    modified: SystemTime,
    len: u64,
}

impl SharedSettingsFileIdentity {
    fn from_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok()?,
            len: metadata.len(),
        })
    }
}

#[derive(Debug)]
pub struct SharedSidebarSettingsService {
    path: PathBuf,
    snapshot: SharedSidebarSettingsSnapshot,
    cached_file_identity: Option<SharedSettingsFileIdentity>,
}

impl SharedSidebarSettingsService {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            snapshot: SharedSidebarSettingsSnapshot::empty(),
            cached_file_identity: None,
        }
    }

    pub fn for_default_path() -> Self {
        let path = shared_sidebar_settings_path();
        maybe_import_legacy_macos_sidebar_settings(&path);
        Self::new(path)
    }

    pub fn read_snapshot(&mut self) -> SharedSidebarSettingsSnapshot {
        // Stat first: when the file identity is unchanged the cached snapshot
        // is current and the read+parse is skipped entirely. The stat is taken
        // before the read so a write racing between the two only leaves a
        // stale identity behind, forcing one redundant re-read on the next
        // call instead of ever serving stale content.
        let identity = SharedSettingsFileIdentity::from_path(&self.path);
        if identity.is_some() && identity == self.cached_file_identity {
            return self.snapshot.clone();
        }
        let read = read_settings_object_from_path(&self.path);
        /*
        CDXC:GPUISettingsRevisionZeroHydrate 2026-07-26:
        Revision is the "GPUI has observed real settings state" signal the React
        app-modal host gates on (`hasNativeSettingsHydrated = revision > 0`). A
        missing, empty, or unreadable settings file reads as empty bytes, whose
        hash equals the initial empty snapshot's hash, so revision stayed 0 and
        Settings hydrated with revision 0 — leaving the modal permanently blank
        with no way to save a first settings file from the UI. The first
        completed read must always publish revision >= 1, even when the file is
        absent and defaults are the canonical state.
        */
        if read.content_hash != self.snapshot.content_hash || self.snapshot.revision == 0 {
            self.snapshot = SharedSidebarSettingsSnapshot::with_signal(
                read.object,
                self.snapshot.revision.wrapping_add(1),
                read.content_hash,
            );
        }
        self.cached_file_identity = identity;
        self.snapshot.clone()
    }

    #[allow(dead_code)]
    pub fn write_json_object_payload(
        &mut self,
        payload: &str,
    ) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
        let value = serde_json::from_str::<Value>(payload)
            .map_err(|_| SharedSidebarSettingsWriteError::MalformedJson)?;
        let object = match value {
            Value::Object(object) => object,
            _ => return Err(SharedSidebarSettingsWriteError::ExpectedObject),
        };
        self.write_json_object(object)
    }

    #[allow(dead_code)]
    pub fn write_json_object(
        &mut self,
        object: Map<String, Value>,
    ) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
        let value = Value::Object(object.clone());
        let bytes = serde_json::to_vec_pretty(&value)?;
        let existing = fs::read(&self.path).ok();

        if existing.as_deref() == Some(bytes.as_slice()) {
            let snapshot = self.apply_observed_settings(object, hash_bytes(&bytes));
            return Ok(SharedSidebarSettingsWriteResult {
                status: SharedSidebarSettingsWriteStatus::Unchanged,
                snapshot,
            });
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = self.atomic_write_temp_path();
        fs::write(&temp_path, &bytes)?;
        if let Err(error) = fs::rename(&temp_path, &self.path) {
            let _ = fs::remove_file(&temp_path);
            return Err(error.into());
        }

        let snapshot = self.apply_observed_settings(object, hash_bytes(&bytes));
        Ok(SharedSidebarSettingsWriteResult {
            status: SharedSidebarSettingsWriteStatus::Changed,
            snapshot,
        })
    }

    pub fn write_sidebar_side(
        &mut self,
        side: SharedSidebarSide,
    ) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
        let mut object = self.read_snapshot().object().clone();
        side.write_to_settings_object(&mut object);
        self.write_json_object(object)
    }

    fn apply_observed_settings(
        &mut self,
        object: Map<String, Value>,
        content_hash: u64,
    ) -> SharedSidebarSettingsSnapshot {
        // A write just went through this service, so the stat-based read cache
        // can no longer vouch for the on-disk file: mtime granularity can be
        // too coarse to distinguish a same-length rewrite. Explicitly drop the
        // cached identity so a write-then-read never serves stale data; the
        // next read_snapshot re-reads once and re-establishes the cache.
        self.cached_file_identity = None;
        if content_hash != self.snapshot.content_hash {
            self.snapshot = SharedSidebarSettingsSnapshot::with_signal(
                object,
                self.snapshot.revision.wrapping_add(1),
                content_hash,
            );
        }
        self.snapshot.clone()
    }

    fn atomic_write_temp_path(&self) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let temp_name = format!(".native-sidebar-settings.{}.{}.tmp", process::id(), stamp);
        self.path
            .parent()
            .map(|parent| parent.join(&temp_name))
            .unwrap_or_else(|| PathBuf::from(temp_name))
    }
}

pub fn shared_sidebar_settings_snapshot() -> SharedSidebarSettingsSnapshot {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.read_snapshot()
}

#[allow(dead_code)]
pub fn write_shared_sidebar_settings_payload(
    payload: &str,
) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.write_json_object_payload(payload)
}

pub fn write_shared_sidebar_settings_object(
    object: Map<String, Value>,
) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.write_json_object(object)
}

#[allow(dead_code)]
pub fn write_shared_sidebar_side(
    side: SharedSidebarSide,
) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.write_sidebar_side(side)
}

fn shared_sidebar_settings_service() -> &'static Mutex<SharedSidebarSettingsService> {
    SHARED_SIDEBAR_SETTINGS_SERVICE
        .get_or_init(|| Mutex::new(SharedSidebarSettingsService::for_default_path()))
}

#[cfg(target_os = "macos")]
fn maybe_import_legacy_macos_sidebar_settings(settings_path: &Path) {
    /*
    CDXC:GPUISettingsMigration 2026-07-12:
    Production Swift builds historically stored Settings only in WKWebView
    localStorage. Match GhostexAppStorage's one-time upgrade behavior when the
    canonical resolved sidebar settings file does not exist:
    inspect only com.madda.ghostex.host localStorage databases, choose the
    richest valid `ghostex-native-settings` object, and atomically establish
    the shared file. Never read production WK data for ~/.ghostex-dev, and
    never replace or merge an existing shared file.
    */
    if settings_path.exists() {
        return;
    }
    if env::var_os("GHOSTEX_HOME")
        .is_some_and(|value| !value.is_empty() && Path::new(&value).is_absolute())
    {
        return;
    }
    let home = &ghostex_storage_paths().home_dir;
    let webkit_root = home.join("Library/WebKit/com.madda.ghostex.host");
    let mut databases = Vec::new();
    collect_legacy_local_storage_databases(&webkit_root, &mut databases);
    let mut selected: Option<(Map<String, Value>, usize, SystemTime, PathBuf)> = None;
    for database in databases {
        let Some(object) = read_legacy_local_storage_settings_object(&database) else {
            continue;
        };
        let score = (object.len() * 1_000)
            + serde_json::to_string(&Value::Object(object.clone()))
                .map(|value| value.len())
                .unwrap_or(0);
        let modified = fs::metadata(&database)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let replace = selected
            .as_ref()
            .is_none_or(|(_, current_score, current_modified, path)| {
                score > *current_score
                    || (score == *current_score && modified > *current_modified)
                    || (score == *current_score
                        && modified == *current_modified
                        && database < *path)
            });
        if replace {
            selected = Some((object, score, modified, database));
        }
    }
    let Some((object, _, _, _)) = selected else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&Value::Object(object)) else {
        return;
    };
    let Some(parent) = settings_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() || settings_path.exists() {
        return;
    }
    let temp_path = parent.join(format!(
        ".native-sidebar-settings.{}.legacy-import.tmp",
        process::id()
    ));
    if fs::write(&temp_path, bytes).is_err() {
        return;
    }
    if settings_path.exists() || fs::rename(&temp_path, settings_path).is_err() {
        let _ = fs::remove_file(temp_path);
    }
}

#[cfg(not(target_os = "macos"))]
fn maybe_import_legacy_macos_sidebar_settings(_settings_path: &Path) {}

#[cfg(target_os = "macos")]
fn collect_legacy_local_storage_databases(root: &Path, databases: &mut Vec<PathBuf>) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if databases.len() >= 128 {
            break;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path.file_name().and_then(|name| name.to_str()) == Some("localstorage.sqlite3")
            {
                databases.push(path);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn read_legacy_local_storage_settings_object(path: &Path) -> Option<Map<String, Value>> {
    let output = process::Command::new("/usr/bin/sqlite3")
        .arg("-readonly")
        .arg(path)
        .arg("select hex(value) from ItemTable where key = 'ghostex-native-settings' limit 1;")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hex = String::from_utf8(output.stdout).ok()?;
    let bytes = legacy_hex_bytes(hex.trim())?;
    legacy_settings_object_from_bytes(&bytes)
}

#[cfg(target_os = "macos")]
fn legacy_hex_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

#[cfg(target_os = "macos")]
fn legacy_settings_object_from_bytes(bytes: &[u8]) -> Option<Map<String, Value>> {
    if let Ok(value) = std::str::from_utf8(bytes)
        && let Ok(Value::Object(object)) =
            serde_json::from_str(value.trim_start_matches('\u{feff}'))
    {
        return Some(object);
    }
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let utf16 = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let value = String::from_utf16(&utf16).ok()?;
    match serde_json::from_str(value.trim_start_matches('\u{feff}')).ok()? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

#[allow(dead_code)] // no caller: config-backed change detection is done per-setting today
pub fn ghostty_terminal_config_backed_settings_changed(
    previous_object: &Map<String, Value>,
    next_object: &Map<String, Value>,
) -> bool {
    !ghostty_terminal_config_backed_setting_keys_changed(previous_object, next_object).is_empty()
}

pub fn ghostty_terminal_config_backed_setting_keys_changed(
    previous_object: &Map<String, Value>,
    next_object: &Map<String, Value>,
) -> Vec<&'static str> {
    let previous_values = SharedGhosttyTerminalConfigValues::from_settings_object(previous_object);
    let next_values = SharedGhosttyTerminalConfigValues::from_settings_object(next_object);
    let mut keys = Vec::new();
    if previous_values.adjust_cell_height_percent != next_values.adjust_cell_height_percent {
        keys.push("adjust-cell-height");
    }
    if previous_values.adjust_cell_width != next_values.adjust_cell_width {
        keys.push("adjust-cell-width");
    }
    if previous_values.clipboard_paste_protection != next_values.clipboard_paste_protection {
        keys.push("clipboard-paste-protection");
    }
    if previous_values.clipboard_trim_trailing_spaces != next_values.clipboard_trim_trailing_spaces
    {
        keys.push("clipboard-trim-trailing-spaces");
    }
    if previous_values.confirm_close_surface != next_values.confirm_close_surface {
        keys.push("confirm-close-surface");
    }
    if previous_values.copy_on_select != next_values.copy_on_select {
        keys.push("copy-on-select");
    }
    if previous_values.cursor_style != next_values.cursor_style {
        keys.push("cursor-style");
    }
    if previous_values.cursor_style_blink != next_values.cursor_style_blink {
        keys.push("cursor-style-blink");
    }
    if previous_values.font_family != next_values.font_family {
        keys.push("font-family");
    }
    if previous_values.font_size != next_values.font_size {
        keys.push("font-size");
    }
    if previous_values.font_variation_weight != next_values.font_variation_weight {
        keys.push("font-variation");
    }
    if previous_values.ghostty_theme != next_values.ghostty_theme {
        keys.push("theme");
    }
    if previous_values.mouse_hide_while_typing != next_values.mouse_hide_while_typing {
        keys.push("mouse-hide-while-typing");
    }
    if previous_values.mouse_scroll_multiplier_discrete
        != next_values.mouse_scroll_multiplier_discrete
        || previous_values.mouse_scroll_multiplier_precision
            != next_values.mouse_scroll_multiplier_precision
    {
        keys.push("mouse-scroll-multiplier");
    }
    if previous_values.scrollback_limit_bytes != next_values.scrollback_limit_bytes {
        keys.push("scrollback-limit");
    }
    if previous_values.scrollbar != next_values.scrollbar {
        keys.push("scrollbar");
    }
    keys
}

pub fn write_ghostty_terminal_config_from_settings_object(
    object: &Map<String, Value>,
    changed_keys: &[&str],
) -> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    let values = SharedGhosttyTerminalConfigValues::from_settings_object(object);
    merge_selected_ghostty_config_file(|existing_config| {
        merge_ghostty_terminal_settings(existing_config, &values, changed_keys)
    })
}

pub fn apply_recommended_ghostty_config_file()
-> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    merge_selected_ghostty_config_file(|existing_config| {
        merge_ghostty_config_lines(existing_config, GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES)
    })
}

pub fn reset_ghostty_config_file_to_defaults()
-> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    merge_selected_ghostty_config_file(|existing_config| {
        merge_ghostty_config_lines(existing_config, &[])
    })
}

pub fn prepare_ghostty_config_file_for_open() -> Result<PathBuf, SharedGhosttyConfigFileError> {
    let path = selected_ghostty_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            atomic_write_file(&path, b"")?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}

fn merge_selected_ghostty_config_file<F>(
    merge: F,
) -> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError>
where
    F: FnOnce(&str) -> String,
{
    let path = selected_ghostty_config_path()?;
    let existing_config = match fs::read_to_string(&path) {
        Ok(config) => config,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let merged_config = merge(&existing_config);
    if existing_config == merged_config {
        return Ok(SharedGhosttyConfigFileWriteStatus::Unchanged);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write_file(&path, merged_config.as_bytes())?;
    Ok(SharedGhosttyConfigFileWriteStatus::Changed)
}

pub(crate) fn selected_ghostty_config_path() -> Result<PathBuf, SharedGhosttyConfigFileError> {
    selected_ghostty_config_path_from_home(env::var_os("HOME"))
        .ok_or(SharedGhosttyConfigFileError::HomeUnavailable)
}

fn selected_ghostty_config_path_from_home(home: Option<OsString>) -> Option<PathBuf> {
    let home = PathBuf::from(home?);
    let candidates = ghostty_config_candidate_paths_from_home(&home);
    candidates
        .iter()
        .find(|candidate| candidate.exists())
        .cloned()
        .or_else(|| Some(home.join(Path::new(GHOSTTY_CONFIG_DEFAULT_RELATIVE_PATH))))
}

fn ghostty_config_candidate_paths_from_home(home: &Path) -> Vec<PathBuf> {
    GHOSTTY_CONFIG_CANDIDATE_RELATIVE_PATHS
        .iter()
        .map(|relative_path| home.join(Path::new(relative_path)))
        .collect()
}

fn merge_ghostty_config_lines(config: &str, managed_lines: &[&str]) -> String {
    merge_ghostty_managed_config_block(
        config,
        managed_lines
            .iter()
            .map(|line| (*line).to_string())
            .collect(),
    )
}

fn merge_ghostty_terminal_settings(
    config: &str,
    values: &SharedGhosttyTerminalConfigValues,
    changed_keys: &[&str],
) -> String {
    let mut replacing_keys = changed_keys.to_vec();
    if changed_keys.contains(&"theme") {
        for key in GHOSTTY_THEME_MANAGED_COLOR_KEYS {
            if !replacing_keys.contains(key) {
                replacing_keys.push(key);
            }
        }
    }
    merge_ghostty_managed_config_block_entries(
        config,
        &replacing_keys,
        values.managed_config_lines_for_keys(changed_keys),
    )
}

fn merge_ghostty_managed_config_block(config: &str, managed_lines: Vec<String>) -> String {
    let mut retained_lines = Vec::new();
    let mut inside_ghostex_block = false;
    for line in config.lines() {
        let marker = line.trim();
        if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_START {
            inside_ghostex_block = true;
            continue;
        }
        if inside_ghostex_block {
            if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_END {
                inside_ghostex_block = false;
            }
            continue;
        }
        retained_lines.push(line.to_string());
    }
    let mut next_lines = retained_lines;
    trim_trailing_blank_ghostty_config_lines(&mut next_lines);
    if managed_lines.is_empty() {
        if next_lines.is_empty() {
            return String::new();
        }
        return format!("{}\n", next_lines.join("\n"));
    }
    if next_lines
        .last()
        .is_some_and(|line| !line.trim().is_empty())
    {
        next_lines.push(String::new());
    }
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_START.to_string());
    next_lines.extend(managed_lines);
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_END.to_string());
    if next_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", next_lines.join("\n"))
    }
}

fn merge_ghostty_managed_config_block_entries(
    config: &str,
    replacing_keys: &[&str],
    managed_lines: Vec<String>,
) -> String {
    let mut retained_lines = Vec::new();
    let mut retained_block_lines = Vec::new();
    let mut inside_ghostex_block = false;
    for line in config.lines() {
        let marker = line.trim();
        if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_START {
            inside_ghostex_block = true;
            continue;
        }
        if inside_ghostex_block {
            if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_END {
                inside_ghostex_block = false;
            } else if !replacing_keys.contains(&read_ghostty_config_key(line).as_str()) {
                retained_block_lines.push(line.to_string());
            }
            continue;
        }
        retained_lines.push(line.to_string());
    }
    trim_trailing_blank_ghostty_config_lines(&mut retained_lines);
    let mut next_block_lines = retained_block_lines;
    next_block_lines.extend(managed_lines);
    trim_trailing_blank_ghostty_config_lines(&mut next_block_lines);
    if next_block_lines.is_empty() {
        if retained_lines.is_empty() {
            return String::new();
        }
        return format!("{}\n", retained_lines.join("\n"));
    }

    let mut next_lines = retained_lines;
    if next_lines
        .last()
        .is_some_and(|line| !line.trim().is_empty())
    {
        next_lines.push(String::new());
    }
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_START.to_string());
    next_lines.extend(next_block_lines);
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_END.to_string());
    format!("{}\n", next_lines.join("\n"))
}

fn trim_trailing_blank_ghostty_config_lines(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
}

fn read_ghostty_config_key(line: &str) -> String {
    let trimmed_line = line.trim();
    if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
        return String::new();
    }
    trimmed_line
        .split_once('=')
        .map(|(key, _)| key.trim().to_string())
        .unwrap_or_else(|| trimmed_line.trim().to_string())
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = atomic_temp_path(path);
    fs::write(&temp_path, bytes)?;
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

fn atomic_temp_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "config.ghostty".into());
    let temp_name = format!(".{file_name}.{}.{}.tmp", process::id(), stamp);
    path.parent()
        .map(|parent| parent.join(&temp_name))
        .unwrap_or_else(|| PathBuf::from(temp_name))
}

pub fn ghostex_storage_paths() -> &'static ghostex_paths::GhostexPaths {
    GHOSTEX_STORAGE_PATHS.get_or_init(|| {
        let paths = ghostex_paths::GhostexPaths::resolve();
        if let Err(error) = paths.migrate_legacy_layout() {
            eprintln!("Ghostex could not migrate legacy storage: {error}");
        }
        paths
    })
}

pub fn shared_sidebar_settings_path() -> PathBuf {
    ghostex_storage_paths().sidebar_settings_file()
}

pub fn normalize_project_editor_auto_sleep_idle_minutes(value: Option<f32>) -> f64 {
    value
        .map(f64::from)
        .filter(|minutes| minutes.is_finite() && *minutes > 0.0)
        .map(|minutes| minutes.min(PROJECT_EDITOR_AUTO_SLEEP_MAX_IDLE_MINUTES))
        .unwrap_or(PROJECT_EDITOR_AUTO_SLEEP_DEFAULT_IDLE_MINUTES)
}

pub fn normalize_terminal_font_size(value: Option<f32>) -> f32 {
    value
        .filter(|font_size| font_size.is_finite())
        .map(|font_size| font_size.clamp(MIN_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE))
        .unwrap_or(DEFAULT_TERMINAL_FONT_SIZE)
}

pub fn normalize_default_prompt_agent_id(value: Option<&str>) -> String {
    let normalized = value
        .unwrap_or("")
        .trim()
        .chars()
        .take(MAX_DEFAULT_PROMPT_AGENT_ID_LEN)
        .collect::<String>();
    if normalized.is_empty() {
        DEFAULT_PROMPT_AGENT_ID.to_string()
    } else {
        normalized
    }
}

pub fn normalize_default_editor_command(value: Option<&str>) -> SharedDefaultEditorCommand {
    match value.unwrap_or(DEFAULT_DEFAULT_EDITOR_COMMAND) {
        "code-insiders" => SharedDefaultEditorCommand::CodeInsiders,
        "zed" => SharedDefaultEditorCommand::Zed,
        "zeditor" => SharedDefaultEditorCommand::Zeditor,
        "cursor" => SharedDefaultEditorCommand::Cursor,
        "windsurf" => SharedDefaultEditorCommand::Windsurf,
        "codium" => SharedDefaultEditorCommand::Codium,
        "subl" => SharedDefaultEditorCommand::Subl,
        "other" => SharedDefaultEditorCommand::Other,
        _ => SharedDefaultEditorCommand::Code,
    }
}

pub fn normalize_custom_default_editor_command(value: Option<&str>) -> String {
    value
        .unwrap_or("")
        .trim()
        .chars()
        .take(MAX_CUSTOM_DEFAULT_EDITOR_COMMAND_CHARS)
        .collect()
}

fn read_bool_field(object: &Map<String, Value>, key: &str, fallback: bool) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn read_finite_number_field(object: &Map<String, Value>, key: &str, fallback: f64) -> f64 {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn read_string_field<'a>(object: &'a Map<String, Value>, key: &str, fallback: &'a str) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

fn normalize_terminal_cursor_style(value: &str) -> String {
    match value {
        "block" | "underline" => value.to_string(),
        _ => DEFAULT_TERMINAL_CURSOR_STYLE.to_string(),
    }
}

fn normalize_ghostty_font_family(value: &str) -> String {
    value.trim().to_string()
}

fn normalize_ghostty_theme(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == GHOSTTY_THEME_UNMANAGED_SENTINEL {
        String::new()
    } else {
        trimmed.to_string()
    }
}

fn normalize_terminal_background_rgb(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = [
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ];
    Some(if rgb == [0, 0, 0] { [1, 1, 1] } else { rgb })
}

fn normalize_terminal_background_image_fit(value: &str) -> String {
    match value.trim() {
        "contain" | "stretch" | "natural" => value.trim().to_string(),
        _ => DEFAULT_TERMINAL_BACKGROUND_IMAGE_FIT.to_string(),
    }
}

fn normalize_ghostty_copy_on_select(value: &str) -> String {
    match value {
        "true" | "clipboard" => value.to_string(),
        _ => DEFAULT_TERMINAL_COPY_ON_SELECT.to_string(),
    }
}

fn normalize_ghostty_confirm_close_surface(value: &str) -> String {
    match value {
        "false" | "true" | "always" => value.to_string(),
        _ => DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE.to_string(),
    }
}

fn normalize_ghostty_scrollbar(value: &str) -> String {
    if value == "never" {
        "never".to_string()
    } else {
        DEFAULT_TERMINAL_SCROLLBAR.to_string()
    }
}

fn format_ghostty_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn format_ghostty_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn format_ghostty_number(value: f64) -> String {
    if value.round() == value {
        return (value as i64).to_string();
    }
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn format_ghostty_percent(value: f64) -> String {
    format!("{}%", format_ghostty_number(value * 100.0))
}

fn insert_string(object: &mut Map<String, Value>, key: &str, value: &str) {
    object.insert(key.to_string(), Value::String(value.to_string()));
}

fn insert_bool(object: &mut Map<String, Value>, key: &str, value: bool) {
    object.insert(key.to_string(), Value::Bool(value));
}

fn insert_number(object: &mut Map<String, Value>, key: &str, value: f64) {
    let number = serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0));
    object.insert(key.to_string(), Value::Number(number));
}

fn read_settings_object_from_path(path: &Path) -> SharedSidebarSettingsRead {
    let bytes = fs::read(path).unwrap_or_default();
    let content_hash = hash_bytes(&bytes);
    let object = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|value| match value {
            Value::Object(object) => Some(object),
            _ => None,
        })
        .unwrap_or_default();

    SharedSidebarSettingsRead {
        object,
        content_hash,
    }
}

struct SharedSidebarSettingsRead {
    object: Map<String, Value>,
    content_hash: u64,
}

fn hash_settings_object(object: &Map<String, Value>) -> u64 {
    serde_json::to_vec(&Value::Object(object.clone()))
        .map(|bytes| hash_bytes(&bytes))
        .unwrap_or_else(|_| hash_bytes(&[]))
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

#[allow(dead_code)] // timestamp formatting helper kept as a pair with shared_settings_civil_from_days
fn shared_settings_iso8601_utc(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_seconds = duration.as_secs() as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = shared_settings_civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    )
}

pub(crate) fn shared_settings_civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[allow(dead_code)] // timestamp formatting helper kept as a pair with shared_settings_civil_from_days
fn shared_settings_iso8601_utc_millis_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 24
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'.'
        && bytes[23] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19 | 23) || byte.is_ascii_digit()
        })
}

fn strict_bool_field(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key)?.as_bool()
}

pub fn web_links_open_in_app_from_object(object: &Map<String, Value>) -> bool {
    if let Some(target) = object.get("webLinkOpenTarget").and_then(Value::as_str) {
        match target {
            "internal-browser" => return true,
            "system-default-browser" => return false,
            _ => {}
        }
    }
    if let Some(open_in_app) = strict_bool_field(object, "openTerminalLinksInApp") {
        return open_in_app;
    }
    match object
        .get("terminalDevServerOpenTarget")
        .and_then(Value::as_str)
    {
        Some("internal-browser") => true,
        Some("system-default-browser") => false,
        _ => DEFAULT_WEB_LINKS_OPEN_IN_APP,
    }
}

fn normalize_keep_awake_duration_minutes(value: Option<&Value>) -> SharedKeepAwakeDurationMinutes {
    let Some(minutes) = value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
    else {
        return DEFAULT_KEEP_AWAKE_DURATION_MINUTES;
    };
    if (minutes - 0.0).abs() < f64::EPSILON {
        SharedKeepAwakeDurationMinutes::UntilTurnedOff
    } else if (minutes - 120.0).abs() < f64::EPSILON {
        SharedKeepAwakeDurationMinutes::TwoHours
    } else if (minutes - 300.0).abs() < f64::EPSILON {
        SharedKeepAwakeDurationMinutes::FiveHours
    } else {
        DEFAULT_KEEP_AWAKE_DURATION_MINUTES
    }
}

fn normalize_keep_awake_battery_threshold_percent(value: Option<&Value>) -> f64 {
    let Some(percent) = value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
    else {
        return DEFAULT_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT;
    };
    percent.clamp(
        MIN_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT,
        MAX_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT,
    )
}

fn json_value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(text) if text.eq_ignore_ascii_case("true") => Some(true),
        Value::String(text) if text.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

fn json_value_to_f32(value: &Value) -> Option<f32> {
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number as f32)
}

fn json_number_value_to_f32(value: &Value) -> Option<f32> {
    let number = value.as_f64()?;
    number.is_finite().then_some(number as f32)
}
