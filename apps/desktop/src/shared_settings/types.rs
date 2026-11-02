use super::*;
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedSettingsAutoSleepTarget {
    CodeEditor,
    Browser,
    ProjectEditor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedSidebarVisibilityMemory {
    Shared,
    PerView,
}

impl SharedSidebarVisibilityMemory {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value {
            Some("perView") => Self::PerView,
            _ => Self::Shared,
        }
    }
}

/// The view a transcript menu row asks for (Open in Files, Open in Code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedChatFileOpenView {
    Docs,
    Code,
}

/// Where a file opens from Quick Access, a chat link or a terminal link
/// (`documentFileOpenTarget`, `otherFileOpenTarget`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharedFileOpenTarget {
    /// The Files view.
    Files,
    /// The built-in Code view.
    Code,
    /// An "Open In" editor app on this computer, by its built-in target id (`vscode`, `cursor`, …).
    Editor(String),
    /// The operating system's own app for the file.
    SystemApp,
}

impl SharedFileOpenTarget {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("code") => Self::Code,
            Some("system-app") => Self::SystemApp,
            Some(id)
                if !id.is_empty()
                    && id != "files"
                    && ghostex_settings_catalog::file_opening::file_open_target_options()
                        .iter()
                        .any(|option| option.value == id) =>
            {
                Self::Editor(id.to_string())
            }
            _ => Self::Files,
        }
    }
}

/// The kinds `documentFileOpenTarget` covers.
pub use ghostex_settings_catalog::file_opening::DocumentFileKind as SharedDocumentFileKind;

/// Where a video or audio file link opens (`videoFileOpenTarget`, `audioFileOpenTarget`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedMediaFileOpenTarget {
    Files,
    SystemApp,
}

impl SharedMediaFileOpenTarget {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value {
            Some("system-app") => Self::SystemApp,
            _ => Self::Files,
        }
    }
}

/*
CDXC:CommandPane 2026-08-16:
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
    pub(crate) fn from_normalized(value: &str) -> Self {
        match value {
            "false" => Self::False,
            "always" => Self::Always,
            _ => Self::True,
        }
    }
}

/*
CDXC:Terminal 2026-10-21 DECISION:
User chose to remove the old GhosttyKit native-surface terminal (kept compiled since 2026-07-11 "for now") to shrink the app. The GPUI-composited engine (libghostty-vt + TerminalElement) is the only terminal pipeline on every OS for Agents, command-pane, restored, and newly launched terminals, and it consumes the shared terminal typography/scrollback/close-confirm settings on every platform. GhosttyKit stays linked on macOS only for config loading.
*/
#[derive(Clone, Debug, PartialEq)]
pub struct SharedGpuiTerminalEngineSettings {
    pub clipboard_trim_trailing_spaces: bool,
    pub copy_on_select: bool,
    pub selection_clipboard_enabled: bool,
    pub cursor_style: String,
    pub cursor_style_blink: bool,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: f32,
    pub ghostty_theme: String,
    pub color_scheme: String,
    pub light_theme: String,
    pub terminal_background: SharedTerminalBackground,
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
    pub bold_uses_heavier_font: bool,
    pub confirm_close_surface: SharedTerminalConfirmCloseSurface,
}

impl SharedGpuiTerminalEngineSettings {
    pub fn uses_light_theme(&self, system_is_light: bool) -> bool {
        self.color_scheme == "light" || (self.color_scheme == "system" && system_is_light)
    }

    /// The colour a terminal paints behind its cells: the Terminal background choice for the
    /// terminal's appearance, or the theme's colour when that choice follows the theme. The pane
    /// body paints this same colour around the grid (`GPUI_TERMINAL_PADDING_BACKGROUND_RGB`).
    pub fn grid_background_rgb(&self, light: bool, theme_background: [u8; 3]) -> [u8; 3] {
        self.terminal_background
            .override_rgb(light)
            .unwrap_or(theme_background)
    }
}

/// The Terminal background choice (`terminalBackgroundMode` plus `workspaceBackgroundColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedTerminalBackground {
    /// Pure black behind dark terminals, pure white behind light ones.
    Pure,
    Theme,
    /// Painted in dark mode only; light terminals keep the theme.
    Custom([u8; 3]),
}

impl SharedTerminalBackground {
    /// The colour behind terminal cells, or `None` to paint the theme's content colour.
    pub fn override_rgb(self, light: bool) -> Option<[u8; 3]> {
        match self {
            Self::Pure => Some(if light { [0xff, 0xff, 0xff] } else { [0, 0, 0] }),
            Self::Theme => None,
            Self::Custom(rgb) => (!light).then_some(rgb),
        }
    }
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
}

/// Which built-in buttons the Agents tab strip action cluster draws. Global
/// Actions render alongside whichever of these the user kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedTabStripBuiltInButtons {
    pub show_new_browser: bool,
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
    pub(crate) default_editor_command: SharedDefaultEditorCommand,
    pub(crate) editor_command: String,
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
    insert_bool(object, "terminalBoldUsesHeavierFont", true);
}
