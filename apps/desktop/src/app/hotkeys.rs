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


/*
CDXC:Hotkeys 2026-08-22:
Cmd-K clears the focused terminal, matching the `clear_screen` binding
ghostty ships by default, so the terminal owns that chord outright and no
configured command may take it over. This mirrors the reserved-chord list in
packages/shared/ghostex-hotkeys.ts: a chord persisted before the reservation falls
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
    if action_id == "toggleChatView"
        && (key.trim().eq_ignore_ascii_case("ctrl+shift+j")
            || key.trim().eq_ignore_ascii_case("cmd+alt+j")
            || key.trim().eq_ignore_ascii_case("ctrl+shift+g")
            || key.trim().eq_ignore_ascii_case("cmd+alt+g"))
    {
        default_key
    } else {
        key
    }
}


pub(crate) fn gpui_platform_hotkey_for_action<'a>(action_id: &str, key: &'a str) -> &'a str {
    if cfg!(target_os = "macos") {
        key
    } else {
        let defaults = match action_id {
            "rotatePanesClockwise" => Some(("ctrl+shift+l", "cmd+alt+l")),
            "mergeAllTabs" => Some(("ctrl+shift+m", "cmd+alt+m")),
            "delayedSend" => Some(("ctrl+shift+s", "cmd+alt+s")),
            "promptEditor" => Some(("ctrl+g", "cmd+shift+g")),
            "forkSession" => Some(("ctrl+shift+f", "cmd+alt+f")),
            "reloadSession" => Some(("ctrl+shift+r", "cmd+alt+r")),
            "popOutPane" => Some(("ctrl+shift+o", "cmd+alt+o")),
            // CDXC:NavigationHistory 2026-08-19: same Mac-Control substitution
            // as the Jump to Project entries below, mirroring the
            // windowsLinuxDefaultKey values in packages/shared/ghostex-hotkeys.ts.
            "navigateHistoryBack" => Some(("cmd+ctrl+[", "cmd+alt+[")),
            "navigateHistoryForward" => Some(("cmd+ctrl+]", "cmd+alt+]")),
            "jumpToProject1" => Some(("cmd+ctrl+1", "cmd+alt+1")),
            "jumpToProject2" => Some(("cmd+ctrl+2", "cmd+alt+2")),
            "jumpToProject3" => Some(("cmd+ctrl+3", "cmd+alt+3")),
            "jumpToProject4" => Some(("cmd+ctrl+4", "cmd+alt+4")),
            "jumpToProject5" => Some(("cmd+ctrl+5", "cmd+alt+5")),
            "jumpToProject6" => Some(("cmd+ctrl+6", "cmd+alt+6")),
            "jumpToProject7" => Some(("cmd+ctrl+7", "cmd+alt+7")),
            "jumpToProject8" => Some(("cmd+ctrl+8", "cmd+alt+8")),
            "jumpToProject9" => Some(("cmd+ctrl+9", "cmd+alt+9")),
            "runActionSlot1" => Some(("ctrl+shift+1", "cmd+shift+1")),
            "runActionSlot2" => Some(("ctrl+shift+2", "cmd+shift+2")),
            "runActionSlot3" => Some(("ctrl+shift+3", "cmd+shift+3")),
            "runActionSlot4" => Some(("ctrl+shift+4", "cmd+shift+4")),
            "runActionSlot5" => Some(("ctrl+shift+5", "cmd+shift+5")),
            _ => None,
        };
        match defaults {
            Some((mac_default, windows_linux_default))
                if key.trim().eq_ignore_ascii_case(mac_default) =>
            {
                /*
                CDXC:Hotkeys 2026-07-30:
                Control is a distinct app modifier on macOS, while Windows and
                Linux use Ctrl as the primary modifier replacing Command. Give
                every Mac-Control default an explicit, conflict-free non-Mac
                chord and migrate persisted values that still equal the Mac
                default. Custom bindings remain unchanged.
                */
                windows_linux_default
            }
            _ => key,
        }
    }
}


