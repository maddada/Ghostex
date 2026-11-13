// C4 light split: CefBrowser creation/teardown, page-appearance
// application, profile/cache-path resolution (including the app-UI profile
// cache-path persistence logic), the native-view<->browser registry, and the
// browser-level DevTools/ContextMenu/Find/LifeSpan/Display/Permission
// handler impls. Pure move out of `cef/shell.rs`. See
// docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

pub(crate) fn apply_browser_page_appearance(browser: &cef::Browser) {
    let Some(host) = browser.host() else {
        return;
    };
    apply_page_color_scheme(browser, browser_page_appearance());

    let mut background_params = match cef::dictionary_value_create() {
        Some(params) => params,
        None => return,
    };
    let mut color = match cef::dictionary_value_create() {
        Some(color) => color,
        None => return,
    };
    for (key, shift) in [("r", 16), ("g", 8), ("b", 0)] {
        let value = ((CEF_BROWSER_PAGE_BACKGROUND_COLOR >> shift) & 0xff) as c_int;
        color.set_int(Some(&CefString::from(key)), value);
    }
    color.set_double(Some(&CefString::from("a")), 1.0);
    background_params.set_dictionary(Some(&CefString::from("color")), Some(&mut color));
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from(
            "Emulation.setDefaultBackgroundColorOverride",
        )),
        Some(&mut background_params),
    );
}

pub(crate) fn next_page_appearance_devtools_message_id() -> c_int {
    PAGE_APPEARANCE_DEVTOOLS_MESSAGE_ID.with(|message_id| {
        let next = message_id.get().checked_add(1).unwrap_or(1);
        message_id.set(next);
        next
    })
}

pub struct CefBrowser {
    pub(crate) browser: RefCell<cef::Browser>,
    pub(crate) _client: Option<cef::Client>,
    pub(crate) _request_context: cef::RequestContext,
    pub(crate) last_bounds: RefCell<Option<(f32, f32, f32, f32, f32)>>,
    pub(crate) last_visible: Cell<Option<bool>>,
    motion_hidden: Cell<bool>,
    pub(crate) uses_system_page_appearance: bool,
    pub(crate) extension_bridge_installed: bool,
    /// The surface the bridge was installed for, so outbound messages can check that the main
    /// frame still shows that extension's page.
    extension_bridge_surface: Option<ExtensionBridgeSurfaceSpec>,
    /// CDXC:Browser 2026-09-23 WHY:
    /// Remote reconnect disposes CEF surfaces while retaining their tabs. Mark app-owned disposal before releasing the native view so synchronous or deferred DoClose callbacks cannot turn that teardown into a user-requested tab close.
    app_initiated_close: StdRc<Cell<bool>>,
    /// CEF's child view, with this browser's own reference to it (`platform::retain_native_view`),
    /// so it cannot go with a window the surface is no longer in (CDXC:CefRuntime 2026-11-13 WHY
    /// in native/macos/GpuiCefAppKitHooks.m). `Drop` takes it out and gives it back once.
    #[cfg(target_os = "macos")]
    retained_native_view: Cell<*mut c_void>,
}

/// The Files embed page (`manage.html?embed=1&path=…`) showing an Excalidraw drawing.
#[cfg(target_os = "macos")]
fn files_embed_shows_drawing(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| {
        url.query_pairs()
            .any(|(key, value)| key == "embed" && value == "1")
            && url.query_pairs().any(|(key, value)| {
                key == "path" && value.to_ascii_lowercase().ends_with(".excalidraw")
            })
    })
}

