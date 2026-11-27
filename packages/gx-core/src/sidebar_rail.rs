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

/// The host command type of the rail's menu rows (pin, unpin, move, run).
pub const RAIL_COMMAND_TYPE: &str = "sidebarRail";

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
///
/// CDXC:Sidebar 2026-11-22 DECISION:
/// User: remove Connect a Phone and Settings from the rail's ⋯ menu (they stay in the ☰ main menu)
/// and drop the now-empty "This computer" heading.
///
/// CDXC:Sidebar 2026-11-25 DECISION:
/// User: "please add by default settings as a button to the bottom of the left side rail of the app" and "please add theme to the 3 dots so i click on it and it shows the mini theming thingy we have", then picked from the rail-settings mockups: the gear fixed at the very bottom of the rail under the Account usage switch, on by default (Q4953 B); Theme under its own Appearance heading at the end of ⋯, after Links and Layout (Q4954 B), opening the compact Theme editor beside ⋯; and both pinnable (Q4955): Theme gets a pin like every other row, so it can sit on the rail too, and Settings comes back to ⋯ (last in Go to) with its pin, so a gear that was unpinned can be pinned again. Pinned, the gear shows at the very bottom whatever its place in `sidebarRailPins` (`RAIL_FOOT_ITEM`); unpinned, it is hidden. Supersedes the 2026-11-22 removal of Settings from ⋯; Connect a Phone stays out.
pub const RAIL_MENU_GROUPS: [&str; 3] = ["Go to", "Agents", "Appearance"];

/// The item that, while pinned, sits at the very bottom of the rail under the Account usage switch
/// rather than among the pins (CDXC:Sidebar 2026-11-25 above): it is never dragged or moved, and
/// its own menu only unpins it.
pub const RAIL_FOOT_ITEM: &str = "settings";

