//! Copying from the voice panel's transcript: its words are selectable text, Cmd+C (Ctrl+C off
//! macOS) copies the selection, and a right press on a line opens Copy and Copy message.
//!
//! CDXC:VoiceMode 2026-11-10 DECISION: User: "I should be able to copy my messages or the agent's messages by long-pressing on them in the voice agent. I should be able to also select text in the transcript." On the desktop both the user's and the agent's lines are selectable like the session chat's transcript, and a right press on a line opens the chat transcript menu's Copy (the selection) with Copy message (the whole line) under it. The session chat answers no long press on the desktop, so neither does the panel. The phone does its part in `src/voice/`.
//! CDXC:VoiceMode 2026-11-10 WHY: the panel is a non-activating pop-up that never takes the keyboard from the main window (`becomesKeyOnlyIfNeeded`), so Cmd+C after selecting reached the main window's terminal or chat instead. A left press on the transcript makes the panel the key window and focuses it, the way a click into any other window would; a click back in the main window gives the keyboard back.

use gpui::{
    AnyElement, ClipboardItem, Context, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement as _, Pixels, Point, SharedString,
    Window, div,
};
use gpui_base::{SelectableText, TextSelection};

use super::{VoicePanelWindow, panel_frame};
use crate::app::actions::CopyVoiceAgentText;
use crate::app::context_menu::GpuiContextMenu;

const ICON_COPY: &str = "titlebar/copy.svg";

/// The window's selection as Cmd+C copies it (`Root::on_action_copy`).
fn panel_selection(window: &mut Window, cx: &mut gpui::App) -> String {
    TextSelection::selected_text(window, cx).trim().to_owned()
}

impl VoicePanelWindow {
    /// One paragraph of line `row`, part of the window's selection in reading order.
    pub(super) fn selectable_part(row: usize, part: usize, text: String) -> SelectableText {
        SelectableText::new(SharedString::from(format!("voice-line-{row}-{part}")), text)
            .document_order((row as u64) * 1_000 + part as u64)
    }

    /// Wraps line `row` so a right press on it opens its menu, copying `message` as a whole.
    pub(super) fn copyable_line(
        &self,
        line: AnyElement,
        row: usize,
        message: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(("voice-line", row))
            .child(line)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.show_line_menu(message.clone(), event.position, window, cx);
                }),
            )
            .into_any_element()
    }

    /// A left press anywhere in the transcript: the panel takes the keyboard so Cmd+C reaches it.
    pub(super) fn take_keyboard(focus: &FocusHandle, window: &mut Window, cx: &mut gpui::App) {
        if !window.is_window_active() {
            window.activate_window();
        }
        focus.focus(window, cx);
    }

    /// Cmd+C (Ctrl+C off macOS) with a selection in the panel copies it.
    pub(super) fn copy_key(event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if keystroke.key != "c" || !modifiers.secondary(cx) || modifiers.shift || modifiers.alt {
            return;
        }
        let text = panel_selection(window, cx);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            cx.stop_propagation();
        }
    }

    /// Copy (the selection, disabled without one) and Copy message, at `at` in the panel.
    fn show_line_menu(
        &self,
        message: String,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = panel_selection(window, cx);
        let menu = GpuiContextMenu::new()
            .menu_with_icon(
                "Copy",
                ICON_COPY,
                selection.is_empty(),
                Box::new(CopyVoiceAgentText { text: selection }),
            )
            .menu_with_icon(
                "Copy message",
                ICON_COPY,
                message.trim().is_empty(),
                Box::new(CopyVoiceAgentText { text: message }),
            );
        let app = self.app.clone();
        cx.defer(move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let (main, origin) = {
                let app = app.read(cx);
                let frame = panel_frame(app.main_window_bounds.size, app.voice_agent.panel_place);
                (app.main_window_handle, frame.origin)
            };
            let Some(main) = main else {
                return;
            };
            // Hosted over the main window like the panel's other menus, at the same spot on screen.
            let _ = main.update(cx, |_, window, cx| {
                menu.show_for_app(app, origin + at, window, cx);
            });
        });
    }
}
