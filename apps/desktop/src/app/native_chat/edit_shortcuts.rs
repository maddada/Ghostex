use super::state::NativeChatView;
use gpui::{Context, EntityInputHandler as _, Focusable as _, Window};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

thread_local! {
    /// The terminal kill buffer Ctrl+U and Ctrl+K fill and Ctrl+Y pastes back.
    static KILL_BUFFER: RefCell<String> = const { RefCell::new(String::new()) };
    /// Set while a background keystroke is being replayed into the focused composer.
    static REPLAYING: Cell<bool> = const { Cell::new(false) };
}

/// The text a keystroke types when it is not a chord: what the platform would insert through the IME path.
fn typed_text(keystroke: &gpui::Keystroke) -> Option<&str> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.function {
        return None;
    }
    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

/// Focuses the text field whose focus handle is `field` and dispatches `keystroke` again on the
/// next turn, so the field's own bindings and key listeners handle it exactly as if it had been
/// focused when the key was pressed.
fn focus_and_replay(
    field: &gpui::FocusHandle,
    keystroke: &gpui::Keystroke,
    window: &mut Window,
    cx: &mut Context<NativeChatView>,
) {
    field.focus(window, cx);
    let replay = keystroke.clone();
    window.defer(cx, move |window, cx| {
        REPLAYING.with(|flag| flag.set(true));
        window.dispatch_keystroke(replay, cx);
        REPLAYING.with(|flag| flag.set(false));
    });
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// Byte range of the logical line `caret` sits on.
fn line_bounds(text: &str, caret: usize) -> (usize, usize) {
    let start = text[..caret].rfind('\n').map_or(0, |index| index + 1);
    let end = text[caret..]
        .find('\n')
        .map_or(text.len(), |index| caret + index);
    (start, end)
}

/// A VS Code line command the composer answers itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ComposerLineCommand {
    MoveUp,
    MoveDown,
    CopyUp,
    CopyDown,
    Delete,
    ExpandSelection,
}

/// Option+Up/Down move the selected lines, Option+Shift+Up/Down copy them, Cmd+Shift+K deletes
/// them and Cmd+L selects the line (Ctrl for Cmd on Windows and Linux).
///
/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: in the GPUI chat composer, Option+Up/Down move the current line and Option+Shift+Up/Down duplicate it, exactly as in VS Code, plus the other popular VS Code line hotkeys.
/// SEE-ALSO: the React composer's table is the key handler in `session-chat-lexical-input.tsx` and `moveComposerLines` / `COMPOSER_EDITOR_COMMANDS` in `session-chat-lexical/commands.ts`; `line_edit` mirrors those edits.
pub(super) fn composer_line_command(keystroke: &gpui::Keystroke) -> Option<ComposerLineCommand> {
    let modifiers = keystroke.modifiers;
    let primary = if cfg!(target_os = "macos") {
        modifiers.platform && !modifiers.control
    } else {
        modifiers.control && !modifiers.platform
    };
    match keystroke.key.as_str() {
        "up" | "down" if modifiers.alt && !modifiers.control && !modifiers.platform => {
            let down = keystroke.key == "down";
            Some(match (modifiers.shift, down) {
                (false, false) => ComposerLineCommand::MoveUp,
                (false, true) => ComposerLineCommand::MoveDown,
                (true, false) => ComposerLineCommand::CopyUp,
                (true, true) => ComposerLineCommand::CopyDown,
            })
        }
        "l" if primary && !modifiers.alt && !modifiers.shift => {
            Some(ComposerLineCommand::ExpandSelection)
        }
        "k" if primary && !modifiers.alt && modifiers.shift => Some(ComposerLineCommand::Delete),
        _ => None,
    }
}

/// A line command's text change and the selection afterwards, as byte offsets.
#[derive(Debug, PartialEq)]
struct LineEdit {
    replace: Option<(std::ops::Range<usize>, String)>,
    select: std::ops::Range<usize>,
}

/// Byte range of the whole lines the selection touches, without the final newline. A selection
/// that ends right after a newline does not include the line below it.
fn selected_lines(text: &str, start: usize, end: usize) -> (usize, usize) {
    let line_start = text[..start].rfind('\n').map_or(0, |index| index + 1);
    let last = if end > start && text.as_bytes()[end - 1] == b'\n' {
        end - 1
    } else {
        end
    };
    let line_end = text[last..]
        .find('\n')
        .map_or(text.len(), |index| last + index);
    (line_start, line_end)
}

