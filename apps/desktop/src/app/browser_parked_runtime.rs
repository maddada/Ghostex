// Cluster: per-project parking of the Browser workarea's live CEF runtime, and
// the project-aware routing every parked surface's async CEF callbacks use to
// reach the tab model that actually owns their tab id.

use crate::app::helpers::*;
use crate::app::model::*;
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
            parked_tabs.add_loaded_popup_tab(requested_url, active_profile_id, placement)
        else {
            return;
        };
        self.pending_sidebar_browser_tab_reveal = Some(PendingSidebarBrowserTabReveal {
            project_id: project_id.to_string(),
            tab_id: popup_tab_id,
        });
        self.persist_shell_layout_state();
        cx.notify();
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
        if !self.forget_parked_browser_tab_runtime(project_id, tab_id) {
            return;
        }
        cx.notify();
    }

    /// Close a tab of a parked project, including its parked page.
    pub(crate) fn close_parked_browser_tab(
        &mut self,
        project_id: &str,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.forget_parked_browser_tab_runtime(project_id, tab_id);
        let active_profile_id = self.browser_profiles.active_profile_id();
        let Some(parked_tabs) = self.parked_browser_tabs_by_project.get_mut(project_id) else {
            return;
        };
        if !parked_tabs.close_tab(tab_id, active_profile_id) {
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
