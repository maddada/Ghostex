//! The rail: the narrow column of pinnable icons at the window's left edge (`sidebarLayout` "rail").
//! Which items exist, their ⋯ menu groups, the default pins, and how a pin, an unpin or a drag
//! changes `sidebarRailPins`. The desktop and the web build draw it (app/sidebar_rail/).
//!
//! CDXC:Sidebar 2026-11-20 DECISION:
//! User (rail page, Q4297 C; then the build brief for 11.12.0): "add a new static sidebar on Ghostex that allows the user to pin stuff there". A 40px rail of 20px icons at the window's left that stays when the sidebar hides. Pinned by default: Search, Notifications, Main Hub, Agents Hub, All Automations, Hermes, Claude and Grok Bot; everything else is under its ⋯ menu, each row with a pin ("Pin to rail"); pinned icons drag into any order; an unpinned row still opens on a click and its icon shows under a dashed line while its view is showing, then leaves. The account usage toggle and the workspace sit at the rail's foot and are not pins.
//! SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (`sidebarRailPins`, the same default list), apps/desktop/src/app/sidebar_rail/ (the renderer).

use serde_json::{Map, Value};

/// The settings key holding the pinned items' keys, top to bottom.
pub const RAIL_PINS_SETTINGS_KEY: &str = "sidebarRailPins";

/// The settings key choosing the rail ("rail") or the classic sidebar ("classic").
pub const SIDEBAR_LAYOUT_SETTINGS_KEY: &str = "sidebarLayout";

/// One thing the rail can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailItem {
    /// Stored in `sidebarRailPins`; never rename one.
    pub key: &'static str,
    pub label: &'static str,
    /// Its ⋯ menu group (`RAIL_MENU_GROUPS`).
    pub group: &'static str,
}

const fn item(key: &'static str, label: &'static str, group: &'static str) -> RailItem {
    RailItem { key, label, group }
}

/// The ⋯ menu's groups, in order.
pub const RAIL_MENU_GROUPS: [&str; 4] = ["Go to", "Agents", "Views", "This computer"];

/// Every item, in ⋯ menu order.
pub const RAIL_ITEMS: &[RailItem] = &[
    item("search", "Search", "Go to"),
    item("notifications", "Notifications", "Go to"),
    item("mainHub", "Main Hub", "Go to"),
    item("agentsHub", "Agents Hub", "Go to"),
    item("allAutomations", "All Automations", "Go to"),
    item("commands", "Commands", "Go to"),
    item("hermes", "Hermes bots", "Agents"),
    item("claude", "Claude", "Agents"),
    item("grok", "Grok Bot", "Agents"),
    item("voiceCall", "Voice Call", "Agents"),
    item("files", "Files", "Views"),
    item("changes", "Changes", "Views"),
    item("browser", "Browser", "Views"),
    item("projectBoard", "Project Board", "Views"),
    item("devServers", "Dev servers", "Views"),
    item("resources", "Resources", "Views"),
    item("extensions", "Extensions", "Views"),
    item("connectPhone", "Connect a Phone", "This computer"),
    item("settings", "Settings", "This computer"),
];

/// Pinned when `sidebarRailPins` is missing.
pub const RAIL_DEFAULT_PINS: &[&str] = &[
    "search",
    "notifications",
    "mainHub",
    "agentsHub",
    "allAutomations",
    "hermes",
    "claude",
    "grok",
];

/// The item with `key`.
pub fn rail_item(key: &str) -> Option<RailItem> {
    RAIL_ITEMS.iter().copied().find(|item| item.key == key)
}

/// Whether `settings` chooses the rail. A file without the key is a new install's: the rail.
pub fn rail_layout_on(settings: &Map<String, Value>) -> bool {
    settings
        .get(SIDEBAR_LAYOUT_SETTINGS_KEY)
        .and_then(Value::as_str)
        != Some("classic")
}

/// The pinned items' keys, top to bottom: known keys only, each once.
pub fn rail_pins(settings: &Map<String, Value>) -> Vec<&'static str> {
    let Some(saved) = settings
        .get(RAIL_PINS_SETTINGS_KEY)
        .and_then(Value::as_array)
    else {
        return RAIL_DEFAULT_PINS.to_vec();
    };
    let mut pins: Vec<&'static str> = Vec::new();
    for key in saved.iter().filter_map(Value::as_str) {
        if let Some(item) = rail_item(key) {
            if !pins.contains(&item.key) {
                pins.push(item.key);
            }
        }
    }
    pins
}

/// `pins` with `key` pinned (at the end) or unpinned.
pub fn rail_pins_with(pins: &[&'static str], key: &str, pinned: bool) -> Vec<&'static str> {
    let mut next: Vec<&'static str> = pins.iter().copied().filter(|pin| *pin != key).collect();
    if pinned {
        next.extend(rail_item(key).map(|item| item.key));
    }
    next
}

/// `pins` with `key` moved to `index` (counted in the list without it), pinning it there when it
/// was not pinned (a ⋯ row dropped on the rail).
pub fn rail_pins_moved(pins: &[&'static str], key: &str, index: usize) -> Vec<&'static str> {
    let Some(item) = rail_item(key) else {
        return pins.to_vec();
    };
    let mut next: Vec<&'static str> = pins.iter().copied().filter(|pin| *pin != key).collect();
    next.insert(index.min(next.len()), item.key);
    next
}

/// `pins` as the settings value.
pub fn rail_pins_value(pins: &[&'static str]) -> Value {
    Value::Array(
        pins.iter()
            .map(|key| Value::String((*key).to_string()))
            .collect(),
    )
}
