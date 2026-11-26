//! The rail's ⋯ menu and the commands its rows, pins and the rail's own right-click menu send.
//! The menu is the sidebar's context menu (native_sidebar/menus.rs), so it looks and behaves like
//! every other menu and works in the web build too.

use gpui::{Bounds, Pixels, Point, Window};
use serde_json::{Value, json};

use ghostex_gx_core::custom_links::CUSTOM_LINK_COMMAND_TYPE;
use ghostex_gx_core::sidebar_rail::{
    RAIL_COMMAND_TYPE, RAIL_FOOT_ITEM, RAIL_ITEMS, RAIL_MENU_GROUPS, RailLayout, RailSection,
    is_rail_separator_key, rail_extension_pin_id, rail_extension_pin_key, rail_item, rail_layout,
    rail_link_pin_id,
};

use super::model::RailDropTarget;
use crate::GhostexGpuiApp;

/// The host command type every rail menu row carries.
const SIDEBAR_RAIL_COMMAND_TYPE: &str = RAIL_COMMAND_TYPE;

/// A row's pin (filled while pinned; an unpinned row's shows on hover, "Pin to rail").
fn pin_button(key: &str, pinned: bool) -> Value {
    json!({
        "icon": if pinned { "pin-filled" } else { "pin-outline" },
        "label": "",
        "ariaLabel": if pinned { "Unpin from rail" } else { "Pin to rail" },
        "tooltip": if pinned { "Unpin from rail" } else { "Pin to rail" },
        "hoverOnly": !pinned,
        "command": {
            "type": SIDEBAR_RAIL_COMMAND_TYPE,
            "op": "pin",
            "key": key,
            "pinned": !pinned,
        },
    })
}

/// The menu icon name (titlebar/<name>.svg, native_sidebar/menus.rs) of item `key`, or its agent
/// logo for the logos.
fn menu_icon(key: &str) -> (&'static str, Option<&'static str>) {
    match key {
        "hermes" => ("robot", Some("hermes-agent")),
        "claude" => ("robot", Some("claude")),
        "grok" => ("robot", Some("grok-build")),
        _ => {
            let path = super::rail_item_icon(key);
            let name = path
                .strip_prefix("titlebar/")
                .and_then(|name| name.strip_suffix(".svg"))
                .unwrap_or("dots");
            (name, None)
        }
    }
}

fn command(op: &str, key: &str) -> Value {
    json!({ "type": SIDEBAR_RAIL_COMMAND_TYPE, "op": op, "key": key })
}

fn section_name(section: RailSection) -> &'static str {
    match section {
        RailSection::Top => "top",
        RailSection::Bottom => "bottom",
    }
}

fn section_from_command(command: &Value) -> RailSection {
    match command.get("section").and_then(Value::as_str) {
        Some("bottom") => RailSection::Bottom,
        _ => RailSection::Top,
    }
}

fn add_separator_row(label: &str, section: RailSection) -> Value {
    json!({
        "label": label,
        "icon": "plus",
        "command": {
            "type": SIDEBAR_RAIL_COMMAND_TYPE,
            "op": "addSeparator",
            "section": section_name(section),
        },
        "checked": false,
        "disabled": false,
    })
}

fn push_position_rows(
    rows: &mut Vec<Value>,
    key: &str,
    section: RailSection,
    index: usize,
    len: usize,
) {
    if index > 0 {
        rows.push(json!({
            "label": "Move up",
            "icon": "arrow-up",
            "command": command("moveUp", key),
            "checked": false,
            "disabled": false,
        }));
    }
    if index + 1 < len {
        rows.push(json!({
            "label": "Move down",
            "icon": "arrow-down",
            "command": command("moveDown", key),
            "checked": false,
            "disabled": false,
        }));
    }
    match section {
        RailSection::Top => rows.push(json!({
            "label": "Move to bottom",
            "icon": "chevron-down",
            "command": command("moveToBottom", key),
            "checked": false,
            "disabled": false,
        })),
        RailSection::Bottom => rows.push(json!({
            "label": "Move to top",
            "icon": "chevron-up",
            "command": command("moveToTop", key),
            "checked": false,
            "disabled": false,
        })),
    }
}

