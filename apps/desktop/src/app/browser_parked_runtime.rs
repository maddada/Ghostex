// Cluster: per-project parking of the Browser workarea's live CEF runtime, and
// the project-aware routing every parked surface's async CEF callbacks use to
// reach the tab model that actually owns their tab id.

use ghostex_gx_core::ProjectKey;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::{GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastLevel};
use crate::*;

impl GhostexGpuiApp {
    /*
    CDXC:Browser 2026-08-26:
    An active-project switch parks the outgoing project's browser pages instead
    of destroying them, the same treatment the terminal and chat runtimes
    already get on that path. Destroying them closed every Chromium browser, so
    the sidebar showed every browser tab of the project the user just left as
    slept and the pages reloaded from scratch on the way back.

    BrowserTabId and BrowserPaneId are project-local counters, so every map
    keyed by them leaves together in one bundle: leaving one behind would let
    the incoming project's colliding ids read the outgoing project's find query,
    address text, or editing state. Each parked surface is hidden, never closed;
    the visibility gate shows it again when its project is mounted.
    */
    pub(crate) fn park_all_browser_surfaces(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> ParkedBrowserRuntime {
        for surface in self.browser_surfaces.values() {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
        ParkedBrowserRuntime {
            runtime_key: self.browser_tabs_runtime_key,
            surfaces: std::mem::take(&mut self.browser_surfaces),
            address_inputs: std::mem::take(&mut self.browser_address_inputs),
            address_input_subscriptions: std::mem::take(
                &mut self.browser_address_input_subscriptions,
            ),
            address_input_editing: std::mem::take(&mut self.browser_address_input_editing),
            find_states: std::mem::take(&mut self.browser_find_states),
            find_inputs: std::mem::take(&mut self.browser_find_inputs),
            find_input_subscriptions: std::mem::take(&mut self.browser_find_input_subscriptions),
        }
    }

    /// Reinstall a project's parked browser pages as the live ones. The caller
    /// has already restored that project's `BrowserTabModel`, so the restored
    /// surfaces match live tab ids and the normal visibility gate shows the
    /// rendered ones again without creating a browser.
    pub(crate) fn restore_parked_browser_surfaces(&mut self, parked: ParkedBrowserRuntime) {
        self.browser_tabs_runtime_key = parked.runtime_key;
        self.browser_surfaces = parked.surfaces;
        self.browser_address_inputs = parked.address_inputs;
        self.browser_address_input_subscriptions = parked.address_input_subscriptions;
        self.browser_address_input_editing = parked.address_input_editing;
        self.browser_find_states = parked.find_states;
        self.browser_find_inputs = parked.find_inputs;
        self.browser_find_input_subscriptions = parked.find_input_subscriptions;
    }

    /// Tear down the live browser runtime for good. Only the projectless
    /// pre-project tab model takes this path on a switch: it has no project key
    /// to park under, so its model is dropped and its pages go with it.
    pub(crate) fn drop_all_browser_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        for surface in self.browser_surfaces.values() {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
        self.browser_surfaces.clear();
        self.browser_address_inputs.clear();
        self.browser_address_input_subscriptions.clear();
        self.browser_address_input_editing.clear();
        self.browser_find_states.clear();
        self.browser_find_inputs.clear();
        self.browser_find_input_subscriptions.clear();
    }

    /// Which tab model a surface created under `runtime_key` belongs to now.
    /// `None` means the model is gone (the projectless model was dropped, or
    /// the parked project was torn down), so its late callbacks are dropped.
    pub(crate) fn browser_runtime_owner_for_key(
        &self,
        runtime_key: u64,
    ) -> Option<BrowserRuntimeOwner> {
        if self.browser_tabs_runtime_key == runtime_key {
            return Some(BrowserRuntimeOwner::Live);
        }
        self.parked_browser_runtimes_by_project
            .iter()
            .find(|(_, runtime)| runtime.runtime_key == runtime_key)
            .map(|(project_id, _)| BrowserRuntimeOwner::Parked(project_id.clone()))
    }

    /// Every parked project's live pages, for accounting and bulk sleep. Never
    /// for visibility, focus, or input routing: those are live-project only,
    /// because the ids in this count collide across projects.
    pub(crate) fn parked_browser_surface_count(&self) -> usize {
        self.parked_browser_runtimes_by_project
            .values()
            .map(|runtime| runtime.surfaces.len())
            .sum()
    }

    /*
    CDXC:Browser 2026-08-26:
    A parked page keeps navigating, so its title, favicon, address, load state,
    and find results must keep landing on the model that owns it. Applying them
    to the live model instead would corrupt whichever tab happens to share the
    id, and dropping them would leave a restored tab permanently stale, so the
    parked model is edited in place and persisted through the same shell-state
    writer that already owns parked tab models. Nothing here activates Browser
    mode, moves focus, or touches CEF visibility: the project is not mounted.
    */
    pub(crate) fn handle_parked_browser_page_metadata_event(
        &mut self,
        project_id: &str,
        tab_id: BrowserTabId,
        event: cef::BrowserPageMetadataEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            cef::BrowserPageMetadataEvent::HistoryRequested
            | cef::BrowserPageMetadataEvent::FindRequested => {}
            cef::BrowserPageMetadataEvent::AddressChanged(url) => {
                let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id)
                else {
                    return;
                };
                if !parked_tabs.record_page_address_change(tab_id, url) {
                    return;
                }
                self.persist_shell_layout_state();
                cx.notify();
            }
            cef::BrowserPageMetadataEvent::CloseRequested => {
                self.close_parked_browser_tab(project_id, tab_id, cx);
            }
            cef::BrowserPageMetadataEvent::CopyToClipboard(item) => {
                gpui_copy_to_clipboard(item, cx);
            }
            cef::BrowserPageMetadataEvent::TitleChanged(title) => {
                let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id)
                else {
                    return;
                };
                if parked_tabs.record_page_title_change(tab_id, title) {
                    cx.notify();
                }
            }
            cef::BrowserPageMetadataEvent::FaviconUrlChanged(favicon_url) => {
                let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id)
                else {
                    return;
                };
                if parked_tabs.record_page_favicon_url_change(tab_id, favicon_url) {
                    cx.notify();
                }
            }
            cef::BrowserPageMetadataEvent::FindResult {
                match_count,
                active_match_ordinal,
                final_update,
            } => {
                let Some(find) = self
                    .parked_browser_runtimes_by_project
                    .get_mut(project_id)
                    .and_then(|runtime| runtime.find_states.get_mut(&tab_id))
                else {
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
                let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id)
                else {
                    return;
                };
                if parked_tabs.record_page_loading_state_change(
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

    /*
    CDXC:Browser 2026-08-26:
    A parked page can still open a popup (a link target, a `window.open`). The
    tab belongs to the project that opened it, so it is appended to that
    project's parked model and queued for the sidebar reveal there. This is not
    an activation route: it never switches the workarea to the popup's project,
    moves shell focus, or creates a surface, and it stays behind the same
    Settings refusal as every other embedded-browser open.
    */
    pub(crate) fn open_parked_browser_popup_tab(
        &mut self,
        project_id: &str,
        requested_url: String,
        remote_machine_id: Option<String>,
        placement: cef::BrowserPopupPlacement,
        cx: &mut gpui::Context<Self>,
    ) {
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Browser) {
            return;
        }
        let active_profile_id = self.browser_profiles.active_profile_id();
        let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id) else {
            return;
        };
        let Some(popup_tab_id) =
            parked_tabs.open_loaded_popup_tab(requested_url, active_profile_id, placement)
        else {
            return;
        };
        if let Some(tab) = parked_tabs
            .tabs
            .iter_mut()
            .find(|tab| tab.id == popup_tab_id)
        {
            tab.remote_machine_id = remote_machine_id;
        }
        if let Some(state) = self.project_view_states_by_project.get_mut(project_id)
            && !state.open_views.contains(&TitlebarMode::Browser)
        {
            state.open_views.push(TitlebarMode::Browser);
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// CDXC:Browser 2026-10-26 WHY:
    /// `ghostex browser open` names the agent's own project, which is often not the one on screen. Such a page goes into that project's parked tab model the way a parked page's popup does: the project on screen keeps its panel, view and tabs, and focus stays put.
    /// The project's saved view moves to Browser, as the open would have done on screen, a toast says where the page went, and the page is shown when the project is next on screen (`land_pending_parked_browser_open`).
    /// The page itself is made at once, hidden, under the Browser profile of the project's own workspace (`start_parked_browser_page`), so the agent's next step (`ghostex_list_pages`, a snapshot, a click) finds a page to drive. This supersedes the 2026-10-25 rule that made it only when the project came on screen, which left the embedded-browser-use loop with no page. A reused tab whose page is already running parked goes to the address at once, as a live one would.
    pub(crate) fn open_browser_url_in_parked_project(
        &mut self,
        project_id: &str,
        url: String,
        reuse: GpuiBrowserRendererOpenReuse,
        remote_machine_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_profile_id = self.browser_profiles.active_profile_id();
        let parked_tabs = self
            .parked_browser_tabs_by_project
            .entry(project_id.to_string())
            .or_insert_with(|| BrowserTabModel::shell_address_only_with_profile(active_profile_id));
        let (tab_id, new_tab) = match parked_tabs.find_renderer_open_reuse_tab(
            &url,
            reuse,
            remote_machine_id.as_deref(),
        ) {
            Some((pane_id, tab_id)) => {
                parked_tabs.select_tab_in_pane(pane_id, tab_id);
                parked_tabs.load_pane_active_tab_url(pane_id, url.clone());
                // A parked page still runs, so it goes to the address as a live one would.
                if let Some(surface) = self
                    .parked_browser_runtimes_by_project
                    .get(project_id)
                    .and_then(|runtime| runtime.surfaces.get(&tab_id))
                {
                    surface.update(cx, |surface, _| surface.load_url(&url));
                }
                (tab_id, false)
            }
            None => {
                let Some(tab_id) = parked_tabs.open_loaded_popup_tab(
                    url,
                    active_profile_id,
                    cef::BrowserPopupPlacement::Selected,
                ) else {
                    return;
                };
                if let Some(tab) = parked_tabs.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    tab.remote_machine_id = remote_machine_id;
                }
                (tab_id, true)
            }
        };
        let page_title = parked_tabs.tab(tab_id).map(|tab| tab.title.clone());
        self.queue_parked_browser_tab(project_id, tab_id, new_tab);
        if !self.start_parked_browser_page(project_id, tab_id, cx) {
            self.waiting_parked_browser_pages
                .insert((project_id.to_string(), tab_id));
        }
        let project_title = self
            .parked_project_title(project_id)
            .unwrap_or_else(|| "another project".to_string());
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                // One toast per project, so a burst of pages updates it instead of stacking.
                id: format!("gpui-browser-opened-in-project-{project_id}"),
                level: GpuiAppToastLevel::from_raw(Some("info")),
                title: format!("Page opened in {project_title}"),
                description: page_title,
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
        self.persist_shell_layout_state();
        cx.notify();
    }

    fn parked_project_title(&self, project_id: &str) -> Option<String> {
        ProjectKey::parse_workspace_project_id(project_id)
            .and_then(|key| self.gx_store.core.presentation().project(&key))
            .map(|project| project.title.trim().to_string())
            .filter(|title| !title.is_empty())
    }

    /// Makes the page of a parked project's tab, hidden, through the same path a page on screen
    /// takes: the project's tab model and runtime key stand in for the live ones for the call, so
    /// the page's callbacks name that project's runtime (`browser_runtime_owner_for_key`) and the
    /// page is parked under it like one left behind by a project switch. It opens in the Browser
    /// profile of the project's own workspace, the one it gets when the project is next on screen
    /// (a project of another workspace switches the window to it). Nothing for a project on
    /// screen, a tab that already has a page, or one with no address. Returns false only for a page
    /// that cannot be made yet (CEF or the workspace profile still starting), which the caller
    /// leaves in `waiting_parked_browser_pages`.
    pub(crate) fn start_parked_browser_page(
        &mut self,
        project_id: &str,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.browser_tabs_project_id.as_deref() == Some(project_id)
            || self
                .parked_browser_runtimes_by_project
                .get(project_id)
                .is_some_and(|runtime| runtime.surfaces.contains_key(&tab_id))
        {
            return true;
        }
        let Some((url, profile_id)) = self
            .parked_browser_tabs_by_project
            .get(project_id)
            .and_then(|tabs| tabs.tab(tab_id))
            .filter(|tab| tab.state == BrowserTabState::Loaded)
            .map(|tab| (tab.cef_url(), tab.profile_id))
        else {
            return true;
        };
        let workspace_id = match self.gx_store_local_project_workspace_id(project_id) {
            Some(workspace_id) => self
                .gx_store_workspaces_state()
                .filter(|state| state.default_id() != workspace_id)
                .map(|_| workspace_id),
            None => self.gx_store_window_non_default_workspace_id(),
        };
        let profile = crate::app::workspace_browser::browser_cef_profile_for_workspace(
            profile_id,
            workspace_id.as_deref(),
        );
        let project_name = self
            .parked_project_title(project_id)
            .unwrap_or_else(|| self.project_name.clone());
        let Some(tabs) = self.parked_browser_tabs_by_project.remove(project_id) else {
            return true;
        };
        let mut runtime = self
            .parked_browser_runtimes_by_project
            .remove(project_id)
            .unwrap_or_else(|| {
                // A fresh identity, as a project switch gives a model with no parked runtime.
                self.browser_tabs_project_epoch = self.browser_tabs_project_epoch.wrapping_add(1);
                ParkedBrowserRuntime {
                    runtime_key: self.browser_tabs_project_epoch,
                    ..Default::default()
                }
            });
        let live_tabs = std::mem::replace(&mut self.browser_tabs, tabs);
        let live_runtime_key =
            std::mem::replace(&mut self.browser_tabs_runtime_key, runtime.runtime_key);
        let live_surfaces = std::mem::replace(
            &mut self.browser_surfaces,
            std::mem::take(&mut runtime.surfaces),
        );
        // A parked page's history entries carry the project name it was made under.
        let live_project_name = std::mem::replace(&mut self.project_name, project_name);
        let surface = self.ensure_browser_surface_for_tab_in_profile(
            tab_id,
            url,
            profile_id,
            Some(profile),
            cx,
        );
        runtime.surfaces = std::mem::replace(&mut self.browser_surfaces, live_surfaces);
        self.project_name = live_project_name;
        self.browser_tabs_runtime_key = live_runtime_key;
        let tabs = std::mem::replace(&mut self.browser_tabs, live_tabs);
        self.parked_browser_tabs_by_project
            .insert(project_id.to_string(), tabs);
        if runtime.holds_runtime_state() {
            self.parked_browser_runtimes_by_project
                .insert(project_id.to_string(), runtime);
        }
        let Some(surface) = surface else {
            return false;
        };
        self.size_background_browser_page(surface, cx);
        true
    }

    /// CDXC:Browser 2026-10-26 WHY:
    /// A CEF page is made 1×1 and only gets its frame when it is painted, so a page made in the background laid out in a 1px window and `ghostex_screenshot` returned a 2×2 blank image until the page had been shown once. It gets the size the Browser panel would give it instead: the side panel's width at its saved split and the window's height. The frame is set after this update, when the window can be read, and only while the page has still not been placed by a paint.
    fn size_background_browser_page(
        &mut self,
        surface: Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(window) = self.main_window_handle else {
            return;
        };
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = app.update(cx, |this, cx| {
                let Ok((bounds, scale_factor)) = window.update(cx, |_, window, _| {
                    (
                        this.background_browser_page_bounds(window),
                        window.scale_factor(),
                    )
                }) else {
                    return;
                };
                surface.read(cx).set_background_bounds(bounds, scale_factor);
            });
        });
    }

    /// Where the Browser panel would sit at rest in this window: right of the Agents column at the
    /// saved split, left of a right-docked command pane, the window's full height.
    fn background_browser_page_bounds(&self, window: &gpui::Window) -> Bounds<Pixels> {
        let window_size = window.bounds().size;
        let workspace_width = crate::app::model::command_pane_workspace_width(
            window,
            self.sidebar_width,
            self.sidebar_collapsed,
        );
        let (x, width) = background_browser_page_columns(
            window_size.width.as_f32(),
            workspace_width,
            self.workarea_header_trailing_dock_reserve_at_rest(window),
            self.project_editor_shell.workarea_split_ratio,
        );
        Bounds::new(
            gpui::point(px(x), px(0.0)),
            gpui::size(px(width), window_size.height),
        )
    }

    /// The pages opened for projects off screen that could not be made at the time
    /// (`waiting_parked_browser_pages`), made now: called when CEF has started and when a
    /// workspace profile's context is ready. A project that came on screen meanwhile gets the page
    /// in its live tabs, hidden unless its tab is in front; a tab closed or emptied since is
    /// dropped.
    pub(crate) fn start_waiting_parked_browser_pages(&mut self, cx: &mut gpui::Context<Self>) {
        let waiting = self
            .waiting_parked_browser_pages
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for (project_id, tab_id) in waiting {
            let done = if self.browser_tabs_project_id.as_deref() == Some(project_id.as_str()) {
                match self
                    .browser_tabs
                    .tab(tab_id)
                    .filter(|tab| tab.state == BrowserTabState::Loaded)
                    .map(|tab| (tab.cef_url(), tab.profile_id))
                {
                    Some((url, profile_id)) => {
                        match self.ensure_browser_surface_for_tab(tab_id, url, profile_id, cx) {
                            Some(surface) => {
                                self.size_background_browser_page(surface, cx);
                                true
                            }
                            None => false,
                        }
                    }
                    None => true,
                }
            } else {
                self.start_parked_browser_page(&project_id, tab_id, cx)
            };
            if done {
                self.waiting_parked_browser_pages
                    .remove(&(project_id, tab_id));
            }
        }
    }

    /// `tab_id`, now in front of `project_id`'s parked tab model, is shown when the project is next
    /// on screen: the project's saved view comes back on Browser, and a `new_tab` goes last in its
    /// saved strip.
    fn queue_parked_browser_tab(&mut self, project_id: &str, tab_id: BrowserTabId, new_tab: bool) {
        self.pending_parked_browser_opens
            .insert(project_id.to_string(), tab_id);
        let workarea_split_ratio = self.project_editor_shell.workarea_split_ratio;
        let saved_view = self
            .project_view_states_by_project
            .entry(project_id.to_string())
            .or_insert_with(|| GpuiProjectViewState::closed(workarea_split_ratio));
        saved_view.show_browser();
        if new_tab && let Some(tabs) = self.parked_browser_tabs_by_project.get(project_id) {
            saved_view.append_browser_tab(tab_id, tabs);
        }
    }

    /// The tab `target` names (its `notification_target`) in this window, and the tab model that
    /// holds it now: the live one, or a parked project's. A tab whose page sleeps counts.
    pub(crate) fn browser_tab_of_notification(
        &self,
        target: uuid::Uuid,
    ) -> Option<(BrowserTabId, BrowserRuntimeOwner)> {
        let named = |tabs: &BrowserTabModel| {
            tabs.tabs
                .iter()
                .find(|tab| tab.notification_target == target)
                .map(|tab| tab.id)
        };
        if let Some(tab_id) = named(&self.browser_tabs) {
            return Some((tab_id, BrowserRuntimeOwner::Live));
        }
        self.parked_browser_tabs_by_project
            .iter()
            .find_map(|(project_id, tabs)| {
                named(tabs).map(|tab_id| (tab_id, BrowserRuntimeOwner::Parked(project_id.clone())))
            })
    }

    /// CDXC:Browser 2026-10-25 WHY:
    /// A click on a page's notification brings back the tab whose page posted it, wherever that tab is now: the window that holds it comes forward and enters the project's workspace when it shows another (the tab lives in that window, so the click is not sent to another one), a project that is not on screen comes on screen without choosing or starting a session (`gx_store_focus_project`), and its Browser shows that tab, waking its page if it slept (`land_pending_parked_browser_open`). A remote project comes back the same way, as with a session link (`gx_store_open_browser_url_in_project`), and the terminals its workspace shows reattach once its computer is connected (`attach_surfaced_remote_workspace_terminals`). The attach of a click on its header (`gx_store_attach_remote_group`) is not used: with no attachable session, or its computer not connected, it stops at a toast, so the page this window holds would not come back. Otherwise it attaches a session the click did not aim at and gives that session's terminal the keyboard. The notification names the tab by a random id that lives only for this run (`notification_target`), not by its address, project, tab number or page: opening the address in the project on screen added a copy of another project's page there; a project taken when the page was made can change (a page adopted by a project); a tab number is reused by the last tab's placeholder; a page is dropped when it sleeps while its tab stays; numbered ids repeat in the next run; and macOS drops a click whose id holds a `/`. A closed tab leaves nothing to bring back.
    pub(crate) fn show_browser_tab_of_notification(
        &mut self,
        target: uuid::Uuid,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((tab_id, owner)) = self.browser_tab_of_notification(target) else {
            return;
        };
        let tabs = match &owner {
            BrowserRuntimeOwner::Live => Some(&mut self.browser_tabs),
            BrowserRuntimeOwner::Parked(project_id) => {
                self.parked_browser_tabs_by_project.get_mut(project_id)
            }
        };
        let Some(tabs) = tabs else {
            return;
        };
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Browser) {
            if let Some(url) = tabs.tab(tab_id).map(|tab| tab.url.clone()) {
                self.copy_path_for_disabled_project_workarea(&url, "Browser", cx);
            }
            return;
        }
        let Some(pane_id) = find_browser_leaf_id_for_tab(&tabs.root, tab_id) else {
            return;
        };
        match owner {
            BrowserRuntimeOwner::Live => {
                self.open_browser_tab_from_view_strip(pane_id, tab_id, window, cx);
            }
            BrowserRuntimeOwner::Parked(project_id) => {
                tabs.select_tab_in_pane(pane_id, tab_id);
                // A project of another workspace: this window, which holds the tab, goes there.
                let switched = self.enter_workspace_of_project(&project_id, cx);
                self.queue_parked_browser_tab(&project_id, tab_id, false);
                self.persist_shell_layout_state();
                self.gx_store_focus_project(&project_id, cx);
                if switched {
                    self.settle_routed_window_workspace(cx);
                }
            }
        }
    }

    /// Shows the page opened for this project while it was not on screen, once its views and tabs
    /// are back (`apply_changed_active_project_snapshot`): Browser wakes, the page made in the
    /// background is shown (made now if it could not be then) and the strip scrolls to its tab. It runs after the other landings, so one that changed the view or
    /// the tab in front (a file opened in Code, a project header's New Browser Tab) wins.
    pub(crate) fn land_pending_parked_browser_open(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(tab_id) = self
            .browser_tabs_project_id
            .as_ref()
            .and_then(|project_id| self.pending_parked_browser_opens.remove(project_id))
        else {
            return;
        };
        if self.active_mode != TitlebarMode::Browser
            || self
                .browser_tabs
                .active_tab_id_for_pane(self.browser_tabs.focused_pane)
                != Some(tab_id)
        {
            return;
        }
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_focused_browser_pane_active_tab();
    }

    /// Explicit per-tab sleep for a tab whose project is parked. Dropping the
    /// parked surface closes that page exactly like live sleep does, while the
    /// tab itself stays in its project's model as a restored placeholder.
    pub(crate) fn sleep_parked_browser_tab(
        &mut self,
        project_id: &str,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) {
        let tab_is_parked = self
            .parked_browser_tabs_by_project
            .get(project_id)
            .is_some_and(|parked_tabs| {
                find_browser_leaf_id_for_tab(&parked_tabs.root, tab_id).is_some()
            });
        if !tab_is_parked {
            return;
        }
        // A page still waiting to be made is not made later: the sleep wins.
        self.waiting_parked_browser_pages
            .remove(&(project_id.to_string(), tab_id));
        if !self.forget_parked_browser_tab_runtime(project_id, tab_id) {
            return;
        }
        cx.notify();
    }

    /// Sleep every parked page of a project that is not mounted, one tab at a time through the
    /// per-tab path, so each tab stays in the project's model as a restored placeholder and the
    /// panes' address text survives. Its pages still waiting to be made
    /// (`waiting_parked_browser_pages`) are not made later either, also when it has no parked
    /// runtime; for the live Browser model's project (`browser_tabs_project_id`), which never has
    /// one, that is all this does. Returns whether any page was dropped.
    pub(crate) fn sleep_parked_browser_project(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.waiting_parked_browser_pages
            .retain(|(waiting, _)| waiting != project_id);
        let Some(runtime) = self.parked_browser_runtimes_by_project.get(project_id) else {
            return false;
        };
        let mut dropped = false;
        for tab_id in runtime.surface_tab_ids() {
            dropped |= self.forget_parked_browser_tab_runtime(project_id, tab_id);
        }
        if dropped {
            cx.notify();
        }
        dropped
    }

    /// Close a tab of a parked project, including its parked page.
    pub(crate) fn close_parked_browser_tab(
        &mut self,
        project_id: &str,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.forget_parked_browser_tab_runtime(project_id, tab_id);
        self.forget_claude_cloud_tab(Some(project_id), tab_id);
        if self.pending_parked_browser_opens.get(project_id) == Some(&tab_id) {
            self.pending_parked_browser_opens.remove(project_id);
        }
        self.waiting_parked_browser_pages
            .remove(&(project_id.to_string(), tab_id));
        let active_profile_id = self.browser_profiles.active_profile_id();
        let drawn_order = self
            .project_view_states_by_project
            .get(project_id)
            .zip(self.parked_browser_tabs_by_project.get(project_id))
            .map(|(state, tabs)| {
                state
                    .view_strip_layout
                    .in_drawn_order(tabs.tab_ids_by_pane(), false, |tab_id| {
                        ViewStripTabKey::Browser(*tab_id)
                    })
            })
            .unwrap_or_default();
        let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id) else {
            return;
        };
        if !parked_tabs.close_tab(tab_id, active_profile_id, &drawn_order) {
            return;
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// Drop one parked tab's page and its parked find state. Returns whether
    /// anything was actually dropped.
    fn forget_parked_browser_tab_runtime(
        &mut self,
        project_id: &str,
        tab_id: BrowserTabId,
    ) -> bool {
        let Some(runtime) = self.parked_browser_runtimes_by_project.get_mut(project_id) else {
            return false;
        };
        let dropped = runtime.forget_tab(tab_id);
        if !runtime.holds_runtime_state() {
            self.parked_browser_runtimes_by_project.remove(project_id);
        }
        dropped
    }
}

