// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: browser address bar, find bar, media permissions, and per-pane surfaces

use std::collections::HashSet;
use std::collections::VecDeque;
use std::rc::Rc;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::App;
use gpui::AppContext as _;
use gpui::Entity;
use gpui::Focusable as _;
use gpui::Window;
use gpui::rgb;
use gpui_component::input::InputEvent;
use gpui_component::input::InputState;

use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn reconcile_browser_address_inputs(&mut self) {
        let pane_ids = self
            .browser_tabs
            .rendered_leaf_order()
            .into_iter()
            .collect::<HashSet<_>>();
        self.browser_address_inputs
            .retain(|pane_id, _| pane_ids.contains(pane_id));
        self.browser_address_input_subscriptions
            .retain(|pane_id, _| pane_ids.contains(pane_id));
        self.browser_address_input_editing
            .retain(|pane_id| pane_ids.contains(pane_id));
        if self
            .pending_browser_address_focus
            .is_some_and(|pane_id| !pane_ids.contains(&pane_id))
        {
            self.pending_browser_address_focus = None;
        }
        if self
            .pending_browser_content_focus
            .is_some_and(|pane_id| !pane_ids.contains(&pane_id))
        {
            self.pending_browser_content_focus = None;
        }
    }

    pub(crate) fn sync_browser_address_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane_ids = self.browser_tabs.rendered_leaf_order();
        let pane_id_set = pane_ids.iter().copied().collect::<HashSet<_>>();
        self.browser_address_inputs
            .retain(|pane_id, _| pane_id_set.contains(pane_id));
        self.browser_address_input_subscriptions
            .retain(|pane_id, _| pane_id_set.contains(pane_id));
        self.browser_address_input_editing
            .retain(|pane_id| pane_id_set.contains(pane_id));
        if self
            .pending_browser_address_focus
            .is_some_and(|pane_id| !pane_id_set.contains(&pane_id))
        {
            self.pending_browser_address_focus = None;
        }
        if self
            .pending_browser_content_focus
            .is_some_and(|pane_id| !pane_id_set.contains(&pane_id))
        {
            self.pending_browser_content_focus = None;
        }

        for pane_id in pane_ids {
            let address_value = self.browser_tabs.address_value_for_pane(pane_id);
            if self.browser_address_inputs.contains_key(&pane_id) {
                if !self.browser_address_input_editing.contains(&pane_id) {
                    self.set_browser_address_input_value_unchecked(
                        pane_id,
                        address_value,
                        window,
                        cx,
                    );
                }
                continue;
            }
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Search or enter address")
                    .default_value(&address_value)
            });
            let subscription = cx.subscribe(
                &input,
                move |this: &mut Self, input, event: &InputEvent, cx| match event {
                    InputEvent::Focus | InputEvent::Change => {
                        this.browser_address_input_editing.insert(pane_id);
                    }
                    InputEvent::Blur => {
                        this.browser_address_input_editing.remove(&pane_id);
                    }
                    InputEvent::PressEnter { .. } => {
                        this.browser_address_input_editing.remove(&pane_id);
                        if let Some(url) = normalize_address(input.read(cx).value().as_ref()) {
                            this.commit_browser_address_for_pane(pane_id, url, cx);
                        } else {
                            this.request_browser_content_focus(pane_id);
                            cx.notify();
                        }
                    }
                },
            );
            self.browser_address_inputs.insert(pane_id, input);
            self.browser_address_input_subscriptions
                .insert(pane_id, subscription);
        }

        self.drain_pending_browser_address_focus(window, cx);
        self.drain_pending_browser_content_focus(window, cx);
    }

    pub(crate) fn start_find_in_focused_browser(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let ShellFocusTarget::BrowserPane(pane_id) = self.shell_focus else {
            return false;
        };
        if self.active_mode != TitlebarMode::Browser {
            return false;
        }
        let Some((tab_id, _, _)) = self.active_loaded_browser_tab_for_pane(pane_id) else {
            return false;
        };
        self.ensure_browser_surface_for_pane(pane_id, cx);
        self.browser_find_states.entry(tab_id).or_default();
        self.pending_browser_address_focus = None;
        self.pending_browser_content_focus = None;
        self.pending_browser_find_focus = Some(tab_id);
        self.sync_browser_find_inputs(window, cx);
        cx.notify();
        true
    }

    pub(crate) fn perform_browser_find_navigation(
        &mut self,
        tab_id: BrowserTabId,
        forward: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(query) = self
            .browser_find_states
            .get(&tab_id)
            .map(|find| find.query.trim().to_string())
            .filter(|query| !query.is_empty())
        else {
            return false;
        };
        let Some(surface) = self.browser_surfaces.get(&tab_id).cloned() else {
            return false;
        };
        surface.update(cx, |surface, _| {
            surface.find_text(&query, forward, true);
        });
        true
    }

    pub(crate) fn handle_browser_find_input_event(
        &mut self,
        tab_id: BrowserTabId,
        input: &Entity<InputState>,
        event: &InputEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let query = input.read(cx).value().to_string();
                let Some(find) = self.browser_find_states.get_mut(&tab_id) else {
                    return;
                };
                find.query = query.clone();
                find.match_count = 0;
                find.active_match_ordinal = 0;
                find.final_update = false;
                if let Some(surface) = self.browser_surfaces.get(&tab_id).cloned() {
                    surface.update(cx, |surface, _| {
                        if query.trim().is_empty() {
                            surface.stop_finding(true);
                        } else {
                            surface.find_text(&query, true, false);
                        }
                    });
                }
                cx.notify();
            }
            InputEvent::PressEnter { shift, .. } => {
                let _ = self.perform_browser_find_navigation(tab_id, !*shift, cx);
            }
            InputEvent::Focus | InputEvent::Blur => {}
        }
    }

    pub(crate) fn sync_browser_find_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let live_tab_ids = self
            .browser_tabs
            .tabs
            .iter()
            .map(|tab| tab.id)
            .collect::<HashSet<_>>();
        self.browser_find_states
            .retain(|tab_id, _| live_tab_ids.contains(tab_id));
        self.browser_find_inputs
            .retain(|tab_id, _| self.browser_find_states.contains_key(tab_id));
        self.browser_find_input_subscriptions
            .retain(|tab_id, _| self.browser_find_states.contains_key(tab_id));

        let active_finds = self
            .browser_find_states
            .iter()
            .map(|(tab_id, find)| (*tab_id, find.query.clone()))
            .collect::<Vec<_>>();
        for (tab_id, query) in active_finds {
            if self.browser_find_inputs.contains_key(&tab_id) {
                continue;
            }
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Find in page")
                    .default_value(&query)
            });
            let subscription = cx.subscribe(
                &input,
                move |this: &mut Self, input, event: &InputEvent, cx| {
                    this.handle_browser_find_input_event(tab_id, &input, event, cx);
                },
            );
            self.browser_find_inputs.insert(tab_id, input);
            self.browser_find_input_subscriptions
                .insert(tab_id, subscription);
        }

        if let Some(tab_id) = self.pending_browser_find_focus.take()
            && let Some(input) = self.browser_find_inputs.get(&tab_id).cloned()
        {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            cef::focus_gpui_root_view(self.parent_ns_view);
            input.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    pub(crate) fn close_browser_find(
        &mut self,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = self.browser_surfaces.get(&tab_id).cloned() {
            surface.update(cx, |surface, _| surface.stop_finding(true));
        }
        self.browser_find_states.remove(&tab_id);
        self.browser_find_inputs.remove(&tab_id);
        self.browser_find_input_subscriptions.remove(&tab_id);
        if self.pending_browser_find_focus == Some(tab_id) {
            self.pending_browser_find_focus = None;
        }
        if let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id) {
            self.focus_browser_content_for_pane(pane_id, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn browser_find_input_owns_keyboard_focus(&self, window: &Window, cx: &App) -> bool {
        self.browser_find_inputs
            .values()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(crate) fn perform_focused_cef_pane_zoom(
        &mut self,
        command: GpuiFocusedSurfaceZoomCommand,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUICefPaneZoomShortcuts 2026-07-14:
        Cmd+=, Cmd+-, and Cmd+0 belong to the exact focused main CEF pane,
        not only Browser tabs. Source, Kanban, Automate, and Docs use the
        same Chromium host zoom API as Browser while sidebar, modal,
        titlebar, and companion CEF surfaces remain outside this command.
        Resolve only from shell focus plus the active mode and an existing
        owned surface; never infer a target or create a fallback surface.
        */
        let surface = match self.shell_focus {
            ShellFocusTarget::BrowserPane(pane_id) if self.active_mode == TitlebarMode::Browser => {
                self.browser_surface_for_pane(pane_id)
            }
            ShellFocusTarget::ProjectEditorSurface(mode) if self.active_mode == mode => {
                let slot_key = match mode {
                    TitlebarMode::Source => ProjectWorkareaCefSurfaceSlotKey::Source,
                    TitlebarMode::Kanban => ProjectWorkareaCefSurfaceSlotKey::Kanban,
                    TitlebarMode::Automate => ProjectWorkareaCefSurfaceSlotKey::Automate,
                    TitlebarMode::Manage => ProjectWorkareaCefSurfaceSlotKey::Manage,
                    TitlebarMode::Extension(id) => ProjectWorkareaCefSurfaceSlotKey::Extension(id),
                    TitlebarMode::Agents | TitlebarMode::Browser => return false,
                };
                self.project_workarea_runtime_cef_surface_for_render(slot_key)
            }
            _ => return false,
        };
        let Some(surface) = surface else {
            return false;
        };
        surface.update(cx, |surface, _| match command {
            GpuiFocusedSurfaceZoomCommand::In => surface.zoom_in(),
            GpuiFocusedSurfaceZoomCommand::Out => surface.zoom_out(),
            GpuiFocusedSurfaceZoomCommand::Reset => surface.reset_zoom(),
        });
        cx.notify();
        true
    }

    pub(crate) fn perform_focused_surface_zoom(
        &mut self,
        command: GpuiFocusedSurfaceZoomCommand,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        Focused-surface zoom keeps the existing CEF behavior and gives the
        composited terminal engine the same shortcuts. Terminal zoom is local
        to the exact focused live view: it does not persist Settings, recreate
        the process, or resize sibling terminals. Cmd+0 returns that view to
        the size derived from its most recently applied terminal config.
        */
        if self.perform_focused_cef_pane_zoom(command, cx) {
            return true;
        }

        let Some(view) = self.focused_gpui_engine_terminal_view() else {
            return false;
        };
        view.update(cx, |view, cx| {
            let changed = match command {
                GpuiFocusedSurfaceZoomCommand::In => view.adjust_font_size(
                    TERMINAL_FONT_ZOOM_STEP,
                    shared_settings::MIN_TERMINAL_FONT_SIZE,
                    shared_settings::MAX_TERMINAL_FONT_SIZE,
                ),
                GpuiFocusedSurfaceZoomCommand::Out => view.adjust_font_size(
                    -TERMINAL_FONT_ZOOM_STEP,
                    shared_settings::MIN_TERMINAL_FONT_SIZE,
                    shared_settings::MAX_TERMINAL_FONT_SIZE,
                ),
                GpuiFocusedSurfaceZoomCommand::Reset => view.reset_font_size(),
            };
            if changed {
                cx.notify();
            }
        });
        true
    }

    pub(crate) fn set_browser_address_input_value(
        &mut self,
        pane_id: BrowserPaneId,
        value: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_address_input_editing.contains(&pane_id) {
            return;
        }
        self.set_browser_address_input_value_unchecked(pane_id, value, window, cx);
    }

    pub(crate) fn set_browser_address_input_value_unchecked(
        &mut self,
        pane_id: BrowserPaneId,
        value: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(input) = self.browser_address_inputs.get(&pane_id).cloned() {
            let value_changed = input.read(cx).value().as_ref() != value.as_str();
            if value_changed {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
    }

    #[allow(dead_code)] // no caller: only the superseded native browser tab strip requested address-bar focus
    pub(crate) fn request_browser_address_focus(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.pending_browser_content_focus = None;
        self.pending_browser_address_focus = Some(pane_id);
        self.browser_address_input_editing.remove(&pane_id);
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        self.set_browser_address_input_value_unchecked(pane_id, address_value, window, cx);
        self.drain_pending_browser_address_focus(window, cx);
    }

    pub(crate) fn drain_pending_browser_address_focus(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pane_id) = self.pending_browser_address_focus else {
            return;
        };
        if self.focus_browser_address_input_for_pane(pane_id, window, cx) {
            self.pending_browser_address_focus = None;
        }
    }

    pub(crate) fn focus_browser_address_input_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.titlebar_mode_available(TitlebarMode::Browser)
            || !self.browser_tabs.focus_pane(pane_id)
        {
            return false;
        }
        let Some(input) = self.browser_address_inputs.get(&pane_id).cloned() else {
            return false;
        };

        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        self.browser_address_input_editing.insert(pane_id);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        cef::focus_gpui_root_view(self.parent_ns_view);
        input.update(cx, |input, cx| input.focus(window, cx));
        true
    }

    pub(crate) fn request_browser_content_focus(&mut self, pane_id: BrowserPaneId) {
        self.pending_browser_address_focus = None;
        self.pending_browser_content_focus = Some(pane_id);
        self.browser_address_input_editing.remove(&pane_id);
    }

    pub(crate) fn drain_pending_browser_content_focus(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pane_id) = self.pending_browser_content_focus.take() else {
            return;
        };
        self.focus_browser_content_for_pane(pane_id, window, cx);
    }

    pub(crate) fn focus_browser_content_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.titlebar_mode_available(TitlebarMode::Browser)
            || !self.browser_tabs.focus_pane(pane_id)
        {
            return false;
        }

        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            let focus_handle = surface.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
            surface.update(cx, |surface, _| surface.focus());
        } else {
            window.blur();
        }
        true
    }

    pub(crate) fn cancel_browser_address_edit_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_tabs.find_leaf(pane_id).is_none() {
            return;
        }
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        self.browser_address_input_editing.remove(&pane_id);
        self.pending_browser_address_focus = None;
        self.set_browser_address_input_value_unchecked(pane_id, address_value, window, cx);
        self.focus_browser_content_for_pane(pane_id, window, cx);
        cx.notify();
    }

    pub(crate) fn commit_browser_address(&mut self, url: String, cx: &mut gpui::Context<Self>) {
        self.commit_browser_address_for_pane(self.browser_tabs.focused_pane, url, cx);
    }

    pub(crate) fn commit_browser_address_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        url: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        self.browser_url = url.clone();
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        /*
        CDXC:SidebarBrowserTabReveal 2026-08-18:
        Committing an address in an address-only "New Tab" is the moment that
        tab first becomes a sidebar row (placeholders are deliberately not
        projected as browser sessions), so it is a new tab as far as the sidebar
        is concerned and gets the same reveal as any other new tab. Navigating
        an already-loaded tab is not.
        */
        let was_address_only_tab = self
            .browser_tabs
            .active_tab_for_pane(pane_id)
            .is_some_and(|tab| tab.state != BrowserTabState::Loaded);
        let Some((committed_tab_id, _profile_id)) = self
            .browser_tabs
            .load_pane_active_tab_url(pane_id, url.clone())
        else {
            return;
        };
        if was_address_only_tab {
            self.request_sidebar_browser_tab_reveal(committed_tab_id);
        }
        self.browser_tabs.focus_pane(pane_id);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        self.load_browser_cef_url_for_pane(pane_id, &url, cx);
        self.request_browser_content_focus(pane_id);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn active_loaded_browser_tab_for_pane(
        &self,
        pane_id: BrowserPaneId,
    ) -> Option<(BrowserTabId, String, BrowserProfileId)> {
        self.browser_tabs
            .active_tab_for_pane(pane_id)
            .filter(|tab| tab.state == BrowserTabState::Loaded)
            .map(|tab| (tab.id, tab.cef_url(), tab.profile_id))
    }

    pub(crate) fn browser_surface_for_pane(
        &self,
        pane_id: BrowserPaneId,
    ) -> Option<Entity<CefSurface>> {
        if !self.browser_surfaces_may_be_visible() {
            return None;
        }

        self.browser_tabs
            .active_tab_for_pane(pane_id)
            .filter(|tab| tab.state == BrowserTabState::Loaded)
            .and_then(|tab| self.browser_surfaces.get(&tab.id).cloned())
    }

    pub(crate) fn browser_surfaces_may_be_visible(&self) -> bool {
        self.browser_runtime_lifecycle_input()
            .allows_cef_child_views()
    }

    pub(crate) fn browser_surface_for_rendered_leaf(
        &self,
        leaf: &BrowserLeaf,
    ) -> Option<Entity<CefSurface>> {
        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:55:
        Browser body rendering may attach an existing tab-owned CEF entity for every rendered split leaf whose active tab is loaded, but rendering must never materialize a restored or inactive tab. Missing loaded CEF entities return no surface so the body renderer can show the restored/sleeping placeholder card, address-only tabs stay blank, and Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hide every surface.
        */
        if !self.browser_surfaces_may_be_visible() {
            return None;
        }

        let tab_id = self.browser_tabs.active_loaded_tab_id_for_leaf(leaf)?;
        self.browser_surfaces.get(&tab_id).cloned()
    }

    pub(crate) fn browser_runtime_lifecycle_input(&self) -> BrowserRuntimeLifecycleInput {
        BrowserRuntimeLifecycleInput {
            active_mode: self.active_mode,
            browser_awake: self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser),
            browser_tab_drag_active: self.browser_tab_drag_active,
            command_tab_drag_active: self.command_tab_drag_active,
            workspace_tab_drag_active: self.workspace_tab_drag_active,
        }
    }

    pub(crate) fn ensure_browser_surface_for_tab(
        &mut self,
        tab_id: BrowserTabId,
        url: String,
        profile_id: BrowserProfileId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        if let Some(surface) = self.browser_surfaces.get(&tab_id) {
            return Some(surface.clone());
        }

        let parent_ns_view = self.parent_ns_view;
        let surface_id = format!("gpui-browser-tab-{}", tab_id.0);
        /*
        CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
        CEF Browser surfaces must be keyed by tab id for runtime ownership but created with the tab's generated Browser profile id. This lets existing loaded surfaces keep their profile after toolbar selection changes while future tabs/surfaces use the newly selected profile.
        */
        let profile = profile_id.cef_profile_string();
        let popup_open_handler = self.browser_popup_open_handler(cx);
        let page_metadata_handler = self.browser_page_metadata_handler(tab_id, cx);
        let media_access_handler = self.browser_media_access_handler(tab_id, profile_id, cx);
        let initially_visible = BrowserRuntimeSurfacePolicy::for_tab(
            self.browser_runtime_lifecycle_input(),
            self.browser_tabs.tab(tab_id).map(|tab| tab.state),
            self.browser_tabs
                .rendered_active_loaded_tab_ids()
                .contains(&tab_id),
            true,
        )
        .shows_cef_child_view();
        let surface = match CefSurface::try_new(
            surface_id,
            parent_ns_view,
            url,
            profile,
            CEF_DARK_PREPAINT_BACKGROUND_COLOR,
            true,
            rgb(0xFFFFFF).into(),
            None,
            initially_visible,
            Some(popup_open_handler),
            Some(page_metadata_handler),
            Some(media_access_handler),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            cx,
        ) {
            Ok(surface) => surface,
            Err(error) => {
                // Ensure-style reconcile: skip this pass, retried the next
                // time the tab needs a surface
                // (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "browserTab", "error": error }),
                );
                return None;
            }
        };
        self.browser_surfaces.insert(tab_id, surface.clone());
        Some(surface)
    }

    /*
    CDXC:GPUIBrowserProjectParking 2026-08-26:
    A surface's async CEF callbacks carry the runtime key of the browser tab
    model that was live when the surface was created, because BrowserTabIds are
    project-local and would otherwise land on whichever project holds the same
    id when the callback arrives. The key resolves to that exact model whether
    it is still mounted or parked behind another project, so a parked page keeps
    updating its own tabs instead of being permanently invalidated (which would
    leave restored tabs with a dead title, favicon, and load state forever).
    */
    pub(crate) fn browser_popup_open_handler(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> cef::BrowserPopupOpenHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        let runtime_key = self.browser_tabs_runtime_key;

        Rc::new(
            move |requested_url: String, placement: cef::BrowserPopupPlacement| {
                let app = app.clone();
                let mut async_cx = async_cx.clone();
                foreground
                    .spawn(async move {
                        let _ = app.update_in(&mut async_cx, |this, window, cx| {
                            match this.browser_runtime_owner_for_key(runtime_key) {
                                Some(BrowserRuntimeOwner::Live) => {
                                    this.open_browser_popup_tab(
                                        requested_url,
                                        placement,
                                        window,
                                        cx,
                                    );
                                }
                                Some(BrowserRuntimeOwner::Parked(project_id)) => {
                                    this.open_parked_browser_popup_tab(
                                        &project_id,
                                        requested_url,
                                        placement,
                                        cx,
                                    );
                                }
                                None => {}
                            }
                        });
                    })
                    .detach();
            },
        )
    }

    pub(crate) fn browser_page_metadata_handler(
        &self,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) -> cef::BrowserPageMetadataHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        let runtime_key = self.browser_tabs_runtime_key;

        Rc::new(move |event: cef::BrowserPageMetadataEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        match this.browser_runtime_owner_for_key(runtime_key) {
                            Some(BrowserRuntimeOwner::Live) => {
                                this.handle_browser_page_metadata_event(tab_id, event, window, cx);
                            }
                            Some(BrowserRuntimeOwner::Parked(project_id)) => {
                                this.handle_parked_browser_page_metadata_event(
                                    &project_id,
                                    tab_id,
                                    event,
                                    cx,
                                );
                            }
                            None => {}
                        }
                    });
                })
                .detach();
        })
    }

    pub(crate) fn browser_media_access_handler(
        &self,
        tab_id: BrowserTabId,
        profile_id: BrowserProfileId,
        cx: &mut gpui::Context<Self>,
    ) -> cef::BrowserMediaAccessHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        let runtime_key = self.browser_tabs_runtime_key;

        Rc::new(move |request: cef::BrowserMediaAccessRequest| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    /*
                    An unanswered request cancels itself when dropped, so a
                    request from a page whose project is not mounted, or a gone
                    app entity, releases the page's `getUserMedia()` promise
                    instead of hanging it. A camera/microphone prompt is an
                    answer the user gives in the Browser workarea, so it is only
                    raised for the project that owns that workarea right now.
                    */
                    let _ = app.update(&mut async_cx, |this, cx| {
                        if !matches!(
                            this.browser_runtime_owner_for_key(runtime_key),
                            Some(BrowserRuntimeOwner::Live)
                        ) {
                            return;
                        }
                        this.handle_browser_media_access_request(tab_id, profile_id, request, cx);
                    });
                })
                .detach();
        })
    }

    pub(crate) fn handle_browser_media_access_request(
        &mut self,
        tab_id: BrowserTabId,
        profile_id: BrowserProfileId,
        request: cef::BrowserMediaAccessRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let kinds = request.kinds();
        let Some(origin) = gpui_browser_media_permission_origin(request.requesting_origin()) else {
            request.deny();
            return;
        };
        let decision = self
            .browser_media_permission_decisions
            .decision(profile_id, &origin);
        let undecided = decision.undecided(kinds);
        if undecided.is_empty() {
            request.allow(decision.granted(kinds));
            return;
        }

        self.browser_media_permission_prompts
            .entry(tab_id)
            .or_default()
            .push_back(GpuiBrowserMediaPermissionPrompt {
                profile_id,
                origin,
                requested: kinds,
                pending: undecided,
                request,
            });
        cx.notify();
    }

    pub(crate) fn resolve_browser_media_permission_prompt(
        &mut self,
        tab_id: BrowserTabId,
        allow: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(queue) = self.browser_media_permission_prompts.get_mut(&tab_id) else {
            return;
        };
        let Some(prompt) = queue.pop_front() else {
            return;
        };
        if queue.is_empty() {
            self.browser_media_permission_prompts.remove(&tab_id);
        }

        /*
        Only the devices this prompt asked about are recorded, so answering a
        camera prompt never rewrites an earlier microphone answer. The grant
        then reads back the full stored decision, which is what lets a
        combined request use a device allowed by an earlier prompt.
        */
        self.browser_media_permission_decisions.record(
            prompt.profile_id,
            &prompt.origin,
            prompt.pending,
            allow,
        );
        persist_gpui_browser_media_permission_decisions(&self.browser_media_permission_decisions);
        let decision = self
            .browser_media_permission_decisions
            .decision(prompt.profile_id, &prompt.origin);
        prompt.request.allow(decision.granted(prompt.requested));
        // A page that asked twice before being answered (or that asked while
        // another prompt was up) is now covered by the stored answer, so those
        // queued prompts resolve silently instead of asking again.
        self.drain_decided_browser_media_permission_prompts(tab_id);
        cx.notify();
    }

    pub(crate) fn drain_decided_browser_media_permission_prompts(&mut self, tab_id: BrowserTabId) {
        let Some(queue) = self.browser_media_permission_prompts.get_mut(&tab_id) else {
            return;
        };
        let mut answered = Vec::new();
        while let Some(prompt) = queue.front() {
            let decision = self
                .browser_media_permission_decisions
                .decision(prompt.profile_id, &prompt.origin);
            if !decision.undecided(prompt.requested).is_empty() {
                break;
            }
            let Some(prompt) = queue.pop_front() else {
                break;
            };
            answered.push((prompt, decision));
        }
        if queue.is_empty() {
            self.browser_media_permission_prompts.remove(&tab_id);
        }
        for (prompt, decision) in answered {
            prompt.request.allow(decision.granted(prompt.requested));
        }
    }

    /// Drops (and therefore cancels) every pending media prompt for a tab.
    pub(crate) fn clear_browser_media_permission_prompts(&mut self, tab_id: BrowserTabId) {
        self.browser_media_permission_prompts.remove(&tab_id);
    }

    /// Navigation invalidates a pending prompt: the page that asked is gone,
    /// so its request is cancelled rather than left over the new document.
    pub(crate) fn clear_browser_media_permission_prompts_for_navigation(
        &mut self,
        tab_id: BrowserTabId,
        url: &str,
    ) {
        let origin = gpui_browser_media_permission_origin(url);
        let stale = self
            .browser_media_permission_prompts
            .get(&tab_id)
            .is_some_and(|queue| {
                queue
                    .front()
                    .is_some_and(|prompt| origin.as_deref() != Some(prompt.origin.as_str()))
            });
        if stale {
            self.clear_browser_media_permission_prompts(tab_id);
        }
    }

    pub(crate) fn browser_media_permission_prompt_for_tab(
        &self,
        tab_id: BrowserTabId,
    ) -> Option<&GpuiBrowserMediaPermissionPrompt> {
        self.browser_media_permission_prompts
            .get(&tab_id)
            .and_then(VecDeque::front)
    }

    /// Origin of the active tab that already has a stored microphone/camera
    /// answer, which is what makes the toolbar reset control appear.
    pub(crate) fn browser_media_permission_reset_target(
        &self,
        pane_id: BrowserPaneId,
    ) -> Option<(BrowserProfileId, String)> {
        let tab = self.browser_tabs.active_tab_for_pane(pane_id)?;
        let origin = gpui_browser_media_permission_origin(&tab.url)?;
        let decision = self
            .browser_media_permission_decisions
            .decision(tab.profile_id, &origin);
        (!decision.is_empty()).then_some((tab.profile_id, origin))
    }

    pub(crate) fn reset_browser_media_permissions_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((profile_id, origin)) = self.browser_media_permission_reset_target(pane_id) else {
            return;
        };
        if !self
            .browser_media_permission_decisions
            .forget(profile_id, &origin)
        {
            return;
        }
        persist_gpui_browser_media_permission_decisions(&self.browser_media_permission_decisions);
        /*
        Chromium keeps the denied stream state on the live page, so the site
        only gets a fresh chance to ask after a reload.
        */
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| surface.reload());
        }
        cx.notify();
    }
}