impl GhostexGpuiApp {
    /// The ⋯ menu: every item that applies here and is not on the rail, under its group, each with
    /// its pin (shown on hover, "Pin to rail"), plus Links and Layout (separators) on desktop, then
    /// Appearance.
    pub(crate) fn sidebar_rail_more_menu(&self, cx: &gpui::App) -> Value {
        let layout = sidebar_rail_layout_now();
        let mut rows = Vec::new();
        // Appearance ends the menu, after Links and Layout (CDXC:Sidebar 2026-11-25 on gx-core
        // `RAIL_MENU_GROUPS`).
        let (last, first): (Vec<&str>, Vec<&str>) = RAIL_MENU_GROUPS
            .into_iter()
            .partition(|group| *group == "Appearance");
        for group in first {
            self.push_sidebar_rail_menu_group(&mut rows, group, &layout, cx);
        }
        self.push_sidebar_rail_extension_rows(&mut rows, &layout);
        // The user's links and Add link… under their own heading (app/custom_links/), plus
        // Layout separators. A page in a browser cannot edit rail pins or open custom links.
        #[cfg(not(ghostex_web))]
        {
            rows.push(
                json!({ "label": "Links", "heading": true, "checked": false, "disabled": false }),
            );
            rows.extend(
                crate::app::custom_links::custom_link_menu_rows_now()
                    .into_iter()
                    .filter_map(|row| with_link_pin(row, &layout)),
            );
            rows.push(
                json!({ "label": "Layout", "heading": true, "checked": false, "disabled": false }),
            );
            rows.push(add_separator_row("Add separator", RailSection::Top));
            rows.push(add_separator_row(
                "Add separator at bottom",
                RailSection::Bottom,
            ));
            // CDXC:Sidebar 2026-11-26 DECISION:
            // User (#775, "fix both"): separators could only be removed from their own right-click
            // menu, so a rail whose right-click failed kept them for good. Layout also offers
            // Remove separators, which takes every separator off the rail at once; it shows only
            // while the rail has one.
            if layout.all().iter().any(|key| is_rail_separator_key(key)) {
                rows.push(json!({
                    "label": "Remove separators",
                    "icon": "trash",
                    "command": { "type": SIDEBAR_RAIL_COMMAND_TYPE, "op": "removeSeparators" },
                    "checked": false,
                    "disabled": false,
                }));
            }
        }
        for group in last {
            self.push_sidebar_rail_menu_group(&mut rows, group, &layout, cx);
        }
        Value::Array(rows)
    }