impl GhostexGpuiApp {
    /// CDXC:Browser 2026-10-26 WHY:
    /// A removed project, and a removed worktree (which is a project of its own), leaves its computer's presentation, and nothing else tells this window: its parked pages, the Browser pages an agent opened in the background included, kept running until the idle sleep. They close as soon as a live presentation of that computer no longer lists the project. A computer that is not live (starting, reconnecting, offline) never counts as having removed anything, so a gxserver restart closes nothing.
    pub(crate) fn close_parked_pages_of_removed_projects(&mut self, cx: &mut gpui::Context<Self>) {
        let mut candidates = self
            .parked_browser_runtimes_by_project
            .keys()
            .chain(self.pending_parked_browser_opens.keys())
            .chain(
                self.waiting_parked_browser_pages
                    .iter()
                    .map(|(project_id, _)| project_id),
            )
            .map(String::as_str)
            .collect::<Vec<_>>();
        candidates.extend(
            self.project_views
                .parked_views()
                .map(|(_, project_id, ..)| project_id),
        );
        candidates.extend(
            self.parked_project_workarea_surfaces
                .iter()
                .filter_map(|parked| parked.project_id.as_deref()),
        );
        if candidates.is_empty() {
            return;
        }
        let presentation = self.gx_store.core.presentation();
        let removed = removed_project_ids(candidates, |project_id| {
            let key = ProjectKey::parse_workspace_project_id(project_id)?;
            let machine = presentation.machine(&key.machine)?;
            if machine.connection().phase != ghostex_gx_core::ConnectionPhase::Live {
                return None;
            }
            let loaded = presentation.loaded_live(&key.machine)?;
            Some(loaded.project(&key.project_id).is_some())
        });
        if removed.is_empty() {
            return;
        }
        for project_id in &removed {
            self.pending_parked_browser_opens.remove(project_id);
            self.sleep_parked_browser_project(project_id, cx);
            let views = self
                .project_views
                .parked_views()
                .filter(|(_, owner, ..)| owner == project_id)
                .map(|(key, ..)| key.to_owned())
                .collect::<Vec<_>>();
            for key in views {
                self.project_views.close_parked_view(&key);
            }
            // Dropping the entity closes the page.
            self.parked_project_workarea_surfaces
                .retain(|parked| parked.project_id.as_deref() != Some(project_id.as_str()));
        }
        cx.notify();
    }
}

