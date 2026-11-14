//! Pins what every hotkey binds to, on each platform, in each section of Settings > Hotkeys.
//!
//! Every hotkey that can act from a terminal has two bindings: the normal one, which applies
//! everywhere but in a terminal, and the Terminal focused one, which applies only while a terminal
//! has the keyboard (CDXC:Hotkeys 2026-10-27 DECISION in src/hotkey_definitions.rs). macOS must
//! keep exactly the bindings it had: the chord of every hotkey in both sections, and the app action
//! each formerly hard-coded key runs (apps/desktop/src/app/shell_hotkeys.rs, read from source
//! because the desktop crate is too heavy to build for a test). On Windows and Linux the normal
//! bindings are the ones the app always had, and the Terminal focused ones follow Ghostty's Linux
//! defaults (.dependencies/ghostty/src/config/Config.zig, `Keybinds.init`).

use std::collections::BTreeMap;

use ghostex_settings_catalog::{
    customized_normal_hotkey, hotkey_definition, hotkey_definitions, hotkey_has_normal_binding,
    hotkey_has_terminal_binding, hotkey_keystroke, modules, platform_hotkey, terminal_hotkey,
    Platform, TERMINAL_ONLY_HOTKEY_IDS, VIEW_SCOPED_HOTKEY_IDS,
};

const WINDOWS_AND_LINUX: [Platform; 2] = [Platform::Windows, Platform::Linux];

/// The GPUI keystroke of each normal binding's default on `platform` (`""` when it has none).
fn normal_keystrokes(platform: Platform) -> BTreeMap<&'static str, String> {
    hotkey_definitions()
        .iter()
        .filter(|definition| hotkey_has_normal_binding(definition.id))
        .map(|definition| {
            let key = platform_hotkey(definition.id, definition.default_key, platform);
            (
                definition.id,
                hotkey_keystroke(key, platform).unwrap_or_default(),
            )
        })
        .collect()
}

/// The GPUI keystroke of each Terminal focused binding's default on `platform`.
fn terminal_keystrokes(platform: Platform) -> BTreeMap<&'static str, String> {
    hotkey_definitions()
        .iter()
        .filter_map(|definition| {
            let key = terminal_hotkey(definition.id, None, None, platform)?;
            Some((
                definition.id,
                hotkey_keystroke(&key, platform).unwrap_or_default(),
            ))
        })
        .collect()
}

