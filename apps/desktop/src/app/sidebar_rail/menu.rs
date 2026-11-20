//! The rail's ⋯ menu and the commands its rows, pins and the rail's own right-click menu send.
//! The menu is the sidebar's context menu (native_sidebar/menus.rs), so it looks and behaves like
//! every other menu and works in the web build too.

use gpui::{Bounds, Pixels, Point, Window, point, px};
use serde_json::{Value, json};

use ghostex_gx_core::sidebar_rail::{
    RAIL_ITEMS, RAIL_MENU_GROUPS, rail_item, rail_pins, rail_pins_moved, rail_pins_with,
};

use crate::GhostexGpuiApp;

/// The host command type every rail menu row carries.
const SIDEBAR_RAIL_COMMAND_TYPE: &str = "sidebarRail";

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
                let pinned = pins.contains(&item.key);
                let (icon, agent_icon) = menu_icon(item.key);
                let mut row = json!({
                    "label": item.label,
                    "icon": icon,
                    "command": command("run", item.key),
                    "checked": false,
                    "disabled": false,
                    "secondary": {
                        "icon": if pinned { "pin-filled" } else { "pin-outline" },
                        "label": "",
                        "ariaLabel": if pinned { "Unpin from rail" } else { "Pin to rail" },
                        "tooltip": if pinned { "Unpin from rail" } else { "Pin to rail" },
                        "hoverOnly": !pinned,
                        "command": {
                            "type": SIDEBAR_RAIL_COMMAND_TYPE,
                            "op": "pin",
                            "key": item.key,
                            "pinned": !pinned,
                        },
                    },
                });
                if let Some(agent_icon) = agent_icon {
                    row["agentIcon"] = json!(agent_icon);
                }
                if cfg!(ghostex_web) {
                    if let Some(row) = row.as_object_mut() {
                        row.remove("secondary");
                    }
                }
                rows.push(row);
            }
        }
        Value::Array(rows)
    }

    /// The right-click menu of a pinned icon: move it, or unpin it.
    pub(crate) fn sidebar_rail_item_menu(&self, key: &str) -> Value {
        let pins = sidebar_rail_pins_now();
        let index = pins.iter().position(|pin| *pin == key);
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
        let at = bounds.map_or(pointer, |bounds| {
            point(bounds.right() + px(4.0 * scale), bounds.top())
        });
        Self::show_native_sidebar_menu(&menu, at, scale, window, cx);
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
        let Some(key) = command
            .get("key")
            .and_then(Value::as_str)
            .and_then(rail_item)
            .map(|item| item.key)
        else {
            return true;
        };
        match command.get("op").and_then(Value::as_str) {
            Some("run") => {
                self.dismiss_native_sidebar_menu(cx);
                self.defer_in_main_window(cx, move |app, window, cx| {
                    app.run_sidebar_rail_item(key, window, cx);
                });
            }
            Some("pin") => {
                let pinned = command.get("pinned").and_then(Value::as_bool) == Some(true);
                let pins = rail_pins_with(&sidebar_rail_pins_now(), key, pinned);
                self.save_sidebar_rail_pins(&pins, cx);
                // The open ⋯ menu redraws its pins in place.
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
                    self.save_sidebar_rail_pins(&rail_pins_moved(&pins, key, to), cx);
                }
                self.dismiss_native_sidebar_menu(cx);
            }
            _ => {}
        }
        true
    }

    /// Moves the dragged pin `key` to `index` (an unpinned item dropped there is pinned there).
    pub(crate) fn move_sidebar_rail_pin(
        &mut self,
        key: &'static str,
        index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let pins = sidebar_rail_pins_now();
        let moved = rail_pins_moved(&pins, key, index);
        if moved != pins {
            self.save_sidebar_rail_pins(&moved, cx);
        }
    }

    /// The web build reads this computer's settings but keeps no settings file of its own, so the
    /// page shows the pins without changing them (its ⋯ rows carry no pin).
    #[cfg(ghostex_web)]
    fn save_sidebar_rail_pins(&mut self, _pins: &[&'static str], _cx: &mut gpui::Context<Self>) {}

    #[cfg(not(ghostex_web))]
    fn save_sidebar_rail_pins(&mut self, pins: &[&'static str], cx: &mut gpui::Context<Self>) {
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

fn sidebar_rail_pins_now() -> Vec<&'static str> {
    rail_pins(crate::shared_settings::shared_sidebar_settings_snapshot().object())
}