/// Where an open aimed at one project goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetedBrowserOpenRoute {
    /// The project is on screen: its live Browser.
    OnScreen,
    /// The project is off screen: its parked Browser (`open_browser_url_in_parked_project`).
    /// `realign_browser_model` when the live Browser model still holds that project: it is parked
    /// first, so the page goes into the project's one tab model rather than a second one the next
    /// project switch would overwrite.
    Background { realign_browser_model: bool },
}

/// Routes an open for `target` by the project on screen (`on_screen`, the Agents workspace's) and
/// the project the live Browser model holds (`browser_model`).
pub(crate) fn targeted_browser_open_route(
    target: &str,
    on_screen: Option<&str>,
    browser_model: Option<&str>,
) -> TargetedBrowserOpenRoute {
    if on_screen == Some(target) {
        return TargetedBrowserOpenRoute::OnScreen;
    }
    TargetedBrowserOpenRoute::Background {
        realign_browser_model: on_screen.is_some() && browser_model == Some(target),
    }
}

/// The candidates `listed` says their computer's live presentation no longer has, once each.
/// `listed` answers `None` when it cannot tell (the computer is not live).
fn removed_project_ids<'a>(
    candidates: impl IntoIterator<Item = &'a str>,
    listed: impl Fn(&str) -> Option<bool>,
) -> Vec<String> {
    let mut removed = candidates
        .into_iter()
        .filter(|project_id| listed(project_id) == Some(false))
        .map(str::to_string)
        .collect::<Vec<_>>();
    removed.sort();
    removed.dedup();
    removed
}

