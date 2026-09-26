//! Native GPUI Session Note dialog, the desktop twin of the React
//! `SessionNoteModal` in packages/core-ui/session-note-modal.tsx.
//!
//! CDXC:SessionNotes 2026-09-15 DECISION:
//! User: "make sure the new gpui modal is EXACTLY 1 to 1 matching the react one": the same layout, copy, colors, states, keyboard behaviour and `setSessionNote` command as the React dialog, in both appearances. The React twin reuses Rename Session's full-height frame (`.session-note-modal-shadcn` at 100vh), so this dialog keeps its 570 x 440 frame and stretches the note editor between the header and the footer instead of fitting the window to its content.
//! SEE-ALSO: packages/core-ui/session-note-modal.tsx and the `.session-rename-modal-shadcn` / `.session-note-modal-shadcn` rules in packages/core-ui/styles/modals.css (the React twin), apps/desktop/src/app/window/native_modal_kit.rs (shared chrome and controls), apps/desktop/src/app/session_note_modal_lifecycle.rs (open, close, the `setSessionNote` command), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _, Subscription, Window, div,
    px,
};
use gpui_component::input::{Enter, Escape, InputEvent, TextareaState};
use gpui_component::v_flex;
use std::rc::Rc;

/// `APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH` / `_HEIGHT`: the note editor opens on Rename Session's frame.
pub(crate) const SESSION_NOTE_MODAL_WIDTH: f32 = 570.0;
pub(crate) const SESSION_NOTE_MODAL_INITIAL_HEIGHT: f32 = 440.0;

const TITLE: &str = "Session Note";
const DESCRIPTION_WITHOUT_TITLE: &str =
    "What to do next here. The note stays with this agent conversation.";
const FIELD_NOTE: &str = "Note";
const PLACEHOLDER: &str = "What to pick up when you come back…";
const DESCRIPTION_CLEAR_HINT: &str = "Save an empty note to clear it.";
const CANCEL: &str = "Cancel";
const SAVE: &str = "Save";
const CLEAR_NOTE: &str = "Clear Note";

fn description_for(session_title: Option<&str>) -> String {
    match session_title {
        Some(title) => format!(
            "What to do next in \u{201c}{title}\u{201d}. The note stays with this agent conversation."
        ),
        None => DESCRIPTION_WITHOUT_TITLE.to_string(),
    }
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending either.
pub(crate) enum SessionNoteModalCommand {
    /// `setSessionNote { note, sessionId, projectId? }` with the trimmed note; an empty note is the explicit clear.
    Save {
        note: String,
    },
    Cancel,
}

pub(crate) type SessionNoteModalHost = Rc<dyn Fn(SessionNoteModalCommand, &mut App)>;

pub(crate) struct SessionNoteModalConfig {
    pub(crate) initial_note: String,
    pub(crate) session_title: Option<String>,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiSessionNoteModalWindow {
    host: SessionNoteModalHost,
    palette: ModalPalette,
    session_title: Option<String>,
    /// A note existed when the dialog opened, so saving an empty field clears it.
    has_existing_note: bool,
    input: Entity<TextareaState>,
    /// Mirror of the editor's text, refreshed on every change.
    note: String,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiSessionNoteModalWindow {
    pub(crate) fn new(
        config: SessionNoteModalConfig,
        host: SessionNoteModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(PLACEHOLDER)
                .default_value(config.initial_note.clone())
        });
        let change_subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.note = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        // Unlike Rename, the caret goes to the END of the existing note: a
        // note is appended to far more often than it is replaced.
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
        });
        // The editor only scrolls to its caret once it has a layout, so seat
        // the caret again after the first frame; a long note then opens
        // scrolled to its end, as the focused React textarea does.
        let input_after_first_frame = input.clone();
        window.on_next_frame(move |_window, cx| {
            input_after_first_frame.update(cx, |input, cx| {
                let end = input.value().len();
                input.set_selected_range(end..end, cx);
            });
        });
        Self {
            host,
            palette: config.palette,
            session_title: config.session_title,
            has_existing_note: !config.initial_note.trim().is_empty(),
            note: config.initial_note,
            input,
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![change_subscription],
        }
    }

    fn trimmed_note(&self) -> &str {
        self.note.trim()
    }

    fn close_window_and_send(
        &mut self,
        command: SessionNoteModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    /// Escape is a cancel, not a save: a dismissed dialog leaves the stored note as it was.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(SessionNoteModalCommand::Cancel, window, cx);
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.trimmed_note().to_string();
        self.close_window_and_send(SessionNoteModalCommand::Save { note }, window, cx);
    }

    /// Enter inserts a newline (notes are multi-line by design); the keyboard
    /// submit is the platform's Cmd/Ctrl+Enter chord. A capture-phase action
    /// listener must stop propagation itself, or the field's own handler and
    /// the frame's key listener run too.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if !action.secondary {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.save(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    /// Keys while the frame itself holds focus.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if event.keystroke.modifiers.secondary() && !event.is_held => {
                self.save(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// `[data-slot='field-description']`: 14px muted copy at line-height 1.5.
    fn render_field_description(&self, text: String) -> AnyElement {
        div()
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(hsla(self.palette.muted))
            .child(text)
            .into_any_element()
    }

    /// `.session-rename-field-group` with its one field taking the height
    /// between the header and the footer.
    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let hint = if self.has_existing_note {
            DESCRIPTION_CLEAR_HINT.to_string()
        } else {
            format!(
                "Press {} to save.",
                crate::hotkey_label::terminal_overlay_hotkey_chord_label("cmd+enter")
            )
        };
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            // `.session-rename-modal-shadcn [data-slot='textarea']`: 14px at line-height 1.45.
            .line_height(px(20.3))
            .child(modal_section_title(&p, FIELD_NOTE))
            .child(modal_text_area(&p, &self.input, None, false, window, cx))
            .child(self.render_field_description(hint))
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "session-note-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let save_label = if self.trimmed_note().is_empty() && self.has_existing_note {
            CLEAR_NOTE
        } else {
            SAVE
        };
        let save = modal_action_button(
            &p,
            "session-note-save",
            save_label,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.save(window, cx),
            cx,
        );
        modal_footer(vec![cancel, save])
    }
}

impl Render for GpuiSessionNoteModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = vec![
            modal_header(
                &p,
                TITLE,
                Some(description_for(self.session_title.as_deref())),
            ),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-session-note-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}
