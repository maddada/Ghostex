//! Focus Chat Box, Copy Last Code Block, Copy Last Reply, Toggle Summary Mode, Maximize Chat Box,
//! Files and Links, Dictate and Read Last Reply Aloud, answered for the chat of the focused session. The per-chat work is in `native_chat/chat_hotkeys.rs`.
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Hotkeys 2026-09-25 WHY:
    /// The chat's key context only covers its chat box, so binding these there made Shift+Esc fire only when the chat box already had focus (the user found it did nothing). They are app hotkeys instead, acting on the chat shown for the focused session from anywhere in the window. Returns false when that session is not in chat view, so the key keeps its ordinary meaning.
    pub(crate) fn run_focused_chat_hotkey(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return false;
        }
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        match action_id {
            "focusChatComposer" => {
                self.reclaim_gpui_root_for_native_chat_composer(window);
                view.update(cx, |view, cx| view.focus_composer_from_hotkey(window, cx));
                true
            }
            "copyLastChatCodeBlock" => {
                view.update(cx, |view, cx| view.copy_last_reply_from_hotkey(true, cx))
            }
            "copyLastChatReply" => {
                view.update(cx, |view, cx| view.copy_last_reply_from_hotkey(false, cx))
            }
            "toggleChatSummaryMode" => {
                view.update(cx, |view, cx| {
                    view.invoke(serde_json::json!({"type":"toggleSummary"}), cx)
                });
                true
            }
            "toggleChatComposerMaximized" => {
                view.update(cx, |view, cx| view.toggle_maximized(window, cx));
                true
            }
            "toggleChatFilesLinks" => {
                view.update(cx, |view, cx| {
                    view.invoke(serde_json::json!({"type":"filesLinks"}), cx)
                });
                true
            }
            // Like pressing the chat box's microphone (native_chat/dictation.rs).
            "toggleDictation" => {
                view.update(cx, |view, cx| view.toggle_dictation_from_hotkey(window, cx))
            }
            // Like Stash prompt in the chat box's More actions menu (native_chat/toolbar.rs).
            "stashPrompt" => view.update(cx, |view, cx| view.stash_prompt_from_hotkey(cx)),
            // The newest final reply of the chat on screen, from the start (the core picks it).
            "readAloudLastReply" => {
                view.update(cx, |view, cx| {
                    view.invoke(serde_json::json!({"type":"readAloudLastReply"}), cx)
                });
                true
            }
            _ => false,
        }
    }

    /// Cmd+F that reached the window root while the focused session shows its chat: the chat's own
    /// search opens, never a search in the terminal parked behind it. Returns false when that
    /// session is not in chat view.
    ///
    /// CDXC:SessionChat 2026-10-01 WHY:
    /// The chat claims Cmd+F with its own binding, and the shell's terminal Find is bound at the root with no context, which GPUI ranks as deep as the focused field's own context, so the two are separated only by the order they were registered in. Whenever the root's Find got the keystroke for a chat pane it searched the terminal parked behind the chat, and the user saw Cmd+F do nothing.
    pub(crate) fn open_search_in_focused_chat(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return false;
        }
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        self.reclaim_gpui_root_for_native_chat_composer(window);
        view.update(cx, |view, cx| view.open_search(window, cx));
        true
    }
}

#[cfg(test)]
mod tests {
    use ghostex_settings_catalog::Platform;

    use crate::app::hotkeys::{RunConfiguredGhostexHotkey, gpui_normal_hotkey_context};

    /// Stash Prompt is Alt+S on every platform and binds outside a terminal, which is where the
    /// chat view's keystrokes arrive (the chat view's Alt+S was swallowed by the terminal branch).
    #[test]
    fn stash_prompt_is_alt_s_outside_a_terminal_on_every_platform() {
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            let key = ghostex_settings_catalog::platform_hotkey("stashPrompt", "alt+s", platform);
            assert_eq!(key, "alt+s", "{platform:?}");
            assert_eq!(
                ghostex_settings_catalog::hotkey_keystroke(key, platform).as_deref(),
                Some("alt-s")
            );
        }
        assert!(ghostex_settings_catalog::hotkey_has_normal_binding(
            "stashPrompt"
        ));
        assert_eq!(
            gpui_normal_hotkey_context("stashPrompt"),
            "!GhostexGpuiTerminal"
        );
    }

    /// macOS types Option+S as "ß": the press still matches the Alt+S binding by its key, so the
    /// hotkey runs before any text reaches the chat box.
    #[test]
    fn option_s_typing_a_special_character_still_matches_alt_s() {
        let binding = gpui::KeyBinding::new(
            "alt-s",
            RunConfiguredGhostexHotkey {
                action_id: "stashPrompt".to_string(),
            },
            None,
        );
        for typed in ["alt-s", "alt-s->ß"] {
            let typed = gpui::Keystroke::parse(typed).expect("keystroke");
            assert_eq!(binding.match_keystrokes(&[typed]), Some(false));
        }
    }
}