fn line_edit(
    text: &str,
    selection: std::ops::Range<usize>,
    command: ComposerLineCommand,
) -> Option<LineEdit> {
    let (sel_start, sel_end) = (selection.start, selection.end);
    let (start, end) = selected_lines(text, sel_start, sel_end);
    let lines = &text[start..end];
    let shifted =
        |delta: isize| sel_start.saturating_add_signed(delta)..sel_end.saturating_add_signed(delta);
    match command {
        ComposerLineCommand::CopyDown => Some(LineEdit {
            replace: Some((end..end, format!("\n{lines}"))),
            select: shifted((end + 1 - start) as isize),
        }),
        ComposerLineCommand::CopyUp => Some(LineEdit {
            replace: Some((start..start, format!("{lines}\n"))),
            select: shifted(0),
        }),
        ComposerLineCommand::MoveDown if end < text.len() => {
            let boundary = text[end + 1..]
                .find('\n')
                .map_or(text.len(), |index| end + 1 + index);
            let following = &text[end + 1..boundary];
            Some(LineEdit {
                replace: Some((start..boundary, format!("{following}\n{lines}"))),
                select: shifted(following.len() as isize + 1),
            })
        }
        ComposerLineCommand::MoveUp if start > 0 => {
            let previous_start = text[..start - 1].rfind('\n').map_or(0, |index| index + 1);
            let previous = &text[previous_start..start - 1];
            Some(LineEdit {
                replace: Some((previous_start..end, format!("{lines}\n{previous}"))),
                select: shifted(previous_start as isize - start as isize),
            })
        }
        ComposerLineCommand::MoveUp | ComposerLineCommand::MoveDown => None,
        ComposerLineCommand::Delete => {
            let from = if end == text.len() && start > 0 {
                start - 1
            } else {
                start
            };
            let to = (end + 1).min(text.len());
            (from < to).then(|| LineEdit {
                replace: Some((from..to, String::new())),
                select: from..from,
            })
        }
        ComposerLineCommand::ExpandSelection => {
            let mut next_end = (end + 1).min(text.len());
            if sel_start == start && sel_end == next_end && next_end < text.len() {
                next_end = text[next_end..]
                    .find('\n')
                    .map_or(text.len(), |index| next_end + index + 1);
            }
            Some(LineEdit {
                replace: None,
                select: start..next_end,
            })
        }
    }
}

/// What Cmd+C / Cmd+X put on the clipboard and the range Cmd+X removes. With nothing selected
/// that is the caret's whole line plus its newline.
///
/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: with nothing selected, Cmd+X in the chat composer cuts the line the caret is in and Cmd+C copies it, as in VS Code. GPUI only: the user no longer uses the React composer.
fn clipboard_range(
    text: &str,
    selection: std::ops::Range<usize>,
) -> Option<(String, std::ops::Range<usize>)> {
    if !selection.is_empty() {
        return Some((text[selection.clone()].to_owned(), selection));
    }
    if text.is_empty() {
        return None;
    }
    let (start, end) = selected_lines(text, selection.start, selection.end);
    let Some(LineEdit {
        replace: Some((removed, _)),
        ..
    }) = line_edit(text, selection, ComposerLineCommand::Delete)
    else {
        return None;
    };
    Some((format!("{}\n", &text[start..end]), removed))
}

impl NativeChatView {
    pub(super) fn composer_copy(
        &mut self,
        _: &gpui_component::input::Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer_clipboard(false, window, cx);
    }

    pub(super) fn composer_cut(
        &mut self,
        _: &gpui_component::input::Cut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer_clipboard(true, window, cx);
    }

