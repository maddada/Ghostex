// C1 wave-3 extraction: the configured-hotkey resolution helpers (RunConfiguredGhostexHotkey and its supporting fns/consts) moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

/// A shared ghostex-hotkeys action id resolved from the user's configured
/// hotkey table (settings `hotkeys`). Dispatching goes through the same
/// runGhostexHotkeyAction route the sidebar and command palette use, so
/// configured chords work even while a Ghostty terminal or CEF surface owns
/// keyboard focus.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunConfiguredGhostexHotkey {
    pub(crate) action_id: String,
}

const LEGACY_VIEW_PANEL_HOTKEY_ACTION_ID: &str = "toggleCompanionPane";

/// CDXC:Hotkeys 2026-09-24 WHY:
/// The view panel replaced the companion pane, but saved hotkeys still contain its old action id. Registering that retired id after the new defaults shadows Ctrl+Alt+B with an action that no longer exists. Read its custom or blank chord through the current id and omit the retired binding, matching shared hotkey normalization.
fn gpui_persisted_hotkey_value<'a>(
    hotkeys: &'a serde_json::Map<String, serde_json::Value>,
    action_id: &str,
) -> Option<&'a serde_json::Value> {
    hotkeys.get(action_id).or_else(|| {
        (action_id == "toggleViewPanel")
            .then(|| hotkeys.get(LEGACY_VIEW_PANEL_HOTKEY_ACTION_ID))
            .flatten()
    })
}

/*
CDXC:Hotkeys 2026-08-22:
Cmd-K clears the focused terminal, matching the `clear_screen` binding
ghostty ships by default, so the terminal owns that chord outright and no
configured command may take it over. This mirrors the reserved-chord list in
packages/shared/ghostex-hotkeys.ts (deleted 2026-10-01): a chord persisted before the reservation falls
back to its action's default, and the chord itself never registers a gpui
binding. macOS only, because that is the only platform ghostty binds
`clear_screen` on, and elsewhere "cmd+k" is how the shared model spells
Ctrl+K. Written in gpui keystroke syntax so every shared-settings spelling
normalizes onto it.
*/
pub(crate) const GPUI_RESERVED_GHOSTEX_HOTKEY_KEYSTROKES: &[&str] = &["cmd-k"];

pub(crate) fn gpui_hotkey_is_reserved(key: &str) -> bool {
    cfg!(target_os = "macos")
        && gpui_keystroke_from_shared_hotkey(key).is_some_and(|keystroke| {
            GPUI_RESERVED_GHOSTEX_HOTKEY_KEYSTROKES.contains(&keystroke.as_str())
        })
}

pub(crate) fn gpui_migrated_hotkey_for_action<'a>(
    action_id: &str,
    key: &'a str,
    default_key: &'a str,
) -> &'a str {
    if gpui_hotkey_is_reserved(key) {
        return default_key;
    }
    let retired_view_key = match action_id {
        "switchAgentsView" => Some("alt+1"),
        "switchSourceView" => Some("alt+2"),
        "switchGitHubView" => Some("alt+3"),
        "switchKanbanView" => Some("alt+4"),
        "switchManageView" => Some("alt+5"),
        _ => None,
    };
    if retired_view_key.is_some_and(|retired| key.trim().eq_ignore_ascii_case(retired)) {
        return default_key;
    }
    if action_id == "toggleChatView"
        && (key.trim().eq_ignore_ascii_case("ctrl+shift+j")
            || key.trim().eq_ignore_ascii_case("cmd+alt+j")
            || key.trim().eq_ignore_ascii_case("ctrl+shift+g")
            || key.trim().eq_ignore_ascii_case("cmd+alt+g"))
    {
        return default_key;
    }
    // CDXC:Hotkeys 2026-09-25: the new-session keys moved (Cmd+Shift+O, Cmd+N picker, Cmd+T browser
    // tab, Cmd+Ctrl+Shift+F fork), mirroring retiredDefaultKeys in packages/settings-catalog/src/hotkey_definitions.rs.
    let retired_new_session_keys: &[&str] = match action_id {
        "createAgentSession" => &["cmd+t"],
        "createSession" => &["cmd+t"],
        "openNewThreadPalette" => &["cmd+shift+t", "cmd+alt+t"],
        "openBrowserPane" => &["cmd+n"],
        "forkSession" => &["ctrl+shift+f", "cmd+alt+f"],
        "openHotkeys" => &["cmd+."],
        "openCommandsPanel" => &["f12"],
        "focusChatComposer" => &["shift+escape"],
        "reloadSession" => &["ctrl+shift+r", "cmd+alt+r"],
        "toggleChatSummaryMode" => &["cmd+ctrl+s"],
        _ => &[],
    };
    if retired_new_session_keys
        .iter()
        .any(|retired| key.trim().eq_ignore_ascii_case(retired))
    {
        return default_key;
    }
    // CDXC:PromptSearch 2026-08-24: retired Alt+F default, mirroring
    // retiredDefaultKeys in packages/settings-catalog/src/hotkey_definitions.rs.
    if action_id == "openFindPrompts" && key.trim().eq_ignore_ascii_case("alt+f") {
        return default_key;
    }
    // Retired Alt+Shift+S default for Saved Prompts, mirroring
    // retiredDefaultKeys in packages/settings-catalog/src/hotkey_definitions.rs.
    if action_id == "stashedPrompts" && key.trim().eq_ignore_ascii_case("alt+shift+s") {
        return default_key;
    }
    // CDXC:Navigation 2026-09-19: Back/Forward now own the plain brackets, so a
    // persisted copy of the old group-focus default has to fall back to the new
    // (unassigned) default instead of firing on the same press. Mirrors
    // retiredDefaultKeys in packages/settings-catalog/src/hotkey_definitions.rs.
    if action_id == "focusPreviousGroup" && key.trim().eq_ignore_ascii_case("cmd+[") {
        return default_key;
    }
    if action_id == "focusNextGroup" && key.trim().eq_ignore_ascii_case("cmd+]") {
        return default_key;
    }
    // Export Transcript's retired defaults, mirroring retiredDefaultKeys in
    // packages/settings-catalog/src/hotkey_definitions.rs (CDXC:Hotkeys 2026-10-27 DECISION there).
    if action_id == "exportTranscript"
        && ["cmd+alt+e", "ctrl+alt+e", "cmd+alt+x"]
            .iter()
            .any(|retired| key.trim().eq_ignore_ascii_case(retired))
    {
        return default_key;
    }
    // CDXC:Hotkeys 2026-09-19: macOS owns Cmd+Tab, so Previous/Next Session
    // moved to Ctrl+Tab and Windows/Linux Back/Forward gave Ctrl+Alt+[ / ] to
    // pane-tab cycling. Mirrors retiredDefaultKeys in
    // packages/settings-catalog/src/hotkey_definitions.rs.
    let retired_key = match action_id {
        "focusNextSession" => Some("cmd+tab"),
        "focusPreviousSession" => Some("cmd+shift+tab"),
        "navigateHistoryBack" => Some("cmd+alt+["),
        "navigateHistoryForward" => Some("cmd+alt+]"),
        _ => None,
    };
    if retired_key.is_some_and(|retired| key.trim().eq_ignore_ascii_case(retired)) {
        return default_key;
    }
    key
}