impl CefBrowser {
    pub fn new(
        parent_native_view: *mut c_void,
        url: &str,
        profile: &str,
        background_color: u32,
        uses_system_page_appearance: bool,
        trusted_clipboard_origin: Option<String>,
        popup_open_handler: Option<BrowserPopupOpenHandler>,
        code_window_popup_handler: Option<BrowserPopupOpenHandler>,
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
        media_access_handler: Option<BrowserMediaAccessHandler>,
        project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        manage_docs_resource_scope: Option<ManageDocsResourceScope>,
        extension_bridge_surface: Option<ExtensionBridgeSurfaceSpec>,
        extension_bridge_event_handler: Option<ExtensionBridgeEventHandler>,
        page_load_end_handler: Option<PageLoadEndHandler>,
        mobile_user_agent: Option<BrowserMobileUserAgent>,
        initial_frame: Option<(Bounds<Pixels>, f32)>,
    ) -> Result<Self, String> {
        let _profile = crate::profiling::span(crate::profiling::Metric::CefCreate);
        /*
        CDXC:CefRuntime 2026-09-04 WHY:
        Every CefBrowser goes through here, so this is the one place that
        refuses to touch libcef before the runtime exists. Packaged macOS
        builds do not bundle Chromium; the first launch (and every launch
        after a CEF component version bump) opens the main window and then
        downloads the component while the app is already running. The
        gxserver bootstrap resolves during that download, and the ensure-style
        surface reconcile used to reach CefString::from with the library not
        loaded yet: cef-rs resolves libcef functions lazily, so the call went
        through a null pointer and the process died on every launch (2026-09-04,
        8.8.0). context_initialized() implies the framework is loaded and
        CEF finished initializing; before that, creation is reported as the
        same fallible "not yet" that callers already retry on their next pass.
        */
        if !context_initialized() {
            request_runtime();
            return Err("CEF runtime is not initialized yet".into());
        }
        let keyboard_zoom_enabled =
            page_metadata_handler.is_some() || project_workarea_bridge_event_handler.is_some();
        /*
        CDXC:CefRuntime 2026-10-01 DECISION:
        User: disable two-finger zoom in every built-in CEF view and modal, including Code, but preserve it in browser pages and custom views (2026-09-09); and "please allow me to pinch zoom in/out in Excalidraw" (2026-10-01). The Files embed showing a drawing lets the pinch reach the page, which cancels the browser's page zoom and leaves the pinch to Excalidraw's own canvas zoom (apps/desktop/views/manage/embed.tsx). Browser pages and custom views use system page appearance; extension payloads also keep their own gesture behavior.
        */
        #[cfg(target_os = "macos")]
        let pinch_zoom_disabled = !uses_system_page_appearance
            && extension_bridge_surface.is_none()
            && !(manage_docs_resource_scope.is_some() && files_embed_shows_drawing(url));
        /*
        CDXC:CefRuntime 2026-07-11:
        CreateBrowserSync returns null when the per-profile request context's
        asynchronous initialization has not completed yet (the same race the
        app-ui profiles dodge via the pre-initialized global context — see
        CDXC:CefRuntime 2026-07-09). This used to be an `.expect`
        that hard-crashed the whole app (five "failed to create cef-rs child
        browser" aborts on 2026-07-10, all from fresh browser
        profile contexts). Creation is now fallible; ensure-style callers skip
        the surface for this pass and naturally create it on their next
        reconcile once the context finishes initializing.
        */
        /*
        CDXC:SessionChat 2026-11-11 WHY:
        User: "after i open a inline html (ghsotex page) i see it show as collpased to the top left for a split second then return to normal size". A browser made at 1x1 and resized afterwards can hand Chromium a frame from before its real size, which Chromium draws in the view's top-left corner until the next one. A caller that knows where the page goes (the floating page window) passes that frame with the window's scale (`initial_frame`), so Chromium's view starts at its real size; `set_bounds` below then places it exactly, as a paint would. The frame is in points on macOS and in physical pixels on Windows and Linux, as each adapter's `set_native_view_frame` takes it.
        */
        let initial_bounds = initial_frame.map_or(
            cef::Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            |(bounds, scale_factor)| {
                let scale = if cfg!(target_os = "macos") {
                    1.0
                } else {
                    scale_factor.max(1.0)
                };
                cef::Rect {
                    x: (bounds.origin.x.as_f32() * scale).round() as i32,
                    y: (bounds.origin.y.as_f32() * scale).round() as i32,
                    width: ((bounds.size.width.as_f32() * scale).round() as i32).max(1),
                    height: ((bounds.size.height.as_f32() * scale).round() as i32).max(1),
                }
            },
        );
        let window_info = platform::child_window_info(parent_native_view, &initial_bounds);
        /*
        macOS `createBrowserIfNeeded` trusted-clipboard parity: only surfaces
        constructed with a trusted clipboard origin (the code-server editor)
        enable JavaScript clipboard access, pre-grant Chromium's clipboard
        read/write content setting for that exact origin, and install the
        permission-prompt handler. Ordinary Browser panes keep CEF defaults.
        */
        let trusted_clipboard_origin = trusted_clipboard_origin
            .as_deref()
            .and_then(cef_normalized_origin);
        let mut browser_settings = cef::BrowserSettings::default();
        if trusted_clipboard_origin.is_some() {
            browser_settings.javascript_access_clipboard = State::ENABLED;
            browser_settings.javascript_dom_paste = State::ENABLED;
        }
        let requested_url = url.to_string();
        /*
        CDXC:Extensions 2026-08-28:
        A `server.url` extension's surface is pinned to a third-party HTTPS
        origin with `bridge_enabled` false. Drop its bridge event handler here
        rather than at each caller, so the browser process is the single place
        that decides a remote page gets no bridge: no install message, no
        `ghostexExtensionHost`, no inbound handler, and no outbound dispatch.
        */
        let extension_bridge_installed = extension_bridge_surface
            .as_ref()
            .is_some_and(|surface| surface.bridge_enabled);
        let extension_bridge_event_handler = extension_bridge_installed
            .then_some(extension_bridge_event_handler)
            .flatten();
        if let Some(surface) = extension_bridge_surface.as_ref()
            && (!surface.matches_url(&requested_url)
                || (extension_bridge_installed && extension_bridge_event_handler.is_none()))
        {
            return Err("extension CEF surface does not match its registered origin".into());
        }
        let creation_url = if uses_system_page_appearance {
            "about:blank"
        } else {
            requested_url.as_str()
        };
        let creation_url = cef::CefString::from(creation_url);
        browser_settings.background_color = if uses_system_page_appearance {
            CEF_BROWSER_PAGE_BACKGROUND_COLOR
        } else {
            background_color
        };
        /*
        CDXC:Browser 2026-10-03:
        The permission handler serves independent decisions: the code-server
        clipboard grant (trusted origin only), Browser-pane microphone/camera
        prompts, and the Local Network Access prompt every page can raise
        (CDXC:Browser 2026-10-03 in site_requests.rs), which is why every
        browser gets one now. Keep the decisions independent inside it.
        */
        let permission_handler = Some(GhostexGpuiPermissionHandler::new(
            trusted_clipboard_origin.clone(),
            media_access_handler,
            profile.to_string(),
        ));
        // Where the page's own requests for a new tab or window go: a Browser pane's tab model, or
        // a Code page's window route. Only the sign-in exception in `on_before_popup` asks for the
        // Browser pane's handler alone (browser_handlers.rs).
        let new_browser_route = popup_open_handler
            .clone()
            .or_else(|| code_window_popup_handler.clone());
        let context_menu_handler = GhostexGpuiContextMenuHandler::new(
            new_browser_route.clone(),
            page_metadata_handler.clone(),
        );
        // Browser pages only: other surfaces keep Alloy's cancel-every-download default.
        let download_handler = page_metadata_handler
            .as_ref()
            .map(|_| GhostexGpuiDownloadHandler::new());
        let display_handler = page_metadata_handler.as_ref().map(|handler| {
            GhostexGpuiDisplayHandler::new(handler.clone(), Cell::new(uses_system_page_appearance))
        });
        let find_handler = page_metadata_handler
            .as_ref()
            .map(|handler| GhostexGpuiFindHandler::new(handler.clone()));
        let manage_docs_resource_base_url = manage_docs_resource_scope
            .as_ref()
            .map(|scope| scope.base_url().to_string());
        let request_handler = manage_docs_resource_scope
            .as_ref()
            .map(|scope| {
                scope.request_handler(
                    &requested_url,
                    project_workarea_bridge_event_handler.clone(),
                )
            })
            .or_else(|| {
                // Browser panes, project website views and Code pages use the same route for
                // middle-click and Cmd/Ctrl-click links as for their popups.
                new_browser_route.clone().map(|new_browser_route| {
                    GhostexGpuiBrowserRequestHandler::new(
                        new_browser_route,
                        page_metadata_handler.clone(),
                    )
                })
            })
            // Every other page still hands app links to the OS (site_requests.rs).
            .or_else(|| Some(GhostexGpuiExternalAppRequestHandler::new()));
        let keyboard_handler =
            surface_keyboard_handler(keyboard_zoom_enabled, page_metadata_handler.clone());
        let browser_lifecycle_handler = page_metadata_handler.clone();
        let app_initiated_close = StdRc::new(Cell::new(false));
        let load_handler = if let Some(surface) = extension_bridge_surface
            .clone()
            .filter(|_| extension_bridge_installed)
        {
            Some(GhostexGpuiExtensionBridgeLoadHandler::new(
                surface,
                page_load_end_handler,
            ))
        } else if project_workarea_bridge_event_handler.is_some() {
            Some(GhostexGpuiProjectWorkareaBridgeLoadHandler::new(
                manage_docs_resource_base_url,
                page_load_end_handler,
            ))
        } else if page_metadata_handler.is_some() || trusted_clipboard_origin.is_some() {
            Some(GhostexGpuiBrowserPageLoadHandler::new(
                page_metadata_handler,
                trusted_clipboard_origin.clone(),
                page_load_end_handler,
            ))
        } else {
            /*
            CDXC:Onboarding 2026-08-18:
            The handler for a surface with no bridge of its own (the tutorial
            video modal). Every bridged surface above reports the same edge
            from inside its own handler, because the client has one LoadHandler
            slot and a second handler would displace the bridge install.
            */
            page_load_end_handler.map(GhostexGpuiPageLoadEndHandler::new)
        };
        // Every GPUI CEF browser needs the client's life-span handler so
        // DoClose is always handled and CEF can never close the host GPUI
        // window when a browser is dropped.
        let mut client = Some(GhostexGpuiCefClient::new(
            Some(GhostexGpuiLifeSpanHandler::new(
                popup_open_handler,
                code_window_popup_handler,
                browser_lifecycle_handler,
                false,
                app_initiated_close.clone(),
            )),
            Some(context_menu_handler),
            display_handler,
            find_handler,
            load_handler,
            project_workarea_bridge_event_handler,
            extension_bridge_surface.clone(),
            extension_bridge_event_handler,
            request_handler,
            permission_handler,
            Some(GhostexGpuiCefFocusHandler::new()),
            keyboard_handler,
            download_handler,
        ));
        let mut request_context = cef_request_context_for_profile(profile)
            .map_err(|error| format!("failed to create GPUI CEF request context: {error}"))?;
        if let Some(origin) = trusted_clipboard_origin.as_deref() {
            let origin = CefString::from(origin);
            request_context.set_content_setting(
                Some(&origin),
                Some(&origin),
                ContentSettingTypes::CLIPBOARD_READ_WRITE,
                ContentSettingValues::ALLOW,
            );
        }
        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            client.as_mut(),
            Some(&creation_url),
            Some(&browser_settings),
            None,
            Some(&mut request_context),
        )
        .ok_or_else(|| {
            "cef-rs child browser creation returned null (request context still initializing)"
                .to_string()
        })?;
        #[cfg(target_os = "macos")]
        let mut retained_native_view = std::ptr::null_mut();
        if let Some(host) = browser.host() {
            let native_view = platform::native_view_ptr(host.window_handle());
            #[cfg(target_os = "macos")]
            {
                retained_native_view = platform::retain_native_view(native_view);
            }
            platform::prepare_native_view_for_focus(native_view);
            #[cfg(target_os = "macos")]
            platform::set_native_view_pinch_zoom_disabled(native_view, pinch_zoom_disabled);
            #[cfg(target_os = "windows")]
            platform::set_native_view_background_color(
                native_view,
                browser_settings.background_color,
            );
            register_native_view_browser(
                native_view,
                &browser,
                uses_system_page_appearance,
                keyboard_zoom_enabled,
            );
            if (profile == "session-chat" || profile == "app-modal") && !native_view.is_null() {
                SYSTEM_APP_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
                    views.borrow_mut().insert(native_view as usize);
                });
                apply_page_color_scheme(&browser, BrowserPageAppearance::System);
            }
        }
        install_page_input_tracker(&browser);
        if uses_system_page_appearance {
            apply_browser_page_appearance(&browser);
            // Set before the first load so the server's first answer is already the mobile page.
            if mobile_user_agent.is_some() {
                apply_browser_mobile_user_agent(&browser, mobile_user_agent);
            }
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(requested_url.as_str())));
            }
        }

        let created = Self {
            browser: RefCell::new(browser),
            _client: client,
            _request_context: request_context,
            last_bounds: RefCell::new(None),
            last_visible: Cell::new(None),
            motion_hidden: Cell::new(false),
            uses_system_page_appearance,
            extension_bridge_installed,
            extension_bridge_surface: extension_bridge_surface
                .filter(|_| extension_bridge_installed),
            app_initiated_close,
            #[cfg(target_os = "macos")]
            retained_native_view: Cell::new(retained_native_view),
        };
        if let Some((bounds, scale_factor)) = initial_frame {
            created.set_bounds(bounds, scale_factor);
        }
        Ok(created)
    }

    /// The view is on screen (`set_visible(true)` was its last change).
    pub fn is_shown(&self) -> bool {
        self.last_visible.get() == Some(true)
    }

    pub fn extension_bridge_installed(&self) -> bool {
        self.extension_bridge_installed
    }

    /// Whether the main frame still shows the extension page the bridge was installed for.
    ///
    /// CDXC:Extensions 2026-10-25 WHY:
    /// Inbound bridge messages and the bridge install already check the frame URL, but outbound context broadcasts and call answers were sent to whatever document the surface had navigated to, where a third-party page could define `__ghostexExtensionBridgeReceive` and read them (security audit 2026-10-25). Outbound dispatch now requires the same match.
    pub fn extension_bridge_main_frame_matches(&self) -> bool {
        let Some(surface) = self.extension_bridge_surface.as_ref() else {
            return false;
        };
        let browser = self.browser.borrow();
        browser.main_frame().is_some_and(|frame| {
            surface.matches_url(&cef::CefString::from(&frame.url()).to_string())
        })
    }

    pub fn identifier(&self) -> i32 {
        self.browser.borrow().identifier()
    }

    pub fn set_bounds(&self, bounds: Bounds<Pixels>, scale_factor: f32) {
        /*
        `scale_factor` is the GPUI window's logical-to-physical ratio at the
        call site. AppKit children are positioned in points and Win32 queries
        per-window DPI itself, but X11 has no per-window scale query at all,
        so the only correct source for the Linux adapter is the value GPUI
        already computed for the parent window.
        */
        let x = bounds.origin.x.as_f32();
        let y = bounds.origin.y.as_f32();
        let width = bounds.size.width.as_f32().max(0.0);
        let height = bounds.size.height.as_f32().max(0.0);
        let raw_bounds = (x, y, width, height, scale_factor);
        {
            let mut last_bounds = self.last_bounds.borrow_mut();
            if last_bounds.as_ref() == Some(&raw_bounds) {
                return;
            }
            *last_bounds = Some(raw_bounds);
        }

        let rect = cef::Rect {
            x: x.round() as i32,
            y: y.round() as i32,
            width: width.round() as i32,
            height: height.round() as i32,
        };

        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        let native_view = platform::native_view_ptr(host.window_handle());
        /*
        CDXC:CefRuntime 2026-06-14-15:25:
        Match Tauri's CEF child-view model: cef-rs owns the browser host while a thin platform adapter positions the native child view inside the GPUI-owned parent. The adapter respects the parent's coordinate/scale conventions (flipped NSView points on macOS, DPI-scaled physical pixels on Windows) so CEF never overlaps GPUI chrome or sibling surfaces.

        GPUI layout can place a surface on a half logical pixel. Preserve that
        raw rectangle through the platform seam: AppKit can position child
        views in fractional points, while the Windows and X11 adapters round
        only after converting to physical pixels. Rounding here shifted a
        half-point Browser origin by one backing pixel on Retina displays and
        exposed the surface background as a vertical seam.
        */
        let started_at = Instant::now();
        platform::set_native_view_frame(
            native_view,
            x as f64,
            y as f64,
            width as f64,
            height as f64,
            scale_factor,
        );
        let frame_elapsed = started_at.elapsed();
        let resize_started_at = Instant::now();
        host.was_resized();
        let resize_elapsed = resize_started_at.elapsed();
        if cef_resize_diagnostics_enabled() {
            platform::log_resize_diagnostic(
                browser.identifier(),
                rect.width,
                rect.height,
                frame_elapsed.as_micros() as u64,
                resize_elapsed.as_micros() as u64,
                started_at.elapsed().as_micros() as u64,
            );
        }
    }

    pub fn native_view(&self) -> Option<*mut c_void> {
        let browser = self.browser.borrow();
        browser
            .host()
            .map(|host| platform::native_view_ptr(host.window_handle()))
    }

    /// Whether `bounds` differs from the frame the native view last took.
    pub fn bounds_differ(&self, bounds: Bounds<Pixels>, scale_factor: f32) -> bool {
        let raw_bounds = (
            bounds.origin.x.as_f32(),
            bounds.origin.y.as_f32(),
            bounds.size.width.as_f32().max(0.0),
            bounds.size.height.as_f32().max(0.0),
            scale_factor,
        );
        self.last_bounds.borrow().as_ref() != Some(&raw_bounds)
    }

    /// Makes the page transparent for the frames a panel slides (app/panel_motion.rs), keeping its
    /// last frame. Unlike `set_visible(false)` this neither blurs the page nor hides its view, which
    /// would hand the window's keyboard focus elsewhere. Coming back, the page fades in over
    /// `fade_in` (macOS), so it arrives the way the panel's other content does.
    pub fn set_motion_hidden(&self, hidden: bool, fade_in: std::time::Duration) {
        if self.motion_hidden.get() == hidden {
            return;
        }
        self.motion_hidden.set(hidden);
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        platform::set_native_view_motion_hidden(
            platform::native_view_ptr(host.window_handle()),
            hidden,
            fade_in.as_secs_f64(),
        );
    }

    pub fn set_visible(&self, visible: bool) {
        if self.last_visible.get() == Some(visible) {
            return;
        }
        // CDXC:Diagnostics 2026-08-24: hiding a CEF surface
        // blurs its document (relatedTarget null in the page) without any
        // first-responder transition, so this is the only place a
        // visibility-driven focus loss can be observed. Real transitions
        // only — the unchanged case returned above.
        crate::support_logs::append(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.cef.surfaceVisibilityChanged",
            serde_json::json!({
                "browserId": self.identifier(),
                "visible": visible,
            }),
        );
        if !visible {
            self.blur();
        }

        let browser = self.browser.borrow();
        if visible && self.uses_system_page_appearance {
            apply_browser_page_appearance(&browser);
        }
        let Some(host) = browser.host() else {
            return;
        };
        let native_view = platform::native_view_ptr(host.window_handle());
        set_cef_native_view_hidden(native_view, !visible);
        platform::set_native_view_visible(native_view, visible);
        self.last_visible.set(Some(visible));
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reparent_native_view(&self, parent: *mut c_void, from: *mut c_void) {
        if self
            .native_view()
            .is_some_and(|view| platform::reparent_pane_native_view(view, parent, from))
        {
            *self.last_bounds.borrow_mut() = None;
        }
    }

    pub fn order_front(&self) {
        /*
        CDXC:Titlebar 2026-07-09:
        Native child views stack in creation order, and terminal host views
        keep being appended as sessions mount. Reused overlay CEF surfaces
        (titlebar dropdown panels) must re-assert their top sibling position
        when shown, or they reappear underneath newer terminal views. Only
        intentional overlay surfaces may call this; normal laid-out surfaces
        rely on non-overlapping frames instead of z-order.
        */
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        platform::order_native_view_front(platform::native_view_ptr(host.window_handle()));
    }

    /// What the page's view shows right now, as an identity that changes when its compositor hands
    /// it a new surface; 0 when it shows nothing or the platform cannot tell.
    pub fn content_identity(&self) -> u64 {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return 0;
        };
        platform::native_view_content_identity(platform::native_view_ptr(host.window_handle()))
    }

    pub fn focus(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:FocusRouting 2026-06-14-16:31:
        Web-page text fields inside CEF must regain both native focus ownership (AppKit first responder / Win32 keyboard focus) and Chromium browser focus after GPUI chrome has been focused. Without this handoff, command shortcuts such as Cmd+A can stay routed to GPUI instead of selecting text in the active page input.
        */
        platform::focus_native_view(platform::native_view_ptr(host.window_handle()));
        host.set_focus(1);
    }

    pub fn blur(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:Browser 2026-06-23-11:32:
        Hiding a GPUI Browser CEF child view for sleep, mode switch, or tab drag must also release Chromium focus and runtime active-view bookkeeping so hidden pages cannot keep command-dispatch ownership. This is a narrow native-view boundary blur; it does not destroy the CEF browser, change layout, persist data, log content, or synthesize native hit routing.
        */
        let native_view = platform::native_view_ptr(host.window_handle());
        host.set_focus(0);
        clear_active_native_view_if_matching(native_view);
    }

    #[allow(dead_code)] // no caller: Select All goes through select_all_in_browser; kept to mirror cef/unsupported.rs
    pub fn select_all(&self) {
        self.focus();
        let browser = self.browser.borrow();
        select_all_in_browser(&browser);
    }

    pub fn load_url(&self, url: &str) {
        let browser = self.browser.borrow();
        if let Some(frame) = browser.main_frame() {
            frame.load_url(Some(&cef::CefString::from(url)));
        }
    }

    pub fn execute_java_script_in_main_frame(&self, script: &str) -> bool {
        let browser = self.browser.borrow();
        let Some(frame) = browser.main_frame() else {
            return false;
        };
        /*
        CDXC:Browser 2026-06-23-11:04:
        GPUI Browser feedback tools now use CEF's normal main-frame JavaScript execution path for app-owned injection scripts. Pass a synthetic script URL and return only main-frame availability so this backend does not log page URLs, titles, script bodies, user content, JS errors, cookies, tokens, paths, command text, or terminal content.

        CDXC:FocusRouting 2026-07-15:
        App-owned renderer notifications are sideband state delivery, not an
        input-focus action. Executing one must preserve the current GPUI,
        terminal, or CEF responder; callers that represent an explicit user
        focus action invoke `focus` separately. Focusing here caused sidebar
        attention/session-selection notifications to steal keyboard ownership
        synchronously during terminal mouse-down handling.
        */
        frame.execute_java_script(
            Some(&cef::CefString::from(script)),
            Some(&cef::CefString::from(BROWSER_APP_OWNED_SCRIPT_URL)),
            1,
        );
        true
    }

    pub fn can_go_back(&self) -> bool {
        self.browser.borrow().can_go_back() != 0
    }

    pub fn go_back(&self) {
        if !self.can_go_back() {
            return;
        }
        self.focus();
        self.browser.borrow().go_back();
    }

    pub fn can_go_forward(&self) -> bool {
        self.browser.borrow().can_go_forward() != 0
    }

    pub fn go_forward(&self) {
        if !self.can_go_forward() {
            return;
        }
        self.focus();
        self.browser.borrow().go_forward();
    }

    pub fn reload(&self) {
        self.focus();
        self.browser.borrow().reload();
    }

    /// Switches the page's user agent and reloads it so the site serves the matching layout.
    pub fn set_mobile_user_agent(&self, agent: Option<BrowserMobileUserAgent>) {
        apply_browser_mobile_user_agent(&self.browser.borrow(), agent);
        self.browser.borrow().reload();
    }

    pub fn stop_load(&self) {
        self.focus();
        self.browser.borrow().stop_load();
    }

    pub fn find_text(&self, search_text: &str, forward: bool, find_next: bool) {
        let search_text = search_text.trim();
        if search_text.is_empty() {
            self.stop_finding(true);
            return;
        }
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.find(
            Some(&CefString::from(search_text)),
            forward as c_int,
            0,
            find_next as c_int,
        );
    }

    pub fn stop_finding(&self, clear_selection: bool) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.stop_finding(clear_selection as c_int);
    }

    pub fn zoom_level(&self) -> f64 {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return 0.0;
        };
        host.zoom_level()
    }

    pub fn zoom_in(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.zoom(ZoomCommand::IN);
    }

    pub fn zoom_out(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.zoom(ZoomCommand::OUT);
    }

    pub fn reset_zoom(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:Browser 2026-06-22-11:59:
        Zoom reset in the GPUI browser toolbar must use Chromium's browser-host zoom level, matching native CEF behavior and avoiding CSS, JavaScript, overlay, or fallback scaling.
        */
        host.zoom(ZoomCommand::RESET);
    }

    pub fn toggle_dev_tools(&self) {
        let mut browser = self.browser.borrow_mut();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:Browser 2026-06-22-11:50:
        Browser toolbar DevTools is a real CEF host action in GPUI. Toggle the browser's associated DevTools surface through CEF itself so the toolbar action is not a silent placeholder and no GPUI overlay, hidden hit region, or synthetic coordinate routing is introduced.
        */
        if host.has_dev_tools() != 0 {
            host.close_dev_tools();
            return;
        }
        show_browser_dev_tools(Some(&mut browser), None);
    }
}