    /// Cmd+C and Cmd+X in the composer: the selection, or the caret's line when nothing is
    /// selected, with the copy feedback every copy in the app gives.
    fn composer_clipboard(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self
            .input
            .clone()
            .filter(|input| input.read(cx).focus_handle(cx).is_focused(window))
        else {
            cx.propagate();
            return;
        };
        cx.stop_propagation();
        let (text, selection) = {
            let input = input.read(cx);
            (input.value().to_string(), input.selected_range())
        };
        if selection.end > text.len()
            || !text.is_char_boundary(selection.start)
            || !text.is_char_boundary(selection.end)
        {
            return;
        }
        let Some((copied, removed)) = clipboard_range(&text, selection) else {
            return;
        };
        crate::app::helpers::gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(copied), cx);
        if cut {
            let utf16 = |offset: usize| text[..offset].encode_utf16().count();
            input.update(cx, |input, cx| {
                input.replace_text_in_range(
                    Some(utf16(removed.start)..utf16(removed.end)),
                    "",
                    window,
                    cx,
                );
            });
        }
    }

    /// Runs a VS Code line command on the composer as one undoable edit.
    pub(super) fn composer_line_edit(
        &mut self,
        command: ComposerLineCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.input.clone() else {
            return;
        };
        let (text, selection) = {
            let input = input.read(cx);
            (input.value().to_string(), input.selected_range())
        };
        if selection.end > text.len()
            || !text.is_char_boundary(selection.start)
            || !text.is_char_boundary(selection.end)
        {
            return;
        }
        let Some(edit) = line_edit(&text, selection, command) else {
            return;
        };
        input.update(cx, |input, cx| {
            if let Some((range, insert)) = &edit.replace {
                let utf16 = |offset: usize| text[..offset].encode_utf16().count();
                input.replace_text_in_range(
                    Some(utf16(range.start)..utf16(range.end)),
                    insert,
                    window,
                    cx,
                );
            }
            input.set_selected_range(edit.select, cx);
        });
    }

    /// Ctrl+U, Ctrl+K and Ctrl+Y in the composer. Returns whether the draft changed.
    ///
    /// CDXC:SessionChat 2026-09-18 SEE-ALSO:
    /// The chord table is `sessionChatTerminalShortcut`
    /// (`packages/core-ui/chat/session-chat-edit-shortcuts.ts`), carrying the user's 2026-09-08
    /// decision that these keys behave like the terminal on logical line boundaries; the kill buffer
    /// is this renderer's own, the way React keeps one per composer.
    pub(super) fn composer_terminal_edit(&mut self, command: &str, cx: &mut Context<Self>) -> bool {
        let Some(input) = self.input.clone() else {
            return false;
        };
        let selection = input.read(cx).selected_range();
        let start = selection.start.min(self.draft.len());
        let end = selection.end.max(start).min(self.draft.len());
        if !self.draft.is_char_boundary(start) || !self.draft.is_char_boundary(end) {
            return false;
        }
        let (from, to) = match command {
            "killLineLeft" => (line_bounds(&self.draft, start).0, end),
            "killLineRight" => {
                let line_end = line_bounds(&self.draft, end).1;
                if start == line_end && line_end < self.draft.len() {
                    // Already at the end of the line: Ctrl+K joins the next one, as a terminal does.
                    (start, line_end + 1)
                } else {
                    (start, line_end)
                }
            }
            "yank" => (start, end),
            _ => return false,
        };
        let insert = if command == "yank" {
            KILL_BUFFER.with(|buffer| buffer.borrow().clone())
        } else {
            String::new()
        };
        if from == to && insert.is_empty() {
            return false;
        }
        if command != "yank" {
            KILL_BUFFER.with(|buffer| *buffer.borrow_mut() = self.draft[from..to].to_owned());
        }
        let next = format!("{}{insert}{}", &self.draft[..from], &self.draft[to..]);
        let caret = next[..from + insert.len()].encode_utf16().count();
        self.insert_prompt(&next, cx);
        self.input_caret = Some(caret);
        true
    }

    /// Caret arrows and editing chords pressed while only the chat background holds focus.
    ///
    /// CDXC:SessionChat 2026-09-18 SEE-ALSO:
    /// The rules are the user's 2026-09-06 and 2026-09-07 decisions in
    /// `session-chat-caret-navigation.ts` and `session-chat-edit-shortcuts.ts`, reached here through
    /// `nativeComposerKeyIntent`. GPUI has no DOM to re-target, so the composer takes focus and the
    /// keystroke is replayed into it instead of each command being reimplemented.
    pub(super) fn composer_background_key(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if REPLAYING.with(Cell::get) {
            return false;
        }
        /*
        CDXC:SessionChat 2026-09-24 DECISION:
        User: Cmd+F focuses the find bar, and while the bar is shown typing goes to it until Escape or its close button, then the chat box receives input again. The find bar is the background typing target while it is open, through this same path.
        */
        if let Some(search) = self.search_input.clone() {
            return self.background_key_into(&search, keystroke, window, cx);
        }
        let Some(input) = self.input.clone() else {
            return false;
        };
        let modifiers = keystroke.modifiers;
        /*
        CDXC:SessionChat 2026-09-22 DECISION:
        User: the GPUI chat view puts typed input into the text box automatically, like the React composer did; clicking outside the text box and then typing writes into the composer where the caret was last set.
        The composer keeps its caret across blur, so focusing it and inserting the keystroke's text lands the character there; the IME path cannot do it because the platform input handler only follows focus on the next paint.
        Enter keeps its view-level meaning from React (send, Shift+Enter newline, Option+Enter compact and send) by running the composer's own Enter handling once the composer has focus.
        */
        if keystroke.key == "enter" && !modifiers.platform && !modifiers.control {
            self.short_pane_composer_open = true;
            self.invoke(json!({"type":"composerExpand","editor":true}), cx);
            if modifiers.shift {
                // The composer's key handling leaves Shift+Enter to the input's own newline, which a replay never reaches.
                input.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.replace_text_in_range(None, "\n", window, cx);
                });
            } else {
                input.read(cx).focus_handle(cx).focus(window, cx);
                self.composer_bound_key("enter", window, cx);
            }
            return true;
        }
        if let Some(text) = typed_text(keystroke) {
            let text = text.to_owned();
            self.short_pane_composer_open = true;
            self.invoke(json!({"type":"composerExpand","editor":true}), cx);
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.replace_text_in_range(None, &text, window, cx);
            });
            return true;
        }
        let event = json!({
            "key": keystroke.key, "alt": modifiers.alt, "control": modifiers.control,
            "platform": modifiers.platform, "shift": modifiers.shift,
        });
        let intent = self.runtime.as_ref().and_then(|runtime| {
            runtime.query(
                "composerKeyIntent",
                vec![event, Value::String(platform().to_owned())],
                std::time::Duration::from_millis(40),
            )
        });
        // A busy runtime answers nothing in time; the key then behaves as it did before.
        if !intent.is_some_and(|intent| intent.is_object()) {
            return false;
        }
        focus_and_replay(&input.read(cx).focus_handle(cx), keystroke, window, cx);
        true
    }

    /// Background typing into a single-line chat field (the find bar): typed text lands at its
    /// caret, and Enter, Escape, the arrows, deletion and editing chords are replayed into it so
    /// the field's own key handling (next/previous match, close) answers them.
    fn background_key_into(
        &mut self,
        field: &gpui::Entity<gpui_component::input::InputState>,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(text) = typed_text(keystroke) {
            let text = text.to_owned();
            field.update(cx, |field, cx| {
                field.focus(window, cx);
                field.replace_text_in_range(None, &text, window, cx);
            });
            return true;
        }
        let modifiers = keystroke.modifiers;
        let navigation = matches!(
            keystroke.key.as_str(),
            "enter"
                | "escape"
                | "backspace"
                | "delete"
                | "up"
                | "down"
                | "left"
                | "right"
                | "home"
                | "end"
        );
        let edit_chord = (modifiers.platform || modifiers.control)
            && matches!(keystroke.key.as_str(), "a" | "c" | "v" | "x" | "z");
        if !navigation && !edit_chord {
            return false;
        }
        focus_and_replay(&field.read(cx).focus_handle(cx), keystroke, window, cx);
        true
    }

    /// Whether any text field inside this chat pane holds GPUI focus: the composer, the Cmd+F
    /// find field, the session note, question answers or the terminal dialog field.
    ///
    /// CDXC:SessionChat 2026-09-22 WHY:
    /// Background typing sends keys to the composer only when no chat field is focused. Checking
    /// just the composer made every other field (the Cmd+F find bar first) lose its typing to the
    /// composer; every in-pane text field must be listed here.
    pub(crate) fn chat_text_field_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.input
            .iter()
            .chain(self.note_input.iter())
            .chain(self.answer_input.iter().map(|(_, input)| input))
            .chain(self.async_answer_input.iter().map(|(_, input)| input))
            .map(|input| input.focus_handle(cx))
            .chain(self.search_input.iter().map(|input| input.focus_handle(cx)))
            .chain(
                self.terminal_dialog_input
                    .iter()
                    .map(|dialog| dialog.field.focus_handle(cx)),
            )
            .any(|focus| focus.is_focused(window))
            || self.terminal_dialog_key_focus.is_focused(window)
    }
}
