use crate::app::helpers::*;
use crate::*;

#[derive(Default)]
pub(crate) struct RemoteBrowserRuntime {
    pub(crate) tunnels: HashMap<String, Arc<RemoteBrowserTunnel>>,
    pending: HashSet<String>,
    context_pending: HashSet<String>,
    pub(crate) errors: HashMap<String, String>,
}

impl GhostexGpuiApp {
    pub(crate) fn browser_project_remote_machine_id(&self) -> Option<String> {
        self.browser_tabs_project_id
            .as_deref()
            .and_then(gpui_remote_project_reference_from_project_id)
            .map(|reference| reference.remote_machine_id)
    }

    /// CDXC:RemoteMachines 2026-09-23 WHY:
    /// Ordinary new Browser tabs belong to their machine-scoped project just like remote links. Assign that computer before the first page load; an unassigned tab uses the local CEF context and sends a remote project's localhost address to this computer.
    pub(crate) fn assign_new_browser_tab_project_machine(&mut self, tab_id: BrowserTabId) {
        let remote_machine_id = self.browser_project_remote_machine_id();
        if let Some(tab) = self
            .browser_tabs
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
        {
            tab.remote_machine_id = remote_machine_id;
            tab.runtime_favicon_image =
                browser_favicon_cache_lookup(&tab.url, tab.remote_machine_id.as_deref());
        }
    }

    /// CDXC:RemoteMachines 2026-09-23 WHY:
    /// A loopback link names the session's computer. The OS browser would open this computer instead, so remote chat and terminal links use the machine's embedded SOCKS context even when ordinary web links prefer the external browser. Carry the originating terminal identity through the deferred open, and scope tab reuse to that same machine.
    pub(crate) fn try_open_remote_loopback_link(
        &mut self,
        url: &str,
        target: GpuiEngineTerminalEventTarget,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !gpui_remote_link_is_loopback(url) {
            return false;
        }
        let Some(remote_project) = self.remote_project_for_terminal_target(target) else {
            return false;
        };
        self.open_remote_machine_loopback_link(url, remote_project, cx);
        true
    }