fn pinned(entries: &[(&'static str, &str)]) -> BTreeMap<&'static str, String> {
    entries
        .iter()
        .map(|(id, keystroke)| (*id, keystroke.to_string()))
        .collect()
}

#[test]
fn macos_normal_bindings_keep_their_chords() {
    let mut expected = pinned(&[
        ("scrollChatToBottom", "ctrl-shift-down"),
        ("focusChatComposer", ""),
        ("copyLastChatCodeBlock", "cmd-shift-;"),
        ("copyLastChatReply", "cmd-shift-c"),
        ("toggleChatSummaryMode", "ctrl-alt-s"),
        ("toggleChatComposerMaximized", "ctrl-alt-m"),
        ("toggleChatFilesLinks", "ctrl-alt-l"),
        ("newChatPickProject", "alt-o"),
        ("newChatPickComputer", "alt-m"),
        ("newChatPickWhere", "alt-w"),
        ("newChatPickBranch", "alt-b"),
        ("toggleVoiceAgent", "ctrl-alt-v"),
        ("voiceCallAboutSession", ""),
        ("holdToTalk", ""),
        ("toggleDictation", "ctrl-alt-space"),
        ("readAloudLastReply", ""),
        ("readAloudStop", ""),
        ("createAgentSession", "cmd-shift-o"),
        ("createSession", "cmd-shift-t"),
        ("openCommandPalette", "cmd-shift-p"),
        ("openSessionSearchPalette", "cmd-p"),
        ("openProjectSearchPalette", "cmd-alt-shift-o"),
        ("openQuickAccessFiles", "cmd-o"),
        ("openNewThreadPalette", "cmd-n"),
        ("openCommandsPanel", "cmd-j"),
        ("openCommandsPanelSecondKey", "shift-escape"),
        ("openSettings", "cmd-,"),
        ("openExtensions", ""),
        ("openGhostexHelp", ""),
        ("openFileInFiles", ""),
        ("openHotkeys", "cmd-/"),
        ("toggleSidebarCollapsed", "cmd-b"),
        ("toggleViewPanel", "cmd-alt-b"),
        ("expandViewPanel", "cmd-ctrl-e"),
        ("expandViewPanelFully", "cmd-ctrl-shift-e"),
        ("renameActiveSession", "cmd-r"),
        ("openBrowserPane", "cmd-t"),
        ("switchAgentsView", ""),
        ("switchSourceView", ""),
        ("switchGitHubView", ""),
        ("switchKanbanView", ""),
        ("switchManageView", ""),
        ("switchAutomateView", ""),
        ("switchChangesView", ""),
        ("switchTerminalView", ""),
        ("switchTitlebarView1", "alt-1"),
        ("switchTitlebarView2", "alt-2"),
        ("switchTitlebarView3", "alt-3"),
        ("switchTitlebarView4", "alt-4"),
        ("switchTitlebarView5", "alt-5"),
        ("switchTitlebarView6", "alt-6"),
        ("switchTitlebarView7", "alt-7"),
        ("switchTitlebarView8", "alt-8"),
        ("switchTitlebarView9", "alt-9"),
        ("rotatePanesClockwise", "ctrl-shift-l"),
        ("mergeAllTabs", "ctrl-shift-m"),
        ("delayedSend", "ctrl-shift-s"),
        ("closeAfterDone", ""),
        ("openModelPicker", "alt-p"),
        ("promptEditor", "ctrl-g"),
        ("attachFileOrFolder", "cmd-alt-p"),
        ("sessionNote", "cmd-alt-n"),
        ("stashPrompt", "alt-s"),
        ("stashedPrompts", "cmd-alt-s"),
        // No default since 2026-10-27 (CDXC:Hotkeys DECISION on its row); it was cmd-alt-e.
        ("exportTranscript", ""),
        ("toggleAgentActions", "cmd-alt-a"),
        ("toggleChatView", "alt-g"),
        ("openFindPrompts", "cmd-shift-f"),
        ("searchProjectFiles", "cmd-shift-f"),
        ("scrollTerminalToTop", ""),
        ("scrollTerminalToBottom", ""),
        ("splitSessionRight", "alt-shift-d"),
        ("forkSession", "cmd-ctrl-shift-f"),
        ("reloadSession", ""),
        ("sleepFocusedSession", "cmd-shift-a"),
        ("wakeFocusedSession", ""),
        ("closeFocusedSession", "cmd-shift-backspace"),
        ("popOutPane", "ctrl-shift-o"),
        ("focusPreviousGroup", ""),
        ("focusNextGroup", ""),
        ("navigateHistoryBack", "cmd-["),
        ("navigateHistoryForward", "cmd-]"),
        ("openNotifications", "cmd-i"),
        ("jumpToLatestUnreadNotification", "cmd-shift-u"),
        ("deferNotificationAndJumpNext", "cmd-ctrl-u"),
        ("focusPreviousSession", "ctrl-shift-tab"),
        ("focusNextSession", "ctrl-tab"),
        ("focusPreviousPaneTab", "cmd-alt-["),
        ("focusNextPaneTab", "cmd-alt-]"),
        ("focusUp", "cmd-alt-up"),
        ("focusRight", "cmd-alt-right"),
        ("focusDown", "cmd-alt-down"),
        ("focusLeft", "cmd-alt-left"),
        ("jumpToProject1", "cmd-ctrl-1"),
        ("jumpToProject2", "cmd-ctrl-2"),
        ("jumpToProject3", "cmd-ctrl-3"),
        ("jumpToProject4", "cmd-ctrl-4"),
        ("jumpToProject5", "cmd-ctrl-5"),
        ("jumpToProject6", "cmd-ctrl-6"),
        ("jumpToProject7", "cmd-ctrl-7"),
        ("jumpToProject8", "cmd-ctrl-8"),
        ("jumpToProject9", "cmd-ctrl-9"),
        ("goToSpace1", "cmd-alt-shift-1"),
        ("goToSpace2", "cmd-alt-shift-2"),
        ("goToSpace3", "cmd-alt-shift-3"),
        ("goToSpace4", "cmd-alt-shift-4"),
        ("goToSpace5", "cmd-alt-shift-5"),
        ("goToSpace6", "cmd-alt-shift-6"),
        ("goToSpace7", "cmd-alt-shift-7"),
        ("goToSpace8", "cmd-alt-shift-8"),
        ("goToSpace9", "cmd-alt-shift-9"),
        ("focusSessionSlot1", "cmd-1"),
        ("focusSessionSlot2", "cmd-2"),
        ("focusSessionSlot3", "cmd-3"),
        ("focusSessionSlot4", "cmd-4"),
        ("focusSessionSlot5", "cmd-5"),
        ("focusSessionSlot6", "cmd-6"),
        ("focusSessionSlot7", "cmd-7"),
        ("focusSessionSlot8", "cmd-8"),
        ("focusSessionSlot9", "cmd-9"),
        ("runActionSlot1", "ctrl-shift-1"),
        ("runActionSlot2", "ctrl-shift-2"),
        ("runActionSlot3", "ctrl-shift-3"),
        ("runActionSlot4", "ctrl-shift-4"),
        ("runActionSlot5", "ctrl-shift-5"),
        ("splitMore", "cmd-d"),
        ("splitMoreDown", "cmd-shift-d"),
        // Hard-coded in apps/desktop/src/main.rs and the terminal until 2026-10-27, at these chords.
        ("pasteIntoTerminal", "cmd-v"),
        ("findInFocusedView", "cmd-f"),
        ("findNextInBrowser", "cmd-g"),
        ("findPreviousInBrowser", "cmd-shift-g"),
        ("zoomInFocusedView", "cmd-="),
        ("zoomOutFocusedView", "cmd--"),
        ("resetFocusedViewZoom", "cmd-0"),
        ("closeFocusedSurface", "cmd-w"),
        ("openNewWindow", "cmd-shift-n"),
        ("toggleAgentsFocusMode", "cmd-ctrl-f"),
        ("terminalCopy", "cmd-c"),
        ("terminalSelectAll", "cmd-a"),
        ("openBrowserHistory", "cmd-y"),
        ("reloadBrowserPage", "cmd-r"),
        ("sleepFocusedSessionSecondKey", "alt-shift-s"),
        // New on 2026-10-27; one key in and out of the terminal.
        ("unfocusTerminal", "cmd-shift-m"),
        ("focusLastTerminal", "cmd-shift-m"),
        ("focusSidebar", "cmd-shift-j"),
    ]);
    // Only the Terminal focused section has these.
    for id in TERMINAL_ONLY_HOTKEY_IDS {
        expected.remove(id);
    }
    assert_eq!(normal_keystrokes(Platform::MacOs), expected);
}

/// On macOS the Terminal focused section starts as a copy of the normal one, so a terminal
/// answers the same keys it always did.
#[test]
fn macos_terminal_focused_bindings_equal_the_normal_ones() {
    let normal = normal_keystrokes(Platform::MacOs);
    let terminal = terminal_keystrokes(Platform::MacOs);
    for (id, keystroke) in &terminal {
        if let Some(normal) = normal.get(id) {
            assert_eq!(keystroke, normal, "{id}");
        }
    }
    assert_eq!(terminal["terminalCopy"], "cmd-c");
    assert_eq!(terminal["terminalSelectAll"], "cmd-a");
    assert_eq!(terminal["unfocusTerminal"], "cmd-shift-m");
}

/// The normal Windows and Linux bindings are the ones the app had before issue #315: the catalog's
/// chords of 2026-10-25 and the literal keys apps/desktop/src/main.rs bound then.
#[test]
fn windows_and_linux_normal_bindings_are_the_ones_the_app_always_had() {
    let expected = pinned(&[
        ("scrollChatToBottom", "ctrl-shift-down"),
        ("focusChatComposer", ""),
        ("copyLastChatCodeBlock", "ctrl-shift-;"),
        ("copyLastChatReply", ""),
        ("toggleChatSummaryMode", "ctrl-alt-shift-s"),
        ("toggleChatComposerMaximized", "ctrl-alt-shift-m"),
        ("toggleChatFilesLinks", "ctrl-alt-shift-l"),
        ("newChatPickProject", "alt-o"),
        ("newChatPickComputer", "alt-m"),
        ("newChatPickWhere", "alt-w"),
        ("newChatPickBranch", "alt-b"),
        ("toggleVoiceAgent", "ctrl-alt-shift-v"),
        ("voiceCallAboutSession", ""),
        ("holdToTalk", ""),
        // Ctrl+Alt+Space since 2026-11-05 (CDXC:VoiceMode in hotkey_definitions.rs).
        ("toggleDictation", "ctrl-alt-space"),
        ("readAloudLastReply", ""),
        ("readAloudStop", ""),
        ("createAgentSession", "ctrl-shift-o"),
        ("createSession", "ctrl-shift-t"),
        ("openCommandPalette", "ctrl-shift-p"),
        ("openSessionSearchPalette", "ctrl-p"),
        ("openProjectSearchPalette", "ctrl-alt-shift-o"),
        ("openQuickAccessFiles", "ctrl-o"),
        ("openNewThreadPalette", "ctrl-n"),
        ("openCommandsPanel", "f12"),
        ("openCommandsPanelSecondKey", "shift-escape"),
        ("openSettings", "ctrl-,"),
        ("openExtensions", ""),
        ("openGhostexHelp", ""),
        ("openFileInFiles", ""),
        ("openHotkeys", "ctrl-/"),
        ("toggleSidebarCollapsed", "ctrl-b"),
        ("toggleViewPanel", "ctrl-alt-b"),
        ("expandViewPanel", "ctrl-alt-e"),
        ("expandViewPanelFully", "ctrl-alt-shift-e"),
        ("renameActiveSession", "ctrl-shift-r"),
        ("openBrowserPane", "ctrl-t"),
        ("switchAgentsView", ""),
        ("switchSourceView", ""),
        ("switchGitHubView", ""),
        ("switchKanbanView", ""),
        ("switchManageView", ""),
        ("switchAutomateView", ""),
        ("switchChangesView", ""),
        ("switchTerminalView", ""),
        ("switchTitlebarView1", "alt-1"),
        ("switchTitlebarView2", "alt-2"),
        ("switchTitlebarView3", "alt-3"),
        ("switchTitlebarView4", "alt-4"),
        ("switchTitlebarView5", "alt-5"),
        ("switchTitlebarView6", "alt-6"),
        ("switchTitlebarView7", "alt-7"),
        ("switchTitlebarView8", "alt-8"),
        ("switchTitlebarView9", "alt-9"),
        ("rotatePanesClockwise", "ctrl-alt-l"),
        ("mergeAllTabs", "ctrl-alt-m"),
        ("delayedSend", "ctrl-alt-s"),
        ("closeAfterDone", ""),
        ("openModelPicker", "alt-p"),
        ("promptEditor", "ctrl-shift-g"),
        ("attachFileOrFolder", "ctrl-alt-p"),
        ("sessionNote", "ctrl-alt-n"),
        ("stashPrompt", "alt-s"),
        ("stashedPrompts", "ctrl-shift-s"),
        // No default since 2026-10-27, which ended its clash with Expand View Panel.
        ("exportTranscript", ""),
        ("toggleAgentActions", "ctrl-alt-a"),
        ("toggleChatView", "alt-g"),
        ("openFindPrompts", "ctrl-shift-f"),
        ("searchProjectFiles", "ctrl-shift-f"),
        ("scrollTerminalToTop", ""),
        ("scrollTerminalToBottom", ""),
        ("splitSessionRight", "alt-shift-d"),
        ("forkSession", "ctrl-alt-shift-f"),
        ("reloadSession", ""),
        ("sleepFocusedSession", "ctrl-shift-a"),
        ("wakeFocusedSession", ""),
        ("closeFocusedSession", "ctrl-shift-backspace"),
        ("popOutPane", "ctrl-alt-o"),
        ("focusPreviousGroup", ""),
        ("focusNextGroup", ""),
        ("navigateHistoryBack", "ctrl-alt-shift-["),
        ("navigateHistoryForward", "ctrl-alt-shift-]"),
        ("openNotifications", "ctrl-i"),
        ("jumpToLatestUnreadNotification", "ctrl-shift-u"),
        ("deferNotificationAndJumpNext", "ctrl-alt-u"),
        ("focusPreviousSession", "ctrl-shift-tab"),
        ("focusNextSession", "ctrl-tab"),
        ("focusPreviousPaneTab", "ctrl-alt-["),
        ("focusNextPaneTab", "ctrl-alt-]"),
        ("focusUp", "ctrl-alt-up"),
        ("focusRight", "ctrl-alt-right"),
        ("focusDown", "ctrl-alt-down"),
        ("focusLeft", "ctrl-alt-left"),
        ("jumpToProject1", "ctrl-alt-1"),
        ("jumpToProject2", "ctrl-alt-2"),
        ("jumpToProject3", "ctrl-alt-3"),
        ("jumpToProject4", "ctrl-alt-4"),
        ("jumpToProject5", "ctrl-alt-5"),
        ("jumpToProject6", "ctrl-alt-6"),
        ("jumpToProject7", "ctrl-alt-7"),
        ("jumpToProject8", "ctrl-alt-8"),
        ("jumpToProject9", "ctrl-alt-9"),
        ("goToSpace1", "ctrl-alt-shift-1"),
        ("goToSpace2", "ctrl-alt-shift-2"),
        ("goToSpace3", "ctrl-alt-shift-3"),
        ("goToSpace4", "ctrl-alt-shift-4"),
        ("goToSpace5", "ctrl-alt-shift-5"),
        ("goToSpace6", "ctrl-alt-shift-6"),
        ("goToSpace7", "ctrl-alt-shift-7"),
        ("goToSpace8", "ctrl-alt-shift-8"),
        ("goToSpace9", "ctrl-alt-shift-9"),
        ("focusSessionSlot1", "ctrl-1"),
        ("focusSessionSlot2", "ctrl-2"),
        ("focusSessionSlot3", "ctrl-3"),
        ("focusSessionSlot4", "ctrl-4"),
        ("focusSessionSlot5", "ctrl-5"),
        ("focusSessionSlot6", "ctrl-6"),
        ("focusSessionSlot7", "ctrl-7"),
        ("focusSessionSlot8", "ctrl-8"),
        ("focusSessionSlot9", "ctrl-9"),
        ("runActionSlot1", "ctrl-shift-1"),
        ("runActionSlot2", "ctrl-shift-2"),
        ("runActionSlot3", "ctrl-shift-3"),
        ("runActionSlot4", "ctrl-shift-4"),
        ("runActionSlot5", "ctrl-shift-5"),
        ("splitMore", "ctrl-d"),
        ("splitMoreDown", "ctrl-shift-d"),
        ("pasteIntoTerminal", "ctrl-v"),
        ("findInFocusedView", "ctrl-f"),
        ("findNextInBrowser", "ctrl-g"),
        ("findPreviousInBrowser", "ctrl-shift-g"),
        ("zoomInFocusedView", "ctrl-="),
        ("zoomOutFocusedView", "ctrl--"),
        ("resetFocusedViewZoom", "ctrl-0"),
        ("closeFocusedSurface", "ctrl-w"),
        ("openNewWindow", "ctrl-shift-n"),
        ("toggleAgentsFocusMode", "ctrl-alt-f"),
        ("openBrowserHistory", "ctrl-h"),
        ("reloadBrowserPage", "ctrl-r"),
        ("sleepFocusedSessionSecondKey", "alt-shift-s"),
        ("focusLastTerminal", "ctrl-shift-m"),
        ("focusSidebar", "ctrl-shift-j"),
    ]);
    for platform in WINDOWS_AND_LINUX {
        assert_eq!(normal_keystrokes(platform), expected, "{platform:?}");
    }
}

/// `SHELL_HOTKEY_ACTIONS` in apps/desktop/src/app/shell_hotkeys.rs: hotkey id to the app action
/// it binds.
fn desktop_shell_actions() -> Vec<(String, String)> {
    let source = shell_hotkeys_source();
    let start = source
        .find("const SHELL_HOTKEY_ACTIONS")
        .expect("SHELL_HOTKEY_ACTIONS");
    let end = start + source[start..].find("];").expect("end of the table");
    // One `("id", || Box::new(Action))` entry per split, however rustfmt wraps it.
    source[start..end]
        .split("(\"")
        .skip(1)
        .filter_map(|entry| {
            let id = entry.split('"').next()?;
            let action = entry.split("Box::new(").nth(1)?.split(')').next()?;
            Some((id.to_string(), action.to_string()))
        })
        .collect()
}

fn shell_hotkeys_source() -> String {
    desktop_source("app/shell_hotkeys.rs")
}

fn desktop_source(path: &str) -> String {
    let path = format!(
        "{}/../../apps/desktop/src/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path}"))
}

#[test]
fn macos_hotkeys_keep_their_app_actions() {
    // The keys apps/desktop/src/main.rs and the terminal bound to these actions before 2026-10-27.
    let expected = [
        ("openBrowserHistory", "OpenBrowserHistory"),
        ("reloadBrowserPage", "ReloadFocusedBrowser"),
        ("pasteIntoTerminal", "PasteIntoFocusedTerminal"),
        ("findInFocusedView", "FindInFocusedTerminal"),
        ("findNextInBrowser", "FindNextInFocusedBrowser"),
        ("findPreviousInBrowser", "FindPreviousInFocusedBrowser"),
        ("zoomInFocusedView", "ZoomInFocusedSurface"),
        ("zoomOutFocusedView", "ZoomOutFocusedSurface"),
        ("resetFocusedViewZoom", "ResetFocusedSurfaceZoom"),
        ("closeFocusedSurface", "CloseFocusedSurface"),
        ("toggleSidebarCollapsed", "ToggleGpuiSidebarCollapsed"),
        ("toggleViewPanel", "ToggleViewPanel"),
        ("sleepFocusedSessionSecondKey", "SleepFocusedSession"),
        ("createSession", "NewTerminalTab"),
        ("openNewWindow", "NewGhostexGpuiWindow"),
        ("splitMore", "SplitFocusedTerminalRight"),
        ("splitMoreDown", "SplitFocusedTerminalDown"),
        ("openBrowserPane", "NewBrowserTab"),
        ("toggleAgentsFocusMode", "ToggleAgentsFocusMode"),
        ("mergeAllTabs", "MergeAllTabs"),
        ("focusLeft", "FocusWorkspaceLeft"),
        ("focusRight", "FocusWorkspaceRight"),
        ("focusUp", "FocusWorkspaceUp"),
        ("focusDown", "FocusWorkspaceDown"),
        ("terminalCopy", "terminal_element::TerminalCopyHotkey"),
        (
            "terminalSelectAll",
            "terminal_element::TerminalSelectAllHotkey",
        ),
    ]
    .map(|(id, action)| (id.to_string(), action.to_string()));
    assert_eq!(desktop_shell_actions(), expected);
    // Every hotkey that exists only to bind an app action has one.
    let shell_ids: Vec<String> = desktop_shell_actions()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    for definition in hotkey_definitions() {
        if definition.kind() == "shellAction" {
            assert!(
                shell_ids.iter().any(|id| id == definition.id),
                "{} binds no action",
                definition.id
            );
        }
    }
}

/// The desktop binds a normal binding with `!GhostexGpuiTerminal` and a Terminal focused one with
/// `GhostexGpuiTerminal`, the context the focused terminal element sets, so the first never fires
/// in a terminal and the second only there.
#[test]
fn the_desktop_binds_each_section_in_or_out_of_the_terminal() {
    assert!(desktop_source("terminal_element.rs")
        .contains("pub(crate) const TERMINAL_KEY_CONTEXT: &str = \"GhostexGpuiTerminal\";"));
    let hotkeys = desktop_source("app/hotkeys.rs");
    assert!(hotkeys.contains(
        "pub(crate) const GPUI_OUTSIDE_TERMINAL_KEY_CONTEXT: &str = \"!GhostexGpuiTerminal\";"
    ));
    for source in [hotkeys.as_str(), shell_hotkeys_source().as_str()] {
        assert!(source.contains("gpui_terminal_hotkey_key_in("));
        assert!(source.contains("Some(terminal_element::TERMINAL_KEY_CONTEXT)"));
        assert!(source.contains("gpui_normal_hotkey_context("));
    }
    assert!(
        !shell_hotkeys_source().contains("\"ctrl-shift-v\""),
        "a fixed Ctrl+Shift+V would keep pasting after Paste is rebound"
    );
}

#[test]
fn windows_and_linux_terminal_focused_bindings_use_ghosttys_linux_keys() {
    for platform in WINDOWS_AND_LINUX {
        let keystrokes = terminal_keystrokes(platform);
        for (id, ghostty) in [
            ("terminalCopy", "ctrl-shift-c"),
            ("pasteIntoTerminal", "ctrl-shift-v"),
            ("terminalSelectAll", "ctrl-shift-a"),
            ("findInFocusedView", "ctrl-shift-f"),
            ("closeFocusedSurface", "ctrl-shift-w"),
            ("createSession", "ctrl-shift-t"),
            ("openNewWindow", "ctrl-shift-n"),
            ("splitMore", "ctrl-shift-o"),
            ("splitMoreDown", "ctrl-shift-e"),
            ("openCommandPalette", "ctrl-shift-p"),
            ("openSettings", "ctrl-,"),
            ("focusNextSession", "ctrl-tab"),
            ("focusPreviousSession", "ctrl-shift-tab"),
            ("zoomInFocusedView", "ctrl-="),
            ("zoomOutFocusedView", "ctrl--"),
            ("resetFocusedViewZoom", "ctrl-0"),
            ("focusLeft", "ctrl-alt-left"),
            ("focusRight", "ctrl-alt-right"),
            ("focusUp", "ctrl-alt-up"),
            ("focusDown", "ctrl-alt-down"),
        ] {
            assert_eq!(keystrokes[id], ghostty, "{id} on {platform:?}");
        }
    }
}

#[test]
fn windows_and_linux_terminal_focused_ghostex_keys() {
    for platform in WINDOWS_AND_LINUX {
        let keystrokes = terminal_keystrokes(platform);
        for (id, keystroke) in [
            ("createAgentSession", "ctrl-shift-d"),
            ("openSessionSearchPalette", "ctrl-alt-shift-p"),
            ("openQuickAccessFiles", "ctrl-shift-y"),
            ("openNewThreadPalette", "ctrl-alt-shift-n"),
            ("openHotkeys", "ctrl-alt-/"),
            ("toggleSidebarCollapsed", "ctrl-shift-b"),
            ("openBrowserPane", "ctrl-shift-l"),
            ("openFindPrompts", "ctrl-alt-h"),
            ("openNotifications", "ctrl-shift-i"),
            ("exportTranscript", ""),
            ("sleepFocusedSession", ""),
            ("toggleAgentsFocusMode", "ctrl-alt-f"),
            ("unfocusTerminal", "ctrl-shift-m"),
        ] {
            assert_eq!(keystrokes[id], keystroke, "{id} on {platform:?}");
        }
    }
}

/// CDXC:Hotkeys 2026-10-27 WHY:
/// Ctrl with a letter or `[ ] \ / @ ^ _ space` sends a control character the terminal and its programs read (Ctrl+W deletes a word, Ctrl+D ends input, Ctrl+I is Tab). The only plain Ctrl keys a Terminal focused default may take on Windows and Linux are Ghostty's own (Settings, zoom, next tab) and the numbered session slots the user kept.
#[test]
fn windows_and_linux_terminal_focused_bindings_leave_plain_ctrl_keys_to_the_terminal() {
    let allowed = [
        "ctrl-,", "ctrl-=", "ctrl--", "ctrl-0", "ctrl-tab", "ctrl-1", "ctrl-2", "ctrl-3", "ctrl-4",
        "ctrl-5", "ctrl-6", "ctrl-7", "ctrl-8", "ctrl-9",
    ];
    for platform in WINDOWS_AND_LINUX {
        for (id, keystroke) in terminal_keystrokes(platform) {
            let Some(key) = keystroke.strip_prefix("ctrl-") else {
                continue;
            };
            assert!(
                key.contains('-') && key != "-" || allowed.contains(&keystroke.as_str()),
                "{id} takes {keystroke} from the terminal on {platform:?}"
            );
        }
    }
}

/// The chat box's Delete Line and the Files editor's Strikethrough, Highlight and Redo on Windows
/// and Linux (`native_chat/keyboard.rs`, zorite-editor): a default on one of these would win over
/// the text field.
#[test]
fn windows_and_linux_leave_text_editing_keys_alone() {
    let editing = [
        "ctrl-shift-k",
        "ctrl-shift-x",
        "ctrl-shift-h",
        "ctrl-shift-z",
    ];
    for platform in WINDOWS_AND_LINUX {
        for (id, keystroke) in normal_keystrokes(platform)
            .into_iter()
            .chain(terminal_keystrokes(platform))
        {
            assert!(
                !editing.contains(&keystroke.as_str()),
                "{id} takes {keystroke} on {platform:?}"
            );
        }
    }
}

/// Conflicts are checked within one section only, and no section has one on any platform (Export
/// Transcript's default, which shared Ctrl+Alt+E with Expand View Panel on Windows and Linux, was
/// removed on 2026-10-27).
#[test]
fn no_two_defaults_share_a_chord_within_a_section() {
    let duplicates = |keystrokes: BTreeMap<&'static str, String>| {
        let mut by_keystroke: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for (id, keystroke) in keystrokes {
            if !keystroke.is_empty() && !VIEW_SCOPED_HOTKEY_IDS.contains(&id) {
                by_keystroke.entry(keystroke).or_default().push(id);
            }
        }
        by_keystroke.retain(|_, ids| ids.len() > 1);
        by_keystroke
    };
    for platform in WINDOWS_AND_LINUX {
        assert!(
            duplicates(normal_keystrokes(platform)).is_empty(),
            "{platform:?}"
        );
        assert!(
            duplicates(terminal_keystrokes(platform)).is_empty(),
            "{platform:?}"
        );
    }
    assert!(duplicates(normal_keystrokes(Platform::MacOs)).is_empty());
    assert!(duplicates(terminal_keystrokes(Platform::MacOs)).is_empty());
}

/// The same keys may be set in both sections: Unfocus Terminal and Focus Last Terminal share one
/// by default, so it goes out of a terminal and back in.
#[test]
fn the_same_chord_may_be_set_in_both_sections() {
    for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        assert_eq!(
            terminal_keystrokes(platform)["unfocusTerminal"],
            normal_keystrokes(platform)["focusLastTerminal"],
            "{platform:?}"
        );
    }
    assert!(!hotkey_has_normal_binding("unfocusTerminal"));
    assert!(!hotkey_has_terminal_binding(
        hotkey_definition("focusLastTerminal").expect("focusLastTerminal")
    ));
}