/// The x and width of the Browser panel at rest: what is left of the band (the workspace less a
/// right-docked command pane) after the split's rail and the Agents column at the saved ratio,
/// against the band's right edge.
fn background_browser_page_columns(
    window_width: f32,
    workspace_width: f32,
    trailing_reserve: f32,
    split_ratio: f32,
) -> (f32, f32) {
    let band = (workspace_width - trailing_reserve).max(0.0);
    let split_span = (band - crate::app::consts::WORKSPACE_SPLIT_HANDLE_THICKNESS).max(0.0);
    let agents =
        split_span * crate::app::model::workarea_split_ratio_for_span(split_ratio, split_span);
    let width = (split_span - agents).max(1.0);
    let x = (window_width - trailing_reserve - width).max(0.0);
    (x, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_for_the_project_on_screen_stays_on_screen() {
        assert_eq!(
            targeted_browser_open_route("P1", Some("P1"), Some("P1")),
            TargetedBrowserOpenRoute::OnScreen
        );
    }

    #[test]
    fn an_open_for_a_project_off_screen_goes_to_its_own_browser() {
        // The user is on P2; the agent of P1 opens a page.
        assert_eq!(
            targeted_browser_open_route("P1", Some("P2"), Some("P2")),
            TargetedBrowserOpenRoute::Background {
                realign_browser_model: false
            }
        );
        // The live Browser model still holds P1 while P2 is on screen: P1's model is parked first,
        // so the page does not go into a second P1 model.
        assert_eq!(
            targeted_browser_open_route("P1", Some("P2"), Some("P1")),
            TargetedBrowserOpenRoute::Background {
                realign_browser_model: true
            }
        );
    }

    #[test]
    fn a_project_the_live_presentation_dropped_is_removed_once() {
        let removed = removed_project_ids(["gone", "kept", "gone", "offline"], |id| match id {
            "gone" => Some(false),
            "kept" => Some(true),
            _ => None,
        });
        assert_eq!(removed, vec!["gone".to_string()]);
    }

    #[test]
    fn nothing_counts_as_removed_while_its_computer_is_not_live() {
        assert!(removed_project_ids(["a", "b"], |_| None).is_empty());
    }

    #[test]
    fn a_background_page_gets_the_panel_column_not_one_pixel() {
        // A 1600pt window, a 280pt sidebar and its divider, no right dock, an even split.
        let workspace = 1600.0 - 281.0;
        let (x, width) = background_browser_page_columns(1600.0, workspace, 0.0, 0.5);
        let span = workspace - crate::app::consts::WORKSPACE_SPLIT_HANDLE_THICKNESS;
        let agents = span * crate::app::model::workarea_split_ratio_for_span(0.5, span);
        assert!((width - (span - agents)).abs() < 0.01);
        assert!(width > 300.0);
        assert!((x + width - 1600.0).abs() < 0.01);
    }

    #[test]
    fn a_right_docked_command_pane_keeps_its_column() {
        let (x, width) = background_browser_page_columns(1600.0, 1319.0, 400.0, 0.5);
        assert!((x + width - 1200.0).abs() < 0.01);
        assert!(width >= 1.0);
    }
}