/// Converts a shared-settings hotkey ("cmd+shift+p") into gpui keystroke
/// syntax ("cmd-shift-p"). Returns None for unbound/invalid entries and for
/// chords without a non-shift modifier, which must never be stolen from
/// terminal or web surfaces.
pub(crate) fn gpui_keystroke_from_shared_hotkey(key: &str) -> Option<String> {
    let mut command = false;
    let mut control = false;
    let mut option = false;
    let mut shift = false;
    let mut key_token: Option<String> = None;
    for token in key.split('+') {
        let token = token.trim().to_ascii_lowercase();
        if token.is_empty() {
            return None;
        }
        match token.as_str() {
            "cmd" | "command" | "meta" => command = true,
            "ctrl" | "control" => control = true,
            "alt" | "opt" | "option" => option = true,
            "shift" => shift = true,
            _ => {
                if key_token.is_some() {
                    return None;
                }
                key_token = Some(match token.as_str() {
                    "arrowup" => "up".to_string(),
                    "arrowdown" => "down".to_string(),
                    "arrowleft" => "left".to_string(),
                    "arrowright" => "right".to_string(),
                    "esc" => "escape".to_string(),
                    "return" => "enter".to_string(),
                    other => other.to_string(),
                });
            }
        }
    }
    let mut key_token = key_token?;
    if option && key_token == "ß" {
        /*
        CDXC:Hotkeys 2026-07-30:
        Older Settings builds persisted macOS Option+S as the produced `ß`
        character. GPUI receives the physical S key, so migrate that known
        recorder value at the settings boundary until the corrected recorder
        writes `alt+s`.
        */
        key_token = "s".to_string();
    }
    let mut modifiers = Vec::new();
    if cfg!(target_os = "macos") {
        if command {
            modifiers.push("cmd");
        }
        if control {
            modifiers.push("ctrl");
        }
    } else {
        if command {
            modifiers.push("ctrl");
        }
        if control {
            modifiers.push(if command { "alt" } else { "ctrl" });
        }
    }
    if option && !modifiers.contains(&"alt") {
        modifiers.push("alt");
    }
    if shift {
        modifiers.push("shift");
    }
    if !modifiers.iter().any(|modifier| *modifier != "shift") {
        return None;
    }
    let mut keystroke = modifiers.join("-");
    keystroke.push('-');
    keystroke.push_str(&key_token);
    Some(keystroke)
}


/// Builds a shell-owned binding from the same cross-platform hotkey spelling
/// used by shared Settings. In that contract `cmd` means the platform primary
/// modifier: Command on macOS and Ctrl on Windows/Linux. Shell defaults must go
/// through this boundary too; literal GPUI `cmd-*` bindings require the
/// Windows/Meta key on non-macOS platforms.
pub(crate) fn gpui_key_binding_from_shared_hotkey<A: Action>(
    hotkey: &str,
    action: A,
    context: Option<&str>,
) -> KeyBinding {
    let keystroke = gpui_keystroke_from_shared_hotkey(hotkey)
        .unwrap_or_else(|| panic!("invalid shell hotkey: {hotkey}"));
    KeyBinding::new(keystroke.as_str(), action, context)
}