#[test]
fn saved_bindings_migrate_safely() {
    // Normal: a saved macOS default moves to the Windows and Linux default where there is one;
    // anything else the user chose stays.
    assert_eq!(
        platform_hotkey("renameActiveSession", "cmd+r", Platform::Linux),
        "cmd+shift+r"
    );
    assert_eq!(
        platform_hotkey("splitMore", "cmd+d", Platform::Linux),
        "cmd+d"
    );
    assert_eq!(
        platform_hotkey("splitMore", "cmd+alt+d", Platform::Linux),
        "cmd+alt+d"
    );
    // Terminal focused: nothing saved is the default, a recorded key stays as recorded (Ctrl+D
    // included, which a user may want to keep in a terminal) and a cleared one stays cleared.
    assert_eq!(
        terminal_hotkey("splitMore", None, None, Platform::Linux).as_deref(),
        Some("cmd+shift+o")
    );
    assert_eq!(
        terminal_hotkey("splitMore", Some("cmd+d"), None, Platform::Linux).as_deref(),
        Some("cmd+d")
    );
    assert_eq!(
        terminal_hotkey("splitMore", Some(""), None, Platform::Linux).as_deref(),
        Some("")
    );
    assert_eq!(
        terminal_hotkey("splitMore", Some("cmd+alt+9"), None, Platform::Linux).as_deref(),
        Some("cmd+alt+9")
    );
    // A hotkey without a Terminal focused binding has none to resolve.
    assert_eq!(
        terminal_hotkey("focusLastTerminal", None, None, Platform::Linux),
        None
    );
    assert_eq!(
        terminal_hotkey("scrollChatToBottom", None, None, Platform::MacOs),
        None
    );
}