    /// One ⋯ menu group: its heading and every item of it that applies here, each with its pin
    /// (none in the web build, which cannot save pins). Nothing when no item applies.
    fn push_sidebar_rail_menu_group(
        &self,
        rows: &mut Vec<Value>,
        group: &str,
        layout: &RailLayout,
        cx: &gpui::App,
    ) {
        // CDXC:Sidebar 2026-11-25 DECISION:
        // User: "if i pin something pls hide it from inside the 3 dots menu for pinned items (hide the header if all are pinned please)". ⋯ lists only what is not on the rail: a pinned item (Settings, Theme and links too) leaves it, and a heading goes when every item under it is pinned. Unpinning, from the icon's right-click menu, puts the row back. An item pinned but not drawn on the rail stays listed, so it can still be reached and unpinned (Main Hub while the Unified sidebar shows its row, `sidebar_rail_main_hub_in_sidebar`). Supersedes the filled pin on pinned rows.
        let main_hub_in_sidebar = self.sidebar_rail_main_hub_in_sidebar();
        let items: Vec<_> = RAIL_ITEMS
            .iter()
            .filter(|item| item.group == group)
            .filter(|item| {
                !layout.contains(item.key) || (item.key == "mainHub" && main_hub_in_sidebar)
            })
            .filter(|item| self.sidebar_rail_item_state(item.key, cx).available)
            .collect();
        if items.is_empty() {
            return;
        }
        rows.push(json!({ "label": group, "heading": true, "checked": false, "disabled": false }));
        for item in items {
            let pinned = layout.contains(item.key);
            let (icon, agent_icon) = menu_icon(item.key);
            let mut row = json!({
                "label": item.label,
                "icon": icon,
                "command": command("run", item.key),
                "checked": false,
                "disabled": false,
                "secondary": pin_button(item.key, pinned),
            });
            // The agents' coloured logos, as the agent launcher draws them.
            if let Some(agent_icon) = agent_icon {
                row["agentIcon"] = json!(agent_icon);
                if let Some(logo) = ghostex_gx_core::colored_agent_logo(agent_icon) {
                    row["imageDataUrl"] = json!(logo);
                    // The logo stands in for the glyph; a row draws both when it has both.
                    if let Some(row) = row.as_object_mut() {
                        row.remove("icon");
                    }
                }
            }
            if cfg!(ghostex_web) {
                if let Some(row) = row.as_object_mut() {
                    row.remove("secondary");
                }
            }
            rows.push(row);
        }
    }

    /// The Extensions group of ⋯: every enabled extension with a side panel view that is not on
    /// the rail, under its own heading, each with its pin (CDXC:Sidebar 2026-11-26 on gx-core
    /// `RAIL_EXTENSION_PIN_PREFIX`). Nothing when there is none.
    fn push_sidebar_rail_extension_rows(&self, rows: &mut Vec<Value>, layout: &RailLayout) {
        let views: Vec<_> = self
            .sidebar_rail_extension_views()
            .into_iter()
            .filter(|view| !layout.contains(&rail_extension_pin_key(&view.id)))
            .collect();
        if views.is_empty() {
            return;
        }
        rows.push(
            json!({ "label": "Extensions", "heading": true, "checked": false, "disabled": false }),
        );
        for view in views {
            let key = rail_extension_pin_key(&view.id);
            let mut row = json!({
                "label": view.title,
                "icon": "puzzle",
                "command": command("run", &key),
                "checked": false,
                "disabled": false,
                "secondary": pin_button(&key, false),
            });
            // The extension's own icon stands in for the glyph, as an agent's logo does.
            if let Some(image) = &view.image {
                use base64::Engine as _;
                let encoded = base64::engine::general_purpose::STANDARD.encode(image.bytes());
                row["imageDataUrl"] = json!(format!("data:image/svg+xml;base64,{encoded}"));
                if let Some(row) = row.as_object_mut() {
                    row.remove("icon");
                }
            }
            rows.push(row);
        }
    }

    /// The right-click menu on empty space in the rail (or on ⋯): lets the user add a separator
    /// to either the top or bottom group, or add a custom link.
    #[cfg_attr(ghostex_web, allow(dead_code))]
    pub(crate) fn sidebar_rail_background_menu(&self, section: RailSection) -> Value {
        let mut rows = match section {
            RailSection::Top => vec![
                add_separator_row("Add separator", RailSection::Top),
                add_separator_row("Add separator at bottom", RailSection::Bottom),
            ],
            RailSection::Bottom => vec![
                add_separator_row("Add separator", RailSection::Bottom),
                add_separator_row("Add separator at top", RailSection::Top),
            ],
        };
        rows.push(json!({ "separator": true }));
        rows.push(json!({
            "label": "Add link…",
            "icon": "plus",
            "command": { "type": CUSTOM_LINK_COMMAND_TYPE, "op": "add" },
            "checked": false,
            "disabled": false,
        }));
        Value::Array(rows)
    }