/// Default hotkey chords mirrored from `DEFAULT_ghostex_HOTKEYS` in
/// packages/shared/ghostex-hotkeys.ts (action id → default chord in shared "+"
/// syntax; an empty chord means the action is intentionally unassigned by
/// default). macOS never persists defaults — it overlays them at read time
/// via `normalizeghostexHotkeySettings` — so GPUI mirrors the same read-time
/// overlay from this table. Kept in lockstep with the TypeScript source by
/// packages/shared/gpui-hotkey-defaults-parity.test.ts.
pub(crate) const GPUI_DEFAULT_GHOSTEX_HOTKEYS: &[(&str, &str)] = &[
    ("createSession", "cmd+t"),
    ("openCommandPalette", "cmd+shift+p"),
    ("openSessionSearchPalette", "cmd+p"),
    ("openCommandsPanel", "f12"),
    ("openSettings", "cmd+,"),
    ("openHotkeys", "cmd+."),
    ("toggleSidebarCollapsed", "cmd+b"),
    ("toggleCompanionPane", "cmd+alt+b"),
    ("moveSidebar", ""),
    ("renameActiveSession", "cmd+r"),
    ("openBrowserPane", "cmd+n"),
    ("switchAgentsView", "alt+1"),
    ("switchSourceView", "alt+2"),
    ("switchGitHubView", "alt+3"),
    ("switchKanbanView", "alt+4"),
    ("switchManageView", "alt+5"),
    ("rotatePanesClockwise", "ctrl+shift+l"),
    ("mergeAllTabs", "ctrl+shift+m"),
    ("delayedSend", "ctrl+shift+s"),
    ("closeAfterDone", ""),
    ("promptEditor", "ctrl+g"),
    ("attachFileOrFolder", ""),
    ("stashPrompt", ""),
    ("stashedPrompts", "alt+shift+s"),
    ("exportTranscript", ""),
    ("toggleAgentActions", ""),
    ("toggleChatView", "alt+g"),
    // CDXC:AgentHistorySearch 2026-08-20: mirrors packages/shared/ghostex-hotkeys.ts.
    ("openFindPrompts", "alt+f"),
    ("scrollTerminalToTop", ""),
    ("scrollTerminalToBottom", ""),
    ("forkSession", "ctrl+shift+f"),
    ("reloadSession", "ctrl+shift+r"),
    ("sleepFocusedSession", ""),
    ("wakeFocusedSession", ""),
    ("closeFocusedSession", ""),
    ("popOutPane", "ctrl+shift+o"),
    ("focusPreviousGroup", "cmd+["),
    ("focusNextGroup", "cmd+]"),
    // CDXC:NavigationHistory 2026-08-19: mirrors packages/shared/ghostex-hotkeys.ts.
    ("navigateHistoryBack", "cmd+ctrl+["),
    ("navigateHistoryForward", "cmd+ctrl+]"),
    ("focusPreviousSession", "cmd+shift+tab"),
    ("focusNextSession", "cmd+tab"),
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
    ("splitMore", "cmd+d"),
    ("splitMoreDown", "cmd+shift+d"),
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
pub(crate) fn gpui_native_hotkey_text(
    keycode: u32,
    modifiers: u64,
    characters_ignoring_modifiers: &str,
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
            let normalized = characters_ignoring_modifiers.to_ascii_lowercase();
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


#[cfg(target_os = "macos")]
pub(crate) fn gpui_configured_hotkey_action_id_for_native_text(hotkey_text: &str) -> Option<String> {
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
        let key = match persisted_hotkeys.and_then(|hotkeys| hotkeys.get(*action_id)) {
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
            if known_action_ids.contains(action_id.as_str()) {
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
        "ctrl+tab" => Some("focusNextSession".to_string()),
        "ctrl+shift+tab" => Some("focusPreviousSession".to_string()),
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


#[cfg(target_os = "macos")]
pub(crate) fn gpui_keyboard_owner_allows_hotkey(owner: GpuiKeyboardOwner, action_id: &str) -> bool {
    match owner {
        GpuiKeyboardOwner::CompositedTerminal(_)
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::TerminalSurface(_)) => true,
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectWorkarea(ProjectWorkareaCefSurfaceSlotKey::Source),
        )) => gpui_source_workarea_allowed_configured_hotkey_action_id(action_id),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::BrowserTab(_),
        )) => matches!(
            action_id,
            "createSession"
                | "focusLeft"
                | "focusNextSession"
                | "focusPreviousSession"
                | "focusRight"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "toggleCompanionPane"
                | "toggleSidebarCollapsed"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::Sidebar,
        )) => matches!(
            action_id,
            "focusNextSession"
                | "focusPreviousSession"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "toggleCompanionPane"
                | "toggleSidebarCollapsed"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectEditorCompanion,
        )) => matches!(
            action_id,
            "createSession"
                | "focusLeft"
                | "focusRight"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "toggleCompanionPane"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectWorkarea(_),
        )) => matches!(
            action_id,
            "createSession"
                | "focusLeft"
                | "focusRight"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "toggleCompanionPane"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::SessionChat(_),
        )) => matches!(
            action_id,
            "toggleChatView"
                | "focusNextSession"
                | "focusPreviousSession"
                | "navigateHistoryBack"
                | "navigateHistoryForward"
                | "toggleCompanionPane"
                | "toggleSidebarCollapsed"
        ),
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(_))
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::GpuiWindow)
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::Other)
        | GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::None) => false,
    }
}