/// `hotkeys` holds the whole table, so a saved value only counts as customized when it resolves to
/// something other than the default (CDXC:Hotkeys 2026-10-27 WHY on `terminal_hotkey`). The cases
/// are the user's own: Expand View Panel on Cmd+Shift+E, Previous/Next Session on Cmd+Ctrl+Tab,
/// and cleared keys.
#[test]
fn only_a_changed_normal_key_counts_as_customized() {
    for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        // Saved but equal to the default.
        let default = hotkey_definition("splitMore")
            .expect("splitMore")
            .platform_default_key(platform);
        assert_eq!(
            customized_normal_hotkey("splitMore", Some(default), platform),
            None
        );
        assert_eq!(customized_normal_hotkey("splitMore", None, platform), None);
        // Changed or cleared.
        assert_eq!(
            customized_normal_hotkey("expandViewPanel", Some("cmd+shift+e"), platform).as_deref(),
            Some("cmd+shift+e")
        );
        assert_eq!(
            customized_normal_hotkey("focusNextSession", Some("cmd+ctrl+tab"), platform).as_deref(),
            Some("cmd+ctrl+tab")
        );
        assert_eq!(
            customized_normal_hotkey("openNotifications", Some(""), platform).as_deref(),
            Some("")
        );
        // A retired default resolves to the default, so it is not a change.
        assert_eq!(
            customized_normal_hotkey("openFindPrompts", Some("alt+f"), platform),
            None
        );
        // Terminal-only hotkeys have no normal key to follow.
        assert_eq!(
            customized_normal_hotkey("terminalCopy", Some("cmd+x"), platform),
            None
        );
    }
    // A saved macOS default on Windows and Linux resolves to the Windows and Linux default.
    assert_eq!(
        customized_normal_hotkey("renameActiveSession", Some("cmd+r"), Platform::Linux),
        None
    );
}

