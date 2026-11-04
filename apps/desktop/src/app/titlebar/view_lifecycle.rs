use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn titlebar_view_mode_for_index(&self, index: u64) -> Option<TitlebarMode> {
        self.titlebar_mode_switcher_items()
            .into_iter()
            .find(|item| item.mode.switcher_index() == index)
            .map(|item| item.mode)
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
            // Pages of this project still waiting to be made are not made later: the sleep wins
            // (`waiting_parked_browser_pages`).
            if let Some(project_id) = self.browser_tabs_project_id.as_deref() {
                self.waiting_parked_browser_pages
                    .retain(|(waiting, _)| waiting != project_id);
            }
            let tab_ids = self.browser_surfaces.keys().copied().collect::<Vec<_>>();
            for tab_id in tab_ids {
                self.remove_browser_surface(tab_id, cx);
            }
        } else if let Some(slot) = ProjectWorkareaCefSurfaceSlotKey::for_titlebar_mode(mode) {
            self.remove_project_workarea_runtime_cef_surface(slot, cx);
        }
        let project_id = self.agents_workspace_project_id.clone();
        self.release_parked_project_pages(project_id.as_deref(), Some(mode));
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
        if mode.is_storybook() {
            self.project_view_command(
                &crate::app::project_views::ProjectViewCommand {
                    id: "storybook".into(),
                    operation: "restart".into(),
                },
                window,
                cx,
            );
            return;
        }
        // The native Automate view has no page to reload; Reload refetches its automations.
        if mode == TitlebarMode::Automate
            && self.project_editor_shell.is_mode_awake(mode)
            && let Some(view) = self.native_automate.clone()
        {
            view.update(cx, |view, cx| view.load(cx));
            cx.notify();
            return;
        }
        let surface = if mode == TitlebarMode::Browser {
            self.browser_surface_for_pane(self.browser_tabs.focused_pane)
        } else {
            ProjectWorkareaCefSurfaceSlotKey::for_titlebar_mode(mode).and_then(|slot| {
                self.project_workarea_runtime_cef_surfaces
                    .get(&self.shown_workarea_slot(slot))
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
