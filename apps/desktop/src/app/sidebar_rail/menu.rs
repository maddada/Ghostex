//! The rail's ⋯ menu and the commands its rows, pins and the rail's own right-click menu send.
//! The menu is the sidebar's context menu (native_sidebar/menus.rs), so it looks and behaves like
//! every other menu and works in the web build too.

use gpui::{Bounds, Pixels, Point, Window};
use serde_json::{Value, json};

use ghostex_gx_core::sidebar_rail::{
    RAIL_COMMAND_TYPE, RAIL_ITEMS, RAIL_MENU_GROUPS, rail_item, rail_link_pin_id, rail_pins,
    rail_pins_moved, rail_pins_with,
};

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

impl GhostexGpuiApp {
    /// The ⋯ menu: every item that applies here, under its group, each with its pin (filled while
    /// pinned; an unpinned row's shows on hover, "Pin to rail").
    pub(crate) fn sidebar_rail_more_menu(&self, cx: &gpui::App) -> Value {
        let pins = rail_pins(crate::shared_settings::shared_sidebar_settings_snapshot().object());
        let mut rows = Vec::new();
        for group in RAIL_MENU_GROUPS {
            let items: Vec<_> = RAIL_ITEMS
                .iter()
                .filter(|item| item.group == group)
                .filter(|item| self.sidebar_rail_item_state(item.key, cx).available)
                .collect();
            if items.is_empty() {
                continue;
            }
            rows.push(
                json!({ "label": group, "heading": true, "checked": false, "disabled": false }),
            );
            for item in items {
                let pinned = pins.iter().any(|pin| pin == item.key);
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
        // The user's links and Add link… under their own heading (app/custom_links/). A page in a
        // browser has no link page to open them in.
        #[cfg(not(ghostex_web))]
        {
            rows.push(
                json!({ "label": "Links", "heading": true, "checked": false, "disabled": false }),
            );
            rows.extend(
                crate::app::custom_links::custom_link_menu_rows_now()
                    .into_iter()
                    .map(|row| with_link_pin(row, &pins)),
            );
        }
        Value::Array(rows)
    }

    /// The right-click menu of a pinned icon: move it, or unpin it.
    pub(crate) fn sidebar_rail_item_menu(&self, key: &str) -> Value {
        let pins = sidebar_rail_pins_now();
        let index = pins.iter().position(|pin| pin == key);
        let mut rows = Vec::new();
        if index.is_some_and(|index| index > 0) {
            rows.push(json!({ "label": "Move up", "icon": "arrow-up", "command": command("moveUp", key), "checked": false, "disabled": false }));
        }
        if index.is_some_and(|index| index + 1 < pins.len()) {
            rows.push(json!({ "label": "Move down", "icon": "arrow-down", "command": command("moveDown", key), "checked": false, "disabled": false }));
        }
        rows.push(json!({
            "label": if index.is_some() { "Unpin from rail" } else { "Pin to rail" },
            "icon": if index.is_some() { "pin-outline" } else { "pin-filled" },
            "command": { "type": SIDEBAR_RAIL_COMMAND_TYPE, "op": "pin", "key": key, "pinned": index.is_none() },
            "checked": false,
            "disabled": false,
        }));
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
            Some(button) => self.show_native_sidebar_menu_beside(&menu, button, scale, window, cx),
            None => Self::show_native_sidebar_menu(&menu, pointer, scale, window, cx),
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
        // A rail item, or a pinned link (`link:<id>`, packages/gx-core/src/sidebar_rail.rs).
        let Some(key) = command
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| rail_item(key).is_some() || rail_link_pin_id(key).is_some())
            .map(str::to_string)
        else {
            return true;
        };
        match command.get("op").and_then(Value::as_str) {
            Some("run") => {
                self.dismiss_native_sidebar_menu(cx);
                self.defer_in_main_window(cx, move |app, window, cx| {
                    if let Some(item) = rail_item(&key) {
                        app.run_sidebar_rail_item(item.key, window, cx);
                    } else if let Some(id) = rail_link_pin_id(&key) {
                        app.run_sidebar_rail_link(id, window, cx);
                    }
                });
            }
            Some("pin") => {
                let pinned = command.get("pinned").and_then(Value::as_bool) == Some(true);
                let pins = rail_pins_with(&sidebar_rail_pins_now(), &key, pinned);
                self.save_sidebar_rail_pins(&pins, cx);
                // From a link row's own menu (beside the ⋯ menu) the menus close; from the ⋯
                // menu's pin it redraws its pins in place.
                if self
                    .native_sidebar
                    .menu
                    .as_ref()
                    .is_some_and(|menu| menu.panels.len() > 1)
                {
                    self.dismiss_native_sidebar_menu(cx);
                    return true;
                }
                let rows = self.sidebar_rail_more_menu(cx);
                if let Some(panel) = self
                    .native_sidebar
                    .menu
                    .as_mut()
                    .and_then(|menu| menu.panels.first_mut())
                    .filter(|panel| panel.items.iter().any(is_rail_row))
                    && let Value::Array(rows) = rows
                {
                    panel.replace_items(rows);
                }
            }
            Some(op @ ("moveUp" | "moveDown")) => {
                let pins = sidebar_rail_pins_now();
                if let Some(index) = pins.iter().position(|pin| *pin == key) {
                    let to = if op == "moveUp" {
                        index.saturating_sub(1)
                    } else {
                        index + 1
                    };
                    self.save_sidebar_rail_pins(&rail_pins_moved(&pins, &key, to), cx);
                }
                self.dismiss_native_sidebar_menu(cx);
            }
            _ => {}
        }
        true
    }

    /// The dragged pin `key` dropped on the pin at `index` (`pins.len()`: after the last one).
    pub(crate) fn move_sidebar_rail_pin(
        &mut self,
        key: &str,
        index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let pins = sidebar_rail_pins_now();
        let moved = ghostex_gx_core::sidebar_rail::rail_pins_dropped(&pins, key, index);
        if moved != pins {
            self.save_sidebar_rail_pins(&moved, cx);
        }
    }

    /// The web build reads this computer's settings but keeps no settings file of its own, so the
    /// page shows the pins without changing them (its ⋯ rows carry no pin).
    #[cfg(ghostex_web)]
    fn save_sidebar_rail_pins(&mut self, _pins: &[String], _cx: &mut gpui::Context<Self>) {}

    #[cfg(not(ghostex_web))]
    pub(crate) fn save_sidebar_rail_pins(&mut self, pins: &[String], cx: &mut gpui::Context<Self>) {
        let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(
            ghostex_gx_core::sidebar_rail::RAIL_PINS_SETTINGS_KEY.to_string(),
            ghostex_gx_core::sidebar_rail::rail_pins_value(pins),
        );
        if let Err(error) = crate::shared_settings::write_shared_sidebar_settings_object(settings) {
            log::error!("Rail: could not save the pinned icons: {error:?}");
        }
        cx.notify();
    }
}

/// A row of the rail's ⋯ menu.
fn is_rail_row(row: &Value) -> bool {
    row["command"]["type"] == SIDEBAR_RAIL_COMMAND_TYPE
}

pub(crate) fn sidebar_rail_pins_now() -> Vec<String> {
    rail_pins(crate::shared_settings::shared_sidebar_settings_snapshot().object())
}

/// A link row of the rail's ⋯ menu with its pin, as every rail item has, and Pin to rail or Unpin
/// from rail in its own menu (`rowMenu.pinned`, packages/gx-core/src/custom_links.rs).
#[cfg(not(ghostex_web))]
fn with_link_pin(mut row: Value, pins: &[String]) -> Value {
    let Some(id) = row["rowMenu"]["key"].as_str().map(str::to_string) else {
        return row;
    };
    let key = ghostex_gx_core::sidebar_rail::rail_link_pin_key(&id);
    let pinned = pins.contains(&key);
    row["rowMenu"]["pinned"] = json!(pinned);
    row["secondary"] = pin_button(&key, pinned);
    row
}
