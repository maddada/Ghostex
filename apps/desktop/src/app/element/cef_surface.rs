// C1 wave-2 extraction: the CefSurface entity and CefElement (Element/IntoElement) impls moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::render::workarea_header::workarea_header_bottom_y;
use crate::*;

pub(crate) struct CefSurface {
    background: Hsla,
    browser: Rc<CefBrowser>,
    pub(crate) focus_handle: FocusHandle,
    id: String,
    visible: bool,
    workarea_theme: Option<(bool, u32, u32, bool)>,
}

impl CefSurface {
    /*
    CDXC:CefRuntime 2026-07-11:
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
        sidebar_gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
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
            sidebar_gxserver_bootstrap,
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
        page_load_end_handler: Option<cef::PageLoadEndHandler>,
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
            Some(bridge_surface),
            Some(bridge_event_handler),
            page_load_end_handler,
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
        #[cfg(target_os = "linux")]
        {
            // CDXC:FocusRouting 2026-09-19 WHY:
            // Native Chromium clicks bypass GPUI's mouse listeners; mirror their focus into the existing handle so the address bar blurs.
            // Subscribe to actual pointer presses rather than CEF focus notifications, which can recursively fire during programmatic handoffs.
            let surface = cx.entity().downgrade();
            let async_cx = cx.to_async();
            let foreground = cx.foreground_executor().clone();
            browser.on_native_pointer_focus(Rc::new(move || {
                let surface = surface.clone();
                let mut async_cx = async_cx.clone();
                foreground
                    .spawn(async move {
                        let _ = surface.update_in(&mut async_cx, |surface, window, cx| {
                            if surface.visible && surface.browser.owns_native_focus() {
                                surface.focus_handle.focus(window, cx);
                            }
                        });
                    })
                    .detach();
            }));
        }
        Self {
            background,
            browser,
            focus_handle: cx.focus_handle().tab_stop(false),
            id,
            visible,
            workarea_theme: None,
        }
    }

    pub(crate) fn browser(&self) -> Rc<CefBrowser> {
        self.browser.clone()
    }

    pub(crate) fn load_url(&mut self, url: &str) {
        self.browser.load_url(url);
    }

    pub(crate) fn refresh_workarea_theme(&mut self, light: bool) {
        let chrome = crate::app::consts::GPUI_TITLEBAR_BACKGROUND_RGB
            .load(std::sync::atomic::Ordering::Relaxed) as u32;
        let content = crate::app::helpers::work_area_background_for_variant(
            crate::shared_settings::shared_sidebar_settings_snapshot().object(),
            light,
        );
        let glass = crate::app::helpers::window_glass_active();
        if self.workarea_theme != Some((light, chrome, content, glass))
            && self.execute_app_owned_script(&workarea_theme_script(light, chrome, content, glass))
        {
            self.workarea_theme = Some((light, chrome, content, glass));
            self.background = gpui::rgb(content).into();
        }
    }

    pub(crate) fn set_background(&mut self, background: Hsla, cx: &mut gpui::Context<Self>) {
        if self.background != background {
            self.background = background;
            cx.notify();
        }
    }

    pub(crate) fn refresh_session_chat_gxserver_bootstrap(
        &mut self,
        gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:SessionChat 2026-07-31:
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

    /// CDXC:Onboarding 2026-08-18: forwards one host-side "f"
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
        CDXC:Browser 2026-06-23-11:04:
        Toolbar feedback injection goes through the tab-owned CEF surface so Browser wake/focus, GitHub disablement, and loaded-surface ownership stay in GPUI's normal Browser path. CEF main-frame execution is fire-and-forget; a false result means only that no main frame was available, not that page-side feedback completed.
        */
        self.browser.execute_java_script_in_main_frame(script)
    }

    pub(crate) fn execute_app_owned_script(&mut self, script: &str) -> bool {
        /*
        CDXC:CefRuntime 2026-06-24-11:03:
        Project-workarea response dispatch uses the same app-owned CEF main-frame execution boundary as Browser tooling. Callers pass only generated event-dispatch scripts built from serialized app responses; the CEF wrapper does not log script bodies, page URLs, file contents, board payloads, cookies, tokens, or paths.

        CDXC:AppModal 2026-06-24-11:09:
        The GPUI app-modal window also uses this app-owned script boundary to dispatch serialized modal-host CustomEvents into the bundled React modal host. Keep callers responsible for generated first-party event scripts only, with no arbitrary page injection, raw bridge payload logging, WebKit fallback, or placeholder modal UI.

        CDXC:RemoteMachines 2026-06-24-16:48:
        GPUI remote-machine status, sanitized request responses, and presentation refreshes use the same first-party script boundary to dispatch sidebar-only CustomEvents. Callers must serialize only app-owned event payloads and never inject tokens, SSH details, command text, URLs, paths, daemon bodies, or renderer-provided scripts.
        */
        self.browser.execute_java_script_in_main_frame(script)
    }

    pub(crate) fn dispatch_extension_bridge_message(
        &mut self,
        message: &serde_json::Value,
    ) -> bool {
        /*
        CDXC:Extensions 2026-08-28:
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
        if visible && !self.visible && crate::app::panel_motion::view_panel_would_slide_open() {
            self.browser
                .set_motion_hidden(true, std::time::Duration::ZERO);
        }
        self.visible = visible;
        self.browser.set_visible(visible);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reparent_native_view(
        &self,
        parent: *mut std::ffi::c_void,
        from: *mut std::ffi::c_void,
    ) {
        self.browser.reparent_native_view(parent, from);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn update_sidebar_hover_reveal(
        &mut self,
        root: *mut std::ffi::c_void,
        collapsed: bool,
        width: f32,
        companion_hidden: bool,
        requested: bool,
        keep_under_pointer: bool,
    ) -> bool {
        let (revealed, expand_companion) = self.browser.update_sidebar_hover_reveal(
            root,
            collapsed,
            width as f64,
            workarea_header_bottom_y() as f64,
            sidebar_cef_prepaint_background_color(),
            companion_hidden,
            requested,
            keep_under_pointer,
        );
        self.set_visible(!collapsed || revealed);
        expand_companion
    }

    pub(crate) fn order_front(&mut self) {
        self.browser.order_front();
    }
}

impl Render for CefSurface {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        #[cfg(target_os = "macos")]
        if let Ok(parent) = cef_parent_native_view(window) {
            self.reparent_native_view(parent, std::ptr::null_mut());
        }
        let view = cx.entity().clone();
        let browser = self.browser.clone();
        let focus_handle = self.focus_handle.clone();
        let id = self.id.clone();

        div()
            .id(id)
            .key_context(CEF_KEY_CONTEXT)
            .track_focus(&focus_handle)
            .size_full()
            .bg(if self.id == "gpui-sidebar" {
                titlebar_background()
            } else if self.id.starts_with("ghostex-gpui-session-chat-renderer-") {
                gpui_session_chat_background_color()
            } else {
                self.background
            })
            .child({
                let view = view.clone();
                canvas(
                    move |bounds, window, cx| {
                        view.update(cx, |surface, _| {
                            if surface.visible {
                                place_browser_view(&surface.browser, bounds, window);
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

/// Gives the page's native view its frame. A slide that only pushes the page (app/panel_motion.rs)
/// resizes it live, the way a divider drag does. While the view panel itself slides, a page whose
/// frame would change, or which would poke outside the clip GPUI draws it in, keeps its last frame
/// and goes transparent until the slide settles, because GPUI cannot clip an AppKit child view.
fn place_browser_view(browser: &CefBrowser, bounds: Bounds<Pixels>, window: &Window) {
    let scale_factor = window.scale_factor();
    let hold = crate::app::panel_motion::view_panel_sliding() && {
        let mask = window.content_mask().bounds;
        let tolerance = px(0.5);
        let clipped = bounds.origin.x < mask.origin.x - tolerance
            || bounds.origin.y < mask.origin.y - tolerance
            || bounds.right() > mask.right() + tolerance
            || bounds.bottom() > mask.bottom() + tolerance;
        clipped || browser.bounds_differ(bounds, scale_factor)
    };
    browser.set_motion_hidden(
        hold,
        crate::app::panel_motion::panel_content_fade_duration(),
    );
    if !hold {
        browser.set_bounds(bounds, scale_factor);
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
        place_browser_view(&self.browser, bounds, window);
        // CDXC:Tooltips 2026-09-24 SEE-ALSO: the tooltip positioners in gpui (`window.rs`) and gpui-component (`tooltip.rs`) lay tooltips out around the regions recorded here, because the page's native view draws over anything GPUI paints inside its frame.
        window.occlude_native_region(bounds);
        #[cfg(target_os = "macos")]
        if self.surface_id != APP_MODAL_HOST_ID
            && let Some(native_view) = self.browser.native_view()
        {
            // CDXC:AppModal 2026-09-15 WHY: App-modal windows have their own AppKit corners. Applying the workspace's 18pt pane mask here rounds Settings a second time and exposes the old backing color at its edges after a live theme change, even with square CSS content.
            // Fullscreen and window-edge changes can change clipping even
            // when the browser's own cached frame is unchanged.
            super::window_corner_pane::refresh_native_window_corner_clip(native_view);
        }
        let hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);
        let browser = self.browser.clone();
        let focus_handle = self.focus_handle.clone();
        let surface_id = self.surface_id.clone();
        window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
            /*
            CDXC:FocusRouting 2026-06-14-16:45:
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
