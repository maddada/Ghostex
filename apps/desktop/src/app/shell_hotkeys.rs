//! The Settings > Hotkeys rows that run an app action directly (paste, find, zoom, close, new
//! window, Focus mode, terminal copy and select all, browser history and reload, and the shell's
//! own copy of the sidebar, view panel, new tab, split and pane-focus keys), bound at the chords
//! Settings gives them and bound again after every save: the normal binding outside a terminal,
//! the Terminal focused one inside it.
//!
//! CDXC:Hotkeys 2026-10-27 WHY:
//! These keys were literal bindings in main.rs and the terminal, so Settings could neither move nor clear them and, on Windows and Linux, Ctrl+W, Ctrl+D, Ctrl+F, Ctrl+T and Ctrl+V never reached the terminal (issue #315). They stay bound to their own actions rather than `RunConfiguredGhostexHotkey`, at the same chords by default, so macOS behaves exactly as before; only the chords now come from the hotkey catalog and the saved settings (CDXC:Hotkeys 2026-10-27 DECISION in packages/settings-catalog/src/hotkey_definitions.rs).
//! SEE-ALSO: packages/settings-catalog/tests/platform_hotkeys.rs reads `SHELL_HOTKEY_ACTIONS` from this file to pin which action each row runs.

use crate::*;

/// The catalog id each app action follows, in the order main.rs bound them. The `shellAction`
/// rows exist only for this; the others are catalog hotkeys whose `RunConfiguredGhostexHotkey`
/// binding wins at the same chord, with the action here as its fallback.
const SHELL_HOTKEY_ACTIONS: &[(&str, fn() -> Box<dyn Action>)] = &[
    ("openBrowserHistory", || Box::new(OpenBrowserHistory)),
    ("reloadBrowserPage", || Box::new(ReloadFocusedBrowser)),
    ("pasteIntoTerminal", || Box::new(PasteIntoFocusedTerminal)),
    ("findInFocusedView", || Box::new(FindInFocusedTerminal)),
    ("findNextInBrowser", || Box::new(FindNextInFocusedBrowser)),
    ("findPreviousInBrowser", || {
        Box::new(FindPreviousInFocusedBrowser)
    }),
    ("zoomInFocusedView", || Box::new(ZoomInFocusedSurface)),
    ("zoomOutFocusedView", || Box::new(ZoomOutFocusedSurface)),
    ("resetFocusedViewZoom", || Box::new(ResetFocusedSurfaceZoom)),
    ("closeFocusedSurface", || Box::new(CloseFocusedSurface)),
    ("toggleSidebarCollapsed", || {
        Box::new(ToggleGpuiSidebarCollapsed)
    }),
    ("toggleViewPanel", || Box::new(ToggleViewPanel)),
    ("sleepFocusedSessionSecondKey", || {
        Box::new(SleepFocusedSession)
    }),
    ("createSession", || Box::new(NewTerminalTab)),
    ("openNewWindow", || Box::new(NewGhostexGpuiWindow)),
    ("splitMore", || Box::new(SplitFocusedTerminalRight)),
    ("splitMoreDown", || Box::new(SplitFocusedTerminalDown)),
    ("openBrowserPane", || Box::new(NewBrowserTab)),
    ("toggleAgentsFocusMode", || Box::new(ToggleAgentsFocusMode)),
    ("mergeAllTabs", || Box::new(MergeAllTabs)),
    ("focusLeft", || Box::new(FocusWorkspaceLeft)),
    ("focusRight", || Box::new(FocusWorkspaceRight)),
    ("focusUp", || Box::new(FocusWorkspaceUp)),
    ("focusDown", || Box::new(FocusWorkspaceDown)),
    ("terminalCopy", || {
        Box::new(terminal_element::TerminalCopyHotkey)
    }),
    ("terminalSelectAll", || {
        Box::new(terminal_element::TerminalSelectAllHotkey)
    }),
];

