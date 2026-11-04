//! The hotkey catalog's shape and the views of it Settings, Quick Access and the Help files read.

use crate::hotkey_definitions::HOTKEY_DEFINITIONS;
use crate::hotkey_label::hotkey_label;
use crate::json::{Json, ToJson, J};
use crate::Platform;

/// One configurable hotkey action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotkeyDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The macOS default (`cmd` is the primary modifier on every platform); empty when unassigned.
    pub default_key: &'static str,
    /// The Windows and Linux default where it differs; `Some("")` means unassigned there.
    pub windows_linux_default_key: Option<&'static str>,
    /// More chords that also run the action while its own binding is not cleared.
    pub alternate_default_keys: &'static [&'static str],
    /// Former defaults: a saved binding equal to one moves to the current default.
    pub retired_default_keys: &'static [&'static str],
    /// `{ id, kind, ... }`: what the hotkey does.
    pub action: J,
}

impl HotkeyDefinition {
    /// `action.kind`.
    pub fn kind(&self) -> &'static str {
        self.action
            .get("kind")
            .and_then(J::as_str)
            .unwrap_or_default()
    }

    /// The default chord on `platform`.
    pub fn platform_default_key(&self, platform: Platform) -> &'static str {
        match platform {
            Platform::MacOs => self.default_key,
            Platform::Windows | Platform::Linux => {
                self.windows_linux_default_key.unwrap_or(self.default_key)
            }
        }
    }

    /// The macOS default formatted for `platform`'s labels (`defaultKeyLabel`); empty when unassigned.
    pub fn default_key_label(&self, platform: Platform) -> String {
        if self.default_key.is_empty() {
            String::new()
        } else {
            hotkey_label(self.default_key, platform)
        }
    }
}

/// Hotkeys that act only while one view has focus, so their chord may also belong to an app-wide
/// hotkey without the two clashing (Settings marks no duplicate for them).
pub const VIEW_SCOPED_HOTKEY_IDS: &[&str] = &[
    "searchProjectFiles",
    // Bound in or answered by the browser page only (CDXC:Hotkeys 2026-10-27 DECISION in
    // hotkey_definitions.rs).
    "findNextInBrowser",
    "findPreviousInBrowser",
    "openBrowserHistory",
    "reloadBrowserPage",
];

/// The settings key that holds the Terminal focused bindings, the way `hotkeys` holds the normal
/// ones (id to chord, `""` for one the user cleared).
pub const TERMINAL_HOTKEYS_SETTING_KEY: &str = "terminalHotkeys";

/// Hotkeys that exist only in the Terminal focused section: they act on the focused terminal.
pub const TERMINAL_ONLY_HOTKEY_IDS: &[&str] =
    &["terminalCopy", "terminalSelectAll", "unfocusTerminal"];

/// Hotkeys with no Terminal focused binding: they act on something a terminal never is (a chat,
/// a browser page, the Files view), or bring the keyboard back to a terminal.
const NO_TERMINAL_BINDING_HOTKEY_IDS: &[&str] = &[
    "readAloudLastReply",
    "searchProjectFiles",
    "findNextInBrowser",
    "findPreviousInBrowser",
    "openBrowserHistory",
    "reloadBrowserPage",
    "focusLastTerminal",
];