/// The chat box answers this chord itself, in its own key context, so it also works while you type
/// and never reaches terminals (`ghostexChatHotkeyActionId` in the shared catalog).
pub(crate) fn gpui_chat_owned_hotkey_action_id(action_id: &str) -> bool {
    matches!(
        action_id,
        "scrollChatToBottom"
            | "newChatPickProject"
            | "newChatPickComputer"
            | "newChatPickWhere"
            | "newChatPickBranch"
    )
}

/// Hotkeys the Files view answers itself while it has focus (`native_docs/render.rs`), so the root
/// and the native key router pass them by: outside Files their chord reaches whatever else binds it
/// (Search in Files shares Cmd+Shift+F with Find Prompts).
pub(crate) fn gpui_files_owned_hotkey_action_id(action_id: &str) -> bool {
    action_id == "searchProjectFiles"
}

/// Hotkeys bound straight to an app action (paste, find, zoom, close, new window, Focus mode), at
/// the chord Settings gives them (`app/shell_hotkeys.rs`), instead of going through
/// `RunConfiguredGhostexHotkey`, so they behave exactly as they did while they were hard-coded.
pub(crate) fn gpui_shell_owned_hotkey_action_id(action_id: &str) -> bool {
    ghostex_settings_catalog::hotkey_definition(action_id)
        .is_some_and(|definition| definition.kind() == "shellAction")
}

/// The hotkeys that act on the focused session's chat (`focused_chat_hotkeys.rs`).
pub(crate) fn gpui_focused_chat_hotkey_action_id(action_id: &str) -> bool {
    matches!(
        action_id,
        "focusChatComposer"
            | "copyLastChatCodeBlock"
            | "copyLastChatReply"
            | "toggleChatSummaryMode"
            | "toggleChatComposerMaximized"
            | "toggleChatFilesLinks"
            | "toggleDictation"
            | "readAloudLastReply"
    )
}

/// The chord `key` runs on for `action_id` on this platform: a saved macOS default moves to the
/// action's Windows and Linux default there (`windows_linux_default_key` in the hotkey catalog).
pub(crate) fn gpui_platform_hotkey_for_action<'a>(action_id: &str, key: &'a str) -> &'a str {
    ghostex_settings_catalog::platform_hotkey(
        action_id,
        key,
        ghostex_settings_catalog::Platform::current(),
    )
}

/// Converts a shared-settings hotkey ("cmd+shift+p") into gpui keystroke
/// syntax ("cmd-shift-p"). Returns None for unbound/invalid entries and for
/// chords without a non-shift modifier, which must never be stolen from
/// terminal or web surfaces, except on keys that never type text (Escape and
/// F1-F24).
pub(crate) fn gpui_keystroke_from_shared_hotkey(key: &str) -> Option<String> {
    ghostex_settings_catalog::hotkey_keystroke(key, ghostex_settings_catalog::Platform::current())
}

/// A key binding for a chord built at runtime (from settings, the hotkey catalog or a per-platform
/// helper). GPUI's `KeyBinding::new` only takes `'static` chords and parses them without knowing
/// the host OS; `KeyBinding::load` reads the host from `cx`, which the web build needs to follow a
/// Mac browser's conventions.
pub(crate) fn gpui_runtime_key_binding<A: Action>(
    keystrokes: &str,
    action: A,
    context: Option<&str>,
    cx: &gpui::App,
) -> KeyBinding {
    let context = context.map(|context| {
        gpui::KeyBindingContextPredicate::parse(context)
            .unwrap_or_else(|error| panic!("invalid key context {context}: {error}"))
            .into()
    });
    KeyBinding::load(keystrokes, Box::new(action), context, false, None, cx)
        .unwrap_or_else(|error| panic!("invalid key binding {keystrokes}: {error}"))
}

/// Whether `keystroke`, a chord built at runtime, parses on this host.
pub(crate) fn gpui_runtime_keystroke_parses(keystroke: &str, cx: &gpui::App) -> bool {
    Keystroke::parse_with_os(keystroke, cx.platform_info().os).is_ok()
}