fn key_binding(
    keystroke: &str,
    action: Box<dyn Action>,
    context: Option<&str>,
    cx: &gpui::App,
) -> Option<KeyBinding> {
    if !gpui_runtime_keystroke_parses(keystroke, cx) {
        return None;
    }
    let context = context.map(|context| {
        gpui::KeyBindingContextPredicate::parse(context)
            .unwrap_or_else(|error| panic!("invalid key context {context}: {error}"))
            .into()
    });
    KeyBinding::load(keystroke, action, context, false, None, cx).ok()
}

/// Every binding of these actions, each as `(keystroke, action, context)`, read from `snapshot`.
fn shell_hotkey_bindings(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
) -> Vec<(String, fn() -> Box<dyn Action>, Option<&'static str>)> {
    let keystroke =
        |key: Option<String>| key.and_then(|key| gpui_keystroke_from_shared_hotkey(&key));
    let mut bindings = Vec::new();
    for (action_id, action) in SHELL_HOTKEY_ACTIONS {
        let normal = keystroke(gpui_normal_hotkey_key_in(snapshot, action_id));
        if let Some(normal) = &normal {
            bindings.push((
                normal.clone(),
                *action,
                Some(gpui_normal_hotkey_context(action_id)),
            ));
        }
        let terminal = keystroke(gpui_terminal_hotkey_key_in(snapshot, action_id));
        if let Some(terminal) = &terminal {
            bindings.push((
                terminal.clone(),
                *action,
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            ));
        }
        // Ghostty's `shift+insert=paste_from_clipboard`, a fixed second key beside Paste.
        if *action_id == "pasteIntoTerminal" {
            bindings.push((
                "shift-insert".to_string(),
                *action,
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            ));
        }
        // Zoom In's second key, the `+` the same key types with Shift, in each section where Zoom
        // In is bound.
        if *action_id == "zoomInFocusedView" {
            let plus = if cfg!(target_os = "macos") {
                "cmd-+"
            } else {
                "ctrl-+"
            };
            if normal.is_some() {
                bindings.push((
                    plus.to_string(),
                    *action,
                    Some(gpui_normal_hotkey_context(action_id)),
                ));
            }
            if terminal.is_some() {
                bindings.push((
                    plus.to_string(),
                    *action,
                    Some(terminal_element::TERMINAL_KEY_CONTEXT),
                ));
            }
        }
    }
    bindings
}

/// The bindings for the current settings. Bind them before
/// `gpui_configured_hotkey_key_bindings_from_settings`, so a configured hotkey on the same chord
/// keeps precedence.
pub(crate) fn gpui_shell_hotkey_key_bindings_from_settings(cx: &gpui::App) -> Vec<KeyBinding> {
    shell_hotkey_bindings(&shared_settings::shared_sidebar_settings_snapshot())
        .into_iter()
        .filter_map(|(keystroke, action, context)| key_binding(&keystroke, action(), context, cx))
        .collect()
}

/// Unbind markers for the bindings `snapshot` made, added before a save rebinds them.
pub(crate) fn gpui_shell_hotkey_unbinds_from_settings(
    snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    cx: &gpui::App,
) -> Vec<KeyBinding> {
    shell_hotkey_bindings(snapshot)
        .into_iter()
        .filter_map(|(keystroke, action, _)| {
            key_binding(
                &keystroke,
                Box::new(gpui::Unbind(action().name().into())),
                None,
                cx,
            )
        })
        .collect()
}

/// The shell bindings and then the configured hotkeys for the current settings, in that order.
pub(crate) fn gpui_bind_hotkeys_from_settings(cx: &mut gpui::App) {
    let shell_hotkeys = gpui_shell_hotkey_key_bindings_from_settings(cx);
    cx.bind_keys(shell_hotkeys);
    let configured_hotkeys = gpui_configured_hotkey_key_bindings_from_settings(cx);
    cx.bind_keys(configured_hotkeys);
}