/// While `terminalHotkeys` has no value for an action, its Terminal focused binding follows the
/// normal key the user customized, so keys they set before the section existed keep working in a
/// terminal; an uncustomized key gets the Terminal focused default; a recorded or cleared
/// Terminal focused value wins.
#[test]
fn a_customized_normal_key_also_applies_in_a_terminal() {
    let follow = |id: &str, saved_normal: Option<&str>, saved_terminal: Option<&str>, platform| {
        let customized = customized_normal_hotkey(id, saved_normal, platform);
        terminal_hotkey(id, saved_terminal, customized.as_deref(), platform)
    };
    // macOS: the customized normal key applies in a terminal too.
    assert_eq!(
        follow(
            "expandViewPanel",
            Some("cmd+shift+e"),
            None,
            Platform::MacOs
        )
        .as_deref(),
        Some("cmd+shift+e")
    );
    assert_eq!(
        follow(
            "focusPreviousSession",
            Some("cmd+ctrl+shift+tab"),
            None,
            Platform::MacOs
        )
        .as_deref(),
        Some("cmd+ctrl+shift+tab")
    );
    assert_eq!(
        follow("openNotifications", Some(""), None, Platform::MacOs).as_deref(),
        Some("")
    );
    for platform in WINDOWS_AND_LINUX {
        // Uncustomized, saved as the default (the whole table is saved): the Ghostty default.
        assert_eq!(
            follow("splitMore", Some("cmd+d"), None, platform).as_deref(),
            Some("cmd+shift+o"),
            "{platform:?}"
        );
        assert_eq!(
            follow("closeFocusedSurface", None, None, platform).as_deref(),
            Some("cmd+shift+w")
        );
        // Customized: inherited, a cleared one included.
        assert_eq!(
            follow("splitMore", Some("cmd+alt+d"), None, platform).as_deref(),
            Some("cmd+alt+d")
        );
        assert_eq!(
            follow("focusNextSession", Some("cmd+ctrl+tab"), None, platform).as_deref(),
            Some("cmd+ctrl+tab")
        );
        assert_eq!(
            follow("splitMore", Some(""), None, platform).as_deref(),
            Some("")
        );
        // A recorded or cleared Terminal focused value wins over the customized normal key.
        assert_eq!(
            follow(
                "splitMore",
                Some("cmd+alt+d"),
                Some("cmd+shift+x"),
                platform
            )
            .as_deref(),
            Some("cmd+shift+x")
        );
        assert_eq!(
            follow("splitMore", Some("cmd+alt+d"), Some(""), platform).as_deref(),
            Some("")
        );
        // Terminal-only hotkeys keep their own default.
        assert_eq!(
            follow("terminalCopy", Some("cmd+x"), None, platform).as_deref(),
            Some("cmd+shift+c")
        );
    }
}