/// Default hotkey chords mirrored from the hotkey catalog (`default_hotkeys()` in
/// packages/settings-catalog; action id → default chord in shared "+" syntax; an empty
/// chord means the action is intentionally unassigned by default). macOS never persists
/// defaults — it overlays them at read time — so GPUI mirrors the same read-time overlay
/// from this table. Kept in lockstep with the catalog by
/// packages/settings-catalog/tests/desktop_hotkey_defaults.rs.
pub(crate) const GPUI_DEFAULT_GHOSTEX_HOTKEYS: &[(&str, &str)] = &[
    ("createAgentSession", "cmd+shift+o"),
    ("createSession", "cmd+shift+t"),
    ("openCommandPalette", "cmd+shift+p"),
    ("openSessionSearchPalette", "cmd+p"),
    ("openProjectSearchPalette", "cmd+alt+shift+o"),
    ("openQuickAccessFiles", "cmd+o"),
    ("openNewThreadPalette", "cmd+n"),
    ("openCommandsPanel", "cmd+j"),
    ("openCommandsPanelSecondKey", "shift+escape"),
    ("openSettings", "cmd+,"),
    ("openExtensions", ""),
    ("openGhostexHelp", ""),
    ("openFileInFiles", ""),
    ("openHotkeys", "cmd+/"),
    ("toggleSidebarCollapsed", "cmd+b"),
    ("toggleViewPanel", "cmd+alt+b"),
    ("expandViewPanel", "cmd+ctrl+e"),
    ("expandViewPanelFully", "cmd+ctrl+shift+e"),
    ("renameActiveSession", "cmd+r"),
    ("openBrowserPane", "cmd+t"),
    ("switchAgentsView", ""),
    ("switchSourceView", ""),
    ("switchGitHubView", ""),
    ("switchKanbanView", ""),
    ("switchManageView", ""),
    ("switchAutomateView", ""),
    ("switchChangesView", ""),
    ("switchTerminalView", ""),
    ("switchTitlebarView1", "alt+1"),
    ("switchTitlebarView2", "alt+2"),
    ("switchTitlebarView3", "alt+3"),
    ("switchTitlebarView4", "alt+4"),
    ("switchTitlebarView5", "alt+5"),
    ("switchTitlebarView6", "alt+6"),
    ("switchTitlebarView7", "alt+7"),
    ("switchTitlebarView8", "alt+8"),
    ("switchTitlebarView9", "alt+9"),
    ("rotatePanesClockwise", "ctrl+shift+l"),
    ("mergeAllTabs", "ctrl+shift+m"),
    ("delayedSend", "ctrl+shift+s"),
    ("closeAfterDone", ""),
    ("promptEditor", "ctrl+g"),
    ("attachFileOrFolder", "cmd+alt+p"),
    ("sessionNote", "cmd+alt+n"),
    ("stashPrompt", "alt+s"),
    ("stashedPrompts", "cmd+alt+s"),
    // No default key (CDXC:Hotkeys 2026-10-27 DECISION in packages/settings-catalog/src/hotkey_definitions.rs).
    ("exportTranscript", ""),
    ("toggleAgentActions", "cmd+alt+a"),
    ("toggleChatView", "alt+g"),
    ("openModelPicker", "alt+p"),
    // CDXC:PromptSearch 2026-08-24: mirrors packages/settings-catalog/src/hotkey_definitions.rs.
    ("openFindPrompts", "cmd+shift+f"),
    // CDXC:Docs 2026-10-25: Files' Search sidebar shares the chord while Files has focus
    // (`gpui_files_owned_hotkey_action_id`); mirrors packages/settings-catalog/src/hotkey_definitions.rs.
    ("searchProjectFiles", "cmd+shift+f"),
    ("scrollTerminalToTop", ""),
    ("scrollTerminalToBottom", ""),
    /*
    CDXC:SessionChat 2026-09-12 WHY:
    The table has to mirror packages/settings-catalog/src/hotkey_definitions.rs entry for entry,
    but chat owns this chord inside its own capture handler so it also fires
    while the composer has focus. Every native path therefore skips the id
    instead of binding or resolving it: binding it natively would swallow the
    page key and capture it in terminals.
    */
    ("scrollChatToBottom", "ctrl+shift+down"),
    ("focusChatComposer", ""),
    ("copyLastChatCodeBlock", "cmd+shift+;"),
    ("copyLastChatReply", "cmd+shift+c"),
    ("toggleChatSummaryMode", "ctrl+alt+s"),
    ("toggleChatComposerMaximized", "ctrl+alt+m"),
    ("toggleChatFilesLinks", "ctrl+alt+l"),
    ("newChatPickProject", "alt+o"),
    ("newChatPickComputer", "alt+m"),
    ("newChatPickWhere", "alt+w"),
    ("newChatPickBranch", "alt+b"),
    ("toggleVoiceAgent", "ctrl+alt+v"),
    // No default key (CDXC:VoiceMode 2026-10-23 DECISION in packages/settings-catalog/src/hotkey_definitions.rs).
    ("voiceCallAboutSession", ""),
    // No default key (CDXC:VoiceMode 2026-10-24 DECISION in packages/settings-catalog/src/hotkey_definitions.rs).
    ("holdToTalk", ""),
    ("toggleDictation", "ctrl+alt+d"),
    // Read aloud's two hotkeys have no default key (packages/settings-catalog/src/hotkey_definitions.rs).
    ("readAloudLastReply", ""),
    ("readAloudStop", ""),
    ("forkSession", "cmd+ctrl+shift+f"),
    ("reloadSession", ""),
    ("sleepFocusedSession", "cmd+shift+a"),
    ("wakeFocusedSession", ""),
    ("closeFocusedSession", "cmd+shift+backspace"),
    ("popOutPane", "ctrl+shift+o"),
    // CDXC:Navigation 2026-09-19: mirrors packages/settings-catalog/src/hotkey_definitions.rs,
    // where the brackets moved from group focus to Back/Forward.
    ("focusPreviousGroup", ""),
    ("focusNextGroup", ""),
    ("navigateHistoryBack", "cmd+["),
    ("navigateHistoryForward", "cmd+]"),
    // CDXC:Notifications 2026-09-11: mirrors packages/settings-catalog/src/hotkey_definitions.rs.
    ("openNotifications", "cmd+i"),
    ("jumpToLatestUnreadNotification", "cmd+shift+u"),
    ("deferNotificationAndJumpNext", "cmd+ctrl+u"),
    ("focusPreviousSession", "ctrl+shift+tab"),
    ("focusNextSession", "ctrl+tab"),
    ("focusPreviousPaneTab", "cmd+alt+["),
    ("focusNextPaneTab", "cmd+alt+]"),
    ("focusUp", "cmd+alt+up"),
    ("focusRight", "cmd+alt+right"),
    ("focusDown", "cmd+alt+down"),
    ("focusLeft", "cmd+alt+left"),
    ("jumpToProject1", "cmd+ctrl+1"),
    ("jumpToProject2", "cmd+ctrl+2"),
    ("jumpToProject3", "cmd+ctrl+3"),
    ("jumpToProject4", "cmd+ctrl+4"),
    ("jumpToProject5", "cmd+ctrl+5"),
    ("jumpToProject6", "cmd+ctrl+6"),
    ("jumpToProject7", "cmd+ctrl+7"),
    ("jumpToProject8", "cmd+ctrl+8"),
    ("jumpToProject9", "cmd+ctrl+9"),
    ("goToSpace1", "cmd+alt+shift+1"),
    ("goToSpace2", "cmd+alt+shift+2"),
    ("goToSpace3", "cmd+alt+shift+3"),
    ("goToSpace4", "cmd+alt+shift+4"),
    ("goToSpace5", "cmd+alt+shift+5"),
    ("goToSpace6", "cmd+alt+shift+6"),
    ("goToSpace7", "cmd+alt+shift+7"),
    ("goToSpace8", "cmd+alt+shift+8"),
    ("goToSpace9", "cmd+alt+shift+9"),
    ("focusSessionSlot1", "cmd+1"),
    ("focusSessionSlot2", "cmd+2"),
    ("focusSessionSlot3", "cmd+3"),
    ("focusSessionSlot4", "cmd+4"),
    ("focusSessionSlot5", "cmd+5"),
    ("focusSessionSlot6", "cmd+6"),
    ("focusSessionSlot7", "cmd+7"),
    ("focusSessionSlot8", "cmd+8"),
    ("focusSessionSlot9", "cmd+9"),
    ("runActionSlot1", "ctrl+shift+1"),
    ("runActionSlot2", "ctrl+shift+2"),
    ("runActionSlot3", "ctrl+shift+3"),
    ("runActionSlot4", "ctrl+shift+4"),
    ("runActionSlot5", "ctrl+shift+5"),
    ("splitSessionRight", "alt+shift+d"),
    ("splitMore", "cmd+d"),
    ("splitMoreDown", "cmd+shift+d"),
    // CDXC:Hotkeys 2026-10-27: the keys hard-coded in main.rs until then, bound by
    // app/shell_hotkeys.rs; mirrors packages/settings-catalog/src/hotkey_definitions.rs.
    ("pasteIntoTerminal", "cmd+v"),
    ("findInFocusedView", "cmd+f"),
    ("findNextInBrowser", "cmd+g"),
    ("findPreviousInBrowser", "cmd+shift+g"),
    ("zoomInFocusedView", "cmd+="),
    ("zoomOutFocusedView", "cmd+-"),
    ("resetFocusedViewZoom", "cmd+0"),
    ("closeFocusedSurface", "cmd+w"),
    ("openNewWindow", "cmd+shift+n"),
    ("toggleAgentsFocusMode", "cmd+ctrl+f"),
    // CDXC:Hotkeys 2026-10-27: the terminal's Copy and Select All, the browser page's History and
    // Reload, Sleep's second key, and the two terminal focus hotkeys; mirrors
    // packages/settings-catalog/src/hotkey_definitions.rs.
    ("terminalCopy", "cmd+c"),
    ("terminalSelectAll", "cmd+a"),
    ("openBrowserHistory", "cmd+y"),
    ("reloadBrowserPage", "cmd+r"),
    ("sleepFocusedSessionSecondKey", "alt+shift+s"),
    ("unfocusTerminal", "cmd+shift+m"),
    ("focusLastTerminal", "cmd+shift+m"),
];