/// Builds native key bindings by overlaying the persisted shared hotkey map
/// on the mirrored default table at read time (macOS
/// `normalizeghostexHotkeySettings` parity). A persisted string — including an
/// explicit blank meaning "intentionally unassigned" — wins over the default;
/// a missing or non-string persisted value falls back to the default chord,
/// so fresh installs get the full default hotkey set.
pub(crate) fn gpui_configured_hotkey_key_bindings_from_settings() -> Vec<KeyBinding> {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let persisted_hotkeys = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    // macOS registers cmd+shift+]/cmd+shift+[ as always-on aliases for session
    // cycling (`defaultHotkeyAliases`) because the system app switcher usually
    // owns the cmd+tab defaults. Bind the aliases first so user-configured
    // chords from the settings map win conflicts.
    let next_session_alias =
        gpui_keystroke_from_shared_hotkey("cmd+shift+]").expect("valid next-session alias");
    let previous_session_alias =
        gpui_keystroke_from_shared_hotkey("cmd+shift+[").expect("valid previous-session alias");
    let mut bindings = vec![
        KeyBinding::new(
            next_session_alias.as_str(),
            RunConfiguredGhostexHotkey {
                action_id: "focusNextSession".to_string(),
            },
            None,
        ),
        KeyBinding::new(
            previous_session_alias.as_str(),
            RunConfiguredGhostexHotkey {
                action_id: "focusPreviousSession".to_string(),
            },
            None,
        ),
    ];
    let mut push_binding = |action_id: &str, key: &str| {
        if key.trim().is_empty() || gpui_hotkey_is_reserved(key) {
            return;
        }
        let Some(keystroke) = gpui_keystroke_from_shared_hotkey(key) else {
            return;
        };
        if Keystroke::parse(&keystroke).is_err() {
            return;
        }
        bindings.push(KeyBinding::new(
            keystroke.as_str(),
            RunConfiguredGhostexHotkey {
                action_id: action_id.to_string(),
            },
            None,
        ));
    };
    for (action_id, default_key) in GPUI_DEFAULT_GHOSTEX_HOTKEYS {
        let key = match persisted_hotkeys.get(*action_id) {
            Some(serde_json::Value::String(persisted_key)) => persisted_key.as_str(),
            _ => default_key,
        };
        let key = gpui_migrated_hotkey_for_action(action_id, key, default_key);
        push_binding(action_id, gpui_platform_hotkey_for_action(action_id, key));
    }
    // Persisted ids beyond the mirrored default table keep binding as before
    // (the shared normalizer only writes known ids, so this is a safety net
    // for maps written by newer app versions).
    let default_action_ids: std::collections::HashSet<&str> = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .map(|(action_id, _)| *action_id)
        .collect();
    for (action_id, key) in &persisted_hotkeys {
        if action_id.trim().is_empty() || default_action_ids.contains(action_id.as_str()) {
            continue;
        }
        let Some(key) = key.as_str() else {
            continue;
        };
        push_binding(action_id, key);
    }
    bindings
}


pub(crate) fn gpui_configured_hotkey_unbinds_from_settings(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
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
            let key = match persisted_hotkeys.and_then(|hotkeys| hotkeys.get(*action_id)) {
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
    keys.into_iter()
        .filter(|keystroke| Keystroke::parse(keystroke).is_ok())
        .map(|keystroke| {
            KeyBinding::new(keystroke.as_str(), gpui::Unbind(action_name.into()), None)
        })
        .collect()
}

