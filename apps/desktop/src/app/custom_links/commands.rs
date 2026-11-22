//! What a link row, Add link… and a row's Edit… and Remove do, and saving the links.

use ghostex_gx_core::custom_links::{
    CUSTOM_LINK_COMMAND_TYPE, CUSTOM_LINKS_MAX, CUSTOM_LINKS_SETTING, CustomLink,
    custom_links_value,
};
use serde_json::{Map, Value, json};

use super::store::*;
use crate::app::claude_web::WebPlace;
use crate::*;

/// A new link id, unique enough for one person's links.
fn new_link_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

impl GhostexGpuiApp {
    /// A `customLink` command from the rail's ⋯ menu. Returns whether it was one.
    pub(crate) fn run_custom_link_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command["type"] != CUSTOM_LINK_COMMAND_TYPE {
            return false;
        }
        let id = command["id"].as_str().map(str::to_string);
        self.dismiss_native_sidebar_menu(cx);
        match (command["op"].as_str(), id) {
            (Some("open"), Some(id)) => {
                self.defer_in_main_window(cx, move |app, _window, cx| {
                    app.open_custom_link(&id, cx);
                });
            }
            (Some("add"), _) => self.open_gpui_custom_link_modal(
                None,
                crate::app::window::CustomLinkModalMode::Full,
                cx,
            ),
            (Some("edit"), Some(id)) => self.open_gpui_custom_link_modal(
                Some(&id),
                crate::app::window::CustomLinkModalMode::Full,
                cx,
            ),
            (Some("changeIcon"), Some(id)) => self.open_gpui_custom_link_modal(
                Some(&id),
                crate::app::window::CustomLinkModalMode::IconOnly,
                cx,
            ),
            (Some("remove"), Some(id)) => self.remove_custom_link(&id, cx),
            _ => {}
        }
        true
    }

    /// Opens `id`'s page in the sessions column's place, the way Grok Bot's opens, or with the page
    /// already on, loads the link there.
    pub(crate) fn open_custom_link(&mut self, id: &str, cx: &mut gpui::Context<Self>) {
        let Some(link) = saved_custom_link(id) else {
            return;
        };
        set_custom_link_page_link(Some(&link.id));
        self.load_custom_link_page(&link.url, cx);
        if !self.show_web_place(WebPlace::Link, cx) {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Link could not open",
                "Open a project in this window first.",
                cx,
            );
        }
    }

    fn save_custom_links(&mut self, links: &[CustomLink], cx: &mut gpui::Context<Self>) {
        let mut patch = Map::new();
        patch.insert(CUSTOM_LINKS_SETTING.to_string(), custom_links_value(links));
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({ "patch": patch, "source": "customLinks" }),
            cx,
        );
    }

    /// The dialog's Add or Save: a new link goes last, an edited one keeps its place. An edited
    /// link whose page is on screen reloads on its new address.
    pub(crate) fn save_custom_link(
        &mut self,
        id: Option<String>,
        name: String,
        url: String,
        icon: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let icon = icon.map(|i| i.trim().to_string()).filter(|i| !i.is_empty());
        let mut links = saved_custom_links();
        match id.and_then(|id| links.iter_mut().find(|link| link.id == id)) {
            Some(link) => {
                let moved = link.url != url;
                let id = link.id.clone();
                link.name = name;
                link.url = url.clone();
                link.icon = icon;
                self.save_custom_links(&links, cx);
                let showing = custom_link_page_link().is_some_and(|link| link.id == id);
                if moved && showing && self.web_place_on() == Some(WebPlace::Link) {
                    self.load_custom_link_page(&url, cx);
                }
            }
            None => {
                if links.len() >= CUSTOM_LINKS_MAX {
                    links.remove(0);
                }
                links.push(CustomLink {
                    id: new_link_id(),
                    name,
                    url,
                    icon,
                });
                self.save_custom_links(&links, cx);
            }
        }
        cx.notify();
    }

    /// A row's Remove: the link leaves the menu, and its page closes when it is on screen.
    pub(crate) fn remove_custom_link(&mut self, id: &str, cx: &mut gpui::Context<Self>) {
        let mut links = saved_custom_links();
        let listed = links.len();
        links.retain(|link| link.id != id);
        if links.len() == listed {
            return;
        }
        let showing = custom_link_page_link().is_some_and(|link| link.id == id);
        self.save_custom_links(&links, cx);
        // Its rail pin goes with it (packages/gx-core/src/sidebar_rail.rs).
        let pin = ghostex_gx_core::sidebar_rail::rail_link_pin_key(id);
        let pins = crate::app::sidebar_rail::sidebar_rail_pins();
        if pins.contains(&pin) {
            let pins = ghostex_gx_core::sidebar_rail::rail_pins_with(&pins, &pin, false);
            self.save_sidebar_rail_pins(&pins, cx);
        }
        if showing {
            set_custom_link_page_link(None);
            if self.web_place_on() == Some(WebPlace::Link) {
                self.turn_claude_off(cx);
            }
            self.forget_custom_link_page(cx);
        }
        cx.notify();
    }
}