#[cfg(target_os = "macos")]
pub(crate) fn normalized_gpui_hotkey_text(value: &str) -> Option<String> {
    let mut command = false;
    let mut control = false;
    let mut option = false;
    let mut shift = false;
    let mut key = None;
    for token in value.split('+') {
        let token = token.trim().to_ascii_lowercase();
        match token.as_str() {
            "cmd" | "command" | "meta" => command = true,
            "ctrl" | "control" => control = true,
            "alt" | "opt" | "option" => option = true,
            "shift" => shift = true,
            "" => return None,
            _ if key.is_none() => {
                key = Some(match token.as_str() {
                    "arrowup" => "up".to_string(),
                    "arrowdown" => "down".to_string(),
                    "arrowleft" => "left".to_string(),
                    "arrowright" => "right".to_string(),
                    "esc" => "escape".to_string(),
                    "return" => "enter".to_string(),
                    other => other.to_string(),
                });
            }
            _ => return None,
        }
    }
    let mut key = key?;
    if option && key == "ß" {
        key = "s".to_string();
    }
    let mut parts = Vec::with_capacity(5);
    if command {
        parts.push("cmd");
    }
    if control {
        parts.push("ctrl");
    }
    if option {
        parts.push("alt");
    }
    if shift {
        parts.push("shift");
    }
    parts.push(key.as_str());
    Some(parts.join("+"))
}

#[cfg(target_os = "macos")]
/// Builds the shared hotkey text ("cmd+shift+p") for a native key press.
/// `shortcut_characters` is the layout-independent key from
/// GhostexGpuiShortcutCharactersForEvent (GpuiKeyboardShortcuts.m): the Latin
/// character the layout assigns to the key with Shift applied, so an Arabic or
/// Cyrillic layout yields the same text as a Latin one.
pub(crate) fn gpui_native_hotkey_text(
    keycode: u32,
    modifiers: u64,
    shortcut_characters: &str,
) -> Option<String> {
    const CAPS_LOCK: u64 = 1 << 16;
    const SHIFT: u64 = 1 << 17;
    const CONTROL: u64 = 1 << 18;
    const OPTION: u64 = 1 << 19;
    const COMMAND: u64 = 1 << 20;

    let key = match keycode {
        126 => "up".to_string(),
        124 => "right".to_string(),
        125 => "down".to_string(),
        123 => "left".to_string(),
        111 => "f12".to_string(),
        53 => "escape".to_string(),
        48 => "tab".to_string(),
        36 | 76 => "enter".to_string(),
        _ => {
            let normalized = shortcut_characters.to_ascii_lowercase();
            match normalized.as_str() {
                "!" if modifiers & SHIFT != 0 => "1".to_string(),
                "@" if modifiers & SHIFT != 0 => "2".to_string(),
                "#" if modifiers & SHIFT != 0 => "3".to_string(),
                "$" if modifiers & SHIFT != 0 => "4".to_string(),
                "%" if modifiers & SHIFT != 0 => "5".to_string(),
                "^" if modifiers & SHIFT != 0 => "6".to_string(),
                "&" if modifiers & SHIFT != 0 => "7".to_string(),
                "*" if modifiers & SHIFT != 0 => "8".to_string(),
                "(" if modifiers & SHIFT != 0 => "9".to_string(),
                ")" if modifiers & SHIFT != 0 => "0".to_string(),
                "{" if modifiers & SHIFT != 0 => "[".to_string(),
                "}" if modifiers & SHIFT != 0 => "]".to_string(),
                _ if normalized.chars().count() == 1 => normalized,
                _ => return None,
            }
        }
    };
    let modifiers = modifiers & !CAPS_LOCK;
    let mut parts = Vec::with_capacity(5);
    if modifiers & COMMAND != 0 {
        parts.push("cmd");
    }
    if modifiers & CONTROL != 0 {
        parts.push("ctrl");
    }
    if modifiers & OPTION != 0 {
        parts.push("alt");
    }
    if modifiers & SHIFT != 0 {
        parts.push("shift");
    }
    parts.push(key.as_str());
    Some(parts.join("+"))
}