impl Drop for CefBrowser {
    fn drop(&mut self) {
        self.app_initiated_close.set(true);
        forget_page_keep_awake_probe(self.identifier());
        forget_claude_cloud_page_probe(self.identifier());
        forget_page_bridge_wait(self.identifier());
        site_prompts_browser_closed(self.identifier());
        #[cfg(target_os = "macos")]
        if let Some(view) = self.native_view() {
            platform::dispose_sidebar_hover_reveal(view);
        }
        // The view this browser holds (macOS), never one read back from CEF now: CEF's handle is
        // a plain pointer that outlived its view before the browser held one.
        #[cfg(target_os = "macos")]
        {
            let retained = self.retained_native_view.replace(std::ptr::null_mut());
            if !retained.is_null() {
                unregister_native_view_browser(retained);
                platform::release_native_view(retained);
            }
        }
        if let Some(host) = self.browser.borrow().host() {
            #[cfg(not(target_os = "macos"))]
            {
                let native_view = platform::native_view_ptr(host.window_handle());
                unregister_native_view_browser(native_view);
                platform::release_native_view(native_view);
            }
            host.close_browser(1);
            /*
            CDXC:CefRuntime 2026-07-11:
            CefBrowser drops happen inside gpui entity updates while the
            AppCell is borrowed. Pumping cef::do_message_loop_work() inline
            here ran arbitrary Chromium tasks and CEF handler callbacks
            synchronously in that borrowed context (a handler touching the
            app re-borrows the AppCell and panics), could nest CEF's message
            loop work if the drop itself ran from a scheduled pump step
            (which CEF forbids, and the ObjC-side re-entrancy guard cannot
            see direct calls), and added unbounded main-thread latency to
            the update. Nothing requires the close to complete within drop:
            close_browser(1) only queues the teardown, so ask the external
            message pump (the same scheduling entry
            BrowserProcessHandler::on_schedule_message_pump_work uses) to
            run soon and let CEF process the close on later runloop turns.

            CDXC:CefRuntime 2026-08-24:
            release_native_view above destroys the CEF child view/window
            (removeFromSuperview on macOS, DestroyWindow on Windows), which can complete the whole browser close
            synchronously before close_browser(1) even runs — in that case CEF
            skips DoClose and close_browser is a no-op backstop. Callbacks that
            can fire synchronously in that window (on_before_close) touch only
            thread-local registries, never the gpui App.
            On Linux release_native_view only unmaps and reparents the embed
            host; CEF closes its child before on_before_close removes the host.
            */
            platform::schedule_message_pump_work(0);
        }
    }
}

