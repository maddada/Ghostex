//! New Chat: Project, Computer, Where It Runs and Branch (Option+O, M, W and B by default): the
//! new chat's bar from the keyboard. They are chat hotkeys (`chatAction` in the hotkey catalog),
//! bound in the chat box's own key context like Scroll Chat to Bottom (`scroll_bottom.rs`), so they
//! act only while the message box has focus, and only while the bar shows; anywhere else the key
//! goes on and types as usual. What each one opens is `worktree_tray.rs` (`run_tray_hotkey`).
//!
//! CDXC:Worktrees 2026-11-04 SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs (the
//! DECISION for the keys), apps/desktop/src/app/hotkeys.rs (`gpui_chat_owned_hotkey_action_id`
//! keeps every app-wide path off them).
use super::state::NativeChatView;
use gpui::{App, Context, EntityInputHandler as _, Focusable as _, Window};
use std::collections::HashSet;

/// The bar's hotkeys, in the order of its parts.
pub(super) const TRAY_HOTKEYS: [&str; 4] = [
    "newChatPickProject",
    "newChatPickComputer",
    "newChatPickWhere",
    "newChatPickBranch",
];

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct TrayHotkey {
    action_id: String,
    shortcut: String,
}

#[derive(Default)]
struct RegisteredTrayHotkeys(HashSet<(String, String)>);
impl gpui::Global for RegisteredTrayHotkeys {}

/// The keystroke `action_id` is set to now (Settings > Hotkeys), or `None` while it has none.
fn shortcut(action_id: &str) -> Option<String> {
    let key = crate::app::hotkeys::gpui_configured_hotkey_key(action_id)?;
    crate::app::hotkeys::gpui_keystroke_from_shared_hotkey(&key)
}

/// Binds each hotkey's current chord in the chat box's context. A chord changed in Settings binds
/// on the next frame; the old chord's binding stays but no longer matches its hotkey
/// ([`NativeChatView::tray_hotkey_action`] lets it through).
pub(super) fn register(cx: &mut App) {
    if !cx.has_global::<RegisteredTrayHotkeys>() {
        cx.set_global(RegisteredTrayHotkeys::default());
    }
    for action_id in TRAY_HOTKEYS {
        let Some(shortcut) = shortcut(action_id) else {
            continue;
        };
        if cx
            .global_mut::<RegisteredTrayHotkeys>()
            .0
            .insert((action_id.to_string(), shortcut.clone()))
        {
            let binding = crate::app::hotkeys::gpui_runtime_key_binding(
                &shortcut,
                TrayHotkey {
                    action_id: action_id.to_string(),
                    shortcut: shortcut.clone(),
                },
                Some("NativeChat > Input"),
                cx,
            );
            cx.bind_keys([binding]);
        }
    }
}

impl NativeChatView {
    pub(super) fn tray_hotkey_action(
        &mut self,
        action: &TrayHotkey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if shortcut(&action.action_id).as_deref() != Some(action.shortcut.as_str()) {
            cx.propagate();
            return;
        }
        // Only the message box (not a question's answer field), only while the bar shows, and
        // never in the middle of composing a character with an input method.
        let Some(input) = self.input.clone() else {
            cx.propagate();
            return;
        };
        let in_box = input.read(cx).focus_handle(cx).is_focused(window);
        let composing = in_box
            && input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            });
        if !in_box || composing || !self.worktree_tray_shown() {
            cx.propagate();
            return;
        }
        self.run_tray_hotkey(&action.action_id, window, cx);
        cx.stop_propagation();
    }
}

#[cfg(test)]
mod tests {
    use super::{TRAY_HOTKEYS, TrayHotkey};
    use ghostex_settings_catalog::Platform;

    /// Option+O, M, W and B on every platform (Alt on Windows and Linux), owned by the chat so no
    /// app-wide binding or the native key router takes them first.
    #[test]
    fn the_bar_keys_are_option_letters_the_chat_owns_on_every_platform() {
        let expected = [
            ("newChatPickProject", "alt-o"),
            ("newChatPickComputer", "alt-m"),
            ("newChatPickWhere", "alt-w"),
            ("newChatPickBranch", "alt-b"),
        ];
        for (id, keystroke) in expected {
            assert!(TRAY_HOTKEYS.contains(&id));
            let definition = ghostex_settings_catalog::hotkey_definition(id).expect(id);
            assert_eq!(definition.kind(), "chatAction", "{id}");
            assert!(crate::app::hotkeys::gpui_chat_owned_hotkey_action_id(id));
            for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
                let key =
                    ghostex_settings_catalog::platform_hotkey(id, definition.default_key, platform);
                assert_eq!(
                    ghostex_settings_catalog::hotkey_keystroke(key, platform).as_deref(),
                    Some(keystroke),
                    "{id} {platform:?}"
                );
            }
            // The description warns about the Option letter it takes on a Mac.
            assert!(definition.description.contains("on a Mac"), "{id}");
        }
    }

    /// macOS types Option+O as "ø": the press still matches the Alt+O binding by its key, so the
    /// bar opens before any text reaches the message box.
    #[test]
    fn option_o_typing_a_special_character_still_matches_alt_o() {
        let binding = gpui::KeyBinding::new(
            "alt-o",
            TrayHotkey {
                action_id: "newChatPickProject".to_string(),
                shortcut: "alt-o".to_string(),
            },
            Some("NativeChat > Input"),
        );
        for typed in ["alt-o", "alt-o->ø"] {
            let typed = gpui::Keystroke::parse(typed).expect("keystroke");
            assert_eq!(binding.match_keystrokes(&[typed]), Some(false));
        }
    }
}
