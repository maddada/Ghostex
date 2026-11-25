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
        // The Size & Zoom page (the Interface page's UI Scale group until 2026-11-18).
        "interfaceScalePercent"
        | "interfaceScaleAreasSeparately"
        | "agentManagerZoomPercent"
        | "sessionChatZoomPercent"
        | "filesZoomPercent"
        | "terminalFontSize"
        | "linuxDisplayScale" => INTERFACE_SCALE,
        "secretReferenceCheck" => &["check a reference"],
        "agentboxBoxes" => &["your boxes"],
        // CDXC:HelperNames 2026-11-21: what people see in a macOS or Windows prompt and search for.
        "helperNames" => &[
            "zmx",
            "gxserver",
            "wmx",
            "permission",
            "privacy",
            "privacy & security",
            "local network",
            "app management",
            "firewall",
        ],
        // CDXC:RemoteMachines 2026-11-15: people say machine and offline as often as computer and
        // not connected.
        "sidebarHideDisconnectedMachines" => &["offline", "disconnected machines", "hide offline"],
        // CDXC:RemoteMachines 2026-11-22: people say machine icon as often as computer icon.
        "hideComputerIconUntilHover" => &[
            "machine icon",
            "machine icons",
            "computer icon",
            "computer icons",
            "hover",
        ],
        // CDXC:RemoteMachines 2026-11-20: what people call the computer they're on, and the dot.
        "markComputerYouAreOn" => &[
            "this computer",
            "this machine",
            "current computer",
            "dot",
            "dock dot",
        ],
        // CDXC:SessionChat 2026-11-19: Live pages were Ghostex pages (gx page) and inline HTML
        // until the user renamed them (Q4207, Q4208); a search for the old names still finds them.
        "chatPageOpenTarget" | "chatAnswersWaitForIdle" => {
            &["ghostex page", "gx page", "inline html"]
        }
        // CDXC:Theming 2026-11-22: Animated was called Live until the user renamed it; a search
        // for the old name still finds its rows.
        "windowGlassSource"
        | "windowGlassLiveStyleDark"
        | "windowGlassLiveStyleLight"
        | "windowGlassLiveSpeed"
        | "windowGlassLiveBrightness"
        | "windowGlassVideoOnlyOnPower" => &["live", "live background"],
        "acceptAll" | "agentAcceptAllEnabled" => &[
            "skip permissions",
            "bypass permissions",
            "full access",
            "accept all",
            "yolo",
        ],
        "analyticsEnabled" => &[
            "privacy",
            "telemetry",
            "posthog",
            "tracking",
            "do not track",
        ],
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