/// The desktop, the Settings page and `ghostex settings hotkeys` all pass the customized normal
/// key into `terminal_hotkey`, each from the normal chord it binds or shows.
#[test]
fn every_surface_lets_the_terminal_binding_follow_a_customized_normal_key() {
    assert!(desktop_source("app/hotkeys.rs")
        .contains("gpui_customized_normal_hotkey_key_in(snapshot, action_id)"));
    assert!(desktop_source("app/window/settings_modal/tabs/hotkeys.rs")
        .contains("customized.get(&definition.id)"));
    assert!(desktop_source("terminal_element.rs").contains("customized_normal_hotkey("));
    let cli = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../server/src/ghostex_cli/settings_hotkeys.rs"
    ))
    .expect("server/src/ghostex_cli/settings_hotkeys.rs");
    assert!(cli.contains("normal.as_str() != platform_default(definition)"));
}

/// Paste on Windows and Linux: Ctrl+Shift+V only while a terminal has the keyboard, Ctrl+V
/// everywhere else (CDXC:Clipboard 2026-11-14 DECISION in src/hotkeys.rs). macOS keeps Cmd+V in
/// both sections.
#[test]
fn paste_is_ctrl_shift_v_in_a_terminal_and_ctrl_v_elsewhere_on_windows_and_linux() {
    for platform in WINDOWS_AND_LINUX {
        assert_eq!(normal_keystrokes(platform)["pasteIntoTerminal"], "ctrl-v");
        assert_eq!(
            terminal_keystrokes(platform)["pasteIntoTerminal"],
            "ctrl-shift-v"
        );
    }
    assert_eq!(
        normal_keystrokes(Platform::MacOs)["pasteIntoTerminal"],
        "cmd-v"
    );
    assert_eq!(
        terminal_keystrokes(Platform::MacOs)["pasteIntoTerminal"],
        "cmd-v"
    );
}