/// The hotkey's built-in extension is on (or it belongs to none). A hotkey of a feature that is off
/// (the Start Action hotkeys while Actions is off) resolves to nothing, so its chord reaches the
/// terminal or page like any unbound key.
pub(crate) fn gpui_hotkey_feature_enabled(action_id: &str) -> bool {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    ghostex_settings_catalog::built_in_extensions::hotkey_shown_with(action_id, |key| {
        snapshot
            .object()
            .get(key)
            .and_then(serde_json::Value::as_bool)
    })
}

/// The configured hotkey a native key press runs. `in_terminal` (a terminal has the keyboard)
/// picks each hotkey's Terminal focused binding instead of its normal one, as the GPUI keymap
/// does with the terminal's key context.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_configured_hotkey_action_id_for_native_text(
    hotkey_text: &str,
    in_terminal: bool,
) -> Option<String> {
    if gpui_hotkey_is_reserved(hotkey_text) {
        // Reserved chords belong to the focused terminal, so the native
        // dispatch layer resolves no action and the key travels onward.
        return None;
    }
    let hotkey_text = normalized_gpui_hotkey_text(hotkey_text)?;
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let persisted_hotkeys = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object);
    for (action_id, default_key) in GPUI_DEFAULT_GHOSTEX_HOTKEYS {
        // Chat owns these chords (see GPUI_DEFAULT_GHOSTEX_HOTKEYS), so the key
        // travels onward to the page instead of resolving to a native action.
        if gpui_chat_owned_hotkey_action_id(action_id)
            || gpui_files_owned_hotkey_action_id(action_id)
            || gpui_shell_owned_hotkey_action_id(action_id)
            || !gpui_hotkey_feature_enabled(action_id)
        {
            continue;
        }
        if in_terminal {
            if gpui_terminal_hotkey_key_in(&snapshot, action_id)
                .and_then(|key| normalized_gpui_hotkey_text(&key))
                .as_deref()
                == Some(hotkey_text.as_str())
            {
                return Some((*action_id).to_string());
            }
            continue;
        }
        if !ghostex_settings_catalog::hotkey_has_normal_binding(action_id) {
            continue;
        }
        let key = match persisted_hotkeys
            .and_then(|hotkeys| gpui_persisted_hotkey_value(hotkeys, action_id))
        {
            Some(serde_json::Value::String(key)) => key.as_str(),
            _ => default_key,
        };
        let key = gpui_migrated_hotkey_for_action(action_id, key, default_key);
        if normalized_gpui_hotkey_text(key).as_deref() == Some(hotkey_text.as_str()) {
            return Some((*action_id).to_string());
        }
    }
    if let Some(persisted_hotkeys) = persisted_hotkeys {
        let known_action_ids = GPUI_DEFAULT_GHOSTEX_HOTKEYS
            .iter()
            .map(|(action_id, _)| *action_id)
            .collect::<HashSet<_>>();
        for (action_id, key) in persisted_hotkeys {
            // CDXC:SessionChat 2026-09-11 WHY: Chat owns this configurable chord in its capture handler, including editor focus; binding it natively would swallow the page key or affect terminals.
            if gpui_chat_owned_hotkey_action_id(action_id)
                || action_id == LEGACY_VIEW_PANEL_HOTKEY_ACTION_ID
                || known_action_ids.contains(action_id.as_str())
            {
                continue;
            }
            if key
                .as_str()
                .and_then(normalized_gpui_hotkey_text)
                .as_deref()
                == Some(hotkey_text.as_str())
            {
                return Some(action_id.clone());
            }
        }
    }
    match hotkey_text.as_str() {
        "cmd+shift+]" => Some("focusNextSession".to_string()),
        "cmd+shift+[" => Some("focusPreviousSession".to_string()),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_application_keyboard_command_for_native_text(
    hotkey_text: &str,
) -> Option<GpuiApplicationKeyboardCommand> {
    match hotkey_text {
        "cmd+h" => Some(GpuiApplicationKeyboardCommand::Hide),
        "cmd+alt+h" => Some(GpuiApplicationKeyboardCommand::HideOthers),
        "cmd+m" => Some(GpuiApplicationKeyboardCommand::MinimizeWindow),
        "cmd+q" => Some(GpuiApplicationKeyboardCommand::Quit),
        _ => None,
    }
}

/// Positional titlebar shortcuts and direct built-in view shortcuts.
/// Switching the top-level view is app chrome rather than page content, so it
/// belongs to the shell no matter which surface owns the keyboard.
pub(crate) fn gpui_workarea_switch_hotkey_action_id(action_id: &str) -> bool {
    gpui_command_palette_switch_workarea_hotkey_mode(action_id).is_some()
        || gpui_titlebar_view_hotkey_index(action_id).is_some()
}

pub(crate) fn gpui_titlebar_view_hotkey_index(action_id: &str) -> Option<usize> {
    let slot = action_id.strip_prefix("switchTitlebarView")?;
    if slot.len() != 1 {
        return None;
    }
    let slot = slot.parse::<usize>().ok()?;
    (1..=9).contains(&slot).then(|| slot - 1)
}

/// The chord an action is bound to, in the shared "+" syntax, resolved through the same
/// settings, defaults, and migrations as native key bindings. `None` when it has no valid chord.
pub(crate) fn gpui_configured_hotkey_key(action_id: &str) -> Option<String> {
    gpui_configured_hotkey_key_in(
        &shared_settings::shared_sidebar_settings_snapshot(),
        action_id,
    )
}

/// [`gpui_configured_hotkey_key`] read from `snapshot` instead of the current settings.
pub(crate) fn gpui_configured_hotkey_key_in(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    action_id: &str,
) -> Option<String> {
    let (_, default_key) = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .find(|(id, _)| *id == action_id)?;
    let key = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
        .and_then(|hotkeys| gpui_persisted_hotkey_value(hotkeys, action_id))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default_key);
    let key = gpui_migrated_hotkey_for_action(action_id, key, default_key);
    let key = gpui_platform_hotkey_for_action(action_id, key);
    gpui_keystroke_from_shared_hotkey(key)?;
    Some(key.to_string())
}