pub(crate) fn cef_root_cache_path() -> Result<PathBuf> {
    /*
    CDXC:Telemetry 2026-06-23-13:18:
    The explicit CEF root cache path prevents Chromium from falling back to its platform default user-data folder. The built-in Default Browser profile and first-party app-UI surfaces use the durable global context, while generated Browser profiles remain memory-backed.
    */
    let os_default_root = Some(crate::shared_settings::ghostex_storage_paths().cef_cache_dir());
    let path = std::env::var_os("GHOSTEX_GPUI_CEF_CACHE_DIR")
        .map(PathBuf::from)
        .or(os_default_root)
        .unwrap_or_else(|| std::env::temp_dir().join("ghostex-gpui/cef"));
    std::fs::create_dir_all(&path).context("failed to create GPUI CEF root cache directory")?;
    Ok(path)
}

pub(crate) fn cef_request_context_for_profile(profile: &str) -> Result<cef::RequestContext> {
    if profile.starts_with("remote-") {
        return super::remote_browser::remote_browser_request_context(profile);
    }
    if cef_profile_is_workspace(profile) {
        return super::workspace_browser::workspace_browser_request_context(profile);
    }
    /*
    CDXC:Browser 2026-07-16:
    Browser profile ids are app-global rather than project- or tab-scoped. The
    built-in Default profile uses CEF's pre-initialized durable global context,
    so ordinary logins survive app restarts and are visible from every
    Default-profile tab/project. Generated profiles remain separate and
    memory-backed. Workspace profiles (`workspace-<id>`) are disk-backed and
    only reach here after their context reported itself initialized
    (workspace_browser.rs).

    CDXC:CefRuntime 2026-07-09-03:40:
    First-party app-UI surfaces (sidebar, app modal, titlebar panels, project workareas) need durable localStorage for UI state (collapse state, Show more/less, project order), matching how the macOS sidebar WKWebViews use the persistent default WKWebsiteDataStore. They and the built-in Default Browser profile use CEF's global persistent request context, which is initialized with the runtime before synchronous browser creation. Creating a new disk-backed request context here races its asynchronous initialization and causes CreateBrowserSync to return null during app startup. Generated Browser profiles stay memory-backed.
    */
    let profile_segment = cef_profile_cache_segment(profile)
        .unwrap_or("default")
        .to_string();
    if cef_profile_is_app_ui(&profile_segment) || profile_segment == "default" {
        return CEF_GLOBAL_REQUEST_CONTEXT.with(|cached| {
            if let Some(context) = cached.borrow().as_ref() {
                return Ok(context.clone());
            }
            let context = cef::request_context_get_global_context()
                .context("failed to access GPUI CEF global persistent request context")?;
            *cached.borrow_mut() = Some(context.clone());
            Ok(context)
        });
    }
    CEF_REQUEST_CONTEXTS_BY_PROFILE.with(|contexts| {
        if let Some(context) = contexts.borrow().get(&profile_segment) {
            return Ok(context.clone());
        }

        let settings = cef::RequestContextSettings {
            persist_session_cookies: 0,
            ..Default::default()
        };
        let context = cef::request_context_create_context(Some(&settings), None)
            .context("failed to create GPUI CEF profile request context")?;
        contexts
            .borrow_mut()
            .insert(profile_segment, context.clone());
        Ok(context)
    })
}