/// Settings > Hotkeys: `(section id, terminal, ids)`.
fn settings_sections() -> Vec<(String, bool, Vec<String>)> {
    let sections = modules::exports(modules::module::SETTINGS_TYPES, Platform::Linux)
        .into_iter()
        .find(|(name, _)| *name == "HOTKEY_SETTINGS_SECTIONS")
        .expect("HOTKEY_SETTINGS_SECTIONS")
        .1
        .to_value();
    sections
        .as_array()
        .expect("sections")
        .iter()
        .map(|section| {
            let ids = section["ids"]
                .as_array()
                .expect("ids")
                .iter()
                .map(|id| id.as_str().expect("id").to_string())
                .collect();
            (
                section["id"].as_str().expect("id").to_string(),
                section["terminal"].as_bool() == Some(true),
                ids,
            )
        })
        .collect()
}

/// Every binding has a row: the normal ones in the normal sections, the Terminal focused ones in
/// the Terminal focused section, the last one.
#[test]
fn settings_lists_every_binding_in_its_section() {
    let sections = settings_sections();
    let (id, terminal, terminal_ids) = sections.last().expect("sections").clone();
    assert_eq!((id.as_str(), terminal), ("terminalFocused", true));
    let normal_ids: Vec<String> = sections[..sections.len() - 1]
        .iter()
        .inspect(|(_, terminal, _)| assert!(!terminal))
        .flat_map(|(_, _, ids)| ids.clone())
        .collect();
    for definition in hotkey_definitions() {
        assert_eq!(
            normal_ids.iter().any(|id| id == definition.id),
            hotkey_has_normal_binding(definition.id),
            "{} in the normal sections",
            definition.id
        );
        assert_eq!(
            terminal_ids.iter().any(|id| id == definition.id),
            hotkey_has_terminal_binding(definition),
            "{} in Terminal focused",
            definition.id
        );
    }
    assert_eq!(terminal_ids[0], "terminalCopy");
}

