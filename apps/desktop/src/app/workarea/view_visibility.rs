//! Which views a project may open, and the switch that hides one. Moved verbatim out of
//! `app/workarea.rs` on 2026-09-20.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
use anyhow::Result;

impl GhostexGpuiApp {
    pub(crate) fn hide_code_view_tab(&mut self, cx: &mut gpui::Context<Self>) {
        let _ = self.set_project_workarea_titlebar_mode_hidden(TitlebarMode::Source, true, cx);
    }

    pub(crate) fn set_project_workarea_titlebar_mode_hidden(
        &mut self,
        mode: TitlebarMode,
        hidden: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        let Some(settings_key) = titlebar_mode_view_tab_hidden_settings_key(mode) else {
            return Err("That built-in feature does not have a visibility switch.".to_string());
        };
        let mut settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(settings_key.to_string(), serde_json::Value::Bool(hidden));
        shared_settings::write_shared_sidebar_settings_object(settings)
            .map_err(|error| format!("Could not update the built-in feature setting: {error:?}"))?;
        self.move_off_view_hidden_just_now(cx);
        self.refresh_gpui_plugins_modal(cx);
        cx.notify();
        Ok(())
    }

    /// CDXC:Workarea 2026-10-26 WHY:
    /// A switch that hides views here moves the panel off the one on screen right after its setting is written, instead of when the settings change comes back from the save, which left the old page drawn with no tab selected for about a second. The settings pass that follows finds nothing left to move.
    pub(crate) fn move_off_view_hidden_just_now(&mut self, cx: &mut gpui::Context<Self>) {
        if self.coerce_active_mode_to_available_project_context(cx) {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
        }
    }

    pub(crate) fn titlebar_mode_available(&self, mode: TitlebarMode) -> bool {
        /*
        CDXC:Extensions 2026-08-23:
        A workarea turned off in Settings → Customize is not just missing its
        titlebar tab: it is a place the shell must never route to. Folding the
        Customize gate into the single availability predicate closes that off
        for every caller at once — hotkeys, command palette, chat/terminal link
        and file opens, saved Browser Actions, `ghostex browser open`, OS
        `ghostex://` opens, restored and persisted active modes — instead of
        leaving each entry point to remember its own check. Entry points that
        answer a click still handle the refusal visibly (they copy the target
        and say why); this predicate is the backstop that keeps the rest from
        silently parking the user on a view they turned off.
        */
        let available = match mode {
            // A sidebar extension's side panel view is drawn natively and needs no project.
            TitlebarMode::Extension(id) if self.extension_side_panel_offered(id) => true,
            TitlebarMode::Extension(id) => {
                if gpui_custom_view(id).is_some() {
                    gpui_enabled_custom_view(id)
                        .is_some_and(|view| self.custom_project_view_visible(&view))
                } else {
                    self.project_scoped_workarea_availability()
                        .titlebar_mode_available(mode)
                        && self.installed_extension_view(id).is_some()
                        && gpui_extension_view_presentation(id).is_some()
                }
            }
            TitlebarMode::Work => self.work_view_available(),
            // Side chats are on screen only as tabs: the mode exists while this project has one.
            TitlebarMode::SideChat => self.has_side_chat_tabs(),
            _ => self
                .project_scoped_workarea_availability()
                .titlebar_mode_available(mode),
        };
        available
            && !gpui_titlebar_mode_hidden_from_settings(mode)
            && self.titlebar_mode_view_scope_allows(mode)
    }

