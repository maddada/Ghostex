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
pub const RAIL_MENU_GROUPS: [&str; 3] = ["Go to", "Agents", "This computer"];

/// Every item, in ⋯ menu order.
///
/// CDXC:Sidebar 2026-11-21 DECISION:
/// User: views on the rail "don't make sense", "even when I have a view open in the side panel, don't show it in the left-side rail"; the rail is for agents and hubs (Hermes, the Claude and Grok bots, Voice Call, All Automations, Main Hub, Agents Hub, Search). So the side panel views (Files, Changes, Browser, Project Board, Dev servers, Resources, Extensions) are no rail items: never an icon, not even while open, never a pin, and no Views group in ⋯ (coordinator's call; they stay in the side panel and their usual buttons). Notifications left too: the bell is in the sidebar's top bar. Saved pins of theirs are not read, and the desktop drops them from the settings file once (`drop_retired_rail_pins` in packages/settings-catalog). Supersedes the Views group and the Notifications row of the 2026-11-20 rail.
pub const RAIL_ITEMS: &[RailItem] = &[
    item("search", "Search", "Go to"),
    item("mainHub", "Main Hub", "Go to"),
    item("agentsHub", "Agents Hub", "Go to"),
    item("allAutomations", "All Automations", "Go to"),
    item("commands", "Commands", "Go to"),
    item("hermes", "Hermes bots", "Agents"),
    item("claude", "Claude", "Agents"),
    item("grok", "Grok Bot", "Agents"),
    item("voiceCall", "Voice Call", "Agents"),
    item("connectPhone", "Connect a Phone", "This computer"),
    item("settings", "Settings", "This computer"),
];

/// Pinned when `sidebarRailPins` is missing.
///
/// CDXC:Sidebar 2026-11-20 DECISION:
/// User (Q4318 B): with the rail on, the notifications bell sits in the sidebar's top bar at its top left, the corner Toggle sidebar left for the rail's foot. So the bell is no longer pinned by default; it stays in ⋯ for anyone who wants it on the rail too, and a saved `sidebarRailPins` that has it keeps it (coordinator's call).
pub const RAIL_DEFAULT_PINS: &[&str] = &[
    "search",
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

/// `pins` with the pinned `key` dropped on the pin at `target` (an index into `pins`; `pins.len()`
/// drops it after the last one): it takes that pin's place, before it, whichever way it moved.
pub fn rail_pins_dropped(pins: &[&'static str], key: &str, target: usize) -> Vec<&'static str> {
    let Some(from) = pins.iter().position(|pin| *pin == key) else {
        return rail_pins_moved(pins, key, target);
    };
    // `rail_pins_moved` counts in the list without the dragged pin, where everything after it
    // sits one place higher.
    let to = if from < target { target - 1 } else { target };
    rail_pins_moved(pins, key, to)
}

/// `pins` as the settings value.
pub fn rail_pins_value(pins: &[&'static str]) -> Value {
    Value::Array(
        pins.iter()
            .map(|key| Value::String((*key).to_string()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINS: &[&str] = &["search", "notifications", "mainHub"];

    #[test]
    fn a_pin_dropped_on_a_later_pin_takes_its_place() {
        assert_eq!(
            rail_pins_dropped(PINS, "search", 2),
            ["notifications", "search", "mainHub"]
        );
        assert_eq!(
            rail_pins_dropped(PINS, "search", 1),
            ["search", "notifications", "mainHub"]
        );
    }

    #[test]
    fn a_pin_dropped_on_an_earlier_pin_takes_its_place() {
        assert_eq!(
            rail_pins_dropped(PINS, "mainHub", 0),
            ["mainHub", "search", "notifications"]
        );
    }

    #[test]
    fn a_pin_dropped_after_the_last_one_ends_the_rail() {
        assert_eq!(
            rail_pins_dropped(PINS, "search", PINS.len()),
            ["notifications", "mainHub", "search"]
        );
    }

    #[test]
    fn pins_keep_known_keys_once() {
        let mut settings = Map::new();
        settings.insert(
            RAIL_PINS_SETTINGS_KEY.to_string(),
            serde_json::json!(["claude", "nope", "claude", "grok"]),
        );
        assert_eq!(rail_pins(&settings), ["claude", "grok"]);
        assert_eq!(rail_pins(&Map::new()), RAIL_DEFAULT_PINS);
    }

    #[test]
    fn pinning_adds_at_the_end_and_unpinning_removes() {
        assert_eq!(
            rail_pins_with(PINS, "grok", true),
            ["search", "notifications", "mainHub", "grok"]
        );
        assert_eq!(
            rail_pins_with(PINS, "search", false),
            ["notifications", "mainHub"]
        );
    }
}