    /// The right-click menu of a rail entry:
    /// - A separator: Move up / Move down, Move to top / Move to bottom, Add separator, Remove.
    /// - A pinned link: Edit…, Change icon…, Move up / Move down, Move to top / Move to bottom,
    ///   Add separator, Unpin from rail.
    /// - A pinned built-in rail item: Move up / Move down, Move to top / Move to bottom,
    ///   Add separator, Unpin from rail.
    /// - A temporary (unpinned) rail item: Pin to rail, Pin to bottom.
    pub(crate) fn sidebar_rail_item_menu(&self, key: &str) -> Value {
        let layout = sidebar_rail_layout_now();
        let pos = layout.position(key);
        if is_rail_separator_key(key) {
            let mut rows = Vec::new();
            if let Some((section, index)) = pos {
                push_position_rows(
                    &mut rows,
                    key,
                    section,
                    index,
                    layout.section(section).len(),
                );
            }
            rows.push(json!({
                "label": "Add separator",
                "icon": "plus",
                "command": command("addSeparator", key),
                "checked": false,
                "disabled": false,
            }));
            rows.push(json!({ "separator": true }));
            rows.push(json!({
                "label": "Remove",
                "icon": "trash",
                "danger": true,
                "command": {
                    "type": SIDEBAR_RAIL_COMMAND_TYPE,
                    "op": "pin",
                    "key": key,
                    "pinned": false,
                },
                "checked": false,
                "disabled": false,
            }));
            return Value::Array(rows);
        }
        // The Settings gear stays at the rail's very bottom: its menu only unpins it (CDXC:Sidebar
        // 2026-11-25 on gx-core `RAIL_FOOT_ITEM`); ⋯ pins it again.
        if key == RAIL_FOOT_ITEM {
            return json!([{
                "label": "Unpin from rail",
                "icon": "pin-outline",
                "command": {
                    "type": SIDEBAR_RAIL_COMMAND_TYPE,
                    "op": "pin",
                    "key": key,
                    "pinned": false,
                },
                "checked": false,
                "disabled": false,
            }]);
        }
        let mut rows = Vec::new();
        if let Some(id) = rail_link_pin_id(key) {
            rows.push(json!({
                "label": "Edit…",
                "icon": "pencil",
                "command": { "type": CUSTOM_LINK_COMMAND_TYPE, "op": "edit", "id": id },
                "checked": false,
                "disabled": false,
            }));
            rows.push(json!({
                "label": "Change icon…",
                "icon": "sparkles",
                "command": { "type": CUSTOM_LINK_COMMAND_TYPE, "op": "changeIcon", "id": id },
                "checked": false,
                "disabled": false,
            }));
            rows.push(json!({ "separator": true }));
        }
        if let Some((section, index)) = pos {
            push_position_rows(
                &mut rows,
                key,
                section,
                index,
                layout.section(section).len(),
            );
            rows.push(json!({
                "label": "Add separator",
                "icon": "plus",
                "command": command("addSeparator", key),
                "checked": false,
                "disabled": false,
            }));
            rows.push(json!({ "separator": true }));
            rows.push(json!({
                "label": "Unpin from rail",
                "icon": "pin-outline",
                "command": {
                    "type": SIDEBAR_RAIL_COMMAND_TYPE,
                    "op": "pin",
                    "key": key,
                    "pinned": false,
                },
                "checked": false,
                "disabled": false,
            }));
        } else {
            rows.push(json!({
                "label": "Pin to rail",
                "icon": "pin-filled",
                "command": {
                    "type": SIDEBAR_RAIL_COMMAND_TYPE,
                    "op": "pin",
                    "key": key,
                    "pinned": true,
                    "section": "top",
                },
                "checked": false,
                "disabled": false,
            }));
            rows.push(json!({
                "label": "Pin to bottom",
                "icon": "pin-filled",
                "command": {
                    "type": SIDEBAR_RAIL_COMMAND_TYPE,
                    "op": "pin",
                    "key": key,
                    "pinned": true,
                    "section": "bottom",
                },
                "checked": false,
                "disabled": false,
            }));
        }
        Value::Array(rows)
    }