/// CDXC:Hotkeys 2026-10-27 WHY:
/// The Terminal focused defaults on Windows and Linux that differ from the normal ones: Ghostty's Linux keys where Ghostty has the action, and a Ctrl+Shift or Ctrl+Alt chord for Ghostex's own actions, so no plain Ctrl key a terminal program reads is taken (CDXC:Hotkeys 2026-10-27 DECISION in hotkey_definitions.rs). The chat box's and the Files editor's own keys (Ctrl+Shift+K, X, H, Z) were left alone even though those never apply in a terminal, so moving a key between the sections never lands on them.
const WINDOWS_LINUX_TERMINAL_DEFAULTS: &[(&str, &str)] = &[
    // CDXC:Clipboard 2026-10-27 DECISION:
    // User, on Windows and Linux paste (issue #315): "only when a terminal is focused please not always and only on windows/linux and allow rebinding it". So there Ctrl+Shift+V pastes only while a terminal has the keyboard (Paste's Terminal focused binding), plain Ctrl+V goes to the program in the terminal, and everywhere else Ctrl+V pastes as usual (Paste's normal binding). Both can be moved or cleared in Settings. macOS keeps Cmd+V in both. This supersedes the 2026-09-23 decision that bound Ctrl+Shift+V in terminals on top of a separate, app-wide Paste key.
    ("pasteIntoTerminal", "cmd+shift+v"),
    ("findInFocusedView", "cmd+shift+f"),
    ("closeFocusedSurface", "cmd+shift+w"),
    ("splitMore", "cmd+shift+o"),
    ("splitMoreDown", "cmd+shift+e"),
    ("createAgentSession", "cmd+shift+d"),
    ("openNewThreadPalette", "cmd+alt+shift+n"),
    ("openSessionSearchPalette", "cmd+alt+shift+p"),
    ("openQuickAccessFiles", "cmd+shift+y"),
    ("openHotkeys", "cmd+alt+/"),
    ("toggleSidebarCollapsed", "cmd+shift+b"),
    ("openBrowserPane", "cmd+shift+l"),
    ("openNotifications", "cmd+shift+i"),
    ("openFindPrompts", "cmd+alt+h"),
    // Ctrl+Shift+A is the terminal's Select All there, as in Ghostty.
    ("sleepFocusedSession", ""),
];

/// Whether the hotkey has a binding in the normal section (it applies outside a terminal).
pub fn hotkey_has_normal_binding(id: &str) -> bool {
    !TERMINAL_ONLY_HOTKEY_IDS.contains(&id)
}

/// Whether the hotkey has a Terminal focused binding. Hotkeys that act on a chat never do.
pub fn hotkey_has_terminal_binding(definition: &HotkeyDefinition) -> bool {
    !NO_TERMINAL_BINDING_HOTKEY_IDS.contains(&definition.id)
        && !matches!(definition.kind(), "chatAction" | "focusedChatAction")
}

/// The Terminal focused default on `platform`: the normal default on macOS and wherever Windows
/// and Linux keep it, otherwise Ghostty's or a Ctrl+Shift / Ctrl+Alt chord.
pub fn terminal_default_key(definition: &HotkeyDefinition, platform: Platform) -> &'static str {
    if platform != Platform::MacOs {
        if let Some((_, key)) = WINDOWS_LINUX_TERMINAL_DEFAULTS
            .iter()
            .find(|(id, _)| *id == definition.id)
        {
            return key;
        }
    }
    definition.platform_default_key(platform)
}

/// The Terminal focused default stored the way `default_key` is: `cmd` spelled for every
/// platform, with the Windows and Linux key beside it where it differs (Settings, Help).
pub fn windows_linux_terminal_default_key(definition: &HotkeyDefinition) -> Option<&'static str> {
    let windows_linux = terminal_default_key(definition, Platform::Linux);
    (windows_linux != definition.default_key).then_some(windows_linux)
}

pub fn hotkey_definitions() -> &'static [HotkeyDefinition] {
    HOTKEY_DEFINITIONS
}

pub fn hotkey_definition(id: &str) -> Option<&'static HotkeyDefinition> {
    HOTKEY_DEFINITIONS
        .iter()
        .find(|definition| definition.id == id)
}

/// `DEFAULT_GHOSTEX_HOTKEYS`: action id to its macOS default chord, in catalog order.
pub fn default_hotkeys() -> Vec<(&'static str, &'static str)> {
    HOTKEY_DEFINITIONS
        .iter()
        .map(|definition| (definition.id, definition.default_key))
        .collect()
}

impl ToJson for HotkeyDefinition {
    fn to_json(&self) -> Json {
        let mut entries = vec![
            ("action", self.action.to_json()),
            ("defaultKey", Json::str(self.default_key)),
        ];
        if let Some(key) = self.windows_linux_default_key {
            entries.push(("windowsLinuxDefaultKey", Json::str(key)));
        }
        entries.push(("description", Json::str(self.description)));
        entries.push(("id", Json::str(self.id)));
        if !self.alternate_default_keys.is_empty() {
            entries.push((
                "alternateDefaultKeys",
                self.alternate_default_keys.to_json(),
            ));
        }
        if !self.retired_default_keys.is_empty() {
            entries.push(("retiredDefaultKeys", self.retired_default_keys.to_json()));
        }
        entries.push(("title", Json::str(self.title)));
        Json::obj(entries)
    }
}