pub(crate) fn cef_profile_is_app_ui(profile_segment: &str) -> bool {
    matches!(
        profile_segment,
        "gpui-sidebar" | "app-modal" | "session-chat"
    ) || profile_segment.starts_with("titlebar-")
        || profile_segment.starts_with("project-workarea-")
}

pub(crate) fn cef_profile_cache_segment(profile: &str) -> Option<&str> {
    let profile = profile.trim();
    if profile.is_empty() || profile.len() > 64 {
        return None;
    }
    if !profile
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !profile
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    profile
        .bytes()
        .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        .then_some(profile)
}

/// CEF's DevTools port, or 0 for none.
///
/// CDXC:Browser 2026-10-25 DECISION:
/// User (security audit, Q11): the port was always open on 127.0.0.1, so any local program could drive every Ghostex page; the browser skill now goes through gxserver (cef/shell/devtools_relay.rs). The port opens only for development: when a port variable names one (tooling and `cargo xtask` side-by-side runs set them), or when Settings > Debugging > Show debug UI controls was on at launch (then on 9334, inside the tooling's 9333-9343 scan range, or on a portable copy's own port from its port table). The GPUI-specific variable stays a more specific override so side-by-side runs can split ports.
pub(crate) fn remote_debugging_port() -> i32 {
    [
        "GHOSTEX_GPUI_CEF_REMOTE_DEBUGGING_PORT",
        "GHOSTEX_CEF_REMOTE_DEBUGGING_PORT",
    ]
    .iter()
    .find_map(|name| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|port| *port > 0)
    })
    // A portable copy's port table only says which port to use; like any install, it opens one
    // only in debugging mode (review 2026-11-05: portable copies always opened it, with no token).
    .or_else(|| {
        crate::shared_settings::shared_sidebar_settings_snapshot()
            .debugging_mode()
            .then(|| {
                ghostex_paths::portable::ports()
                    .map_or(9334, |ports| i32::from(ports.cef_remote_debugging))
            })
    })
    .unwrap_or(0)
}