    /// Opens the ⋯ menu beside its button.
    pub(crate) fn open_sidebar_rail_more_menu(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        pointer: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let menu = self.sidebar_rail_more_menu(cx);
        // CDXC:Sidebar 2026-11-21 DECISION:
        // User: "Open the ⋯ menu right next to the rail (a small gap like other popovers), not
        // about 40px away." It opened as a right-click's menu does, centred across the sidebar,
        // so it hangs beside ⋯ now, the sidebar menus' 3pt from it, its first row level with it.
        match bounds {
            Some(button) => Self::show_native_rail_menu(&menu, button, scale, window, cx),
            None => self.show_sidebar_rail_menu(&menu, None, pointer, window, cx),
        }
    }

    /// A rail menu row's command. Returns whether it was one.
    pub(crate) fn run_sidebar_rail_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some(SIDEBAR_RAIL_COMMAND_TYPE) {
            return false;
        }
        let op = command.get("op").and_then(Value::as_str);
        // Adding a separator works either after a specific pinned `key` (right-click on an item)
        // or at the end of a `section` (from the ⋯ menu or right-clicking empty rail space).
        if op == Some("addSeparator") {
            let layout = sidebar_rail_layout_now();
            let sep_key = layout.next_separator_key();
            let next = match command.get("key").and_then(Value::as_str) {
                Some(after_key) if layout.contains(after_key) => {
                    layout.insert_after(after_key, &sep_key)
                }
                _ => layout.push_in(section_from_command(command), &sep_key),
            };
            self.save_sidebar_rail_layout(&next, cx);
            self.dismiss_native_sidebar_menu(cx);
            return true;
        }
        if op == Some("removeSeparators") {
            let layout = sidebar_rail_layout_now();
            let next = layout
                .all()
                .iter()
                .filter(|key| is_rail_separator_key(key))
                .fold(layout.clone(), |next, key| next.without(key));
            if next != layout {
                self.save_sidebar_rail_layout(&next, cx);
            }
            self.dismiss_native_sidebar_menu(cx);
            return true;
        }
        // A rail item, a pinned link (`link:<id>`), an extension's view (`ext:<id>`), or a
        // separator (`sep:<id>`).
        let Some(key) = command
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| {
                rail_item(key).is_some()
                    || rail_link_pin_id(key).is_some()
                    || rail_extension_pin_id(key).is_some()
                    || is_rail_separator_key(key)
            })
            .map(str::to_string)
        else {
            return true;
        };
        match op {
            Some("run") => {
                self.dismiss_native_sidebar_menu(cx);
                self.defer_in_main_window(cx, move |app, window, cx| {
                    if let Some(item) = rail_item(&key) {
                        app.run_sidebar_rail_item(item.key, window, cx);
                    } else if let Some(id) = rail_link_pin_id(&key) {
                        app.run_sidebar_rail_link(id, window, cx);
                    } else if let Some(id) = rail_extension_pin_id(&key) {
                        app.run_sidebar_rail_extension(id, window, cx);
                    }
                });
            }
            Some("pin") => {
                let pinned = command.get("pinned").and_then(Value::as_bool) == Some(true);
                let section = section_from_command(command);
                let next = sidebar_rail_layout_now().with_pin(&key, section, pinned);
                self.save_sidebar_rail_layout(&next, cx);
                // From the ⋯ menu's own hover pin button, refresh its pins in place. From a
                // right-click context menu or a link row's own submenu, close the menu.
                let is_more_menu = self.native_sidebar.menu.as_ref().is_some_and(|menu| {
                    menu.panels.len() == 1
                        && menu
                            .panels
                            .first()
                            .is_some_and(|panel| panel.items.iter().any(is_rail_more_menu_row))
                });
                if !is_more_menu {
                    self.dismiss_native_sidebar_menu(cx);
                    return true;
                }
                let rows = self.sidebar_rail_more_menu(cx);
                if let Some(panel) = self
                    .native_sidebar
                    .menu
                    .as_mut()
                    .and_then(|menu| menu.panels.first_mut())
                    && let Value::Array(rows) = rows
                {
                    panel.replace_items(rows);
                }
            }
            Some(op @ ("moveUp" | "moveDown")) => {
                let layout = sidebar_rail_layout_now();
                if let Some((section, index)) = layout.position(&key) {
                    let to = if op == "moveUp" {
                        index.saturating_sub(1)
                    } else {
                        index + 1
                    };
                    self.save_sidebar_rail_layout(&layout.moved_in_section(&key, section, to), cx);
                }
                self.dismiss_native_sidebar_menu(cx);
            }
            Some("moveToTop") => {
                let layout = sidebar_rail_layout_now();
                self.save_sidebar_rail_layout(&layout.move_to_section(&key, RailSection::Top), cx);
                self.dismiss_native_sidebar_menu(cx);
            }
            Some("moveToBottom") => {
                let layout = sidebar_rail_layout_now();
                self.save_sidebar_rail_layout(
                    &layout.move_to_section(&key, RailSection::Bottom),
                    cx,
                );
                self.dismiss_native_sidebar_menu(cx);
            }
            _ => {}
        }
        true
    }

    /// The dragged pin `key` dropped at `target` (top or bottom section, before `target.index`).
    #[cfg_attr(ghostex_web, allow(dead_code))]
    pub(crate) fn move_sidebar_rail_pin(
        &mut self,
        key: &str,
        target: RailDropTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let layout = sidebar_rail_layout_now();
        let moved = layout.dropped(key, target.section, target.index);
        if moved != layout {
            self.save_sidebar_rail_layout(&moved, cx);
        }
    }

    /// The web build reads this computer's settings but keeps no settings file of its own, so the
    /// page shows the pins without changing them (its ⋯ rows carry no pin).
    #[cfg(ghostex_web)]
    fn save_sidebar_rail_layout(&mut self, _layout: &RailLayout, _cx: &mut gpui::Context<Self>) {}

    #[cfg(not(ghostex_web))]
    pub(crate) fn save_sidebar_rail_layout(
        &mut self,
        layout: &RailLayout,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(
            ghostex_gx_core::sidebar_rail::RAIL_PINS_SETTINGS_KEY.to_string(),
            layout.to_value(),
        );
        if let Err(error) = crate::shared_settings::write_shared_sidebar_settings_object(settings) {
            log::error!("Rail: could not save the pinned icons: {error:?}");
        }
        cx.notify();
    }
}

/// A row of the rail's ⋯ menu (carries a secondary rail pin toggle).
fn is_rail_more_menu_row(row: &Value) -> bool {
    row["secondary"]["command"]["type"] == SIDEBAR_RAIL_COMMAND_TYPE
}

pub(crate) fn sidebar_rail_layout_now() -> RailLayout {
    rail_layout(crate::shared_settings::shared_sidebar_settings_snapshot().object())
}

/// A link row of the rail's ⋯ menu with its pin, as every rail item has, and Pin to rail / Pin to
/// bottom in its own menu (`rowMenu.pinned`, packages/gx-core/src/custom_links.rs). None for a
/// pinned link: it is on the rail, and leaves ⋯ like any pinned item. Add link… stays.
#[cfg(not(ghostex_web))]
fn with_link_pin(mut row: Value, layout: &RailLayout) -> Option<Value> {
    let Some(id) = row["rowMenu"]["key"].as_str().map(str::to_string) else {
        return Some(row);
    };
    let key = ghostex_gx_core::sidebar_rail::rail_link_pin_key(&id);
    if layout.contains(&key) {
        return None;
    }
    row["rowMenu"]["pinned"] = json!(false);
    row["secondary"] = pin_button(&key, false);
    Some(row)
}
