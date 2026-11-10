use super::*;

pub const PROJECT_EDITOR_AUTO_SLEEP_DEFAULT_IDLE_MINUTES: f64 = 5.0;
pub const PROJECT_EDITOR_AUTO_SLEEP_MAX_IDLE_MINUTES: f64 = 300.0;
pub const DEFAULT_TERMINAL_FONT_SIZE: f32 = 13.0;
pub const MIN_TERMINAL_FONT_SIZE: f32 = 8.0;
pub const MAX_TERMINAL_FONT_SIZE: f32 = 32.0;
pub const DEFAULT_AGENT_ACCEPT_ALL_ENABLED: bool = false;
pub const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
pub const MAX_DEFAULT_PROMPT_AGENT_ID_LEN: usize = 120;
pub const DEFAULT_DEFAULT_EDITOR_COMMAND: &str = "code";
pub const DEFAULT_KEEP_AWAKE_DURATION_MINUTES: SharedKeepAwakeDurationMinutes =
    SharedKeepAwakeDurationMinutes::UntilTurnedOff;
pub const DEFAULT_KEEP_AWAKE_ALLOW_DISPLAY_SLEEP: bool = false;
pub const DEFAULT_KEEP_AWAKE_ACTIVATE_ON_EXTERNAL_DISPLAY: bool = false;
pub const DEFAULT_KEEP_AWAKE_ACTIVATE_ON_LAUNCH: bool = false;
pub const DEFAULT_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT: f64 = 0.0;
pub const DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_LOW_POWER_MODE: bool = false;
pub const DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_USER_SWITCH: bool = false;
pub const DEFAULT_KEEP_AWAKE_PREVENT_LID_SLEEP: bool = false;
pub const DEFAULT_KEEP_AWAKE_WHILE_WORKING_SESSIONS: bool = false;
pub const DEFAULT_HIDE_KEEP_AWAKE_TITLEBAR_CONTROL: bool = false;
pub const DEFAULT_GHOSTEX_CAPTURE_ENABLED: bool = false;
pub const DEFAULT_GHOSTEX_CAPTURE_SWITCH_TO_SESSION: bool = true;
/// `agentboxDefaultLocation`: new threads run on this computer unless the user picks a box.
pub const DEFAULT_AGENTBOX_LOCATION: &str = "local";
/// The agentbox providers a box location may name, in the order Settings and the picker list them.
pub const AGENTBOX_PROVIDER_IDS: [&str; 6] = [
    "docker",
    "hetzner",
    "vercel",
    "daytona",
    "e2b",
    "digitalocean",
];
pub(crate) const MIN_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT: f64 = 10.0;
pub(crate) const MAX_KEEP_AWAKE_BATTERY_THRESHOLD_PERCENT: f64 = 90.0;
pub(crate) const MAX_CUSTOM_DEFAULT_EDITOR_COMMAND_CHARS: usize = 240;
pub(crate) const DEFAULT_TERMINAL_CURSOR_STYLE: &str = "bar";
pub(crate) const DEFAULT_TERMINAL_FONT_FAMILY: &str = "JetBrains Mono";
pub(crate) const DEFAULT_TERMINAL_FONT_WEIGHT: f64 = 300.0;
pub(crate) const NORMAL_TERMINAL_FONT_WEIGHT: f64 = 400.0;
pub(crate) const DEFAULT_TERMINAL_GHOSTTY_THEME: &str = "GitHub Dark";
/// Empty: no custom terminal background colour (`workspaceBackgroundColor` in
/// packages/settings-catalog/src/data/defaults.rs).
pub(crate) const DEFAULT_TERMINAL_BACKGROUND_COLOR: &str = "";
pub(crate) const DEFAULT_TERMINAL_BACKGROUND_IMAGE: &str = "";
pub(crate) const DEFAULT_TERMINAL_BACKGROUND_IMAGE_OPACITY: f64 = 1.0;
pub(crate) const DEFAULT_TERMINAL_BACKGROUND_IMAGE_FIT: &str = "cover";
pub(crate) const DEFAULT_TERMINAL_LETTER_SPACING: f64 = 0.0;
pub(crate) const DEFAULT_TERMINAL_LINE_HEIGHT: f64 = 1.2;
pub(crate) const DEFAULT_TERMINAL_CURSOR_STYLE_BLINK: bool = true;
pub(crate) const DEFAULT_TERMINAL_SCROLLBACK_LIMIT_MB: f64 = 15.0;
pub(crate) const DEFAULT_TERMINAL_COPY_ON_SELECT: &str = "false";
pub(crate) const DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE: &str = "false";
pub(crate) const DEFAULT_TERMINAL_CLIPBOARD_TRIM_TRAILING_SPACES: bool = true;
pub(crate) const DEFAULT_TERMINAL_CLIPBOARD_PASTE_PROTECTION: bool = true;
pub(crate) const DEFAULT_TERMINAL_PASTE_PREVIEWABLE_IMAGES: bool = true;
pub(crate) const DEFAULT_TERMINAL_MOUSE_HIDE_WHILE_TYPING: bool = false;
pub(crate) const DEFAULT_TERMINAL_SCROLLBAR: &str = "system";
pub(crate) const DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_DISCRETE: f64 = 1.0;
pub(crate) const DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_PRECISION: f64 = 1.0;
pub(crate) const DEFAULT_TERMINAL_SCROLL_TO_BOTTOM_WHEN_TYPING: bool = true;
pub(crate) const DEFAULT_TERMINAL_BOLD_USES_HEAVIER_FONT: bool = true;
pub(crate) const DEFAULT_WEB_LINKS_OPEN_IN_APP: bool = true;
pub(crate) const DEFAULT_TERMINAL_WIDTH_APPLY_TO_COMMAND_PANE_TERMINALS: bool = false;
pub(crate) const MIN_TERMINAL_FONT_WEIGHT: f64 = 100.0;
pub(crate) const MAX_TERMINAL_FONT_WEIGHT: f64 = 900.0;
pub(crate) const MIN_TERMINAL_LINE_HEIGHT: f64 = 0.8;
pub(crate) const MAX_TERMINAL_LINE_HEIGHT: f64 = 2.0;
pub(crate) const MIN_TERMINAL_LETTER_SPACING: f64 = -2.0;
pub(crate) const MAX_TERMINAL_LETTER_SPACING: f64 = 8.0;
pub(crate) const MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER: f64 = 0.25;
pub(crate) const MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER: f64 = 8.0;
pub(crate) const MIN_GHOSTTY_SCROLLBACK_LIMIT_MB: f64 = 1.0;
pub(crate) const MAX_GHOSTTY_SCROLLBACK_LIMIT_MB: f64 = 200.0;
pub(crate) const GHOSTTY_THEME_UNMANAGED_SENTINEL: &str = "__ghostex_ghostty_theme_unmanaged__";
pub(crate) const GHOSTTY_CONFIG_DEFAULT_RELATIVE_PATH: &str =
    "Library/Application Support/com.mitchellh.ghostty/config.ghostty";
pub(crate) const GHOSTTY_CONFIG_CANDIDATE_RELATIVE_PATHS: &[&str] = &[
    "Library/Application Support/com.mitchellh.ghostty/config.ghostty",
    "Library/Application Support/com.ghostty.org/config.ghostty",
    "Library/Application Support/Ghostty/config.ghostty",
    "Library/Application Support/com.mitchellh.ghostty/config",
    "Library/Application Support/com.ghostty.org/config",
    "Library/Application Support/Ghostty/config",
];
pub(crate) const GHOSTEX_GHOSTTY_CONFIG_BLOCK_START: &str =
    "# BEGIN Ghostex managed terminal settings";
pub(crate) const GHOSTEX_GHOSTTY_CONFIG_BLOCK_END: &str = "# END Ghostex managed terminal settings";
pub(crate) const GHOSTTY_THEME_MANAGED_COLOR_KEYS: &[&str] = &[
    "background",
    "foreground",
    "palette",
    "selection-background",
    "selection-foreground",
    "cursor-color",
    "cursor-text",
];
pub(crate) const GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES: &[&str] = &[
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
