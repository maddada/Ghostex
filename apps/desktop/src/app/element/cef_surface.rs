// C1 wave-2 extraction: the CefSurface entity and CefElement (Element/IntoElement) impls moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

pub(crate) struct CefSurface {
    background: Hsla,
    browser: Rc<CefBrowser>,
    pub(crate) focus_handle: FocusHandle,
    id: String,
    visible: bool,
}

impl CefSurface {
    /*
    CDXC:GPUICefBrowserCreateFallible 2026-07-11:
    CEF child-browser creation can fail transiently (CreateBrowserSync
    returns null while a fresh per-profile request context is still
    initializing asynchronously). Surface construction is therefore fallible
    and happens BEFORE entity creation; callers must handle the error path
    (skip this pass, record a failure, or surface a toast) instead of the
    previous process-aborting expect. Ensure-style reconcile callers retry
    naturally on their next pass.
    */
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_new(
        id: String,
        parent_ns_view: *mut std::ffi::c_void,
        url: String,
        profile: String,
        prepaint_background_color: u32,
        uses_system_page_appearance: bool,
        background: Hsla,
        trusted_clipboard_origin: Option<String>,
        visible: bool,
        popup_open_handler: Option<cef::BrowserPopupOpenHandler>,
        page_metadata_handler: Option<cef::BrowserPageMetadataHandler>,
        media_access_handler: Option<cef::BrowserMediaAccessHandler>,
        sidebar_runtime_settings: Option<cef::SidebarRuntimeSettingsSnapshot>,
        sidebar_gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
        sidebar_bridge_event_handler: Option<cef::SidebarBridgeEventHandler>,
        project_workarea_bridge_event_handler: Option<cef::ProjectWorkareaBridgeEventHandler>,
        manage_docs_resource_scope: Option<cef::ManageDocsResourceScope>,
        app_modal_host_bridge_surface: Option<cef::AppModalHostBridgeSurface>,
        app_modal_host_bridge_event_handler: Option<cef::AppModalHostBridgeEventHandler>,
        page_load_end_handler: Option<cef::PageLoadEndHandler>,
        cx: &mut gpui::App,
    ) -> Result<gpui::Entity<Self>, String> {
        let browser = Rc::new(CefBrowser::new(
            parent_ns_view,
            &url,
            &profile,
            prepaint_background_color,
            uses_system_page_appearance,
            trusted_clipboard_origin,
            popup_open_handler,
            page_metadata_handler,
            media_access_handler,
            sidebar_runtime_settings,
            sidebar_gxserver_bootstrap,
            sidebar_bridge_event_handler,
            project_workarea_bridge_event_handler,
            manage_docs_resource_scope,
            app_modal_host_bridge_surface,
            app_modal_host_bridge_event_handler,
            None,
            None,
            page_load_end_handler,
        )?);
        Ok(cx.new(|cx| Self::from_browser(id, background, visible, browser, cx)))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_new_extension(
        id: String,
        parent_ns_view: *mut std::ffi::c_void,
        url: String,
        profile: String,
        prepaint_background_color: u32,
        uses_system_page_appearance: bool,
        background: Hsla,
        visible: bool,
        bridge_surface: cef::ExtensionBridgeSurfaceSpec,
        bridge_event_handler: cef::ExtensionBridgeEventHandler,
        cx: &mut gpui::App,
    ) -> Result<gpui::Entity<Self>, String> {
        let browser = Rc::new(CefBrowser::new(
            parent_ns_view,
            &url,
            &profile,
            prepaint_background_color,
            uses_system_page_appearance,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(bridge_surface),
            Some(bridge_event_handler),
            None,
        )?);
        Ok(cx.new(|cx| Self::from_browser(id, background, visible, browser, cx)))
    }

    pub(crate) fn from_browser(
        id: String,
        background: Hsla,
        visible: bool,
        browser: Rc<CefBrowser>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        browser.set_visible(visible);
        Self {
            background,
            browser,
            focus_handle: cx.focus_handle().tab_stop(false),
            id,
            visible,
        }
    }

    pub(crate) fn browser(&self) -> Rc<CefBrowser> {
        self.browser.clone()
    }

    pub(crate) fn load_url(&mut self, url: &str) {
        self.browser.load_url(url);
    }

    pub(crate) fn refresh_sidebar_runtime_settings(
        &mut self,
        runtime_settings: cef::SidebarRuntimeSettingsSnapshot,
    ) {
        /*
        CDXC:GPUIProjectSidebarBridge 2026-06-23-06:57:
        The GPUI sidebar needs a callable post-load refresh path for strict debug/beta gates plus the saved shared Settings object without adding a broad settings watcher or event bus. Keep this as a narrow CEF surface forwarder so future callers can target only the sidebar main frame.
        */
        self.browser
            .refresh_sidebar_runtime_settings(runtime_settings);
    }

    pub(crate) fn refresh_sidebar_gxserver_bootstrap(
        &mut self,
        gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:GPUISidebarGxserverBootstrap 2026-06-24-11:17:
        Sidebar bootstrap refreshes use the existing CEF surface wrapper only for the sidebar main frame. This forwards an app-owned snapshot to the private browser-to-renderer message path and must not call generic JavaScript injection, touch Browser/workarea/modal CEF surfaces, persist tokens, log URLs/tokens/paths/titles, or synthesize fallback gxserver data.
        */
        self.browser
            .refresh_sidebar_gxserver_bootstrap(gxserver_bootstrap);
    }

    pub(crate) fn refresh_session_chat_gxserver_bootstrap(
        &mut self,
        gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:GPUISessionChatSurface 2026-07-31:
        Session Chat surfaces refresh their bootstrap through the dedicated
        chat process message, with the same non-logging/no-persistence scope
        as the sidebar refresh above.
        */
        self.browser
            .refresh_session_chat_gxserver_bootstrap(gxserver_bootstrap);
    }

    pub(crate) fn can_go_back(&self) -> bool {
        self.browser.can_go_back()
    }

    pub(crate) fn go_back(&mut self) {
        self.browser.go_back();
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        self.browser.can_go_forward()
    }

    pub(crate) fn go_forward(&mut self) {
        self.browser.go_forward();
    }

    pub(crate) fn reload(&mut self) {
        self.browser.reload();
    }

    pub(crate) fn stop_load(&mut self) {
        self.browser.stop_load();
    }

    pub(crate) fn find_text(&mut self, search_text: &str, forward: bool, find_next: bool) {
        self.browser.find_text(search_text, forward, find_next);
    }

    pub(crate) fn stop_finding(&mut self, clear_selection: bool) {
        self.browser.stop_finding(clear_selection);
    }

    pub(crate) fn is_zoomed(&self) -> bool {
        self.browser.zoom_level().abs() > BROWSER_ZOOM_EPSILON
    }

    pub(crate) fn zoom_level(&self) -> f64 {
        self.browser.zoom_level()
    }

    pub(crate) fn zoom_in(&mut self) {
        self.browser.zoom_in();
    }

    pub(crate) fn zoom_out(&mut self) {
        self.browser.zoom_out();
    }

    pub(crate) fn reset_zoom(&mut self) {
        self.browser.reset_zoom();
    }

    pub(crate) fn toggle_dev_tools(&mut self) {
        self.browser.toggle_dev_tools();
    }

    pub(crate) fn browser_identifier(&self) -> i32 {
        self.browser.identifier()
    }

    pub(crate) fn focus(&mut self) {
        self.browser.focus();
    }

    pub(crate) fn paste(&mut self) -> bool {
        self.browser.paste()
    }

    /// CDXC:GPUITutorialVideoFullscreen 2026-08-18: forwards one host-side "f"
    /// key press (see the CEF backend for why injected JavaScript cannot put
    /// the tutorial player in fullscreen).
    pub(crate) fn send_fullscreen_toggle_key(&self) {
        self.browser.send_fullscreen_toggle_key();
    }

    /// The surface's CEF child view, for the AppKit pointer observer that turns
    /// pointer crossings of the sidebar frame into page-side hover state.
    #[cfg(target_os = "macos")]
    pub(crate) fn native_view_for_sidebar_pointer_tracking(&self) -> Option<*mut std::ffi::c_void> {
        self.browser.native_view()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn native_view_contains_responder(&self, responder: *mut std::ffi::c_void) -> bool {
        self.browser
            .native_view()
            .is_some_and(|native_view| cef::native_view_contains_responder(native_view, responder))
    }

    pub(crate) fn inject_feedback_tool_script(&mut self, script: &str) -> bool {
        /*
        CDXC:GPUIBrowserFeedback 2026-06-23-11:04:
        Toolbar feedback injection goes through the tab-owned CEF surface so Browser wake/focus, GitHub disablement, and loaded-surface ownership stay in GPUI's normal Browser path. CEF main-frame execution is fire-and-forget; a false result means only that no main frame was available, not that page-side feedback completed.
        */
        self.browser.execute_java_script_in_main_frame(script)
    }

    pub(crate) fn execute_app_owned_script(&mut self, script: &str) -> bool {
        /*
        CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
        Project-workarea response dispatch uses the same app-owned CEF main-frame execution boundary as Browser tooling. Callers pass only generated event-dispatch scripts built from serialized app responses; the CEF wrapper does not log script bodies, page URLs, file contents, board payloads, cookies, tokens, or paths.

        CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
        The GPUI app-modal window also uses this app-owned script boundary to dispatch serialized modal-host CustomEvents into the bundled React modal host. Keep callers responsible for generated first-party event scripts only, with no arbitrary page injection, raw bridge payload logging, WebKit fallback, or placeholder modal UI.

        CDXC:GPUIRemoteMachines 2026-06-24-16:48:
        GPUI remote-machine status, sanitized request responses, and presentation refreshes use the same first-party script boundary to dispatch sidebar-only CustomEvents. Callers must serialize only app-owned event payloads and never inject tokens, SSH details, command text, URLs, paths, daemon bodies, or renderer-provided scripts.
        */
        self.browser.execute_java_script_in_main_frame(script)
    }

    pub(crate) fn dispatch_extension_bridge_message(
        &mut self,
        message: &serde_json::Value,
    ) -> bool {
        /*
        CDXC:GPUIExtensionRemoteUrlSurface 2026-08-28:
        Outbound bridge dispatch injects the app-owned context payload as script
        source in the page's own world, and a remote `server.url` page could
        define `__ghostexExtensionBridgeReceive` itself to read it. Surfaces
        without an installed bridge get nothing.
        */
        if !self.browser.extension_bridge_installed() {
            return false;
        }
        let Ok(payload) = serde_json::to_string(message) else {
            return false;
        };
        let Ok(serialized_payload) = serde_json::to_string(&payload) else {
            return false;
        };
        self.browser.execute_java_script_in_main_frame(&format!(
            "window.__ghostexExtensionBridgeReceive?.({serialized_payload});"
        ))
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        self.browser.set_visible(visible);
    }

    pub(crate) fn order_front(&mut self) {
        self.browser.order_front();
    }
}

impl Render for CefSurface {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let browser = self.browser.clone();
        let focus_handle = self.focus_handle.clone();
        let id = self.id.clone();

        div()
            .id(id)
            .key_context(CEF_KEY_CONTEXT)
            .track_focus(&focus_handle)
            .size_full()
            .bg(self.background)
            .child({
                let view = view.clone();
                canvas(
                    move |bounds, window, cx| {
                        let scale_factor = window.scale_factor();
                        view.update(cx, |surface, _| {
                            if surface.visible {
                                surface.browser.set_bounds(bounds, scale_factor);
                            } else {
                                surface.browser.set_visible(false);
                            }
                        })
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            })
            .child(CefElement::new(
                browser,
                view,
                focus_handle,
                self.id.clone(),
                window,
                cx,
            ))
    }
}

pub(crate) struct CefElement {
    browser: Rc<CefBrowser>,
    focus_handle: FocusHandle,
    parent: Entity<CefSurface>,
    surface_id: String,
}

impl CefElement {
    fn new(
        browser: Rc<CefBrowser>,
        parent: Entity<CefSurface>,
        focus_handle: FocusHandle,
        surface_id: String,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self {
        Self {
            browser,
            focus_handle,
            parent,
            surface_id,
        }
    }
}

impl IntoElement for CefElement {
    type Element = CefElement;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CefElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: Size::full(),
            flex_shrink: 1.,
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if !self.parent.read(cx).visible {
            self.browser.set_visible(false);
            return None;
        }

        self.browser.set_visible(true);
        self.browser.set_bounds(bounds, window.scale_factor());
        let hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);
        let browser = self.browser.clone();
        let focus_handle = self.focus_handle.clone();
        let surface_id = self.surface_id.clone();
        window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
            /*
            CDXC:GPUICefFocusRouting 2026-06-14-16:45:
            The CEF child view owns normal web-page input behavior after it is clicked. Focus a GPUI handle with a CEF key context before restoring CEF focus so page text fields receive command-key shortcuts such as Cmd+A instead of leaving the GPUI address bar as the action target.
            */
            if phase.bubble()
                && event.button == MouseButton::Left
                && bounds.contains(&event.position)
            {
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.cef.gpuiPointerFocusMatch",
                    serde_json::json!({
                        "surfaceId": surface_id,
                        "browserId": browser.identifier(),
                        "eventPosition": [event.position.x.as_f32(), event.position.y.as_f32()],
                        "bounds": [
                            bounds.origin.x.as_f32(),
                            bounds.origin.y.as_f32(),
                            bounds.size.width.as_f32(),
                            bounds.size.height.as_f32(),
                        ],
                    }),
                );
                focus_handle.focus(window, cx);
                browser.focus();
                window.refresh();
            }
        });
        Some(hitbox)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let bounds = hitbox
            .as_ref()
            .map(|hitbox| hitbox.bounds)
            .unwrap_or(bounds);
        window.with_content_mask(Some(ContentMask { bounds }), |_window| {});
    }
}