/// The user kept Ctrl+1-9 for the session slots on Windows and Linux, rebindable there (Cmd+1-9 on
/// macOS, pinned above).
#[test]
fn session_slots_are_ctrl_digits_on_windows_and_linux_and_can_be_rebound() {
    let slots: Vec<String> = (1..=9)
        .map(|slot| format!("focusSessionSlot{slot}"))
        .collect();
    let (_, _, section) = settings_sections()
        .into_iter()
        .find(|(id, _, _)| id == "sessionSlots")
        .expect("Session Slots section");
    assert_eq!(section, slots);
    for platform in WINDOWS_AND_LINUX {
        let normal = normal_keystrokes(platform);
        let terminal = terminal_keystrokes(platform);
        for (index, slot) in slots.iter().enumerate() {
            assert_eq!(normal[slot.as_str()], format!("ctrl-{}", index + 1));
            assert_eq!(terminal[slot.as_str()], format!("ctrl-{}", index + 1));
            // A chord the user records stays theirs instead of moving back to Ctrl+N.
            let custom = format!("alt+shift+{}", index + 1);
            let key = platform_hotkey(slot, &custom, platform);
            assert_eq!(key, custom);
            assert_eq!(
                hotkey_keystroke(key, platform).as_deref(),
                Some(format!("alt-shift-{}", index + 1).as_str())
            );
            // Cleared, it binds nothing, so Ctrl+N reaches the terminal.
            assert_eq!(
                hotkey_keystroke(platform_hotkey(slot, "", platform), platform),
                None
            );
        }
    }
}

/// Export Transcript has no default key (CDXC:Hotkeys 2026-10-27 DECISION on its row), and its old
/// defaults are retired: the user's own saved `"exportTranscript": "cmd+alt+e"` (`hotkeys` stores
/// the whole table), a Windows and Linux `ctrl+alt+e`, and the short-lived Terminal focused
/// `cmd+alt+x` all resolve to unassigned instead of counting as keys the user chose.
#[test]
fn export_transcript_has_no_default_and_its_old_defaults_retire() {
    let definition = hotkey_definition("exportTranscript").expect("exportTranscript");
    for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        assert_eq!(
            normal_keystrokes(platform)["exportTranscript"],
            "",
            "{platform:?}"
        );
        assert_eq!(
            terminal_keystrokes(platform)["exportTranscript"],
            "",
            "{platform:?}"
        );
        for old in ["cmd+alt+e", "ctrl+alt+e", "cmd+alt+x"] {
            assert!(definition.retired_default_keys.contains(&old));
            // A saved normal copy is not a customization, so the terminal row does not inherit it.
            assert_eq!(
                customized_normal_hotkey("exportTranscript", Some(old), platform),
                None,
                "{old} on {platform:?}"
            );
            let customized = customized_normal_hotkey("exportTranscript", Some(old), platform);
            assert_eq!(
                terminal_hotkey("exportTranscript", None, customized.as_deref(), platform)
                    .as_deref(),
                Some(""),
                "{old} on {platform:?}"
            );
            // A saved Terminal focused copy resolves to unassigned too.
            assert_eq!(
                terminal_hotkey("exportTranscript", Some(old), None, platform).as_deref(),
                Some(""),
                "{old} on {platform:?}"
            );
        }
        // A key the user records is kept.
        assert_eq!(
            customized_normal_hotkey("exportTranscript", Some("cmd+alt+shift+x"), platform)
                .as_deref(),
            Some("cmd+alt+shift+x")
        );
    }
    // The desktop retires the same keys for the bindings it makes.
    let hotkeys = desktop_source("app/hotkeys.rs");
    assert!(hotkeys.contains("(\"exportTranscript\", \"\"),"));
    assert!(hotkeys.contains("[\"cmd+alt+e\", \"ctrl+alt+e\", \"cmd+alt+x\"]"));
}
