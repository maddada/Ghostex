//! A Browser page's right-click menu, drawn by Ghostex instead of CEF.

use gpui::Window;

use crate::app::context_menu::GpuiContextMenu;
use crate::*;

/// The shared context menu holding the rows CEF offered for a right-click on a page
/// (`cef/shell/browser_handlers.rs`, CDXC:ContextMenus 2026-11-16); a picked row runs its command
/// on that page.
pub(crate) fn browser_page_gpui_context_menu(menu: cef::BrowserPageContextMenu) -> GpuiContextMenu {
    let mut context_menu = GpuiContextMenu::new();
    for entry in menu.entries {
        context_menu = match entry {
            cef::BrowserPageContextMenuEntry::Separator => context_menu.separator(),
            cef::BrowserPageContextMenuEntry::Item {
                command_id,
                label,
                enabled,
            } => {
                let run = menu.run.clone();
                context_menu.menu_with_callback(label, !enabled, move |_, _| run(command_id))
            }
        };
    }
    context_menu
}

impl GhostexGpuiApp {
    /// Opens a Browser pane page's right-click menu where the page was clicked.
    pub(crate) fn show_browser_page_context_menu(
        &mut self,
        tab_id: BrowserTabId,
        menu: cef::BrowserPageContextMenu,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(origin) = self
            .browser_surfaces
            .get(&tab_id)
            .and_then(|surface| surface.read(cx).view_origin())
        else {
            return;
        };
        let position = point(origin.x + px(menu.x), origin.y + px(menu.y));
        browser_page_gpui_context_menu(menu).show_for_app(cx.entity(), position, window, cx);
    }
}