/// The key context a hotkey's normal binding applies in: outside a terminal, where its Terminal
/// focused binding applies instead (CDXC:Hotkeys 2026-10-27 DECISION in
/// packages/settings-catalog/src/hotkey_definitions.rs), and only in the Browser view for the
/// browser page's own keys, as before.
pub(crate) fn gpui_normal_hotkey_context(action_id: &str) -> &'static str {
    match action_id {
        "openBrowserHistory" | "reloadBrowserPage" => BROWSER_KEY_CONTEXT,
        _ => GPUI_OUTSIDE_TERMINAL_KEY_CONTEXT,
    }
}

/// `!` + `terminal_element::TERMINAL_KEY_CONTEXT`. GPUI's `!` checks every context on the focus
/// path, so a binding with it never fires while focus is anywhere inside a terminal.
pub(crate) const GPUI_OUTSIDE_TERMINAL_KEY_CONTEXT: &str = "!GhostexGpuiTerminal";

/// The normal binding's chord ([`gpui_configured_hotkey_key_in`]), `None` for a hotkey that has
/// only a Terminal focused binding.
pub(crate) fn gpui_normal_hotkey_key_in(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    action_id: &str,
) -> Option<String> {
    ghostex_settings_catalog::hotkey_has_normal_binding(action_id)
        .then(|| gpui_configured_hotkey_key_in(snapshot, action_id))
        .flatten()
}

/// The normal chord the user changed `action_id` to in `hotkeys` (`""` when they cleared it), or
/// `None` while it is still its default. `hotkeys` holds the whole table, so a saved value only
/// counts when it resolves, through the same migrations the normal binding uses, to something
/// other than the default does.
pub(crate) fn gpui_customized_normal_hotkey_key_in(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    action_id: &str,
) -> Option<String> {
    if !ghostex_settings_catalog::hotkey_has_normal_binding(action_id) {
        return None;
    }
    let (_, default_key) = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .find(|(id, _)| *id == action_id)?;
    let saved = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
        .and_then(|hotkeys| gpui_persisted_hotkey_value(hotkeys, action_id))
        .and_then(serde_json::Value::as_str)?;
    let resolve = |key: &str| {
        gpui_platform_hotkey_for_action(
            action_id,
            gpui_migrated_hotkey_for_action(action_id, key, default_key),
        )
        .trim()
        .to_ascii_lowercase()
    };
    let current = resolve(saved);
    (current != resolve(default_key)).then_some(current)
}

/// The Terminal focused binding's chord: what `terminalHotkeys` in `snapshot` holds for it, or
/// else the normal chord the user customized, or else the Terminal focused default
/// (`ghostex_settings_catalog::terminal_hotkey`); `None` when it has none or it is cleared.
pub(crate) fn gpui_terminal_hotkey_key_in(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    action_id: &str,
) -> Option<String> {
    let saved = snapshot
        .object()
        .get(ghostex_settings_catalog::TERMINAL_HOTKEYS_SETTING_KEY)
        .and_then(serde_json::Value::as_object)
        .and_then(|hotkeys| hotkeys.get(action_id))
        .and_then(serde_json::Value::as_str);
    let customized_normal = gpui_customized_normal_hotkey_key_in(snapshot, action_id);
    let key = ghostex_settings_catalog::terminal_hotkey(
        action_id,
        saved,
        customized_normal.as_deref(),
        ghostex_settings_catalog::Platform::current(),
    )?;
    (!gpui_hotkey_is_reserved(&key) && gpui_keystroke_from_shared_hotkey(&key).is_some())
        .then_some(key)
}

