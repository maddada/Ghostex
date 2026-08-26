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

use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;
use gpui_component::native_menu::NativeMenu;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// The titlebar Git control opens an in-app PopupMenu projected from the
    /// sidebar runtime's shared Git menu builders. Selections keep dispatching
    /// fixed action selectors only.
    pub(crate) fn show_gpui_titlebar_git_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Git);
        if open {
            // Ask the runtime for a background state refresh so the menu
            // converges on fresh rows; the open menu renders the last projected
            // state honestly instead of fabricating fresh values.
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

    pub(crate) fn dispatch_gpui_titlebar_git_action_selector(
        &mut self,
        selector: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "action": selector,
            "type": GPUI_SIDEBAR_TITLEBAR_GIT_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_TITLEBAR_GIT_ACTION_MESSAGE_VERSION,
        });
        let script = gpui_titlebar_git_action_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn show_titlebar_settings_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
        The GPUI titlebar Settings glyph opens this NativeMenu so Settings, Hotkeys, and Command Palette all have typed titlebar actions into the shared React app-modal host. Keep this menu OS-owned and action-backed, with no visual-only dropdown, fake control, WebKit surface, overlay, hidden hit region, or generic fallback behavior.

        CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
        Previous Sessions is exposed from the same titlebar NativeMenu so GPUI opens the shared history/restore modal through a typed app-modal action rather than duplicating the React UI or adding an overlay surface.

        CDXC:GPUIAgentsHubModal 2026-06-24-12:26:
        Agents Hub belongs in the same typed GPUI app-modal route as the Settings utility surfaces. The menu action must open the shared React Hub in the owned CEF app-modal host while Rust supplies the real filesystem catalog/content bridge instead of duplicate modal UI or fallback rows.

        CDXC:GPUISettingsEntryModals 2026-06-24-12:22:
        Configure Agents, Configure Actions, and Open Targets belong in the same typed NativeMenu because macOS/React already treat them as Settings-modal entry points. Keep the menu action-backed so GPUI opens the shared Settings host with the requested initial tab instead of introducing a second modal surface.
        */
        NativeMenu::new()
            .menu("Settings", Box::new(OpenGpuiSettingsModal))
            .menu("Plugins", Box::new(OpenGpuiPluginsModal))
            .menu("Hotkeys", Box::new(OpenGpuiHotkeysModal))
            .menu("Quick Access", Box::new(OpenGpuiCommandPaletteModal))
            .menu("Configure Agents", Box::new(OpenGpuiConfigureAgentsModal))
            .menu("Configure Actions", Box::new(OpenGpuiConfigureActionsModal))
            .menu("Open Targets", Box::new(OpenGpuiOpenTargetsModal))
            .menu("Previous Sessions", Box::new(OpenGpuiPreviousSessionsModal))
            .menu("Agents Hub", Box::new(OpenGpuiAgentsHubModal))
            .show(position, window, cx);
    }

    pub(crate) fn show_gpui_titlebar_mode_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarCompactMode 2026-07-04-01:00:
        The compact titlebar mode picker is an OS-owned NativeMenu for narrow windows. Rows are projected from the same titlebar_mode_switcher_items list as the center tabs, disabled states stay disabled in Quick/projectless contexts, and selections dispatch through set_active_mode rather than mutating active_mode directly.
        */
        let mut menu = NativeMenu::new();
        for item in self.titlebar_mode_switcher_items() {
            let action = Box::new(SelectGpuiTitlebarMode {
                mode_index: item.mode.switcher_index(),
            });
            let label = match item.mode {
                TitlebarMode::Extension(id) => gpui_extension_view_presentation(id)
                    .map(|presentation| presentation.title)
                    .unwrap_or_else(|| id.as_str().to_string()),
                mode => mode.display_label().to_string(),
            };
            if item.is_available {
                menu = menu.menu_with_check(label, self.active_mode == item.mode, action);
            } else {
                menu = menu.menu_with_disabled(label, true, action);
            }
        }
        menu.show(position, window, cx);
    }

    pub(crate) fn show_gpui_titlebar_customize_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarCustomize 2026-08-11:
        Right-clicking blank titlebar chrome or a workarea mode button should
        expose the page that owns titlebar visibility. Keep this as an OS-owned
        NativeMenu action into the existing Settings > Customize route; normal
        titlebar layout and hit testing remain unchanged.
        */
        NativeMenu::new()
            .menu("Customize", Box::new(OpenGpuiCustomizeSettingsModal))
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

    pub(crate) fn show_gpui_open_targets_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
        The visible GPUI titlebar folder control mirrors macOS Open In behavior with an in-app gpui-component PopupMenu. Menu rows remain typed GPUI actions, Configure routes to the shared Open Targets Settings tab, and the control must not add React overlays, WebKit dropdowns, invisible hit regions, hit-test overrides, or synthetic coordinate routing.
        */
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::OpenTargets,
            !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::OpenTargets),
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn show_gpui_titlebar_actions_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Right-clicking the visible GPUI titlebar Actions play control opens an in-app gpui-component PopupMenu of sidebar actions plus Configure. Rows dispatch typed GPUI actions by visible index into the current projected action list, unconfigured rows route through the existing Settings > Actions path, and the control must not add React overlays, WebKit dropdowns, invisible hit regions, hit-test overrides, or synthetic coordinate routing.
        */
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Actions,
            !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Actions),
            trigger_bounds,
            window,
            cx,
        );
    }
}
