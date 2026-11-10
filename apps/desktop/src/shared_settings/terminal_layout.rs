//! The terminal's padding and its Terminal width (Full, Custom, Match chat width), read from the
//! settings object. The GPUI web build compiles this file too (symlinked), so the browser's
//! terminal takes the same layout (`app/render/terminal_content_layout.rs` draws it).

use serde_json::{Map, Value};

pub(crate) const DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX: f64 = 0.0;
pub(crate) const DEFAULT_TERMINAL_PANE_VERTICAL_PADDING_PX: f64 = 0.0;
pub(crate) const MIN_TERMINAL_PANE_PADDING_PX: f64 = 0.0;
pub(crate) const MAX_TERMINAL_PANE_PADDING_PX: f64 = 64.0;
pub(crate) const DEFAULT_TERMINAL_VIEW_WIDTH_MODE: &str = "full";
pub(crate) const DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT: f64 = 75.0;
pub(crate) const MIN_TERMINAL_VIEW_WIDTH_PERCENT: f64 = 50.0;
pub(crate) const MAX_TERMINAL_VIEW_WIDTH_PERCENT: f64 = 100.0;
pub(crate) const DEFAULT_CHAT_CONTENT_MAX_WIDTH_PX: f32 = 768.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TerminalContentWidth {
    MaxWidth(f32),
    Percent(f32),
    /// At most this percentage of the window: the chat transcript's custom width
    /// (native_chat/transcript.rs `transcript_column_cap`), which Match chat width follows.
    WindowPercent(f32),
}

fn number(object: &Map<String, Value>, key: &str, fallback: f64) -> f64 {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn switch_on(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn terminal_pane_padding_px_from_settings(object: &Map<String, Value>) -> (f32, f32) {
    (
        number(
            object,
            "terminalPaneHorizontalPaddingPx",
            DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX,
        )
        .clamp(MIN_TERMINAL_PANE_PADDING_PX, MAX_TERMINAL_PANE_PADDING_PX) as f32,
        number(
            object,
            "terminalPaneVerticalPaddingPx",
            DEFAULT_TERMINAL_PANE_VERTICAL_PADDING_PX,
        )
        .clamp(MIN_TERMINAL_PANE_PADDING_PX, MAX_TERMINAL_PANE_PADDING_PX) as f32,
    )
}

/// The terminal's horizontal and vertical padding and, when `apply_width_mode`, the width its
/// Terminal width mode gives it (`None` fills the pane).
pub fn terminal_pane_layout_from_settings(
    object: &Map<String, Value>,
    apply_width_mode: bool,
) -> (f32, f32, Option<TerminalContentWidth>) {
    let (horizontal_padding, vertical_padding) = terminal_pane_padding_px_from_settings(object);
    let width = if apply_width_mode {
        let legacy_mode = if switch_on(object, "terminalNarrowerViewEnabled") {
            "custom"
        } else {
            DEFAULT_TERMINAL_VIEW_WIDTH_MODE
        };
        match object
            .get("terminalViewWidthMode")
            .and_then(Value::as_str)
            .unwrap_or(legacy_mode)
        {
            "match-chat" => {
                if switch_on(object, "sessionChatCustomTranscriptWidthEnabled") {
                    Some(TerminalContentWidth::WindowPercent(
                        number(
                            object,
                            "sessionChatTranscriptWidthPercent",
                            DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT,
                        )
                        .clamp(
                            MIN_TERMINAL_VIEW_WIDTH_PERCENT,
                            MAX_TERMINAL_VIEW_WIDTH_PERCENT,
                        ) as f32,
                    ))
                } else {
                    Some(TerminalContentWidth::MaxWidth(
                        DEFAULT_CHAT_CONTENT_MAX_WIDTH_PX,
                    ))
                }
            }
            "custom" => Some(TerminalContentWidth::Percent(
                number(
                    object,
                    "terminalViewWidthPercent",
                    DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT,
                )
                .clamp(
                    MIN_TERMINAL_VIEW_WIDTH_PERCENT,
                    MAX_TERMINAL_VIEW_WIDTH_PERCENT,
                ) as f32,
            )),
            _ => None,
        }
    } else {
        None
    };
    (horizontal_padding, vertical_padding, width)
}