/// Every item, in ⋯ menu order.
///
/// CDXC:Sidebar 2026-11-21 DECISION:
/// User: views on the rail "don't make sense", "even when I have a view open in the side panel, don't show it in the left-side rail"; the rail is for agents and hubs (Hermes, the Claude and Grok bots, Voice Call, All Automations, Main Hub, Agents Hub, Search). So the side panel views (Files, Changes, Browser, Project Board, Dev servers, Resources, Extensions) are no rail items: never an icon, not even while open, never a pin, and no Views group in ⋯ (coordinator's call; they stay in the side panel and their usual buttons). Notifications left too: the bell is in the sidebar's top bar. Saved pins of theirs are not read, and the desktop drops them from the settings file once (`drop_retired_rail_pins` in packages/settings-catalog). Supersedes the Views group and the Notifications row of the 2026-11-20 rail. Since 2026-11-26 the user's extensions' views are pinnable (`RAIL_EXTENSION_PIN_PREFIX`); the built-in views stay off.
pub const RAIL_ITEMS: &[RailItem] = &[
    item("search", "Search", "Go to"),
    // CDXC:Sessions 2026-11-27 DECISION: Search by Prompt left the rail with its window; the
    // Sessions search finds chats by what was said in them (gx-core quick_access/sessions.rs).
    // Opens Ask Ghostex AI's chat (CDXC:Chief 2026-11-21 in server/src/chief/mod.rs); the key stays.
    item("mainHub", "Ask Ghostex AI", "Go to"),
    item("agentsHub", "Agents Hub", "Go to"),
    item("allAutomations", "All Automations", "Go to"),
    item("commands", "Commands", "Go to"),
    item("settings", "Settings", "Go to"),
    item("hermes", "Hermes bots", "Agents"),
    item("claude", "Claude", "Agents"),
    item("grok", "Grok Bot", "Agents"),
    item("voiceCall", "Voice Call", "Agents"),
    item("theme", "Theme", "Appearance"),
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
    "settings",
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

/// The start of a link's pin key (`link:<id>`): one of the user's own links (custom_links.rs).
///
/// CDXC:Sidebar 2026-11-22 DECISION:
/// User: "please allow pinning custom links that are added by the user to the rail". A link pins
/// like any rail item: Pin to rail and Unpin from rail on its ⋯ menu row (hover pin, or the row's
/// right-click menu), drag into any order among the other pins, Move up and Move down. Its pin is
/// `link:<id>` in `sidebarRailPins`, the same list, so the order is one order; the icon is the
/// site's icon once its page has been opened (the globe until then), the tooltip its name, and a
/// click opens its page. A pin whose link is gone is dropped when the pins are read, and removing
/// a link unpins it. Like the links (and `sidebarRailPins` itself), the pins stay on this computer.
pub const RAIL_LINK_PIN_PREFIX: &str = "link:";

/// The pin key of link `id`.
pub fn rail_link_pin_key(id: &str) -> String {
    format!("{RAIL_LINK_PIN_PREFIX}{id}")
}

/// The link id of pin `key`, when it is a link's.
pub fn rail_link_pin_id(key: &str) -> Option<&str> {
    key.strip_prefix(RAIL_LINK_PIN_PREFIX)
        .filter(|id| !id.is_empty())
}

/// The start of an extension view's pin key (`ext:<id>`): an installed extension with a side
/// panel view.
///
/// CDXC:Sidebar 2026-11-26 DECISION:
/// User (#772 follow-up): "please also allow me to pin views / extensions that are views that get shown as a view to the left rail (only show enabled ones please)". This narrows the 2026-11-21 rule on `RAIL_ITEMS` that kept side panel views off the rail: the built-in views stay off, but every enabled extension with a side panel view is a row under Extensions in ⋯, pinnable like a link, and a click opens or focuses its view in the side panel the way `ghostex extensions open` does. Its pin is `ext:<id>` in `sidebarRailPins`, the same list and order as every other pin. The desktop knows which extensions are on, so it draws (and lists) only those: a pin of an extension that is off or uninstalled is kept but not drawn, as a pinned Hermes is while bots are off, and it comes back when the extension does.
pub const RAIL_EXTENSION_PIN_PREFIX: &str = "ext:";

/// The pin key of extension `id`'s view.
pub fn rail_extension_pin_key(id: &str) -> String {
    format!("{RAIL_EXTENSION_PIN_PREFIX}{id}")
}

/// The extension id of pin `key`, when it is an extension view's.
pub fn rail_extension_pin_id(key: &str) -> Option<&str> {
    key.strip_prefix(RAIL_EXTENSION_PIN_PREFIX)
        .filter(|id| !id.is_empty())
}

/// The start of a separator's key (`sep:<id>`): a thin horizontal divider line on the rail.
///
/// CDXC:Sidebar 2026-11-22 DECISION:
/// User: "Add separator" in any rail icon's right-click menu inserts a non-clickable thin line
/// right below that icon, draggable to reorder like icons, with its own right-click menu (Add
/// separator, Remove), saved and synced with `sidebarRailPins`.
pub const RAIL_SEPARATOR_PREFIX: &str = "sep:";

/// The rail key of separator `id`.
pub fn rail_separator_key(id: &str) -> String {
    format!("{RAIL_SEPARATOR_PREFIX}{id}")
}

/// Whether `key` is a rail separator (`sep:<id>`).
pub fn is_rail_separator_key(key: &str) -> bool {
    key.strip_prefix(RAIL_SEPARATOR_PREFIX)
        .is_some_and(|id| !id.is_empty())
}

/// The next unused separator key (`sep:1`, `sep:2`, …) not already in `pins`.
pub fn next_rail_separator_key<S: AsRef<str>>(pins: &[S]) -> String {
    let mut n: usize = 1;
    loop {
        let candidate = rail_separator_key(&n.to_string());
        if !pins.iter().any(|pin| pin.as_ref() == candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Whether `key` can be pinned: a rail item, a link, an extension's view, or a separator.
fn pinnable(key: &str) -> bool {
    rail_item(key).is_some()
        || rail_link_pin_id(key).is_some()
        || rail_extension_pin_id(key).is_some()
        || is_rail_separator_key(key)
}

/// The section of the rail where a pinned entry sits: top (below Toggle sidebar, above ⋯) or
/// bottom (above the foot's workspace and account usage buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailSection {
    Top,
    Bottom,
}

/// The sentinel entry inside `sidebarRailPins` separating top pins from bottom pins.
///
/// CDXC:Sidebar 2026-11-22 DECISION:
/// User: "please allow me to pin things bottom of the sidebar or top of the sidebar anywhere i
/// want there. i dont see separator that i can pin in the sidebar also please fix this too".
/// Keeping `sidebarRailPins` as a flat JSON string array with an optional `"slot:bottom"` marker
/// keeps older saved layouts working unchanged (arrays without the marker place all entries at the
/// top), lets older builds skip the marker cleanly, and preserves existing settings migrations
/// (`unpin_bell_from_saved_rail_pins`, `drop_retired_rail_pins`). When `bottom` is empty, the
/// marker is omitted on save so top-only layouts keep the exact same JSON shape.
pub const RAIL_BOTTOM_MARKER: &str = "slot:bottom";

/// The rail's pinned entries split into its top and bottom groups, each in top-to-bottom order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailLayout {
    pub top: Vec<String>,
    pub bottom: Vec<String>,
}

impl RailLayout {
    /// Every pinned key across top then bottom, without the section marker.
    pub fn all(&self) -> Vec<String> {
        let mut all = Vec::with_capacity(self.top.len() + self.bottom.len());
        all.extend(self.top.iter().cloned());
        all.extend(self.bottom.iter().cloned());
        all
    }

    /// Whether `key` is pinned in either section.
    pub fn contains(&self, key: &str) -> bool {
        self.top.iter().any(|pin| pin == key) || self.bottom.iter().any(|pin| pin == key)
    }

    /// Which section and index within that section `key` sits at.
    pub fn position(&self, key: &str) -> Option<(RailSection, usize)> {
        if let Some(index) = self.top.iter().position(|pin| pin == key) {
            return Some((RailSection::Top, index));
        }
        self.bottom
            .iter()
            .position(|pin| pin == key)
            .map(|index| (RailSection::Bottom, index))
    }

    pub fn section(&self, section: RailSection) -> &[String] {
        match section {
            RailSection::Top => &self.top,
            RailSection::Bottom => &self.bottom,
        }
    }

    fn section_mut(&mut self, section: RailSection) -> &mut Vec<String> {
        match section {
            RailSection::Top => &mut self.top,
            RailSection::Bottom => &mut self.bottom,
        }
    }

    /// Next unused separator key (`sep:1`, `sep:2`, …) across both sections.
    pub fn next_separator_key(&self) -> String {
        next_rail_separator_key(&self.all())
    }

    /// This layout with `key` removed from both sections and, when `pinned`, appended to `section`.
    pub fn with_pin(&self, key: &str, section: RailSection, pinned: bool) -> Self {
        let mut next = self.without(key);
        if pinned && pinnable(key) {
            next.section_mut(section).push(key.to_string());
        }
        next
    }

    /// This layout with `key` removed from whichever section holds it.
    pub fn without(&self, key: &str) -> Self {
        Self {
            top: self
                .top
                .iter()
                .filter(|pin| pin.as_str() != key)
                .cloned()
                .collect(),
            bottom: self
                .bottom
                .iter()
                .filter(|pin| pin.as_str() != key)
                .cloned()
                .collect(),
        }
    }

    /// This layout with `new_key` inserted right after `after_key` in the same section (or appended
    /// to the top section when `after_key` is not pinned).
    pub fn insert_after(&self, after_key: &str, new_key: &str) -> Self {
        let mut next = self.without(new_key);
        if !pinnable(new_key) {
            return next;
        }
        match next.position(after_key) {
            Some((section, index)) => next
                .section_mut(section)
                .insert(index + 1, new_key.to_string()),
            None => next.top.push(new_key.to_string()),
        }
        next
    }

    /// This layout with `new_key` appended to `section`.
    pub fn push_in(&self, section: RailSection, new_key: &str) -> Self {
        self.with_pin(new_key, section, true)
    }

    /// This layout with `key` placed at `index` in `section` (counted in `section` without `key`).
    pub fn moved_in_section(&self, key: &str, section: RailSection, index: usize) -> Self {
        let mut next = self.without(key);
        if !pinnable(key) {
            return next;
        }
        let target = next.section_mut(section);
        let at = index.min(target.len());
        target.insert(at, key.to_string());
        next
    }

    /// This layout with `key` dropped before `target_index` in `target_section` (or at the end of
    /// `target_section` when `target_index >= section.len()`).
    pub fn dropped(&self, key: &str, target_section: RailSection, target_index: usize) -> Self {
        let to = match self.position(key) {
            Some((from_section, from_index))
                if from_section == target_section && from_index < target_index =>
            {
                target_index - 1
            }
            _ => target_index,
        };
        self.moved_in_section(key, target_section, to)
    }

    /// This layout with `key` moved to the end of `target_section`.
    pub fn move_to_section(&self, key: &str, target_section: RailSection) -> Self {
        self.with_pin(key, target_section, true)
    }

    /// Serializes this layout for `sidebarRailPins`. Omits `RAIL_BOTTOM_MARKER` when `bottom` is
    /// empty so top-only layouts keep the exact legacy array format.
    pub fn to_value(&self) -> Value {
        let mut entries: Vec<Value> = self
            .top
            .iter()
            .map(|key| Value::String(key.clone()))
            .collect();
        if !self.bottom.is_empty() {
            entries.push(Value::String(RAIL_BOTTOM_MARKER.to_string()));
            entries.extend(self.bottom.iter().map(|key| Value::String(key.clone())));
        }
        Value::Array(entries)
    }
}

/// The rail's pinned layout (top and bottom groups): known items, saved links, and separators,
/// each at most once across both groups.
pub fn rail_layout(settings: &Map<String, Value>) -> RailLayout {
    let Some(saved) = settings
        .get(RAIL_PINS_SETTINGS_KEY)
        .and_then(Value::as_array)
    else {
        return RailLayout {
            top: RAIL_DEFAULT_PINS
                .iter()
                .map(|key| key.to_string())
                .collect(),
            bottom: Vec::new(),
        };
    };
    let links = crate::custom_links::custom_links_from_settings(settings);
    let mut layout = RailLayout::default();
    let mut in_bottom = false;
    for key in saved.iter().filter_map(Value::as_str) {
        if key == RAIL_BOTTOM_MARKER {
            in_bottom = true;
            continue;
        }
        let known = match rail_link_pin_id(key) {
            Some(id) => links.iter().any(|link| link.id == id),
            None => {
                rail_item(key).is_some()
                    || rail_extension_pin_id(key).is_some()
                    || is_rail_separator_key(key)
            }
        };
        if known && !layout.contains(key) {
            let target = if in_bottom {
                &mut layout.bottom
            } else {
                &mut layout.top
            };
            target.push(key.to_string());
        }
    }
    layout
}

/// The pinned items' keys, top to bottom across both groups: known items, saved links, and
/// separators, each once.
pub fn rail_pins(settings: &Map<String, Value>) -> Vec<String> {
    rail_layout(settings).all()
}

/// `pins` with `key` pinned (at the end) or unpinned.
pub fn rail_pins_with<S: AsRef<str>>(pins: &[S], key: &str, pinned: bool) -> Vec<String> {
    let mut next: Vec<String> = pins
        .iter()
        .map(|pin| pin.as_ref().to_string())
        .filter(|pin| pin != key)
        .collect();
    if pinned && pinnable(key) {
        next.push(key.to_string());
    }
    next
}

/// `pins` with `new_key` inserted right after `after_key` (or at the end when `after_key` is not in
/// `pins`).
pub fn rail_pins_insert_after<S: AsRef<str>>(
    pins: &[S],
    after_key: &str,
    new_key: &str,
) -> Vec<String> {
    let mut next: Vec<String> = pins
        .iter()
        .map(|pin| pin.as_ref().to_string())
        .filter(|pin| pin != new_key)
        .collect();
    if !pinnable(new_key) {
        return next;
    }
    let at = next
        .iter()
        .position(|pin| pin == after_key)
        .map_or(next.len(), |index| index + 1);
    next.insert(at, new_key.to_string());
    next
}

/// `pins` with `key` moved to `index` (counted in the list without it), pinning it there when it
/// was not pinned (a ⋯ row dropped on the rail).
pub fn rail_pins_moved<S: AsRef<str>>(pins: &[S], key: &str, index: usize) -> Vec<String> {
    let mut next: Vec<String> = pins.iter().map(|pin| pin.as_ref().to_string()).collect();
    if !pinnable(key) {
        return next;
    }
    next.retain(|pin| pin != key);
    next.insert(index.min(next.len()), key.to_string());
    next
}

/// `pins` with the pinned `key` dropped on the pin at `target` (an index into `pins`; `pins.len()`
/// drops it after the last one): it takes that pin's place, before it, whichever way it moved.
pub fn rail_pins_dropped<S: AsRef<str>>(pins: &[S], key: &str, target: usize) -> Vec<String> {
    let Some(from) = pins.iter().position(|pin| pin.as_ref() == key) else {
        return rail_pins_moved(pins, key, target);
    };
    // `rail_pins_moved` counts in the list without the dragged pin, where everything after it
    // sits one place higher.
    let to = if from < target { target - 1 } else { target };
    rail_pins_moved(pins, key, to)
}

/// `pins` as the settings value.
pub fn rail_pins_value<S: AsRef<str>>(pins: &[S]) -> Value {
    Value::Array(
        pins.iter()
            .map(|key| Value::String(key.as_ref().to_string()))
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
    fn extension_view_pins_are_kept_in_order() {
        let mut settings = Map::new();
        settings.insert(
            RAIL_PINS_SETTINGS_KEY.to_string(),
            serde_json::json!(["search", "ext:mobile-pane", "ext:", "claude"]),
        );
        assert_eq!(
            rail_pins(&settings),
            ["search", "ext:mobile-pane", "claude"]
        );
        assert_eq!(
            rail_extension_pin_id("ext:mobile-pane"),
            Some("mobile-pane")
        );
        assert_eq!(rail_extension_pin_key("mobile-pane"), "ext:mobile-pane");
        assert_eq!(
            rail_pins_with(PINS, "ext:mobile-pane", true),
            ["search", "notifications", "mainHub", "ext:mobile-pane"]
        );
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
