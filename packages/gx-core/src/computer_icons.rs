//! Every computer's icon: the one picked for it, else one made from what kind of computer it is.
//!
//! CDXC:RemoteMachines 2026-11-12 DECISION:
//! User: "please show the machine icon on all sessions in the sidebar even local ones when we hav multiple machines connected. i want each machine to please have a different icon but i'm not able to set this so far. pls fix. no need to show WN for short machine name i just want different icon that i can pick per machine in settings". A computer's mark is an icon, picked per computer in Settings > Remote from [`COMPUTER_ICONS`], and drawn with no short name beside it (its full name is in the tooltip). The pick is kept on the computer itself, by its gxserver (server/src/computer_icon.rs), so every desktop and phone that reaches it shows the same icon. Unpicked, a laptop shows the laptop, another computer the desktop, and one whose kind is unknown (an older Ghostex there, a page) its system's mark, else the desktop. This supersedes the short names of 2026-11-06 (machine_short_names.rs) as the computer's mark, and the system marks of 2026-11-11 as the only icon.
//! SEE-ALSO: server/src/computer_icon.rs (where it is kept), apps/desktop/src/app/gx_store/computer_icons.rs (the desktop's copy of every computer's), apps/desktop/src/app/native_sidebar/machines.rs (the computers row), native_sidebar/threads.rs (`computer_badge`).

/// One icon a computer can wear: its id (what gxserver keeps), its name in the picker, and its
/// drawing among the app's `titlebar/` icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputerIcon {
    pub id: &'static str,
    pub label: &'static str,
    pub asset: &'static str,
}

const fn icon(id: &'static str, label: &'static str, asset: &'static str) -> ComputerIcon {
    ComputerIcon { id, label, asset }
}

/// The icons Settings offers, in the picker's order.
pub const COMPUTER_ICONS: [ComputerIcon; 18] = [
    icon("laptop", "Laptop", "titlebar/device-laptop.svg"),
    icon("desktop", "Desktop", "titlebar/device-desktop.svg"),
    icon("server", "Server", "titlebar/server.svg"),
    icon("cloud", "Cloud", "titlebar/cloud.svg"),
    icon("terminal", "Terminal", "titlebar/terminal-2.svg"),
    icon("cpu", "Chip", "titlebar/cpu.svg"),
    icon("database", "Database", "titlebar/database.svg"),
    icon("home", "House", "titlebar/home.svg"),
    icon("building", "Building", "titlebar/building.svg"),
    icon("briefcase", "Work", "titlebar/briefcase.svg"),
    icon("world", "Globe", "titlebar/world.svg"),
    icon("bolt", "Lightning", "titlebar/bolt.svg"),
    icon("star", "Star", "titlebar/star.svg"),
    icon("moon", "Moon", "titlebar/moon.svg"),
    icon("leaf", "Leaf", "titlebar/leaf.svg"),
    icon("rocket", "Rocket", "titlebar/rocket.svg"),
    icon("flask", "Flask", "titlebar/flask.svg"),
    icon("coffee", "Coffee", "titlebar/coffee.svg"),
];

/// The icon with this id, when it is one of [`COMPUTER_ICONS`].
pub fn computer_icon(id: &str) -> Option<&'static ComputerIcon> {
    let id = id.trim();
    COMPUTER_ICONS.iter().find(|icon| icon.id == id)
}

/// What gxserver can tell of the computer it runs on: it has a battery (a laptop) or not.
pub const COMPUTER_KIND_LAPTOP: &str = "laptop";
pub const COMPUTER_KIND_DESKTOP: &str = "desktop";

/// The marks of a computer's system, for one whose kind is unknown, as `titlebar/` drawings.
fn system_asset(system: Option<&str>) -> Option<&'static str> {
    match system? {
        "windows" => Some("titlebar/brand-windows.svg"),
        "macos" => Some("titlebar/brand-apple.svg"),
        "linux" => Some("titlebar/brand-ubuntu.svg"),
        _ => None,
    }
}

/// The drawing a computer wears: the picked icon, else its kind's (`laptop`, `desktop`), else its
/// system's mark (`windows`, `macos`, `linux`), else the desktop.
pub fn computer_icon_asset(
    picked: Option<&str>,
    kind: Option<&str>,
    system: Option<&str>,
) -> &'static str {
    picked
        .and_then(computer_icon)
        .or_else(|| kind.and_then(computer_icon))
        .map(|icon| icon.asset)
        .or_else(|| system_asset(system))
        .unwrap_or("titlebar/device-desktop.svg")
}
