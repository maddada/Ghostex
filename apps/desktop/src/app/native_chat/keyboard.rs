use super::state::NativeChatView;
use gpui::{Context, EntityInputHandler as _, Focusable as _, Window};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct ComposerKey {
    key: String,
}
struct ComposerKeysRegistered;
impl gpui::Global for ComposerKeysRegistered {}

/// Whether the chat's own chords and their labels use the Mac keys (Cmd for the primary
/// modifier, ⌘⌥⌃⇧ glyphs). The desktop decides when it is compiled; the browser build asks the
/// browser, so a Mac browser gets Cmd like the desktop.
///
/// CDXC:WebGpui 2026-10-16 DECISION:
/// User: "Fix them", for the chat's chords that a Mac browser got as Ctrl: Cmd+Enter (send), Cmd+Home/End, the notice's Cmd+Enter and the ⌘ shortcut labels use Cmd like the desktop. gpui's `secondary-` and `cfg!(target_os = "macos")` are fixed at compile time (Ctrl and the Windows labels in wasm), so chat-owned chords and labels go through this instead. App hotkeys keep their own bindings and labels (`app/hotkeys.rs`).
pub(super) fn chat_mac_keys() -> bool {
    crate::terminal_element::uses_macos_text_editing()
}

/// A chat chord with the primary modifier (`cmd-` on a Mac keyboard, `ctrl-` elsewhere): what
/// gpui's `secondary-` means, decided by [`chat_mac_keys`].
pub(super) fn primary_chord(rest: &str) -> String {
    format!("{}-{rest}", if chat_mac_keys() { "cmd" } else { "ctrl" })
}

/// Whether `modifiers` hold exactly the primary modifier's key out of Cmd and Ctrl
/// (`Modifiers::secondary`, decided by [`chat_mac_keys`]).
pub(super) fn primary_held(modifiers: &gpui::Modifiers) -> bool {
    if chat_mac_keys() {
        modifiers.platform
    } else {
        modifiers.control
    }
}

/// A shortcut label for a chat-owned chord, in the Mac or the Windows/Linux spelling by
/// [`chat_mac_keys`].
pub(super) fn chat_chord_label(chord: &str) -> String {
    crate::hotkey_label::hotkey_chord_label(chord, chat_mac_keys())
}

pub(super) fn register(cx: &mut gpui::App) {
    if cx.has_global::<ComposerKeysRegistered>() {
        return;
    }
    cx.set_global(ComposerKeysRegistered);
    // CDXC:SessionChat 2026-09-23 SEE-ALSO:
    // React's session-chat-lexical-input.tsx uses the primary modifier with Home/End for document movement and Shift selection. The native input only supplies macOS document-arrow defaults.
    let document_moves = [
        crate::app::hotkeys::gpui_runtime_key_binding(
            &primary_chord("home"),
            gpui_component::input::MoveToStart,
            Some("NativeChat > Input"),
            cx,
        ),
        crate::app::hotkeys::gpui_runtime_key_binding(
            &primary_chord("end"),
            gpui_component::input::MoveToEnd,
            Some("NativeChat > Input"),
            cx,
        ),
    ];
    let document_selections = [
        crate::app::hotkeys::gpui_runtime_key_binding(
            &primary_chord("shift-home"),
            gpui_component::input::SelectToStart,
            Some("NativeChat > Input"),
            cx,
        ),
        crate::app::hotkeys::gpui_runtime_key_binding(
            &primary_chord("shift-end"),
            gpui_component::input::SelectToEnd,
            Some("NativeChat > Input"),
            cx,
        ),
    ];
    cx.bind_keys(document_moves);
    cx.bind_keys(document_selections);
    let send = primary_chord("enter");
    let select_line = primary_chord("l");
    let delete_line = primary_chord("shift-k");
    let composer_keys: Vec<_> = [
        ("enter", "enter"),
        ("shift-enter", "enter"),
        (send.as_str(), "enter"),
        // Interrupt and send; on Windows and Linux this is the send chord too.
        ("ctrl-enter", "enter"),
        ("alt-enter", "enter"),
        ("up", "up"),
        ("shift-up", "up"),
        ("alt-up", "up"),
        ("down", "down"),
        ("shift-down", "down"),
        // Line commands (`composer_line_command` in `edit_shortcuts.rs`).
        ("alt-down", "down"),
        ("alt-shift-up", "up"),
        ("alt-shift-down", "down"),
        (select_line.as_str(), "l"),
        (delete_line.as_str(), "k"),
        ("tab", "tab"),
        ("shift-tab", "tab"),
        ("escape", "escape"),
        // CDXC:SessionChat 2026-09-18 SEE-ALSO: The terminal chords the composer answers itself, matching `sessionChatTerminalShortcut`; `edit_shortcuts.rs` keeps the kill buffer.
        ("ctrl-u", "u"),
        ("ctrl-k", "k"),
        ("ctrl-y", "y"),
    ]
    .into_iter()
    .map(|(binding, key)| {
        crate::app::hotkeys::gpui_runtime_key_binding(
            binding,
            ComposerKey { key: key.into() },
            Some("NativeChat > Input"),
            cx,
        )
    })
    .collect();
    cx.bind_keys(composer_keys);
}

