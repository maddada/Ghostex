use crate::app::actions::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn titlebar_view_mode_for_index(&self, index: u64) -> Option<TitlebarMode> {
        self.titlebar_mode_switcher_items()
            .into_iter()
            .find(|item| item.mode.switcher_index() == index)
            .map(|item| item.mode)
    }

    /// CDXC:Titlebar 2026-09-13 DECISION:
    /// User: right-clicking a web-based view's titlebar button offers Reload, Sleep, a separator, then Extensions.
    /// Actions target the clicked view even when another view is selected; the compact button targets its displayed view.
    pub(crate) fn titlebar_view_lifecycle_menu(&self, mode: TitlebarMode) -> GpuiContextMenu {
        let menu = GpuiContextMenu::new();
        if !mode.is_project_editor_mode() {
            return menu;
        }
        let unavailable = !self.titlebar_mode_available(mode);
        menu.menu_with_disabled(
            "Reload",
            unavailable,
            Box::new(ReloadGpuiTitlebarView {
                mode_index: mode.switcher_index(),
            }),
        )
        .menu_with_disabled(
            "Sleep",
            unavailable || !self.project_editor_shell.is_mode_awake(mode),
            Box::new(SleepGpuiTitlebarView {
                mode_index: mode.switcher_index(),
            }),
        )
        .separator()
    }

    pub(crate) fn show_gpui_titlebar_view_menu(
        &self,
        mode: TitlebarMode,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.titlebar_view_lifecycle_menu(mode)
            .menu("Extensions", Box::new(OpenGpuiExtensionsModal))
            .show(position, window, cx);
    }

    fn titlebar_view_surface_slot(mode: TitlebarMode) -> Option<ProjectWorkareaCefSurfaceSlotKey> {
        Some(match mode {
            TitlebarMode::Source => ProjectWorkareaCefSurfaceSlotKey::Source,
            TitlebarMode::Kanban => ProjectWorkareaCefSurfaceSlotKey::Kanban,
            TitlebarMode::Automate => ProjectWorkareaCefSurfaceSlotKey::Automate,
            TitlebarMode::Manage => ProjectWorkareaCefSurfaceSlotKey::Manage,
            TitlebarMode::Extension(id) => ProjectWorkareaCefSurfaceSlotKey::Extension(id),
            TitlebarMode::Agents | TitlebarMode::Browser => return None,
        })
    }

    pub(crate) fn sleep_titlebar_view(&mut self, mode: TitlebarMode, cx: &mut gpui::Context<Self>) {
        if !mode.is_project_editor_mode() {
            return;
        }
        // Close the visibility/startup gate before releasing runtime ownership.
        self.project_editor_shell.mark_mode_sleeping(mode);
        self.project_editor_auto_sleep_epochs.bump(mode);
        if mode == TitlebarMode::Source {
            self.stop_source_code_server_runtime(cx);
        } else if mode == TitlebarMode::Browser {
            let tab_ids = self.browser_surfaces.keys().copied().collect::<Vec<_>>();
            for tab_id in tab_ids {
                self.remove_browser_surface(tab_id, cx);
            }
        } else if let Some(slot) = Self::titlebar_view_surface_slot(mode) {
            self.remove_project_workarea_runtime_cef_surface(slot, cx);
        }
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn reload_titlebar_view(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !mode.is_project_editor_mode() || !self.titlebar_mode_available(mode) {
            return;
        }
        let surface = if mode == TitlebarMode::Browser {
            self.browser_surface_for_pane(self.browser_tabs.focused_pane)
        } else {
            Self::titlebar_view_surface_slot(mode).and_then(|slot| {
                self.project_workarea_runtime_cef_surfaces
                    .get(&slot)
                    .map(|owned| owned.surface.clone())
            })
        };
        if self.project_editor_shell.is_mode_awake(mode)
            && let Some(surface) = surface
        {
            surface.update(cx, |surface, _| surface.reload());
        } else {
            self.set_active_mode(mode, window, cx);
        }
        cx.notify();
    }
}
