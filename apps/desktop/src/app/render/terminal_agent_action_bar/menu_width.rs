// The ⋯ menu's width, fitted to its rows. Windows spells a shortcut out
// (`Ctrl+Alt+Shift+F`) where macOS draws glyphs (`⌃⌥⇧F`), so a fixed width sized
// for the glyphs let the shortcut column run into the labels and past the edge.

use gpui::App;
use gpui::TextRun;
use gpui::WindowTextSystem;
use gpui::px;

use super::{
    TERMINAL_AGENT_BAR_MENU_ICON_SIZE, TERMINAL_AGENT_BAR_MENU_MAX_WIDTH,
    TERMINAL_AGENT_BAR_MENU_MIN_WIDTH, TERMINAL_AGENT_BAR_MENU_ROWS,
    TERMINAL_AGENT_BAR_MENU_SHORTCUT_SIZE, TerminalAgentBarAction, TerminalAgentBarSurface,
};
use crate::app::model::*;
use crate::*;

/// Row geometry that `render_terminal_agent_bar_menu_item` lays out with:
/// 9px side padding, a 9px gap between icon, label, spacer, shortcut and chevron.
const ROW_SIDE_PADDING: f32 = 9.0;
const ROW_GAP: f32 = 9.0;
const ROW_TEXT_SIZE: f32 = 13.0;
const SUBMENU_CHEVRON_SIZE: f32 = 12.0;
/// The menu's own 5px padding and 1px border on both sides.
const MENU_CHROME: f32 = 12.0;
/// Room for a trailing glyph's rounding, so a label never ellipsizes at its own width.
const MEASURE_SLACK: f32 = 4.0;

impl GhostexGpuiApp {
    /// The narrowest width, between the menu's minimum and maximum, that shows
    /// every row's icon, label and shortcut without overlap. A shortcut counts
    /// even on a disabled row so the menu does not change width as rows enable.
    pub(super) fn terminal_agent_bar_menu_width(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        has_switchable_agents: bool,
        cx: &App,
    ) -> f32 {
        let text_system = WindowTextSystem::new(cx.text_system().clone());
        let font = gpui::font(crate::ui_fonts::UI_FONT);
        let text_width = |text: &str, size: f32| {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                ..Default::default()
            };
            f32::from(
                text_system
                    .shape_line(text.to_owned().into(), px(size), &[run], None)
                    .width,
            )
        };
        let widest = TERMINAL_AGENT_BAR_MENU_ROWS
            .iter()
            .flatten()
            .filter(|action| {
                has_switchable_agents || **action != TerminalAgentBarAction::SwitchAccount
            })
            .map(|action| {
                let state = self.terminal_agent_bar_action_state(surface, session_id, *action, None);
                let shortcut = crate::terminal_element::terminal_overlay_hotkey_label(
                    state.hotkey_action_id,
                )
                .map_or(0.0, |shortcut| {
                    ROW_GAP + text_width(&shortcut, TERMINAL_AGENT_BAR_MENU_SHORTCUT_SIZE)
                });
                let chevron = if action.opens_flyout() {
                    ROW_GAP + SUBMENU_CHEVRON_SIZE
                } else {
                    0.0
                };
                // icon, label, the flexible spacer (its own gap), then the shortcut.
                ROW_SIDE_PADDING * 2.0
                    + TERMINAL_AGENT_BAR_MENU_ICON_SIZE
                    + ROW_GAP
                    + text_width(state.label, ROW_TEXT_SIZE)
                    + ROW_GAP
                    + shortcut
                    + chevron
            })
            .fold(0.0, f32::max);
        (widest.ceil() + MENU_CHROME + MEASURE_SLACK).clamp(
            TERMINAL_AGENT_BAR_MENU_MIN_WIDTH,
            TERMINAL_AGENT_BAR_MENU_MAX_WIDTH,
        )
    }
}