    pub(crate) fn available_titlebar_mode_or_agents(&self, mode: TitlebarMode) -> TitlebarMode {
        if self.titlebar_mode_available(mode) {
            mode
        } else {
            TitlebarMode::Agents
        }
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// The scope filter is applied one layer up, because the view panel's `+` menu needs the list
    /// BEFORE it: `Hidden here ▸` is exactly the views this list holds and the scoped one does not.
    /// A view switched off in Settings stays out of both, since it is turned off rather than hidden
    /// in a place.
    pub(crate) fn titlebar_mode_switcher_items(&self) -> Vec<TitlebarModeSwitcherItem> {
        let mut items = self
            .titlebar_mode_switcher_items_unscoped()
            .into_iter()
            .filter(|item| self.titlebar_mode_view_scope_allows(item.mode))
            .collect::<Vec<_>>();
        if items.len() == 1 && items[0].mode == TitlebarMode::Agents {
            items.clear();
        }
        items
    }

    pub(crate) fn titlebar_mode_switcher_items_unscoped(&self) -> Vec<TitlebarModeSwitcherItem> {
        let mut items = titlebar_mode_switcher_items(self.project_scoped_workarea_availability())
            .into_iter()
            .filter(|item| !gpui_titlebar_mode_hidden_from_settings(item.mode))
            .collect::<Vec<_>>();
        let installed_extension_available = self
            .project_scoped_workarea_availability()
            .project_context
            .has_project_scoped_workareas();
        let mut extension_modes = self
            .extensions_snapshot
            .installed
            .values()
            .filter(|extension| {
                extension.enabled
                    && extension.placements.contains(&GpuiExtensionPlacement::View)
                    && extension.placement == Some(GpuiExtensionPlacement::View)
            })
            .filter_map(|extension| {
                let id = ExtensionId::new(&extension.id)?;
                if gpui_custom_view(id).is_some() {
                    return None;
                }
                let title = gpui_extension_view_presentation(id)?.title;
                Some((title, id, installed_extension_available))
            })
            .collect::<Vec<_>>();
        extension_modes.extend(
            self.extension_side_panel_modes()
                .into_iter()
                .filter_map(|mode| {
                    let TitlebarMode::Extension(id) = mode else {
                        return None;
                    };
                    Some((mode.tab_label(), id, true))
                }),
        );
        extension_modes.sort_by(|left, right| left.0.cmp(&right.0));
        items.extend(extension_modes.into_iter().map(|(_, id, is_available)| {
            TitlebarModeSwitcherItem {
                mode: TitlebarMode::Extension(id),
                is_available,
                disabled_reason: (!is_available)
                    .then_some(TITLEBAR_PROJECT_CONTEXT_DISABLED_REASON),
            }
        }));
        // CDXC:WorkMode 2026-10-09 DECISION:
        // User: "Work" is listed in the side panel's "Open a view" picker whenever the window shows a work-mode project. (Called Issues since 2026-10-24; its sidebar briefcase button was removed 2026-10-29, see CDXC:WorkMode 2026-10-29 in work_view/host.rs.)
        if self.work_view_available() {
            items.push(TitlebarModeSwitcherItem {
                mode: TitlebarMode::Work,
                is_available: true,
                disabled_reason: None,
            });
        }
        items.extend(
            gpui_custom_views_from_settings()
                .into_iter()
                .filter(|view| self.custom_project_view_visible(view))
                .map(|view| TitlebarModeSwitcherItem {
                    mode: TitlebarMode::Extension(view.id),
                    is_available: true,
                    disabled_reason: None,
                }),
        );
        // CDXC:Titlebar 2026-09-20 DECISION:
        // User: the view order mixes built-in, extension, and custom views. Option+1..9 follows the tabs in the view panel (screen 07), falling through to this order for a number past the last tab.
        // This supersedes the 2026-09-09 wording that the numbers followed the titlebar's displayed list, which no longer exists. Since 2026-09-24 a newly opened tab goes at the end of the tabs bar instead of taking its place from this order (app/view_panel.rs).
        // SEE-ALSO: packages/shared/ghostex-settings/titlebar-view-order.ts (deleted 2026-10-01) uses the same mode slugs for Settings.
        let snapshot = shared_settings::shared_sidebar_settings_snapshot();
        if let Some(order) = snapshot
            .object()
            .get("titlebarViewOrder")
            .and_then(serde_json::Value::as_array)
        {
            let order: Vec<&str> = order.iter().filter_map(serde_json::Value::as_str).collect();
            items.sort_by_cached_key(|item| {
                crate::app::window::settings_modal::tabs::extensions::titlebar_view_order_rank(
                    &order,
                    &item.mode.element_slug(),
                )
                .unwrap_or(usize::MAX)
            });
        }
        if items.len() == 1 && items[0].mode == TitlebarMode::Agents {
            items.clear();
        }
        items
    }

    /// Returns whether the view on screen could no longer be shown here and the panel moved on. Most
    /// callers (the settings, Work mode and extension refreshes) have no `Window`, so a Browser
    /// successor's address field follows in the main window on the next turn, from that turn's state.
    ///
    /// CDXC:Workarea 2026-10-25 WHY:
    /// Hiding the view on screen now behaves like closing its tab: the panel shows the tab to its right, else the one to its left, and with no other tab it follows the last-tab rule (`closeSidePanelWithLastTab`). This holds everywhere a view goes away: Show in › This Project or This Space, Settings › Customize, "Choose where it's shown…", turning its extension or Work mode off, and switching to a project where it is hidden (`finish_project_view_restore`). It replaces the 2026-06-22 fallback to Agents, which since the 2026-09-20 side panel closed the whole panel even with other tabs open.
    pub(crate) fn coerce_active_mode_to_available_project_context(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let gone = self.active_mode;
        if self.available_titlebar_mode_or_agents(gone) == gone {
            return false;
        }
        self.show_leaving_view_successor(self.leaving_view_successor(gone), false, cx);
        if self.active_mode == TitlebarMode::Browser {
            self.defer_in_main_window(cx, |app, window, cx| {
                app.sync_browser_address_input_if_shown(window, cx);
            });
        }
        true
    }
}