/// CDXC:SessionChat 2026-09-17 WHY:
/// GPUI resolves input key bindings before key listeners. Chat-scoped actions let suggestions, history and queueing precede the editor's Enter, arrows and indentation; unhandled bindings continue to the input.
pub(super) trait ComposerInputActions: gpui::InteractiveElement + Sized {
    fn composer_input_actions(self, cx: &Context<NativeChatView>) -> Self {
        self.key_context("NativeChat").capture_action(cx.listener(
            |chat, action: &ComposerKey, window, cx| {
                chat.composer_bound_key(&action.key, window, cx);
            },
        ))
    }
}
impl<T: gpui::InteractiveElement> ComposerInputActions for T {}

impl NativeChatView {
    pub(super) fn composer_bound_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_held = self.composer_held_key.as_deref() == Some(key);
        self.composer_held_key = Some(key.to_owned());
        self.composer_key_down(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke {
                    key: key.to_owned(),
                    modifiers: window.modifiers(),
                    key_char: None,
                },
                is_held,
                prefer_character_input: false,
            },
            window,
            cx,
        );
    }
    pub(crate) fn composer_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // React dropped a pending pill open on any key press (`onKeyDownCapture` on its composer).
        self.cancel_composer_reference_open();
        // The search field owns Enter, the arrows and Escape while it has focus.
        if self.search_key_down(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // An open `/resume` picker or pass-through screen takes the arrows, Enter and Escape.
        if self.terminal_picker_key_down(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        /*
        CDXC:SessionChat 2026-09-18 WHY:
        React's image viewer closed on Escape from a capture listener on the whole chat surface
        (session-chat-image-viewer.tsx (deleted 2026-10-01)), ahead of the composer's interrupt. The native viewer is a
        child window over the same pane, so the pane answers Escape for it too and the picture
        closes whichever of the two windows the keystroke reached.
        */
        if self.image_viewer.request.is_some()
            && event.keystroke.key == "escape"
            && !event.keystroke.modifiers.platform
        {
            self.close_image_viewer(cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // The subagent transcript is modal: Escape closes it and nothing behind it takes a
        // keystroke, the focus trap React's dialog applied. Application chords still pass.
        if self.snapshot["subagent"].is_object() && !event.keystroke.modifiers.platform {
            if event.keystroke.key == "escape" && !event.is_held {
                self.invoke(json!({"type":"subagentClose"}), cx);
            }
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        let this = self;

        let key = &event.keystroke;
        for input in this
            .input
            .iter()
            .chain(this.answer_input.iter().map(|(_, input)| input))
            .chain(this.async_answer_input.iter().map(|(_, input)| input))
        {
            if input.read(cx).focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                return;
            }
        }
        if key.key == "enter" && !key.modifiers.shift {
            let focused = |input: &gpui::Entity<gpui_component::input::TextareaState>| {
                input.read(cx).focus_handle(cx).is_focused(window)
            };
            let command = if this
                .async_answer_input
                .as_ref()
                .is_some_and(|(_, input)| focused(input))
            {
                Some("asyncQuestionSend")
            } else if this
                .answer_input
                .as_ref()
                .is_some_and(|(_, input)| focused(input))
            {
                Some("questionNext")
            } else {
                None
            };
            if let Some(command) = command {
                if !event.is_held {
                    this.invoke(json!({"type":command}), cx);
                }
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
        }
        // Answer editors own typing, selection and newline keys after their explicit submit shortcut above.
        // Match React's editable-target guard before considering background typing into the composer.
        if this
            .answer_input
            .iter()
            .chain(this.async_answer_input.iter())
            .map(|(_, input)| input)
            .chain(this.note_input.iter())
            .chain(this.form_inputs.values().map(|entry| &entry.input))
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
        {
            cx.propagate();
            return;
        }
        if this.snapshot["questionCard"]["visible"] == true
            && this.snapshot["prompt"]["kind"] == "question"
        {
            let editing = this
                .input
                .iter()
                .chain(this.answer_input.iter().map(|(_, input)| input))
                .chain(this.async_answer_input.iter().map(|(_, input)| input))
                .any(|input| input.read(cx).focus_handle(cx).is_focused(window));
            let collapsed = this
                .collapsed
                .contains(&format!("question:{}", this.snapshot["prompt"]));
            if !editing
                && !collapsed
                && !key.modifiers.platform
                && !key.modifiers.control
                && !key.modifiers.alt
                && let Ok(digit @ 1..=9) = key.key.parse::<usize>()
            {
                this.invoke(json!({"type":"questionOption","index":digit - 1}), cx);
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        // A live terminal dialog's numbered rows answer their number keys like a question card's
        // options (dialog_rows.rs in the chat core).
        if this.snapshot["noticeVisible"] == true
            && this.snapshot["terminalNotice"]["numbered"] == true
            && this.snapshot["questionCard"]["busy"] != true
            && !key.modifiers.platform
            && !key.modifiers.control
            && !key.modifiers.alt
            && let Ok(digit @ 1..=9) = key.key.parse::<u64>()
        {
            let editing = this
                .input
                .iter()
                .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
                || this
                    .terminal_dialog_input
                    .as_ref()
                    .is_some_and(|input| input.field.focus_handle(cx).is_focused(window));
            let answer = this.snapshot["terminalNotice"]["choices"]
                .as_array()
                .and_then(|choices| choices.iter().find(|choice| choice["number"] == digit))
                .map(|choice| choice["answer"].clone());
            if !editing && let Some(answer) = answer {
                this.invoke(json!({"type":"answer","answer":answer}), cx);
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
        }
        let Some(input) = this.input.clone() else {
            return;
        };
        if !input.read(cx).focus_handle(cx).is_focused(window) {
            // Another chat field (find bar, note, answer) owns its own typing.
            if this.chat_text_field_focused(window, cx) {
                return;
            }
            /*
            CDXC:SessionChat 2026-09-18 DECISION:
            User (2026-09-06 and 2026-09-07, React composer): arrows and text-editing chords act on
            the composer even when only the chat background is focused. The shared rules decide what
            counts as typing intent; everything else keeps its own keyboard ownership here.
            */
            if this.side_question_key(key, cx) || this.composer_background_key(key, window, cx) {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        let terminal_chord = match key.key.as_str() {
            "u" => Some("killLineLeft"),
            "k" => Some("killLineRight"),
            "y" => Some("yank"),
            _ => None,
        }
        .filter(|_| {
            key.modifiers.control
                && !key.modifiers.shift
                && !key.modifiers.alt
                && !key.modifiers.platform
        });
        if let Some(command) = terminal_chord {
            this.composer_terminal_edit(command, cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if key.key == "enter"
            && key.modifiers.alt
            && !key.modifiers.shift
            && !key.modifiers.control
            && !key.modifiers.platform
        {
            this.submit("compact", window, cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        this.update_suggestion_selection(cx);
        let suggestions = &this.snapshot["suggestions"];
        if suggestions.is_object()
            && (key.key == "escape"
                || suggestions["rows"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty())
                    && (matches!(key.key.as_str(), "up" | "down" | "tab")
                        || key.key == "enter" && !key.modifiers.shift))
        {
            this.suggestions.clear_hover();
            if key.key == "enter" && suggestions["sendOnEnter"] == true {
                this.send(false, window, cx);
            } else {
                this.invoke(json!({"type":"suggestionKey","key":key.key}), cx);
            }
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        let primary = key.key == "enter"
            && !key.modifiers.shift
            && !key.modifiers.alt
            && primary_held(&key.modifiers)
            && !(key.modifiers.platform && key.modifiers.control);
        let secondary = key.key == "escape"
            && !key.modifiers.shift
            && !key.modifiers.alt
            && !key.modifiers.platform
            && !key.modifiers.control;
        if this.snapshot["noticeVisible"] == true
            && this.snapshot["questionCard"]["busy"] != true
            && (primary || secondary)
            && !event.is_held
        {
            let notice = &this.snapshot["terminalNotice"];
            // No primary row (a y/n prompt's Yes, dialog_rows.rs) means Cmd+Enter answers nothing.
            let position = if primary {
                notice["primaryChoice"].as_u64().map(|index| index as usize)
            } else {
                notice["secondaryChoice"]
                    .as_u64()
                    .map(|index| index as usize)
            };
            let choice = notice["choices"]
                .as_array()
                .zip(position)
                .and_then(|(choices, position)| choices.get(position));
            // Trust and Remember is click-only, like the host's
            // `terminalNoticeActionShortcutEligible` says.
            let has_action = primary
                && notice["actions"].as_array().is_some_and(|actions| {
                    actions.iter().any(|action| {
                        action["answer"].is_object() && action["kind"] != "trustAndRemember"
                    })
                });
            let standalone_dialog = notice["dialog"]["rows"]
                .as_array()
                .is_some_and(Vec::is_empty);
            if choice.is_some() || (has_action && !standalone_dialog) {
                this.invoke(
                    json!({"type":if primary {"noticePrimary"} else {"noticeSecondary"}}),
                    cx,
                );
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
            // A card whose rows are its buttons has no Cancel button any more (CDXC:SessionChat
            // 2026-10-27 in the chat core's terminal_prompts.rs), so Escape on a dialog with no
            // "No" row still closes it the way that button did.
            let dialog = &notice["dialog"];
            if secondary
                && notice["numbered"] == true
                && dialog["actions"]
                    .as_array()
                    .is_some_and(|actions| actions.iter().any(|action| action == "cancel"))
            {
                this.invoke(
                    json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":dialog["id"],"dialogAction":"cancel"}}),
                    cx,
                );
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
        }
        // The approval card's Allow and Deny carry these two keys as hints (approval.rs).
        if this.snapshot["questionCard"]["visible"] == true
            && this.snapshot["questionCard"]["loading"] != true
            && this.snapshot["questionCard"]["busy"] != true
            && this.snapshot["prompt"]["kind"] == "approval"
            && (primary || secondary)
            && !event.is_held
        {
            this.invoke(
                json!({"type":"answer","answer":{"kind":"approval","approvalSend":if primary {"1"} else {""}}}),
                cx,
            );
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if key.key == "up"
            && key.modifiers.alt
            && !key.modifiers.shift
            && !key.modifiers.control
            && !key.modifiers.platform
            && this.draft.trim().is_empty()
            && this.snapshot["queue"]["capabilities"]["canEdit"] == true
            && let Some(prompt) = this.snapshot["queue"]["prompts"]
                .as_array()
                .and_then(|prompts| prompts.iter().rev().find(|prompt| prompt["busy"] != true))
        {
            this.invoke(
                json!({"type":"removeQueue","promptId":prompt["id"],"edit":true}),
                cx,
            );
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Claude Code's own key for its queue: Up in an empty composer takes the newest message the
        // agent queued back into it (CDXC:SessionChat 2026-10-11 in
        // packages/gx-chat-core/src/composer/queue_take_back.rs). History browsing keeps Up.
        if key.key == "up"
            && !key.modifiers.shift
            && !key.modifiers.alt
            && !key.modifiers.control
            && !key.modifiers.platform
            && this.draft.trim().is_empty()
            && this.snapshot["historyActive"] != true
            && this.snapshot["queueTakeBackIds"]
                .as_array()
                .is_some_and(|ids| !ids.is_empty())
        {
            this.invoke(json!({"type":"takeBackQueued"}), cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if (key.key == "up"
            && (this.draft.trim().is_empty() || this.snapshot["historyActive"] == true)
            || key.key == "down" && this.snapshot["historyActive"] == true)
            && !key.modifiers.shift
            && !key.modifiers.control
            && !key.modifiers.platform
            && (!key.modifiers.alt || this.draft.trim().is_empty())
        {
            this.invoke(json!({"type":"recallHistory","direction":key.key}), cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if let Some(command) = super::edit_shortcuts::composer_line_command(key) {
            this.composer_line_edit(command, window, cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "tab"
            && !key.modifiers.shift
            && !key.modifiers.alt
            && !key.modifiers.control
            && !key.modifiers.platform
            && !this.draft.trim().is_empty()
            && this.snapshot["queue"]["capabilities"]["canQueue"] == true
        {
            this.send(true, window, cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "escape" && !key.modifiers.shift && this.cancel_dictation(cx) {
            // Escape cancels Dictate's recording or transcription first (the recording is kept).
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "escape" && !key.modifiers.shift {
            // Shift+Esc opens the Commands panel (openCommandsPanelSecondKey), never an interrupt.
            if this.maximized_window.is_some() {
                this.close_maximized(cx);
            } else {
                this.invoke(
                    json!({"type":"interrupt","confirm":confirm_escape_interrupt()}),
                    cx,
                );
            }
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "enter" && !key.modifiers.shift {
            // Ctrl+Enter is Claude Code's own "send now" key in the terminal, on every platform.
            if key.modifiers.control && !key.modifiers.alt && !key.modifiers.platform {
                this.submit("interrupt", window, cx);
            } else {
                this.send(false, window, cx);
            }
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "tab"
            && !key.modifiers.alt
            && !key.modifiers.platform
            && !key.modifiers.control
        {
            if key.modifiers.shift {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            cx.stop_propagation();
            window.prevent_default();
        }
    }
}

/// `sessionChatConfirmEscapeInterrupt`; a missing key reads as its default, on. The core decides
/// whether this Escape would stop the agent and so has to wait for a second one (send.rs,
/// `interrupt`).
fn confirm_escape_interrupt() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("sessionChatConfirmEscapeInterrupt")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
}
