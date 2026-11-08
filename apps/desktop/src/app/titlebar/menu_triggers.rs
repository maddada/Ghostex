// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the titlebar Git/settings/mode/customize/open-targets/actions menu trigger methods.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::context_menu::GpuiContextMenu;
use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// The titlebar Git control opens an in-app PopupMenu drawn from gx-core's Git menu
    /// (gx_store/git/hud.rs). Selections keep dispatching fixed action selectors only.
    pub(crate) fn show_gpui_titlebar_git_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Git);
        if open {
            // Ask for a background state refresh so the menu converges on fresh rows; the open
            // menu renders the last read state honestly instead of fabricating fresh values.
            self.dispatch_gpui_titlebar_git_action_selector(
                GPUI_TITLEBAR_GIT_ACTION_REFRESH_SELECTOR,
                cx,
            );
        }
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Git,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn run_gpui_titlebar_git_menu_row(
        &mut self,
        row_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(row) = self
            .titlebar_git_menu_state
            .as_ref()
            .and_then(|state| state.rows.get(row_index))
        else {
            return;
        };
        if row.disabled {
            return;
        }
        let action = row.action;
        self.dispatch_gpui_titlebar_git_action_selector(action.selector(), cx);
    }

    /// A Git menu row, the Commit button, a Git hotkey, or `refresh` as the menu opens: a fixed
    /// selector, answered in Rust (gx_store/git/actions.rs).
    pub(crate) fn dispatch_gpui_titlebar_git_action_selector(
        &mut self,
        selector: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.gx_store_git_titlebar_action(selector, cx)
    }

    pub(crate) fn show_gpui_titlebar_customize_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Titlebar 2026-08-11:
        Right-clicking blank titlebar chrome or a workarea mode button should
        expose the page that owns titlebar visibility. Keep this as an owned GPUI popup
        action into the existing Settings > Extensions route (the
        page formerly called Customize); normal titlebar layout and hit testing
        remain unchanged.
        */
        GpuiContextMenu::new()
            .menu("Extension Store", Box::new(OpenGpuiExtensionsModal))
            .show(position, window, cx);
    }

    /// CDXC:ContextMenus 2026-11-08 DECISION:
    /// User: right-clicking a pinned extension's icon in the work area header offers "Unpin from header" and "Disable extension", and keeps "Extension Store" (it showed only that). The standard context menu look. Unpin is the extensions menu's pin toggle; Disable is Settings > Extensions' Enabled switch.
    pub(crate) fn show_gpui_titlebar_pinned_extension_menu(
        &self,
        extension_id: String,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        GpuiContextMenu::new()
            .menu(
                "Unpin from header",
                Box::new(ToggleGpuiExtensionPin {
                    extension_id: extension_id.clone(),
                    pinned: false,
                }),
            )
            .menu(
                "Disable extension",
                Box::new(DisableGpuiExtension { extension_id }),
            )
            .separator()
            .menu("Extension Store", Box::new(OpenGpuiExtensionsModal))
            .show(position, window, cx);
    }

    /// CDXC:ContextMenus 2026-09-17 DECISION:
    /// User: right-clicking a titlebar account shows Accounts below Extensions in that context menu.
    pub(crate) fn show_gpui_titlebar_account_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        GpuiContextMenu::new()
            .menu("Extension Store", Box::new(OpenGpuiExtensionsModal))
            .menu("Accounts", Box::new(OpenGpuiAccountsModal))
            .show(position, window, cx);
    }

    pub(crate) fn select_titlebar_mode_from_menu(
        &mut self,
        mode_index: u64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mode) = self
            .titlebar_mode_switcher_items()
            .into_iter()
            .find(|item| item.mode.switcher_index() == mode_index)
            .map(|item| item.mode)
        else {
            return;
        };
        if self.set_active_mode(mode, window, cx) {
            cx.notify();
        }
    }

    pub(crate) fn titlebar_popup_menu_open(&self, kind: GpuiTitlebarPopupKind) -> bool {
        self.titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| state.kind == kind)
    }
}