    /// The remote machine and project a terminal or chat session lives on, `None` for this
    /// computer's own sessions.
    ///
    /// CDXC:RemoteMachines 2026-11-11 WHY: An Agents shell id is only unique inside one project's workspace (each parked workspace counts its own ids), and `remote_attach_sessions` keeps the entries of remote projects that are not on screen. Matching the id against the whole map took a local chat for a session on another computer whenever a remote project once on screen had used the same id, so its Ghostex page card (a 127.0.0.1 address) went to that remote project's Browser with "Page opened in <project>" instead of the floating page window. The id is resolved the way the rest of the app resolves it (`workspace_terminal_key_for_shell_session`: this project's local tabs, side chats, then the remote project on screen).
    pub(crate) fn remote_project_for_terminal_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<GpuiRemoteProjectReference> {
        match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => {
                match self.workspace_terminal_key_for_shell_session(session_id)? {
                    GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                        Some(GpuiRemoteProjectReference {
                            remote_machine_id: key.remote_machine_id,
                            project_id: key.project_id,
                        })
                    }
                    GpuiWorkspaceTerminalSessionKey::Local(_) => None,
                }
            }
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_remote_action_session_for_command_tab(session_id)
                .map(|reference| GpuiRemoteProjectReference {
                    remote_machine_id: reference.remote_machine_id,
                    project_id: reference.project_id,
                }),
        }
    }

    /// Opens a loopback address of `remote_project`'s machine in a Browser tab on that machine's
    /// network, the project's own tab set.
    pub(crate) fn open_remote_machine_loopback_link(
        &mut self,
        url: &str,
        remote_project: GpuiRemoteProjectReference,
        cx: &mut gpui::Context<Self>,
    ) {
        let message = GpuiSidebarOpenBrowserUrlMessage {
            url: url.to_string(),
            reuse: GpuiBrowserRendererOpenReuse::Similar,
            from_quick_header: false,
            project_id: Some(gpui_remote_scoped_project_id(
                &remote_project.remote_machine_id,
                &remote_project.project_id,
            )),
        };
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_browser_url_from_renderer_command_with_machine(
                    message,
                    Some(remote_project.remote_machine_id),
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    /// The address the system browser opens for a Browser tab's page. A remote machine's tab
    /// (`remote_machine_id`) may show that machine's own localhost, which the system browser cannot
    /// reach: its gxserver opens through this computer's tunnel, and anything else is refused with
    /// the reason (`gpui_remote_machine_url`).
    pub(crate) fn browser_tab_system_browser_url(
        &self,
        url: &str,
        remote_machine_id: Option<&str>,
    ) -> Result<String, String> {
        let Some(remote_machine_id) = remote_machine_id else {
            return Ok(url.to_string());
        };
        let machine = || {
            gpui_remote_machine_name_from_settings(remote_machine_id)
                .unwrap_or_else(|| "the remote computer".to_string())
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(remote_machine_id) else {
            return if gpui_remote_link_is_loopback(url) {
                Err(format!("This page is on {}. Connect it first.", machine()))
            } else {
                Ok(url.to_string())
            };
        };
        match gpui_remote_machine_url(url, &target) {
            GpuiRemoteMachineUrl::Gxserver(url) => Ok(url),
            GpuiRemoteMachineUrl::AsWritten => Ok(url.to_string()),
            GpuiRemoteMachineUrl::MachineLoopback => Err(format!(
                "This page runs on {}, which only Ghostex's Browser reaches.",
                machine()
            )),
        }
    }

    pub(crate) fn render_remote_browser_placeholder(
        &self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let name = gpui_remote_machine_name_from_settings(machine_id)
            .unwrap_or_else(|| "Remote computer".into());
        let connected = self.remote_gxserver_connections.contains_key(machine_id);
        let error = self.remote_browser.errors.get(machine_id).cloned();
        let pending = self.remote_browser.pending.contains(machine_id);
        let detail = error.unwrap_or_else(|| {
            if !connected {
                "Connect this computer to open its local sites.".into()
            } else if pending {
                "Connecting the browser to this computer…".into()
            } else {
                "Preparing the remote browser…".into()
            }
        });
        let id = machine_id.to_string();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .p(px(24.0))
            .bg(glass_clear(chrome_color(0x101010, 0xffffff).into()))
            .child(
                svg()
                    .path(BROWSER_ICON_WORLD)
                    .size(px(28.0))
                    .text_color(chrome_color(0xaaaaaa, 0x666666)),
            )
            .child(
                div()
                    .text_size(px(20.0))
                    .text_color(chrome_color(0xeeeeee, 0x252525))
                    .child(name),
            )
            .child(
                div()
                    .max_w(px(480.0))
                    .text_size(px(12.0))
                    .text_color(chrome_color(0xaaaaaa, 0x666666))
                    .child(detail),
            )
            .when(!pending, |body| {
                body.child(
                    div()
                        .id("remote-browser-retry")
                        .px(px(12.0))
                        .py(px(7.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(glass_divider(chrome_color(0x3b3b3b, 0xd4d4d4).into()))
                        .bg(glass_card(chrome_color(0x252525, 0xf0f0f0).into(), 1.5))
                        .text_color(chrome_color(0xeeeeee, 0x252525))
                        .cursor_pointer()
                        .child(if connected { "Retry" } else { "Connect" })
                        .on_click(cx.listener(move |app, _, window, cx| {
                            if connected {
                                app.ensure_remote_browser_tunnel(&id, true, cx);
                                app.sync_active_browser_tab_to_surface(window, cx);
                            } else {
                                let command = serde_json::json!({ "remoteMachineId": id });
                                app.handle_gpui_reconnect_remote_machine_message(
                                    command.as_object().expect("object"),
                                    cx,
                                );
                            }
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    pub(crate) fn prepare_remote_browser_context(
        &mut self,
        machine_id: &str,
        profile: &str,
        port: u16,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match cef::prepare_remote_browser_context(profile, port) {
            Ok(true) => return true,
            Err(error) => {
                self.remote_browser
                    .errors
                    .insert(machine_id.to_string(), error.to_string());
                return false;
            }
            Ok(false) => {}
        }
        if !self
            .remote_browser
            .context_pending
            .insert(profile.to_string())
        {
            return false;
        }
        let profile = profile.to_string();
        let machine_id = machine_id.to_string();
        cx.spawn(async move |this, cx| {
            for _ in 0..200 {
                cx.background_executor().timer(Duration::from_millis(25)).await;
                let done = this.update_in(cx, |app, window, cx| {
                    if app.remote_browser.tunnels.get(&machine_id).is_none_or(|tunnel| tunnel.port != port) {
                        app.remote_browser.context_pending.remove(&profile); return true;
                    }
                    match cef::prepare_remote_browser_context(&profile, port) {
                        Ok(false) => false,
                        result => {
                            app.remote_browser.context_pending.remove(&profile);
                            if let Err(error) = result { app.remote_browser.errors.insert(machine_id.clone(), error.to_string()); }
                            else { app.sync_active_browser_tab_to_surface(window, cx); }
                            cx.notify(); true
                        }
                    }
                }).unwrap_or(true);
                if done { return; }
            }
            let _ = this.update(cx, |app, cx| {
                app.remote_browser.context_pending.remove(&profile);
                app.remote_browser.errors.insert(machine_id, "The remote browser network did not initialize. Reconnect the computer to try again.".into());
                cx.notify();
            });
        }).detach();
        false
    }
    pub(crate) fn fetch_remote_tab_favicon(
        &self,
        runtime_key: u64,
        tab_id: BrowserTabId,
        favicon_url: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let tabs = match self.browser_runtime_owner_for_key(runtime_key) {
            Some(BrowserRuntimeOwner::Live) => &self.browser_tabs,
            Some(BrowserRuntimeOwner::Parked(ref id)) => {
                match self.parked_browser_tabs_by_project.get(id) {
                    Some(tabs) => tabs,
                    None => return,
                }
            }
            None => return,
        };
        let Some(tab) = tabs.tab(tab_id) else {
            return;
        };
        let Some(machine_id) = tab.remote_machine_id.as_deref() else {
            return;
        };
        let Some(tunnel) = self.remote_browser.tunnels.get(machine_id).cloned() else {
            return;
        };
        let page_url = tab.url.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let image = background
                .spawn(async move { fetch_remote_browser_favicon(&favicon_url, tunnel.port) })
                .await;
            let Some(image) = image else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                let tabs = match app.browser_runtime_owner_for_key(runtime_key) {
                    Some(BrowserRuntimeOwner::Live) => &mut app.browser_tabs,
                    Some(BrowserRuntimeOwner::Parked(ref id)) => {
                        match app.parked_browser_tabs_by_project.get_mut(id) {
                            Some(tabs) => tabs,
                            None => return,
                        }
                    }
                    None => return,
                };
                if let Some(tab) = tabs
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.id == tab_id && tab.url == page_url)
                {
                    if let Some(cache_key) =
                        browser_favicon_cache_key(&tab.url, tab.remote_machine_id.as_deref())
                    {
                        browser_favicon_cache_store_image(&cache_key, &image);
                    }
                    tab.runtime_favicon_image = Some(image);
                    app.record_browser_history_page(runtime_key, tab_id, "", false);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn ensure_remote_browser_tunnel(
        &mut self,
        machine_id: &str,
        retry: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Arc<RemoteBrowserTunnel>> {
        if let Some(tunnel) = self.remote_browser.tunnels.get(machine_id)
            && tunnel.is_alive()
        {
            return Some(tunnel.clone());
        }
        if self.remote_browser.tunnels.contains_key(machine_id) {
            self.stop_remote_browser_tunnel(machine_id);
        }
        if retry {
            self.remote_browser.errors.remove(machine_id);
        }
        if self.remote_browser.pending.contains(machine_id)
            || self.remote_browser.errors.contains_key(machine_id)
        {
            return None;
        }
        let target = self.gpui_remote_gxserver_request_target(machine_id)?;
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let config = gpui_remote_machine_config_from_settings(settings.object(), machine_id)?;
        let generation = self
            .remote_gxserver_connect_generations
            .get(machine_id)
            .copied();
        let route = gpui_remote_browser_route_for_machine(machine_id);
        let machine_id = machine_id.to_string();
        self.remote_browser.pending.insert(machine_id.clone());
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result =
                background
                    .spawn(async move {
                        start_remote_browser_tunnel(&config, &target, route).map(Arc::new)
                    })
                    .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this
                    .remote_gxserver_connect_generations
                    .get(&machine_id)
                    .copied()
                    != generation
                    || !this.remote_gxserver_connections.contains_key(&machine_id)
                {
                    return;
                }
                this.remote_browser.pending.remove(&machine_id);
                match result {
                    Ok(tunnel) => {
                        this.remote_browser.tunnels.insert(machine_id, tunnel);
                    }
                    Err(message) => {
                        this.remote_browser.errors.insert(machine_id, message);
                    }
                }
                this.sync_active_browser_tab_to_surface(window, cx);
                cx.notify();
            });
        })
        .detach();
        None
    }

    pub(crate) fn stop_remote_browser_tunnel(&mut self, machine_id: &str) {
        if let Some(tunnel) = self.remote_browser.tunnels.remove(machine_id) {
            tunnel.stop();
        }
        self.remote_browser.pending.remove(machine_id);
        self.remote_browser.errors.remove(machine_id);
        self.browser_surfaces.retain(|tab_id, _| {
            self.browser_tabs
                .tab(*tab_id)
                .is_none_or(|tab| tab.remote_machine_id.as_deref() != Some(machine_id))
        });
        for (project_id, runtime) in &mut self.parked_browser_runtimes_by_project {
            if let Some(tabs) = self.parked_browser_tabs_by_project.get(project_id) {
                runtime.surfaces.retain(|tab_id, _| {
                    tabs.tab(*tab_id)
                        .is_none_or(|tab| tab.remote_machine_id.as_deref() != Some(machine_id))
                });
            }
        }
    }

    /// CDXC:Browser 2026-09-05 DECISION:
    /// User: open remote localhost sites in the embedded GPUI browser from a globe button to the left of Resources, grouped by machine with page titles and favicons where available.
    pub(crate) fn open_local_dev_server(
        &mut self,
        site: RemoteBrowserSite,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            self.copy_path_for_disabled_project_workarea(&site.url, "Browser", cx);
            return;
        }
        let profile = self.browser_profiles.active_profile_id();
        if let Some(tab_id) = self.browser_tabs.open_loaded_popup_tab(
            site.url.clone(),
            profile,
            cef::BrowserPopupPlacement::Selected,
        ) {
            if let Some(tab) = self
                .browser_tabs
                .tabs
                .iter_mut()
                .find(|tab| tab.id == tab_id)
            {
                tab.remote_machine_id = None;
                tab.runtime_page_title = site.title;
                tab.runtime_favicon_image = site.favicon;
            }
            self.open_gpui_browser_action_url(site.url, window, cx);
            self.reveal_new_browser_tab(tab_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn open_remote_browser_site(
        &mut self,
        machine_id: String,
        site: RemoteBrowserSite,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            self.copy_path_for_disabled_project_workarea(&site.url, "Browser", cx);
            return;
        }
        if self
            .ensure_remote_browser_tunnel(&machine_id, true, cx)
            .is_none()
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote browser unavailable",
                "Reconnect the machine and recheck its sites before opening.",
                cx,
            );
            return;
        }
        let profile = self.browser_profiles.active_profile_id();
        if let Some(tab_id) = self.browser_tabs.open_loaded_popup_tab(
            site.url.clone(),
            profile,
            cef::BrowserPopupPlacement::Selected,
        ) {
            if let Some(tab) = self
                .browser_tabs
                .tabs
                .iter_mut()
                .find(|tab| tab.id == tab_id)
            {
                tab.remote_machine_id = Some(machine_id);
                tab.runtime_page_title = site.title;
                tab.runtime_favicon_image = site.favicon;
            }
            self.open_gpui_browser_action_url(site.url, window, cx);
            self.reveal_new_browser_tab(tab_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }
}