/// Resolve labels through the same settings, defaults, and migrations as native key bindings.
pub(crate) fn gpui_configured_hotkey_label(action_id: &str) -> Option<String> {
    let key = gpui_configured_hotkey_key(action_id)?;
    Some(terminal_element::terminal_overlay_hotkey_chord_label(&key))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_keyboard_owner_allows_hotkey(
    owner: GpuiKeyboardOwner,
    action_id: &str,
    hotkey_text: &str,
    terminal_model_picker_session: Option<TerminalSessionId>,
) -> bool {
    /*
    CDXC:Hotkeys 2026-08-23:
    Workarea switching is shell chrome, so it is owner-independent. Only the
    Source workarea listed the switch ids before, which meant every other
    CEF-backed surface (Browser, Kanban, Automate, Docs, Session Chat, the
    sidebar, and the unclassified `Other`/`None` responders) sent Option+1..5
    onward as ordinary page keys: once focus left an Agents terminal the
    keyboard could no longer change views at all. Answer here, before the
    per-owner page-key policy, instead of repeating the ids in every arm.
    */
    if gpui_workarea_switch_hotkey_action_id(action_id) {
        return true;
    }
    // SEE-ALSO: session_chat_model_picker.rs owns the opt-out and supported-session scope.
    if action_id == "openModelPicker" {
        match owner {
            GpuiKeyboardOwner::CompositedTerminal(GpuiEngineTerminalEventTarget::Agents(
                session,
            )) => return terminal_model_picker_session == Some(session),
            GpuiKeyboardOwner::CompositedTerminal(_) => {
                return false;
            }
            _ => {}
        }
    }
    // CDXC:Hotkeys 2026-09-05 DECISION:
    // User: Option+P must always open the focused chat's model picker, including when input focus has left the composer.
    // The picker handler resolves the actual chat pane; sidebar and shell responders must not swallow this command.
    if matches!(
        action_id,
        "openExtensions" | "openGhostexHelp" | "openModelPicker" | "openFileInFiles"
    ) {
        return true;
    }
    // CDXC:AgentLauncher 2026-09-09 WHY:
    // The New Thread picker targets the active project, not the focused surface, so its chord must win from every responder (sidebar, Browser, Session Chat, workareas) the same way the model picker does; otherwise it only works while an Agents terminal has focus.
    // The focused-chat keys only mean something while GPUI (where chats are drawn) owns the
    // keyboard; a terminal or page keeps Shift+Esc and the copy chords (focused_chat_hotkeys.rs).
    if gpui_focused_chat_hotkey_action_id(action_id) {
        return false;
    }
    // New Agent Session starts the last-used agent in the active project, the picker's first row.
    // CDXC:Hotkeys 2026-09-25 DECISION:
    // User: while the Code editor itself has keyboard focus, Cmd+N and Cmd+Shift+O belong to VS Code (New File, Go to Symbol); only then, not whenever the Code view is merely open.
    if matches!(action_id, "openNewThreadPalette" | "createAgentSession") {
        return !matches!(
            owner,
            GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
                FirstResponderCefSurface::ProjectWorkarea(
                    ProjectWorkareaCefSurfaceSlotKey::Source
                        | ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_)
                ),
            ))
        );
    }
    // CDXC:Workarea 2026-09-21 WHY:
    // The view panel toggle is window chrome, so it must win from every responder like the New Thread picker. It was only listed for the owners a view leaves behind while it is open, so Cmd+Option+B closed the panel but could not open it again once the keyboard sat with a responder outside those lists: the router passed the chord on to that responder and GPUI never saw it.
    // Stop reading stops Read aloud from anywhere in the window (CDXC:VoiceMode 2026-10-21 DECISION
    // in packages/settings-catalog/src/hotkey_definitions.rs).
    if matches!(
        action_id,
        "toggleViewPanel"
            | "expandViewPanel"
            | "expandViewPanelFully"
            | "toggleVoiceAgent"
            | "voiceCallAboutSession"
            | "holdToTalk"
            | "readAloudStop"
            // Unfocus Terminal only ever resolves while a terminal has the keyboard, and Focus
            // Last Terminal must bring it back from any surface.
            | "unfocusTerminal"
            | "focusLastTerminal"
    ) {
        return true;
    }
    match owner {
        GpuiKeyboardOwner::CompositedTerminal(_) => true,
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectWorkarea(
                ProjectWorkareaCefSurfaceSlotKey::Source
                | ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_),
            ),
        )) => {
            gpui_source_workarea_allowed_configured_hotkey_action_id(action_id)
                || (gpui_sidebar_session_walk_hotkey_reverse(action_id).is_some()
                    && !gpui_vs_code_editor_tab_chord(hotkey_text))
        }
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::BrowserTab(_),
        )) => matches!(
            action_id,
            "createSession"
                | "openBrowserPane"
                | "deferNotificationAndJumpNext"
                | "focusLeft"
                | "focusNextPaneTab"
                | "focusNextSession"
                | "focusPreviousSession"
                | "focusPreviousPaneTab"
                | "focusRight"
                | "jumpToLatestUnreadNotification"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "openNotifications"
                | "toggleSidebarCollapsed"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::Sidebar,
        )) => matches!(
            action_id,
            "deferNotificationAndJumpNext"
                | "focusNextPaneTab"
                | "focusNextSession"
                | "focusPreviousSession"
                | "focusPreviousPaneTab"
                | "jumpToLatestUnreadNotification"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "openNotifications"
                | "toggleSidebarCollapsed"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectWorkarea(_),
        )) => matches!(
            action_id,
            "createSession"
                | "openBrowserPane"
                | "deferNotificationAndJumpNext"
                | "focusLeft"
                | "focusNextSession"
                | "focusPreviousSession"
                | "focusRight"
                | "jumpToLatestUnreadNotification"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "openNotifications"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(_))
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::GpuiWindow)
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::Other)
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::None) => false,
    }
}

/// VS Code's own editor-tab chords on macOS (Ctrl+Tab / Ctrl+Shift+Tab open the editor switcher,
/// Cmd+Shift+] / [ move between editor tabs). Next/Previous Session's defaults and aliases are the
/// same chords, so while the Code view has the keyboard they stay with VS Code.
///
/// CDXC:Hotkeys 2026-10-25 WHY:
/// Previous/Next Session works from inside every web view, but in the VS Code view VS Code keeps its editor-tab keys (Ctrl+Tab, Ctrl+Shift+Tab, Cmd+Shift+] / [, which are also the session defaults and aliases), following the 2026-09-25 DECISION that Cmd+N and Cmd+Shift+O belong to VS Code while the Code editor has keyboard focus. Only a chord VS Code does not use (such as Ctrl+Cmd+Tab) switches sessions there; Browser and the other pages take the defaults too, as Chrome does.
#[cfg(target_os = "macos")]
fn gpui_vs_code_editor_tab_chord(hotkey_text: &str) -> bool {
    matches!(
        normalized_gpui_hotkey_text(hotkey_text).as_deref(),
        Some("ctrl+tab" | "ctrl+shift+tab" | "cmd+shift+]" | "cmd+shift+[")
    )
}

