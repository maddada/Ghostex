// The top bar's Updates list: what the update badge counts (app/updates/), with Update all and
// Open Updates… one click away.

use gpui::{FontWeight, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use gpui_component::{h_flex, menu::PopupMenu, v_flex};
use serde_json::{Value, json};

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

const HEADER_HEIGHT: f32 = 52.0;
/// The list shows this many items; Open Updates… shows the rest.
const MAX_ROWS: usize = 8;

pub(crate) fn titlebar_updates_popup_content_height(rows: usize) -> f32 {
    let mut heights = vec![HEADER_HEIGHT];
    heights.extend(std::iter::repeat_n(
        TITLEBAR_POPUP_MENU_ROW_HEIGHT,
        rows.min(MAX_ROWS),
    ));
    heights.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
    heights.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
    heights.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
    titlebar_popup_menu_height_for_rows(&heights)
}

/// What an item's row says on its right: Failed, Needs you, Updating, or the new version.
fn item_trailing(item: &Value) -> String {
    match item["state"].as_str() {
        Some("failed") => "Failed".to_string(),
        Some("needsYou") => "Needs you".to_string(),
        Some("updating") => "Updating".to_string(),
        _ => item["latest"]
            .as_str()
            .or(item["summary"].as_str())
            .unwrap_or("Update")
            .to_string(),
    }
}

fn header(count: usize) -> impl IntoElement {
    v_flex()
        .w_full()
        .min_h(px(HEADER_HEIGHT))
        .justify_center()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(titlebar_active_text_color())
                .child(if count == 1 {
                    "1 update".to_string()
                } else {
                    format!("{count} updates")
                }),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(titlebar_inactive_text_color())
                .child("Ghostex, the agents you use and what failed."),
        )
}

fn item_row(item: Value) -> impl IntoElement {
    let failed = item["state"] == "failed";
    h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(13.0))
                .text_color(titlebar_popup_menu_foreground())
                .child(item["name"].as_str().unwrap_or_default().to_string()),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .when(failed, |this| {
                    this.child(
                        div()
                            .size(px(6.0))
                            .rounded_full()
                            .bg(titlebar_popup_git_deletions_color()),
                    )
                })
                .text_size(px(12.0))
                .text_color(titlebar_inactive_text_color())
                .child(item_trailing(&item)),
        )
}

pub(crate) fn build_gpui_titlebar_updates_popup_menu(
    menu: PopupMenu,
    items: &[Value],
    width: f32,
    max_height: f32,
    scrollable: bool,
) -> PopupMenu {
    let mut menu = titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable);
    let count = items.len();
    // Disabled header row: the placeholder action the Git, Tips and Help menus use for labels.
    menu =
        menu.menu_element_with_disabled(Box::new(CopyGpuiTitlebarGitBranch), true, move |_, _| {
            header(count)
        });
    for item in items.iter().take(MAX_ROWS) {
        let item = item.clone();
        menu = menu.menu_element(
            Box::new(RunGpuiTitlebarUpdates { update_all: false }),
            move |_, _| item_row(item.clone()),
        );
    }
    menu = menu.separator();
    menu = menu.menu_element(
        Box::new(RunGpuiTitlebarUpdates { update_all: true }),
        |_, _| {
            titlebar_popup_standard_menu_row(
                TITLEBAR_ICON_DOWNLOAD,
                TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                "Update all".to_string(),
                false,
            )
        },
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
}