pub(crate) fn register_native_view_browser(
    native_view: *mut c_void,
    browser: &cef::Browser,
    uses_system_page_appearance: bool,
    keyboard_zoom_enabled: bool,
) {
    if native_view.is_null() {
        return;
    }

    CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
        browsers
            .borrow_mut()
            .insert(native_view as usize, browser.clone());
    });
    if keyboard_zoom_enabled {
        KEYBOARD_ZOOM_CEF_NATIVE_VIEWS.with(|views| {
            views.borrow_mut().insert(native_view as usize);
        });
    }
    if uses_system_page_appearance {
        SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
            views.borrow_mut().insert(native_view as usize);
        });
    }
}

pub(crate) fn unregister_native_view_browser(native_view: *mut c_void) {
    if native_view.is_null() {
        return;
    }
    #[cfg(target_os = "linux")]
    remove_pointer_focus_handler(native_view);
    remove_page_focus_handler(native_view);

    let closed = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow_mut().remove(&(native_view as usize)));
    if let Some(closed) = closed {
        cef_devtools_relay_browser_closed(closed.identifier());
    }
    KEYBOARD_ZOOM_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    SYSTEM_APP_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    set_cef_native_view_hidden(native_view, false);
    clear_active_native_view_if_matching(native_view);
}

