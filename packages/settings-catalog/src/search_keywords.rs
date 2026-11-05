//! Words Settings search matches besides a row's title, subtitle and options, hidden from the UI.
//!
//! CDXC:Settings 2026-11-06 DECISION:
//! User: "make searching for Code Editor find VS Code also". A row (or a hotkey) is found by its
//! title, subtitle and options; some things go by another name people type ("code editor" for
//! VS Code) that reads badly in a description. Those names live here, keyed by the row key or
//! hotkey id, and Settings search matches each one like a title. `rows::row` copies a row's words
//! into `SettingRow::keywords`; the Hotkeys page looks a hotkey's up with `search_keywords`.

const CODE_EDITOR: &[&str] = &["code editor", "editor"];

/// The extra search words of a row key or hotkey id; empty for most.
pub fn search_keywords(key: &str) -> &'static [&'static str] {
    match key {
        // The Extensions page's VS Code card, the VS Code view's General rows, and its hotkey.
        "code"
        | "codeServerLinkVscodeUserConfig"
        | "codeServerUseVscodeInsidersUserConfig"
        | "codeServerNewWindowsAsTabs"
        | "switchSourceView" => CODE_EDITOR,
        _ => &[],
    }
}
