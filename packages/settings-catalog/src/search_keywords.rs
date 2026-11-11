//! Words Settings search matches besides a row's title, subtitle and options, hidden from the UI.
//!
//! CDXC:Settings 2026-11-06 DECISION:
//! User: "make searching for Code Editor find VS Code also". A row (or a hotkey) is found by its
//! title, subtitle and options; some things go by another name people type ("code editor" for
//! VS Code) that reads badly in a description. Those names live here, keyed by the row key or
//! hotkey id, and Settings search matches each one like a title. `rows::row` copies a row's words
//! into `SettingRow::keywords`; the Hotkeys page looks a hotkey's up with `search_keywords`.

const CODE_EDITOR: &[&str] = &["code editor", "editor"];

/// CDXC:Settings 2026-11-11 DECISION:
/// User (Q2602-Q2611), renaming Settings groups that repeated their page's name: UI Scale (was Interface scale), Session Tags (was Sidebar Tags), Behavior and Scrolling (were Terminal Behavior and Terminal Scrolling), Past recordings (was Dictation history), Log (was Security log), Configuration (was Accounts), Your agentbox boxes (was Your boxes), and Check a 1Password reference moved into the 1Password group. A search for a group's old name still finds its rows: the old names below are search words of those rows.
const INTERFACE_SCALE: &[&str] = &["interface scale"];

/// The extra search words of a row key or hotkey id; empty for most.
pub fn search_keywords(key: &str) -> &'static [&'static str] {
    match key {
        // The Extensions page's VS Code card, the VS Code view's General rows, and its hotkey.
        "code"
        | "codeServerLinkVscodeUserConfig"
        | "codeServerUseVscodeInsidersUserConfig"
        | "codeServerNewWindowsAsTabs"
        | "switchSourceView" => CODE_EDITOR,
        // The Interface page's UI Scale group.
        "interfaceScalePercent"
        | "interfaceScaleAreasSeparately"
        | "agentManagerZoomPercent"
        | "sessionChatZoomPercent"
        | "filesZoomPercent"
        | "terminalFontSize"
        | "linuxDisplayScale" => INTERFACE_SCALE,
        "secretReferenceCheck" => &["check a reference"],
        "agentboxBoxes" => &["your boxes"],
        _ => &[],
    }
}

/// The old names of a renamed group (a catalog section, by its id), which Settings search
/// matches on each of the group's rows like a title. See [`INTERFACE_SCALE`]'s decision.
pub fn section_search_keywords(section_id: &str) -> &'static [&'static str] {
    match section_id {
        "sidebarTags" => &["sidebar tags"],
        "terminalBehavior" => &["terminal behavior"],
        "terminalScrolling" => &["terminal scrolling"],
        "dictations" => &["dictation history"],
        _ => &[],
    }
}
