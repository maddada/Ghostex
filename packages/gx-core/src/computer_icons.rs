//! Every computer's icon: the one picked for it, else its system's logo.
//!
//! CDXC:RemoteMachines 2026-11-12 DECISION:
//! User: "please show the machine icon on all sessions in the sidebar even local ones when we hav multiple machines connected. i want each machine to please have a different icon but i'm not able to set this so far. pls fix. no need to show WN for short machine name i just want different icon that i can pick per machine in settings". A computer's mark is an icon, picked per computer in Settings > Remote from [`COMPUTER_ICONS`], and drawn with no short name beside it (its full name is in the tooltip). The pick is kept on the computer itself, by its gxserver (server/src/computer_icon.rs), so every desktop and phone that reaches it shows the same icon. This supersedes the short names of 2026-11-06 (machine_short_names.rs) as the computer's mark.
//!
//! CDXC:RemoteMachines 2026-11-14 DECISION:
//! User: "why can't i set apple logo here for my mac? ... also same for arch logo and windows logos, we need to show all the logos in the picker for machine icons pls. And we need to be smarter about the icon shown". The picker offers the systems' logos (Apple, Windows, Tux and the Linux distributions in [`COMPUTER_ICONS`]) before the plain icons, and a computer with no pick wears its own system's logo, its Linux distribution's where its gxserver can tell ([`system_logo`]), not the laptop or desktop its kind gave it on 2026-11-12. Kind is left only for a computer whose system is unknown.
//! SEE-ALSO: server/src/computer_icon.rs (where it is kept), apps/desktop/src/app/gx_store/computer_icons.rs (the desktop's copy of every computer's), apps/desktop/src/app/native_sidebar/machines.rs (the computers row), native_sidebar/threads.rs (`computer_badge`), the phone's src/machines/computerIcons.ts and src/components/sessions/computerIconPaths.ts (its copy of the list and drawings).

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
pub const COMPUTER_ICONS: [ComputerIcon; 26] = [
    icon("apple", "Apple", "titlebar/brand-apple.svg"),
    icon("windows", "Windows", "titlebar/brand-windows.svg"),
    icon("linux", "Linux", "titlebar/brand-linux.svg"),
    icon("ubuntu", "Ubuntu", "titlebar/brand-ubuntu.svg"),
    icon("debian", "Debian", "titlebar/brand-debian.svg"),
    icon("fedora", "Fedora", "titlebar/brand-fedora.svg"),
    icon("arch", "Arch Linux", "titlebar/brand-arch.svg"),
    icon("redhat", "Red Hat", "titlebar/brand-redhat.svg"),
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

/// The logo of a Linux distribution, from `/etc/os-release`'s `ID` and `ID_LIKE` (a derivative such
/// as Pop!_OS or Manjaro wears its parent's), else Tux.
pub fn linux_distribution_logo(id: &str, id_like: &str) -> &'static str {
    fn logo(name: &str) -> Option<&'static str> {
        match name.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "ubuntu" => Some("ubuntu"),
            "debian" | "raspbian" => Some("debian"),
            "fedora" => Some("fedora"),
            "arch" | "archarm" | "archlinux" | "manjaro" | "endeavouros" | "cachyos" => {
                Some("arch")
            }
            "rhel" | "centos" | "rocky" | "almalinux" => Some("redhat"),
            _ => None,
        }
    }
    // A derivative names its parents nearest first (Pop!_OS: "ubuntu debian").
    logo(id)
        .or_else(|| id_like.split_whitespace().find_map(logo))
        .unwrap_or("linux")
}

/// The logo (an id in [`COMPUTER_ICONS`]) a computer with no pick wears: its system's
/// (`macos`, `windows`, `linux`), and for Linux its distribution's when known (`linux_logo`, from
/// [`linux_distribution_logo`]).
pub fn system_logo(system: Option<&str>, linux_logo: Option<&str>) -> Option<&'static str> {
    match system? {
        "windows" => Some("windows"),
        "macos" => Some("apple"),
        "linux" => Some(
            linux_logo
                .and_then(computer_icon)
                .map_or("linux", |icon| icon.id),
        ),
        _ => None,
    }
}

/// The drawing a computer wears: the picked icon, else the system logo its gxserver named
/// (`logo`, which knows its Linux distribution and that a WSL computer is Windows), else its
/// system's logo (`windows`, `macos`, `linux`), else its kind's (`laptop`, `desktop`), else the
/// desktop.
pub fn computer_icon_asset(
    picked: Option<&str>,
    logo: Option<&str>,
    kind: Option<&str>,
    system: Option<&str>,
) -> &'static str {
    picked
        .and_then(computer_icon)
        .or_else(|| logo.and_then(computer_icon))
        .or_else(|| system_logo(system, None).and_then(computer_icon))
        .or_else(|| kind.and_then(computer_icon))
        .map_or("titlebar/device-desktop.svg", |icon| icon.asset)
}