/// Builds native key bindings by overlaying the persisted shared hotkey map
/// on the mirrored default table at read time (macOS
/// `normalizeghostexHotkeySettings` parity). A persisted string — including an
/// explicit blank meaning "intentionally unassigned" — wins over the default;
/// a missing or non-string persisted value falls back to the default chord,
/// so fresh installs get the full default hotkey set.
pub(crate) fn gpui_configured_hotkey_key_bindings_from_settings(cx: &gpui::App) -> Vec<KeyBinding> {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let persisted_hotkeys = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    // cmd+shift+]/cmd+shift+[ are always-on aliases for Previous/Next Session
    // (`alternateDefaultKeys`), matching Chrome's second tab-switch pair on
    // macOS. Bind the aliases first so user-configured chords from the
    // settings map win conflicts.
    let next_session_alias =
        gpui_keystroke_from_shared_hotkey("cmd+shift+]").expect("valid next-session alias");
    let previous_session_alias =
        gpui_keystroke_from_shared_hotkey("cmd+shift+[").expect("valid previous-session alias");
    let mut bindings = vec![
        gpui_runtime_key_binding(
            next_session_alias.as_str(),
            RunConfiguredGhostexHotkey {
                action_id: "focusNextSession".to_string(),
            },
            None,
            cx,
        ),
        gpui_runtime_key_binding(
            previous_session_alias.as_str(),
            RunConfiguredGhostexHotkey {
                action_id: "focusPreviousSession".to_string(),
            },
            None,
            cx,
        ),
    ];
    let mut push_binding = |action_id: &str, key: &str, context: Option<&str>| {
        if gpui_chat_owned_hotkey_action_id(action_id)
            || gpui_shell_owned_hotkey_action_id(action_id)
            || key.trim().is_empty()
            || gpui_hotkey_is_reserved(key)
        {
            return;
        }
        let Some(keystroke) = gpui_keystroke_from_shared_hotkey(key) else {
            return;
        };
        if !gpui_runtime_keystroke_parses(&keystroke, cx) {
            return;
        }
        bindings.push(gpui_runtime_key_binding(
            keystroke.as_str(),
            RunConfiguredGhostexHotkey {
                action_id: action_id.to_string(),
            },
            context,
            cx,
        ));
    };
    for (action_id, default_key) in GPUI_DEFAULT_GHOSTEX_HOTKEYS {
        if ghostex_settings_catalog::hotkey_has_normal_binding(action_id) {
            let key = match gpui_persisted_hotkey_value(&persisted_hotkeys, action_id) {
                Some(serde_json::Value::String(persisted_key)) => persisted_key.as_str(),
                _ => default_key,
            };
            let key = gpui_migrated_hotkey_for_action(action_id, key, default_key);
            push_binding(
                action_id,
                gpui_platform_hotkey_for_action(action_id, key),
                Some(gpui_normal_hotkey_context(action_id)),
            );
        }
        if let Some(key) = gpui_terminal_hotkey_key_in(&snapshot, action_id) {
            push_binding(
                action_id,
                &key,
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            );
        }
    }
    // Persisted ids beyond the mirrored default table keep binding as before
    // (the shared normalizer only writes known ids, so this is a safety net
    // for maps written by newer app versions).
    let default_action_ids: std::collections::HashSet<&str> = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .map(|(action_id, _)| *action_id)
        .collect();
    for (action_id, key) in &persisted_hotkeys {
        if action_id.trim().is_empty()
            || action_id == LEGACY_VIEW_PANEL_HOTKEY_ACTION_ID
            || default_action_ids.contains(action_id.as_str())
        {
            continue;
        }
        let Some(key) = key.as_str() else {
            continue;
        };
        push_binding(action_id, key, None);
    }
    bindings
}

pub(crate) fn gpui_configured_hotkey_unbinds_from_settings(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    cx: &gpui::App,
) -> Vec<KeyBinding> {
    let persisted_hotkeys = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object);
    let action_name = RunConfiguredGhostexHotkey {
        action_id: String::new(),
    }
    .name();
    let mut keys = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .filter_map(|(action_id, default_key)| {
            // Chat owns these chords (see GPUI_DEFAULT_GHOSTEX_HOTKEYS): they are
            // never bound natively, so there is nothing to unbind either.
            if gpui_chat_owned_hotkey_action_id(action_id)
                || gpui_shell_owned_hotkey_action_id(action_id)
            {
                return None;
            }
            let key = match persisted_hotkeys
                .and_then(|hotkeys| gpui_persisted_hotkey_value(hotkeys, action_id))
            {
                Some(serde_json::Value::String(key)) => key.as_str(),
                _ => default_key,
            };
            gpui_keystroke_from_shared_hotkey(gpui_platform_hotkey_for_action(action_id, key))
        })
        .collect::<HashSet<_>>();
    if let Some(persisted_hotkeys) = persisted_hotkeys {
        keys.extend(
            persisted_hotkeys
                .values()
                .filter_map(serde_json::Value::as_str)
                .filter_map(gpui_keystroke_from_shared_hotkey),
        );
    }
    keys.extend(
        GPUI_DEFAULT_GHOSTEX_HOTKEYS
            .iter()
            .filter_map(|(action_id, _)| {
                gpui_terminal_hotkey_key_in(snapshot, action_id)
                    .and_then(|key| gpui_keystroke_from_shared_hotkey(&key))
            }),
    );
    keys.into_iter()
        .filter(|keystroke| gpui_runtime_keystroke_parses(keystroke, cx))
        .map(|keystroke| {
            gpui_runtime_key_binding(
                keystroke.as_str(),
                gpui::Unbind(action_name.into()),
                None,
                cx,
            )
        })
        .collect()
}

impl GhostexGpuiApp {
    /// Runs a configured hotkey once its window's keyboard owner has let it through. Returns
    /// false when nothing took it, so the chord can reach whatever else binds it. The main
    /// window's root and the floating reveal panel both answer hotkeys through this.
    pub(crate) fn run_configured_ghostex_hotkey(
        &mut self,
        action_id: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !gpui_hotkey_feature_enabled(action_id) || gpui_files_owned_hotkey_action_id(action_id) {
            return false;
        }
        if action_id == "openModelPicker" {
            return self.request_focused_session_model_picker(window, cx);
        }
        if gpui_focused_chat_hotkey_action_id(action_id) {
            return self.run_focused_chat_hotkey(action_id, window, cx);
        }
        // Whichever chat is being read stops, which only the chat host knows.
        if action_id == "readAloudStop" {
            crate::app::gx_chat::read_aloud_stop();
            return true;
        }
        self.handle_gpui_app_modal_sidebar_command(
            serde_json::json!({
                "message": {
                    "actionId": action_id,
                    "type": "runGhostexHotkeyAction",
                },
            }),
            window,
            cx,
        );
        true
    }
}
