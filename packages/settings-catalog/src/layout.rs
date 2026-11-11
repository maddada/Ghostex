//! The pages the old General page split into, each with the search sections it renders.
//!
//! CDXC:Settings 2026-10-29 DECISION:
//! User (Q5, "tiers"): General is gone. Each of its rail groups is a Settings page of its own (Chat,
//! Sidebar & Sessions, Terminal, Notifications, Files & Links, Power & Sleep), listed in the rail's
//! App tier with Theme and Hotkeys; the Setup tier holds Agents, Accounts, Integrations, Remote,
//! Projects and About; Extensions and its folded pages stay at the bottom. (The App and Setup tiers
//! are superseded by the labelled groups of CDXC:Settings 2026-11-11 at `RailTier` in model.rs.) Q7: the Tools group is
//! called Files & Links. Q8: the Terminal page is always in the rail. The old Advanced group is
//! gone too: Sleeping Sessions joined Sidebar & Sessions and Experimental moved to About. Q6 ("keep
//! mode", superseded the same day by the Advanced fold, CDXC:Settings 2026-10-29 in
//! data/sections.rs): advanced rows fold at the bottom of each page. A group's id
//! is its page's tab id (`SettingsTabId::id`), and `settings` still opens the first page, Chat.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/model.rs (`SettingsTabId::RAIL_ORDER`, `general_page`).

/// A page the General page split into, and the search sections it renders, in rendered order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneralGroup {
    pub id: &'static str,
    pub title: &'static str,
    pub sections: &'static [&'static str],
}

/// One of the General pages, in rail order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavItem {
    pub id: &'static str,
    pub title: &'static str,
}

/// Which search sections each General page renders, in rendered order. Single-section pages
/// keep using that section's own search result (no separate page-title match). `appearance` is
/// the Interface page, which has its own tab.
pub const GENERAL_GROUPS: &[GeneralGroup] = &[
    group("appearance", "Interface", &["theming"]),
    group("chat", "Chat", &["chat", "markedHandovers"]),
    group(
        "sidebar",
        "Sidebar & Sessions",
        &[
            "sidebar",
            "spaces",
            "sessionCards",
            "sidebarTags",
            "sleepingSessions",
        ],
    ),
    group(
        "terminal",
        "Terminal",
        &["terminal", "terminalBehavior", "terminalScrolling"],
    ),
    group(
        "tools",
        "Files & Links",
        &["fileOpening", "browser", "editor", "terminalDevServers"],
    ),
    group("notifications", "Notifications", &["sounds"]),
    group(
        "system",
        "Power & Sleep",
        &["computerStartup", "autoSleep", "power"],
    ),
];

const fn group(
    id: &'static str,
    title: &'static str,
    sections: &'static [&'static str],
) -> GeneralGroup {
    GeneralGroup {
        id,
        title,
        sections,
    }
}

pub fn general_group(id: &str) -> Option<&'static GeneralGroup> {
    GENERAL_GROUPS.iter().find(|group| group.id == id)
}

/// The General pages in rail order (the Help files list them in this order too). Theme sits
/// between Terminal and Notifications in the rail but keeps its own tab, so it is not listed.
///
/// CDXC:Theming 2026-09-23 DECISION:
/// User: "make theme into it's own page in settings below General". The Theme and App Icon sections render on the Theme page, so they are not a General page; their search rows and the `appearance` group stay in this catalog so one query still finds them.
///
/// CDXC:Settings 2026-06-12-04:13:
/// Ghostty terminal controls belong in the Settings window's own search so one query can find app settings and terminal settings together.
pub fn general_navigation() -> Vec<NavItem> {
    vec![
        nav("chat", "Chat"),
        nav("sidebar", "Sidebar & Sessions"),
        nav("terminal", "Terminal"),
        nav("notifications", "Notifications"),
        nav("tools", "Files & Links"),
        nav("system", "Power & Sleep"),
    ]
}

const fn nav(id: &'static str, title: &'static str) -> NavItem {
    NavItem { id, title }
}
