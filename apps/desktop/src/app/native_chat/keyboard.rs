use super::state::NativeChatView;
use gpui::{Context, EntityInputHandler as _, Focusable as _, Window};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct ComposerKey {
    key: String,
}

/// Keys intercepted by the Reply popover's compact mention menu (`reply_popover.rs`) only while
/// the menu is open (the `"ReplyPopoverMenu"` key context is attached only then, so the textarea's
/// own bindings stay untouched whenever the menu is closed).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ReplyPopoverMenuKey {
    pub(super) key: String,
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
        // Interrupt and send; on Windows and Linux this is the send chord when Enter adds a line,
        // and Ctrl+Shift+Enter keeps Interrupt and send available.
        ("ctrl-enter", "enter"),
        ("ctrl-shift-enter", "enter"),
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
    let reply_menu_keys: Vec<_> = [
        ("up", "up"),
        ("down", "down"),
        ("enter", "enter"),
        ("tab", "tab"),
        ("escape", "escape"),
    ]
    .into_iter()
    .map(|(binding, key)| {
        crate::app::hotkeys::gpui_runtime_key_binding(
            binding,
            ReplyPopoverMenuKey { key: key.into() },
            Some("ReplyPopoverMenu > Input"),
            cx,
        )
    })
    .collect();
    cx.bind_keys(reply_menu_keys);
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
        // The Work panel's Topic field owns Escape while it has focus: it cancels the rewrite.
        if self.work_topic_key_down(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Session Inbox keyboard triage (Cmd+Shift+I / Up / Down / Enter / E / S / Escape; all
        // rebindable in Settings > Hotkeys).
        if self.inbox_key_down(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // The search field owns Enter, the arrows and Escape while it has focus.
        if self.search_key_down(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // An open `/resume` picker or pass-through screen takes the arrows, Enter and Escape, but
        // never from another of the chat's text fields (the Topic, the find bar, a note): what is
        // typed there stays there.
        if !self.other_chat_text_field_focused(window, cx)
            && self.terminal_picker_key_down(event, window, cx)
        {
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
        // A held Enter's repeats never send text the key warning is holding (secret_send_warning.rs).
        // A held Tab's repeats never queue either: Tab is Queue's key, and its repeats came faster
        // than the second press the warning waits for.
        let composer_focused = self
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window));
        if event.is_held
            && ((event.keystroke.key == "enter"
                && (self.secret_send_warned() || self.compact_prompt_open()))
                || (event.keystroke.key == "tab" && composer_focused))
        {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // An open secret request card owns every keystroke bound for the chat box (#648).
        if self.secret_card_key(&event.keystroke, window, cx) {
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
            // The Work panel's Topic field (work_topic.rs): its digits never answer a card.
            || this
                .board
                .topic_edit
                .iter()
                .any(|(input, _)| input.read(cx).focus_handle(cx).is_focused(window))
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
        // options (dialog_rows.rs in the chat core). Not while the dialog is an approval drawn as a
        // dock row, which shows no numbers (gx-chat-core extras/dock_approvals.rs).
        if this.snapshot["noticeVisible"] == true
            && this.snapshot["terminalNotice"]["numbered"] == true
            && this.approval_in_dock() != "notice"
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
            if key.key == "escape" && !key.modifiers.shift && this.dismiss_compact_prompt(cx) {
                // Escape closes an open compact-before-sending prompt even when triggered by
                // clicking Send or after focus moved to the card or chat background.
                cx.stop_propagation();
                window.prevent_default();
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
        // CDXC:SessionChat 2026-11-20 WHY:
        // Cmd+Enter (Ctrl+Enter on Windows and Linux) also sends a typed message now, so with a draft in the box it sends the draft and never answers the notice: an urgent "stop" must not approve a command (security review of the dock approval rows).
        let draft_typed = this
            .input
            .as_ref()
            .is_some_and(|input| !input.read(cx).value().trim().is_empty());
        if this.snapshot["noticeVisible"] == true
            && this.snapshot["questionCard"]["busy"] != true
            && (secondary || (primary && !draft_typed))
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
        // The approval card's Allow and Deny carry these two keys as hints (approval.rs). As for
        // a notice, Cmd+Enter with a draft in the box sends the draft and never allows.
        if this.snapshot["questionCard"]["visible"] == true
            && this.snapshot["questionCard"]["loading"] != true
            && this.snapshot["questionCard"]["busy"] != true
            && this.snapshot["prompt"]["kind"] == "approval"
            && (secondary || (primary && !draft_typed))
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
        // Up recalls this chat's sent messages (CDXC:SessionChat 2026-11-13 in
        // packages/gx-chat-core/src/composer/history.rs): only in an empty box, or while an
        // unedited recalled message shows (`historyActive`; any edit clears it), so Up/Down keep
        // stepping once started. It never takes a queued message back; that is the queued
        // bubble's Back to composer button.
        // CDXC:SessionChat 2026-11-14 DECISION: User: "when i have new text that i wrote inside
        // the composer, then pressing up while im at the top of the text shouldn't start showing
        // me older stuff". A typed or edited draft keeps Up and Down as plain caret movement (Up
        // on the first line goes to the start of the text); it is never replaced. Supersedes
        // the 2026-11-13 "caret at the very start of a draft" trigger.
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
            && tab_queues()
        {
            this.send(true, window, cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "escape" && !key.modifiers.shift && this.close_send_popup(cx) {
            // Escape closes Send's popup (send_popup.rs) before it would interrupt the agent.
            cx.stop_propagation();
            window.prevent_default();
        } else if key.key == "escape" && !key.modifiers.shift && this.dismiss_compact_prompt(cx) {
            // Escape closes an open compact-before-sending prompt without interrupting the agent.
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
        } else if key.key == "enter" {
            let enter_sends = enter_sends();
            let mac = chat_mac_keys();
            let ctrl_only = key.modifiers.control && !key.modifiers.alt && !key.modifiers.platform;
            let primary_only = primary_held(&key.modifiers)
                && !key.modifiers.shift
                && !key.modifiers.alt
                && !(key.modifiers.platform && key.modifiers.control);
            let plain = !key.modifiers.shift
                && !key.modifiers.alt
                && !key.modifiers.control
                && !key.modifiers.platform;
            if !enter_sends && !mac && ctrl_only && key.modifiers.shift {
                // On Linux and Windows with Ctrl+Enter set to send, Ctrl+Shift+Enter keeps
                // Interrupt & Send available without clashing with normal Send.
                this.submit("interrupt", window, cx);
                cx.stop_propagation();
                window.prevent_default();
            } else if ctrl_only && !key.modifiers.shift && (enter_sends || mac) {
                // Ctrl+Enter is Claude Code's own "send now" key in the terminal: on macOS in
                // both modes, and on Linux/Windows while Enter sends.
                this.submit("interrupt", window, cx);
                cx.stop_propagation();
                window.prevent_default();
            } else if primary_only || (enter_sends && !key.modifiers.shift) {
                this.send(false, window, cx);
                cx.stop_propagation();
                window.prevent_default();
            } else if !enter_sends && plain {
                // Plain Enter adds a new line; make sure the textarea isn't holding submit_on_enter
                // before GPUI falls through to TextareaState::enter.
                input.update(cx, |input, cx| input.set_submit_on_enter(false, cx));
            }
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

    /// Session Inbox keyboard triage (Q4687): arrows move across visible Inbox rows, Enter opens
    /// the highlighted row, E marks read/dismisses, S opens Snooze, and Escape returns focus to
    /// the chat box. Every key reads its configured chord from Settings > Hotkeys.
    pub(super) fn inbox_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if cfg!(ghostex_web)
            && matches_configured_inbox_hotkey("focusSessionInbox", &event.keystroke)
        {
            return self.focus_inbox_from_hotkey(window, cx);
        }
        let Some(current) = self.board.inbox_focus else {
            return false;
        };
        let focused = self
            .board
            .inbox_focus_handle
            .as_ref()
            .is_some_and(|handle| handle.is_focused(window));
        if !focused {
            self.board.inbox_focus = None;
            cx.notify();
            return false;
        }
        if event.keystroke.key == "escape"
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            self.board.inbox_focus = None;
            self.focus_composer_from_hotkey(window, cx);
            return true;
        }
        let dock = &self.snapshot["workDock"];
        let rows: Vec<serde_json::Value> = dock["rows"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            self.board.inbox_focus = None;
            cx.notify();
            return false;
        }
        let limit = dock["limit"].as_u64().unwrap_or(4) as usize;
        let visible_count = if self.expanded.contains("dock-rows:all") {
            rows.len()
        } else {
            rows.len().min(limit)
        };
        if visible_count == 0 {
            self.board.inbox_focus = None;
            cx.notify();
            return false;
        }
        let index = current.min(visible_count - 1);
        if matches_configured_inbox_hotkey("inboxNextItem", &event.keystroke) {
            self.board.inbox_focus = Some((index + 1) % visible_count);
            cx.notify();
            return true;
        }
        if matches_configured_inbox_hotkey("inboxPreviousItem", &event.keystroke) {
            self.board.inbox_focus = Some((index + visible_count - 1) % visible_count);
            cx.notify();
            return true;
        }
        let row = &rows[index];
        if matches_configured_inbox_hotkey("inboxOpenItem", &event.keystroke) {
            let key = row["key"].as_str().unwrap_or_default().to_string();
            if row["kind"] == "pageBundle" {
                let fold_key = format!("dock-bundle:{key}");
                if !self.expanded.remove(&fold_key) {
                    self.expanded.insert(fold_key);
                }
                cx.notify();
            } else if row["open"].is_object() {
                self.open_work_target(row["open"].clone(), window, cx);
            } else if row["settled"] != true && !key.is_empty() {
                self.open_dock_ask_modal(&key, window, cx);
            }
            return true;
        }
        if matches_configured_inbox_hotkey("inboxSnooze", &event.keystroke) {
            let snooze: Vec<serde_json::Value> = row["snooze"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|choice| {
                    json!({
                        "label": choice["label"].as_str().unwrap_or_default(),
                        "iconPath": "modals/settings/moon.svg",
                        "command": choice["command"],
                    })
                })
                .collect();
            if !snooze.is_empty() {
                let y = self.board.dock_top.get().unwrap_or(gpui::px(240.0));
                let x = gpui::px(360.0);
                self.show_dock_row_menu(snooze, gpui::point(x, y), window, cx);
            }
            return true;
        }
        false
    }
}

/// Whether `keystroke` matches the user's configured chord for `action_id` in Settings > Hotkeys
/// (including single keys like `up`, `down`, `enter`, `e`, `s`).
fn matches_configured_inbox_hotkey(action_id: &str, keystroke: &gpui::Keystroke) -> bool {
    let Some(chord) = crate::app::hotkeys::gpui_configured_hotkey_key(action_id) else {
        return false;
    };
    let chord = chord.trim().to_ascii_lowercase();
    if chord.is_empty() {
        return false;
    }
    let mut want_primary = false;
    let mut want_ctrl = false;
    let mut want_alt = false;
    let mut want_shift = false;
    let mut want_key: Option<&str> = None;
    for part in chord.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part {
            "cmd" | "meta" | "super" => want_primary = true,
            "ctrl" | "control" => want_ctrl = true,
            "alt" | "option" => want_alt = true,
            "shift" => want_shift = true,
            other => want_key = Some(other),
        }
    }
    let Some(want_key) = want_key else {
        return false;
    };
    let actual_key = keystroke.key.to_ascii_lowercase();
    let key_matches = actual_key == want_key
        || (want_key == "enter" && actual_key == "return")
        || (want_key == "escape" && actual_key == "esc");
    if !key_matches {
        return false;
    }
    let mac = chat_mac_keys();
    let (expect_platform, expect_control) = if mac {
        (want_primary, want_ctrl)
    } else {
        (false, want_primary || want_ctrl)
    };
    keystroke.modifiers.platform == expect_platform
        && keystroke.modifiers.control == expect_control
        && keystroke.modifiers.alt == want_alt
        && keystroke.modifiers.shift == want_shift
}

/// Settings > Chat > Send with (`chatEnterSends`); a missing key reads as its default, true
/// (Enter sends, Shift+Enter adds a new line). False makes Cmd+Enter (Ctrl+Enter on Linux and
/// Windows) send while Enter adds a new line.
pub(super) fn enter_sends() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("chatEnterSends")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
}

/// The chord key for Send in tooltips and Send's popup.
pub(super) fn send_shortcut() -> &'static str {
    if enter_sends() { "enter" } else { "cmd+enter" }
}

/// The chord key for Interrupt & Send in Send's popup. On Linux and Windows in Ctrl+Enter-sends
/// mode, Ctrl+Enter sends normally so Interrupt & Send moves to Ctrl+Shift+Enter.
pub(super) fn interrupt_send_shortcut() -> &'static str {
    if !enter_sends() && !chat_mac_keys() {
        "ctrl+shift+enter"
    } else {
        "ctrl+enter"
    }
}

/// Settings > Chat > Tab to queue (`chatTabQueues`); a missing key reads as its default, on.
///
/// CDXC:SessionChat 2026-11-28 DECISION: User: a Settings > Chat Power User Flags toggle that
/// turns off Tab queueing in the chat box, on by default. Off, Tab with text in the box never
/// queues (and never sends): it does what it does in an empty box. The Send button's Queue row
/// then shows no Tab key and the placeholder drops "Tab to Queue" (`desktop_composer_placeholder`
/// in gx-chat-core composer/policy.rs). The phone has no Tab binding, so nothing changes there.
pub(super) fn tab_queues() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("chatTabQueues")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
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
