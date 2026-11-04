//! Browser tabs and panes per project: swapping, page metadata, toolbar actions, profiles, tab add, select, close and popups.

use crate::app::context_menu::GpuiContextMenu;
use gpui::Entity;
use gpui::Pixels;
use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::view_strip_order::ViewStripEntry;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn swap_browser_tabs_for_active_project(&mut self, cx: &mut gpui::Context<Self>) {
        let new_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        self.swap_browser_tabs_to_project_id(new_project_id, cx);
    }

    pub(crate) fn swap_browser_tabs_to_project_id(
        &mut self,
        new_project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_tabs_project_id == new_project_id {
            return;
        }

        // A pre-project-scoping workspace belongs to the first real project
        // that claims it. Subsequent project changes park and restore the
        // complete tab model instead of sharing it across projects.
        if self.browser_tabs_project_id.is_none()
            && new_project_id.as_ref().is_some_and(|project_id| {
                !self.parked_browser_tabs_by_project.contains_key(project_id)
            })
        {
            self.browser_tabs_project_id = new_project_id;
            self.persist_shell_layout_state();
            return;
        }

        self.browser_tabs_project_epoch = self.browser_tabs_project_epoch.wrapping_add(1);

        /*
        CDXC:Browser 2026-08-26:
        Browser ids are project-local, so the live surface/input maps can only
        describe one project at a time — but that is a reason to move the
        outgoing project's runtime out of the way, not to destroy it. The whole
        bundle parks under the outgoing project id (hidden, still loaded) and
        the incoming project's bundle, if it has one, becomes live again, so a
        project switch no longer sleeps and reloads every browser tab of the
        project the user just left.

        The projectless pre-project model is the one exception: it has no key to
        park under and its model is dropped here, so its pages go with it.
        */
        if let Some(old_project_id) = self.browser_tabs_project_id.take() {
            self.parked_browser_tabs_by_project
                .insert(old_project_id.clone(), self.browser_tabs.clone());
            let parked_runtime = self.park_all_browser_surfaces(cx);
            if parked_runtime.holds_runtime_state() {
                self.parked_browser_runtimes_by_project
                    .insert(old_project_id, parked_runtime);
            } else {
                self.parked_browser_runtimes_by_project
                    .remove(&old_project_id);
            }
        } else {
            self.drop_all_browser_surfaces(cx);
        }
        self.browser_tabs = new_project_id
            .as_ref()
            .and_then(|project_id| self.parked_browser_tabs_by_project.remove(project_id))
            .unwrap_or_else(|| {
                BrowserTabModel::shell_address_only_with_profile(
                    self.browser_profiles.active_profile_id(),
                )
            });
        match new_project_id
            .as_ref()
            .and_then(|project_id| self.parked_browser_runtimes_by_project.remove(project_id))
        {
            Some(parked_runtime) => self.restore_parked_browser_surfaces(parked_runtime),
            // A model that never had a parked runtime starts a fresh runtime
            // identity, so no surface parked under an older one can claim it.
            None => self.browser_tabs_runtime_key = self.browser_tabs_project_epoch,
        }
        self.browser_tabs_project_id = new_project_id;
        self.browser_url = self.browser_tabs.active_address_value();

        // A pending media prompt belongs to the page that raised it and cannot
        // be answered from another project's workarea; dropping it releases the
        // page's `getUserMedia()` promise instead of leaving it hanging.
        self.browser_media_permission_prompts.clear();
        self.browser_tab_scroll_handles.clear();
        self.browser_leaf_layout_bounds.clear();
        self.browser_split_layout_metrics.clear();
        self.browser_tab_drop_feedback = None;
        self.browser_tab_drag_active = false;
        self.browser_split_drag = None;
        self.hovered_browser_tab = None;
        self.pending_browser_find_focus = None;
        self.pending_browser_address_focus = None;
        // Tab ids are per project, so the incoming project's page with the same id is another page.
        self.browser_tab_opener = None;

        if matches!(
            self.shell_focus,
            ShellFocusTarget::BrowserPane(_) | ShellFocusTarget::BrowserSurface
        ) {
            self.focus_shell_target(
                ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
                cx,
            );
        }
        /*
        CDXC:Browser 2026-08-26:
        Restored surfaces come back hidden, because parking hid them. Run the
        normal visibility gate so the incoming project's rendered tabs are shown
        again by the one owner of that decision, instead of leaving the Browser
        workarea black until the next unrelated repaint.
        */
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn handle_browser_page_metadata_event(
        &mut self,
        tab_id: BrowserTabId,
        event: cef::BrowserPageMetadataEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            cef::BrowserPageMetadataEvent::HistoryRequested => {
                if let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id)
                {
                    self.show_browser_history_popup(pane_id, window, cx);
                }
            }
            cef::BrowserPageMetadataEvent::FindRequested => {
                if let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id)
                    && self.browser_tabs.active_tab_id_for_pane(pane_id) == Some(tab_id)
                    && self.browser_tabs.focus_pane(pane_id)
                {
                    self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
                    self.start_find_in_focused_browser(window, cx);
                }
            }
            cef::BrowserPageMetadataEvent::AddressChanged(url) => {
                /*
                CDXC:Browser 2026-06-22-07:23:
                CEF-reported Browser navigation owns the runtime URL for that tab. Update the URL-derived fallback title, refresh the active address field only when the reporting tab is selected, keep the selected tab's own CEF surface visible, and persist through the existing Browser URL sanitizer instead of writing raw navigation details directly.
                */
                // A cross-origin navigation invalidates any prompt the previous
                // document raised; same-origin navigation keeps it pending.
                self.clear_browser_media_permission_prompts_for_navigation(tab_id, &url);
                if !self.browser_tabs.record_page_address_change(tab_id, url) {
                    return;
                }
                if let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id)
                    && self.browser_tabs.active_tab_id_for_pane(pane_id) == Some(tab_id)
                {
                    let address_value = self.browser_tabs.address_value_for_pane(pane_id);
                    if self.browser_tabs.active_tab == tab_id {
                        self.browser_url = address_value.clone();
                    }
                    self.set_browser_address_input_value(pane_id, address_value, window, cx);
                } else if self.browser_tabs.active_tab == tab_id {
                    let address_value = self.browser_tabs.active_address_value();
                    self.browser_url = address_value.clone();
                }
                self.update_active_mode_cef_child_visibility(cx);
                self.persist_shell_layout_state();
                cx.notify();
            }
            cef::BrowserPageMetadataEvent::CloseRequested => {
                self.close_browser_tab(tab_id, window, cx);
            }
            cef::BrowserPageMetadataEvent::CopyToClipboard(item) => {
                gpui_copy_to_clipboard(item, cx);
            }
            cef::BrowserPageMetadataEvent::TitleChanged(title) => {
                /*
                CDXC:Browser 2026-06-22-07:23:
                CEF page titles change the visible Browser tab-strip label while the app runs.

                CDXC:Browser 2026-07-12:
                Shell-state serialization now persists the bounded last displayed title (`cachedTitle`) so restart keeps the same label; the independent Browser visit store also saves titles for the history popup.
                */
                if self.browser_tabs.record_page_title_change(tab_id, title) {
                    cx.notify();
                }
            }
            cef::BrowserPageMetadataEvent::FaviconUrlChanged(favicon_url) => {
                /*
                CDXC:Browser 2026-06-22-09:11:
                CEF favicon metadata updates Browser tab chrome and the independent saved visit store, without changing shell-state JSON.
                */
                if self
                    .browser_tabs
                    .record_page_favicon_url_change(tab_id, favicon_url)
                {
                    cx.notify();
                }
            }
            cef::BrowserPageMetadataEvent::FindResult {
                match_count,
                active_match_ordinal,
                final_update,
            } => {
                let Some(find) = self.browser_find_states.get_mut(&tab_id) else {
                    return;
                };
                if find.match_count == match_count
                    && find.active_match_ordinal == active_match_ordinal
                    && find.final_update == final_update
                {
                    return;
                }
                find.match_count = match_count.max(0);
                find.active_match_ordinal = active_match_ordinal.max(0);
                find.final_update = final_update;
                cx.notify();
            }
            cef::BrowserPageMetadataEvent::LoadingStateChanged {
                is_loading,
                can_go_back,
                can_go_forward,
            } => {
                if self.browser_tabs.record_page_loading_state_change(
                    tab_id,
                    is_loading,
                    can_go_back,
                    can_go_forward,
                ) {
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn ensure_active_browser_surface(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        self.ensure_browser_surface_for_pane(self.browser_tabs.focused_pane, cx)
    }

    pub(crate) fn ensure_browser_surface_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        let (tab_id, url, profile_id) = self.active_loaded_browser_tab_for_pane(pane_id)?;
        self.ensure_browser_surface_for_tab(tab_id, url, profile_id, cx)
    }

    pub(crate) fn load_browser_cef_url_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        url: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = self.ensure_browser_surface_for_pane(pane_id, cx) {
            surface.update(cx, |surface, _| surface.load_url(url));
        }
        self.update_active_mode_cef_child_visibility(cx);
    }

    pub(crate) fn perform_browser_toolbar_action(
        &mut self,
        pane_id: BrowserPaneId,
        action: BrowserToolbarAction,
        cx: &mut gpui::Context<Self>,
    ) {
        if !matches!(
            action,
            BrowserToolbarAction::Back
                | BrowserToolbarAction::Forward
                | BrowserToolbarAction::Reload
                | BrowserToolbarAction::StopLoading
        ) {
            return;
        }
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }

        self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        if !self.browser_tabs.focus_pane(pane_id) {
            return;
        }
        self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);

        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| match action {
                BrowserToolbarAction::Back => surface.go_back(),
                BrowserToolbarAction::Forward => surface.go_forward(),
                BrowserToolbarAction::Reload => surface.reload(),
                BrowserToolbarAction::StopLoading => surface.stop_load(),
                BrowserToolbarAction::Home
                | BrowserToolbarAction::FeedbackTool
                | BrowserToolbarAction::ResetZoom
                | BrowserToolbarAction::ResetMediaPermissions
                | BrowserToolbarAction::HistoryMenu
                | BrowserToolbarAction::ProfileMenu
                | BrowserToolbarAction::DevTools => {}
            });
        }

        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn navigate_browser_home_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The Browser home target is the current project's primary remote web
        page, carried by the active-project snapshot from the owning gxserver
        machine. Navigate the selected tab through the normal
        address commit path so CEF history, tab metadata, focus, and shell
        persistence all stay under their existing owners.
        */
        let home_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        self.browser_address_input_editing.remove(&pane_id);
        self.pending_browser_address_focus = None;
        self.commit_browser_address_for_pane(pane_id, home_url.clone(), cx);
        self.set_browser_address_input_value_unchecked(pane_id, home_url, window, cx);
    }

    pub(crate) fn prepare_browser_toolbar_right_action(
        &mut self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Titlebar 2026-06-22-15:52:
        Browser toolbar commands are user-facing Browser activation routes. In Quick/projectless GPUI context they must no-op through the same titlebar availability guard as mode clicks and Option workarea hotkeys, instead of directly switching activeMode to Browser.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return false;
        }
        if !self.browser_tabs.focus_pane(pane_id) {
            return false;
        }
        self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        true
    }

    pub(crate) fn run_browser_feedback_tool_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        if browser_feedback_tool_unavailable_url(&address_value) {
            cx.notify();
            return;
        }
        let Some(surface) = self.browser_surface_for_pane(pane_id) else {
            window.push_notification(
                Notification::warning("Open a Browser page before starting feedback."),
                cx,
            );
            cx.notify();
            return;
        };
        let script = browser_agentation_feedback_injection_script();
        let injected = surface.update(cx, |surface, _| {
            surface.inject_feedback_tool_script(&script)
        });
        if !injected {
            window.push_notification(
                Notification::warning(format!(
                    "{} feedback is not ready on this Browser page.",
                    BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL
                )),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn reset_browser_zoom_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| surface.reset_zoom());
        } else {
            window.push_notification(
                Notification::warning("Open a Browser page before resetting zoom."),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn toggle_browser_devtools_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| surface.toggle_dev_tools());
        } else {
            window.push_notification(
                Notification::warning("Open a Browser page before toggling DevTools."),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn show_browser_profile_menu(
        &mut self,
        pane_id: BrowserPaneId,
        trigger_bounds: gpui::Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Browser 2026-06-23-11:14:
        Browser Profiles are a normal GPUI Browser toolbar feature. The menu reflects real shell profile state through an owned GPUI popup window with checked generated profile rows and New Profile; do not use GPUI overlays, hidden hit regions, hit-test routing, or user-entered profile names.
        */
        // CDXC:Browser 2026-09-22 WHY:
        // Opening the menu must not run prepare_browser_toolbar_right_action: its keyboard handoff makes the CEF page first responder on the next render, and that responder transition is the boundary that closes every titlebar popup, so the menu appeared and vanished. The Select/New Profile handlers activate the pane when a row is chosen.
        if !self.titlebar_mode_available(TitlebarMode::Browser)
            || self.browser_tabs.find_leaf(pane_id).is_none()
        {
            return;
        }

        let selected_profile = self
            .browser_tabs
            .active_tab_for_pane(pane_id)
            .map(|tab| tab.profile_id)
            .unwrap_or_else(|| self.browser_profiles.active_profile_id());
        let mut menu = GpuiContextMenu::new();
        for profile_id in self.browser_profiles.profile_ids() {
            menu = menu.menu_with_check(
                profile_id.display_label(),
                profile_id == selected_profile,
                Box::new(SelectBrowserProfile {
                    pane_id: pane_id.0,
                    profile_id: profile_id.0,
                }),
            );
        }

        menu.separator()
            .menu(
                "New Profile...",
                Box::new(CreateBrowserProfile { pane_id: pane_id.0 }),
            )
            .toggle_below(trigger_bounds, window, cx);
    }

    pub(crate) fn select_browser_profile_from_menu(
        &mut self,
        pane_id: BrowserPaneId,
        profile_id: BrowserProfileId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.browser_profile_actions_available() {
            return;
        }
        if !self.browser_profiles.contains_profile(profile_id) {
            return;
        }
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        let Some(tab_id) = self.browser_tabs.active_tab_id_for_pane(pane_id) else {
            return;
        };
        let selected_profile_changed = self.browser_profiles.select_profile(profile_id);
        let tab_profile_changed = self.browser_tabs.set_tab_profile(tab_id, profile_id);
        if tab_profile_changed {
            self.remove_browser_surface(tab_id, cx);
            self.sync_active_browser_tab_to_surface(window, cx);
        }
        if selected_profile_changed || tab_profile_changed {
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn create_browser_profile_from_menu(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.browser_profile_actions_available() {
            return;
        }
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        let Some(profile_id) = self.browser_profiles.create_generated_profile() else {
            window.push_notification(Notification::warning("Browser profile limit reached."), cx);
            cx.notify();
            return;
        };
        if let Some(tab_id) = self.browser_tabs.active_tab_id_for_pane(pane_id)
            && self.browser_tabs.set_tab_profile(tab_id, profile_id)
        {
            self.remove_browser_surface(tab_id, cx);
            self.sync_active_browser_tab_to_surface(window, cx);
        }
        self.persist_shell_layout_state();
        window.push_notification(
            Notification::info(format!(
                "Created Browser profile: {}.",
                profile_id.display_label()
            )),
            cx,
        );
        cx.notify();
    }

    pub(crate) fn browser_profile_actions_available(&self) -> bool {
        /*
        CDXC:Browser 2026-06-23-11:28:
        Profile menu actions are registered globally like other GPUI popup menu commands, so the handler boundary must repeat the Browser availability gate. Stale or direct action dispatch cannot create, select, persist, or touch CEF profile state outside the Browser workspace.
        */
        self.titlebar_mode_available(TitlebarMode::Browser)
    }

    pub(crate) fn remove_browser_surface(
        &mut self,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = self.browser_surfaces.remove(&tab_id) {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
        // The page that asked is gone with its surface; cancel its request
        // instead of leaving a prompt bound to a dead tab.
        self.clear_browser_media_permission_prompts(tab_id);
    }

    pub(crate) fn sync_active_browser_tab_to_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Browser 2026-06-22-05:56:
        Browser tab selection is shell state in this slice but still has to feel real: selecting a tab updates the active tab id, the toolbar URL state, the address input text, and the active tab's owned CEF surface without reloading existing tab page state.

        CDXC:Browser 2026-06-22-06:59:
        Selecting a Browser tab should reveal that tab's CEF entity and hide any Browser CEF entity that is not the active loaded tab of a rendered Browser leaf. Address-only placeholder tabs deliberately do not create or show a CEF surface, so the Browser body stays empty instead of displaying stale page state from another tab.

        CDXC:Browser 2026-06-22-09:55:
        Selection still materializes only the focused/global active loaded tab for toolbar parity, but the visibility gate now also keeps any other rendered Browser leaf's already-created active loaded surface visible. Inactive restored loaded tabs without CEF entities remain placeholders instead of being created from render or visibility updates.
        */
        self.sync_active_browser_tab_surface(cx);
        self.sync_browser_address_input(window, cx);
    }

    /// The part of `sync_active_browser_tab_to_surface` that needs no `Window`.
    pub(crate) fn sync_active_browser_tab_surface(&mut self, cx: &mut gpui::Context<Self>) {
        let pane_id = self.browser_tabs.focused_pane;
        self.browser_url = self.browser_tabs.address_value_for_pane(pane_id);
        self.ensure_browser_surface_for_pane(pane_id, cx);
        self.update_active_mode_cef_child_visibility(cx);
    }

    pub(crate) fn sync_browser_address_input(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane_id = self.browser_tabs.focused_pane;
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        self.set_browser_address_input_value(pane_id, address_value, window, cx);
    }

    pub(crate) fn select_browser_tab_in_pane(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if self.browser_tabs.select_tab_in_pane(pane_id, tab_id) {
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_browser_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn focus_browser_pane(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return false;
        }
        if self.browser_tabs.focus_pane(pane_id) {
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_browser_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
            true
        } else {
            false
        }
    }

    /// CDXC:Browser 2026-09-24 DECISION:
    /// User: Cmd+N with the side panel closed must not open two tabs. A project whose Browser holds
    /// only the empty "New Tab" placeholder gets that placeholder loaded as its new tab instead of
    /// a second tab beside it; the placeholder used to survive next to the new page with no way
    /// to close it.
    pub(crate) fn add_browser_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if !self.seed_current_project_browser_tab_if_empty() {
            let default_url = browser_shell_default_url(
                self.latest_sidebar_project_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
            );
            let created_tab_id = self.browser_tabs.add_loaded_popup_tab(
                default_url.clone(),
                self.browser_profiles.active_profile_id(),
                cef::BrowserPopupPlacement::Selected,
            );
            if let Some(created_tab_id) = created_tab_id {
                self.assign_new_browser_tab_project_machine(created_tab_id);
                self.reveal_new_browser_tab(created_tab_id);
            }
            self.browser_url = default_url;
        }
        let pane_id = self.browser_tabs.focused_pane;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_focused_browser_pane_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn add_browser_tab_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusMode 2026-06-22-12:51:
        New Browser Tab is an explicit Browser-opening command in the GPUI shell. Switch to Browser before reusing the normal new-tab helper so the new address-only tab is inserted in the focused Browser pane, Browser lifecycle is marked awake, shell focus moves to Browser, address/CEF visibility sync runs, the active tab scrolls into view, and shell state persists.

        CDXC:Titlebar 2026-06-22-15:52:
        New Browser Tab must respect Quick/projectless titlebar availability before switching modes. Browser placeholder tabs stay part of durable shell state, but user-facing Browser creation commands cannot make Browser active when the native titlebar would show it disabled.

        CDXC:Hotkeys 2026-09-25 DECISION:
        User: Cmd+T always opens a new browser tab (Ctrl+T on Windows and Linux), so it works from the Commands pane and the Terminal view too. This supersedes the 2026-06-26 CommandPalette rule that made it a no-op while a command terminal had focus; New Terminal (Cmd+Shift+T) opens a terminal tab there.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
        self.add_browser_tab(window, cx);
    }

    pub(crate) fn add_browser_tab_in_pane_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if self.browser_tabs.focus_pane(pane_id) {
            self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
            self.add_browser_tab(window, cx);
        }
    }

    pub(crate) fn split_browser_pane_with_new_tab_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        zone: WorkspaceDropZone,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let default_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        if let Some(created_tab_id) = self.browser_tabs.split_new_loaded_tab_to_pane(
            pane_id,
            zone,
            self.browser_profiles.active_profile_id(),
            default_url.clone(),
        ) {
            self.assign_new_browser_tab_project_machine(created_tab_id);
            self.reveal_new_browser_tab(created_tab_id);
            self.browser_url = default_url;
            self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.focus_shell_target(
                ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
                cx,
            );
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_focused_browser_pane_active_tab();
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn close_browser_tab(
        &mut self,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.close_browser_tab_model(tab_id, window, cx) {
            return;
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn select_browser_tab_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.select_browser_tab_in_pane(pane_id, tab_id, window, cx);
    }

    pub(crate) fn close_browser_tab_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .browser_tabs
            .find_leaf(pane_id)
            .is_some_and(|leaf| leaf.tab_group.has_tab(tab_id))
        {
            self.close_browser_tab(tab_id, window, cx);
        }
    }

    pub(crate) fn close_browser_tab_model(
        &mut self,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.browser_tabs.tabs.iter().any(|tab| tab.id == tab_id) {
            return false;
        }
        let source_pane_id = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id);
        let closing_last_browser_tab = self.browser_tabs.tabs.len() == 1;
        // Both read the page's row before the tab leaves it.
        let closing_key = ViewStripTabKey::Browser(tab_id);
        let row = self.browser_pages_drawn_row();
        let successor = (!closing_last_browser_tab
            && self.browser_page_on_screen() == Some(tab_id))
        .then(|| self.view_strip_successor(&row, closing_key, |_| false))
        .flatten();
        let drawn_order = row
            .iter()
            .filter_map(|entry| match entry {
                ViewStripEntry::Browser(tab) => Some(tab.tab_id),
                ViewStripEntry::View(_) | ViewStripEntry::SideChat(_) => None,
            })
            .collect::<Vec<_>>();
        self.remove_browser_surface(tab_id, cx);
        self.forget_claude_cloud_tab(self.browser_tabs_project_id.as_deref(), tab_id);
        self.browser_find_states.remove(&tab_id);
        self.browser_find_inputs.remove(&tab_id);
        self.browser_find_input_subscriptions.remove(&tab_id);
        if self.pending_browser_find_focus == Some(tab_id) {
            self.pending_browser_find_focus = None;
        }
        if !self.browser_tabs.close_tab(
            tab_id,
            self.browser_profiles.active_profile_id(),
            &drawn_order,
        ) {
            return false;
        }
        self.reconcile_browser_address_inputs();
        // CDXC:Browser 2026-09-21 DECISION:
        // User: closing the last browser tab closes the Browser view, the way closing any view's tab
        // does, so the panel moves on to the neighbouring open view and, when Browser was the last
        // one, goes back to the view picker (the 2026-09-22 side panel rule). Supersedes the
        // 2026-09-08 rule that it always switched back to Agents, which closed the whole panel even
        // with Docs or Code still open beside it. The tab model retains
        // an address-only placeholder, so count tabs before closing; New Browser Tab reopens from it.
        if closing_last_browser_tab {
            self.close_view_tab(TitlebarMode::Browser, window, cx);
            return true;
        }
        // The page on screen closed: the next tab is a view, or a page in this or another pane.
        if let Some(successor) = successor {
            self.show_view_strip_entry(successor, window, cx);
            return true;
        }
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.focus_shell_target(
            ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
            cx,
        );
        self.sync_active_browser_tab_to_surface(window, cx);
        if let Some(source_pane_id) = source_pane_id {
            self.scroll_browser_pane_active_tab(source_pane_id);
        }
        self.scroll_focused_browser_pane_active_tab();
        true
    }

    /// Returns the new tab when it is the one on screen now (not a background tab).
    pub(crate) fn open_browser_popup_tab(
        &mut self,
        requested_url: String,
        remote_machine_id: Option<String>,
        placement: cef::BrowserPopupPlacement,
        source_tab: Option<BrowserTabId>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<BrowserTabId> {
        /*
        CDXC:Browser 2026-06-22-07:14:
        CEF popup callbacks enter the shell outside normal GPUI mouse/key handlers, so handle them as Browser tab model mutations: create a selected tab, switch to Browser mode, show only that tab's CEF surface, update the address field, and persist through the existing sanitized Browser shell metadata path.

        CDXC:Browser 2026-06-23-11:43:
        Empty-target CEF popups must return handled with no shell side effects: no selected-tab mutation, Browser wake/focus, CEF surface sync/creation, shell-state persistence, notification, or fallback content transfer. Non-empty target URLs still follow the selected-tab Browser activation path.

        CDXC:Titlebar 2026-06-22-15:52:
        Popup callbacks are another Browser activation route. If Quick/projectless context has disabled Browser, discard the popup request before mutating Browser tab state so background CEF callbacks cannot bypass the titlebar guard.

        CDXC:Browser 2026-08-18:
        Middle-click and Cmd/Ctrl-click link opens arrive here with background
        placement. They are not an activation route: append the tab, persist,
        and repaint the tab strip without switching modes, moving shell focus,
        selecting the new tab, or scrolling the strip away from the page the
        user is still reading.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return None;
        }
        // The Browser tab the popup came from: a popup from a Claude account's claude.ai page (its
        // Google sign-in) stays in that account's profile. Code and website views have no Browser
        // tab, so their links never take on the account of whichever tab is selected.
        let source_tab = source_tab
            .and_then(|id| self.browser_tabs.tab(id))
            .map(|tab| (tab.id, tab.url.clone()));
        let popup_tab_id = self.browser_tabs.open_loaded_popup_tab(
            requested_url,
            self.browser_profiles.active_profile_id(),
            placement,
        )?;
        if let Some((source_id, source_url)) = source_tab {
            self.inherit_claude_cloud_tab_profile(source_id, &source_url, popup_tab_id);
        }
        if let Some(tab) = self
            .browser_tabs
            .tabs
            .iter_mut()
            .find(|tab| tab.id == popup_tab_id)
        {
            tab.remote_machine_id = remote_machine_id;
        }
        // A website view can open the project's first Browser tab without activating Browser.
        self.record_open_view_tab(TitlebarMode::Browser);
        self.reveal_new_browser_tab(popup_tab_id);
        if matches!(placement, cef::BrowserPopupPlacement::Background) {
            /*
            CDXC:Browser 2026-08-18:
            A background tab is the one open with no other visible feedback: the
            page does not change, so with the sidebar chrome collapsed the row it
            created is off screen entirely. Say so, instead of letting the click
            look like it did nothing. (The reveal itself is still queued above, so
            the sections are already expanded when the sidebar comes back.)
            */
            if !gpui_sidebar_chrome_visible(self.sidebar_collapsed) {
                self.upsert_gpui_app_toast(
                    GpuiAppToast {
                        copy_text: None,
                        id: "gpui-browser-tab-created-in-sidebar".to_string(),
                        level: GpuiAppToastLevel::from_raw(Some("info")),
                        title: "New tab created in sidebar".to_string(),
                        description: None,
                        loading: false,
                        persistent: false,
                        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                        action: None,
                        epoch: 0,
                    },
                    cx,
                );
            }
            self.persist_shell_layout_state();
            cx.notify();
            return None;
        }
        self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.focus_shell_target(
            ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
            cx,
        );
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_focused_browser_pane_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
        Some(popup_tab_id)
    }

    /// CDXC:Browser 2026-10-26 WHY:
    /// A page opened from another page (a link or an image that opens a new tab, `target=_blank`, `window.open` without popup features) goes back to that page when it is closed, but only while it has stayed the tab on screen since it opened; once any other tab has been shown, the opener included, the strip's neighbour rule applies (`view_strip_successor`). This is narrower than Chrome, which keeps the link while the user switches between the two pages and first tries pages the closed one opened, then pages its opener opened. Every route that shows another tab (a click, a hotkey, a view switch, a split pane or its address field, a project switch) notifies the app, because the strip redraws its selection, so one observer forgets the link the moment something else is on screen instead of each route having to remember to; a new route that selects without notifying would keep a stale link. Only a page's own popup makes the link (`browser_popup_open_handler`), not a History open, which passes its source tab for the profile alone.
    pub(crate) fn forget_browser_tab_opener_once_hidden(&mut self, cx: &mut gpui::Context<Self>) {
        cx.observe_self(|this, _cx| {
            if let Some(opener) = this.browser_tab_opener
                && this.browser_page_on_screen() != Some(opener.tab)
            {
                this.browser_tab_opener = None;
            }
        })
        .detach();
    }
}