pub(crate) fn refresh_system_page_appearance_for_native_view(native_view: *mut c_void) -> c_int {
    if !native_view.is_null()
        && SYSTEM_APP_PAGE_APPEARANCE_CEF_NATIVE_VIEWS
            .with(|views| views.borrow().contains(&(native_view as usize)))
        && let Some(browser) = CEF_BROWSERS_BY_NATIVE_VIEW
            .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned())
    {
        apply_page_color_scheme(&browser, BrowserPageAppearance::System);
        return 1;
    }
    if native_view.is_null()
        || !SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS
            .with(|views| views.borrow().contains(&(native_view as usize)))
    {
        return 0;
    }
    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };
    apply_browser_page_appearance(&browser);
    1
}

pub(crate) fn clear_active_native_view_if_matching(native_view: *mut c_void) {
    if native_view.is_null() {
        return;
    }

    let _ = ACTIVE_CEF_NATIVE_VIEW.compare_exchange(
        native_view as usize,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}

pub(crate) fn clear_active_native_view() {
    ACTIVE_CEF_NATIVE_VIEW.store(0, Ordering::Release);
}

pub(crate) fn select_all_in_browser(browser: &cef::Browser) -> bool {
    if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
        frame.select_all();
        true
    } else {
        false
    }
}
