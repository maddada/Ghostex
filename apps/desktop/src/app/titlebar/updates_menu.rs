// The top bar's Updates list: what the update badge counts (app/updates/), with Update all and
// Open Updates… one click away. A row runs its own update and the list stays open on its spinner
// (rows drawn by updates_menu_rows.rs).

use gpui::{Window, px, size};
use gpui_component::menu::PopupMenu;
use serde_json::{Value, json};

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use super::updates_menu_rows::{
    UPDATES_MENU_HEADER_HEIGHT, updates_menu_header, updates_menu_item_row,
    updates_menu_update_all_row,
};
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// The list shows this many items; Open Updates… shows the rest.
const MAX_ROWS: usize = 8;

/// The items the open list was last built from: a refresh with nothing new leaves it alone, so a
/// tooltip being read doesn't vanish each time the watch reads the list.
static SHOWN_ITEMS: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn titlebar_updates_popup_content_height(rows: usize) -> f32 {
    let mut heights = vec![UPDATES_MENU_HEADER_HEIGHT];
    heights.extend(std::iter::repeat_n(
        TITLEBAR_POPUP_MENU_ROW_HEIGHT,
        rows.min(MAX_ROWS),
    ));
    heights.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
    heights.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
    heights.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
    titlebar_popup_menu_height_for_rows(&heights)
}

pub(crate) fn build_gpui_titlebar_updates_popup_menu(
    menu: PopupMenu,
    items: &[Value],
    width: f32,
    max_height: f32,
    scrollable: bool,
) -> PopupMenu {
    let mut menu = titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable);
    *SHOWN_ITEMS.lock().unwrap_or_else(|e| e.into_inner()) = items.to_vec();
    let count = items.len();
    let failed = items
        .iter()
        .filter(|item| item["state"] == "failed")
        .count();
    let other_computers = items.iter().any(|item| {
        item["computer"]
            .as_str()
            .is_some_and(|computer| computer != crate::app::updates::THIS_COMPUTER)
    });
    // Disabled header row: the placeholder action the Git, Tips and Help menus use for labels.
    menu =
        menu.menu_element_with_disabled(Box::new(CopyGpuiTitlebarGitBranch), true, move |_, _| {
            updates_menu_header(count, failed, other_computers, width)
        });
    for item in items.iter().take(MAX_ROWS) {
        let item = item.clone();
        let id = item["id"].as_str().unwrap_or_default().to_string();
        menu = menu.menu_element(Box::new(RunGpuiTitlebarUpdateItem { id }), move |_, _| {
            updates_menu_item_row(&item, width)
        });
    }
    menu = menu.separator();
    menu = menu.menu_element(
        Box::new(RunGpuiTitlebarUpdates { update_all: true }),
        |_, _| updates_menu_update_all_row(),
    );
    menu.menu_element(
        Box::new(RunGpuiTitlebarUpdates { update_all: false }),
        |_, _| {
            titlebar_popup_standard_menu_row(
                "titlebar/settings.svg",
                TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                "Open Updates…".to_string(),
                false,
            )
        },
    )
}

impl GhostexGpuiApp {
    /// The update button's click: the Updates list while anything counts, else (only the app's
    /// own update is known, before gxserver's list arrived) the Ghostex update dialog.
    pub(crate) fn toggle_gpui_titlebar_updates_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.updates_badge_items().is_empty() {
            self.check_for_gpui_updates_from_any_window(window, cx);
            return;
        }
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Updates);
        let trigger_bounds = self.titlebar_update_button_bounds.get();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Updates,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    /// A row of the Updates list: Update all runs every update (Settings > Updates' button);
    /// every other row opens that page.
    pub(crate) fn run_gpui_titlebar_updates(
        &mut self,
        update_all: bool,
        window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        if update_all {
            let mut command = serde_json::Map::new();
            command.insert("op".into(), json!("install"));
            command.insert("all".into(), json!(true));
            self.handle_updates_command(&command, cx);
            return;
        }
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = json!({
            "initialTab": "updates",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, window, cx);
    }

    /// An item row of the Updates list: runs (or retries) its update while the list stays open,
    /// or opens Settings > Updates when only the page can finish it (Needs you, a manual update).
    pub(crate) fn run_gpui_titlebar_update_item(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let item = self
            .updates_badge_items()
            .into_iter()
            .find(|item| item["id"] == id);
        let Some(item) = item else {
            return;
        };
        if item["state"] == "updating" {
            return;
        }
        if matches!(item["state"].as_str(), Some("available" | "failed"))
            && item["canUpdate"] == true
        {
            self.install_updates(vec![id.to_string()], cx);
            return;
        }
        // Deferred: this runs inside the list window's own action, which closing it would re-enter.
        self.defer_in_main_window(cx, |app, window, cx| {
            app.close_gpui_titlebar_popup(Some(GpuiTitlebarPopupKind::Updates), window, cx);
            app.run_gpui_titlebar_updates(false, Some(window), cx);
        });
    }

    /// Redraws an open Updates list from the latest list (its spinners and failures change while
    /// it is open), resized to its rows; closes it once nothing is left to show.
    pub(crate) fn refresh_open_updates_popup(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Updates) {
            return;
        }
        let items = self.updates_badge_items();
        if items.is_empty() {
            self.close_gpui_titlebar_popup(Some(GpuiTitlebarPopupKind::Updates), window, cx);
            return;
        }
        if *SHOWN_ITEMS.lock().unwrap_or_else(|e| e.into_inner()) == items {
            return;
        }
        let Some(popup) = self.titlebar_popup_window else {
            return;
        };
        let content_height = titlebar_updates_popup_content_height(items.len());
        let bottom_limit = window.bounds().bottom() - px(8.0);
        let _ = popup.update(cx, |popup, window, cx| {
            let bounds = window.bounds();
            let height = px(content_height.min(TITLEBAR_POPUP_MENU_MAX_HEIGHT))
                .min((bottom_limit - bounds.top()).max(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT)));
            let width = (bounds.size.width.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
            let max_height = (height.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
            let scrollable = content_height > height.as_f32();
            let menu = PopupMenu::build(window, cx, move |menu, _, _| {
                build_gpui_titlebar_updates_popup_menu(menu, &items, width, max_height, scrollable)
            });
            window.resize(size(bounds.size.width, height));
            popup.replace_menu(menu, window, cx);
        });
    }
}
