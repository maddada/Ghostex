use std::io;

use super::*;
use serde_json::{Map, Value};

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
pub(crate) struct SharedGhosttyTerminalConfigValues {
    pub(crate) adjust_cell_height_percent: f64,
    pub(crate) adjust_cell_width: f64,
    pub(crate) clipboard_paste_protection: bool,
    pub(crate) clipboard_trim_trailing_spaces: bool,
    pub(crate) confirm_close_surface: String,
    pub(crate) copy_on_select: String,
    pub(crate) cursor_style: String,
    pub(crate) cursor_style_blink: bool,
    pub(crate) font_family: String,
    pub(crate) font_size: f64,
    pub(crate) font_variation_weight: Option<i64>,
    pub(crate) ghostty_theme: String,
    pub(crate) mouse_hide_while_typing: bool,
    pub(crate) mouse_scroll_multiplier_discrete: f64,
    pub(crate) mouse_scroll_multiplier_precision: f64,
    pub(crate) scrollback_limit_bytes: i64,
    pub(crate) scrollbar: String,
}

impl SharedGhosttyTerminalConfigValues {
    pub(crate) fn from_settings_object(object: &Map<String, Value>) -> Self {
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
                // The size Interface scale gives the terminal (CDXC:Settings 2026-11-05 in the catalog).
                ghostex_settings_catalog::interface_scale::scaled_area_value(
                    object,
                    "terminalFontSize",
                )
                .map(|size| size as f32),
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

    pub(crate) fn managed_config_lines_for_keys(&self, keys: &[&str]) -> Vec<String> {
        self.managed_config_line_entries()
            .into_iter()
            .filter(|(key, _)| keys.contains(key))
            .map(|(_, line)| line)
            .collect()
    }
}
