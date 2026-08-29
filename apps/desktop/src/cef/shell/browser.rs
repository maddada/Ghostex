// C4 light split: CefBrowser creation/teardown, page-appearance
// application, profile/cache-path resolution (including the app-UI profile
// cache-path persistence logic), the native-view<->browser registry, and the
// browser-level DevTools/ContextMenu/Find/LifeSpan/Display/Permission
// handler impls. Pure move out of `cef/shell.rs`. See
// docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

pub(crate) fn show_browser_dev_tools(
    browser: Option<&mut cef::Browser>,
    inspect_element_at: Option<&cef::Point>,
) -> bool {
    let Some(browser) = browser else {
        return false;
    };
    let Some(host) = browser.host() else {
        return false;
    };
    let window_info = cef::WindowInfo {
        window_name: cef::CefString::from("Chromium DevTools"),
        ..Default::default()
    };
    let browser_settings = cef::BrowserSettings::default();
    let mut devtools_client = Some(GhostexGpuiCefClient::new(
        Some(GhostexGpuiLifeSpanHandler::new(None, None, true)),
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
        Some(GhostexGpuiCefFocusHandler::new()),
        None,
        None,
    ));
    host.show_dev_tools(
        Some(&window_info),
        devtools_client.as_mut(),
        Some(&browser_settings),
        inspect_element_at,
    );
    true
}

wrap_task! {
    pub(crate) struct GhostexRegisterDevToolsNativeView {
        browser: cef::Browser,
    }

    impl Task {
        fn execute(&self) {
            let Some(host) = self.browser.host() else {
                return;
            };
            let native_view = platform::native_view_ptr(host.window_handle());
            platform::prepare_native_view_for_focus(native_view);
            register_native_view_browser(native_view, &self.browser, false, false);
            /*
            CDXC:GPUICefDevToolsFocus 2026-07-15:
            OnAfterCreated precedes native DevTools window attachment on macOS,
            so its host handle can still be null. A CEF UI task runs after that
            creation callback, at which point the final native root can be
            registered before the explicit OS/Chromium focus grant. This keeps
            Copy/Paste on DevTools' real responder chain without broad routing.
            */
            #[cfg(target_os = "macos")]
            platform::activate_native_view_window(native_view);
            platform::focus_native_view(native_view);
            host.set_focus(1);
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.cef.nativeViewBrowserRegistered",
                serde_json::json!({
                    "browserId": self.browser.identifier(),
                    "isPopup": self.browser.is_popup() != 0,
                    "nativeViewWasNull": native_view.is_null(),
                    "explicitFocusGranted": !native_view.is_null(),
                }),
            );
        }
    }
}

wrap_context_menu_handler! {
    pub(crate) struct GhostexGpuiContextMenuHandler {
        popup_open_handler: Option<BrowserPopupOpenHandler>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let Some(model) = model else {
                return;
            };
            /*
            CDXC:GPUICefContextMenuParity 2026-07-10:
            Match the production macOS CEF browser menu by preserving CEF's
            normal page/edit/link commands and appending one real Inspect
            Element command. This remains Chromium-owned menu UI and does not
            add GPUI overlays, hit-test routing, or page-content logging.
            */
            if model.count() > 0 {
                model.add_separator();
            }
            model.add_item(
                CEF_CONTEXT_MENU_INSPECT_ELEMENT_COMMAND_ID,
                Some(&CefString::from("Inspect Element")),
            );
        }

        fn on_context_menu_command(
            &self,
            browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: c_int,
            _event_flags: EventFlags,
        ) -> c_int {
            if command_id == CEF_CONTEXT_MENU_INSPECT_ELEMENT_COMMAND_ID {
                let inspect_point = params.as_deref().map(|params| cef::Point {
                    x: params.xcoord(),
                    y: params.ycoord(),
                });
                return show_browser_dev_tools(browser, inspect_point.as_ref()) as c_int;
            }

            if !matches!(
                command_id,
                CEF_CONTEXT_MENU_OPEN_LINK_NEW_TAB_COMMAND_ID
                    | CEF_CONTEXT_MENU_OPEN_LINK_NEW_WINDOW_COMMAND_ID
            ) {
                return 0;
            }
            let (Some(popup_open_handler), Some(params)) =
                (self.popup_open_handler.as_ref(), params)
            else {
                return 0;
            };
            let unfiltered = params.unfiltered_link_url();
            let mut requested_url = CefString::from(&unfiltered).to_string();
            if requested_url.trim().is_empty() {
                let filtered = params.link_url();
                requested_url = CefString::from(&filtered).to_string();
            }
            let requested_url = requested_url.trim();
            if requested_url.is_empty() {
                return 0;
            }
            popup_open_handler(requested_url.to_string(), BrowserPopupPlacement::Selected);
            1
        }
    }
}

wrap_find_handler! {
    pub(crate) struct GhostexGpuiFindHandler {
        page_metadata_handler: BrowserPageMetadataHandler,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut cef::Browser>,
            _identifier: c_int,
            match_count: c_int,
            _selection_rect: Option<&cef::Rect>,
            active_match_ordinal: c_int,
            final_update: c_int,
        ) {
            (self.page_metadata_handler)(BrowserPageMetadataEvent::FindResult {
                match_count,
                active_match_ordinal,
                final_update: final_update != 0,
            });
        }
    }
}

wrap_life_span_handler! {
    pub(crate) struct GhostexGpuiLifeSpanHandler {
        popup_open_handler: Option<BrowserPopupOpenHandler>,
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
        register_created_native_view: bool,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
            if !self.register_created_native_view {
                return;
            }
            let Some(browser) = browser else {
                return;
            };
            let mut task = GhostexRegisterDevToolsNativeView::new(browser.clone());
            post_task(ThreadId::UI, Some(&mut task));
        }

        fn do_close(&self, _browser: Option<&mut cef::Browser>) -> c_int {
            /*
            CDXC:GPUIResourcesTitlebar 2026-07-09:
            All GPUI CEF browsers are child NSViews inside app-owned GPUI
            windows. CEF's default DoClose flow (returning 0) sends a native
            close to the browser's top-level host window, so dropping any
            short-lived browser (e.g. the fresh-per-open titlebar Resources
            panel) closed the MAIN window and the quit-on-last-window hook
            then terminated the whole app. Return handled: browser teardown
            is fully owned by `CefBrowser::drop`, and the host GPUI window
            must never receive a close from CEF.

            CDXC:GPUICefCloseContract 2026-08-24:
            Returning handled here does NOT end the close on its own — per
            cef_life_span_handler.h the app must still complete it by
            proceeding with window/view-hierarchy tear-down, or the browser is
            left partially closed and its renderer process never exits. That
            step is `CefBrowser::drop` calling `platform::release_native_view`,
            which removes the CEF child view from its superview (macOS) or
            destroys the embed-host window (Linux). Drop is only "fully owning"
            teardown because it performs that removal; do not turn
            release_native_view back into a no-op.

            CDXC:GPUIBrowserAgentClose 2026-08-21:
            DevTools Target.closeTarget and /json/close enter through this
            same CEF close request. Browser panes must hand that request back
            to the GPUI tab model before returning handled; otherwise CEF
            accepts the request but the app-owned pane remains. App-initiated
            closes may report this during teardown too, and the model close
            path deliberately treats that as a no-op.
            */
            if let Some(handler) = self.page_metadata_handler.as_ref() {
                handler(BrowserPageMetadataEvent::CloseRequested);
            }
            1
        }

        fn on_before_close(&self, browser: Option<&mut cef::Browser>) {
            /*
            CDXC:GPUICefTeardownRegistry 2026-07-11:
            The main-thread native-view registries (CEF_BROWSERS_BY_NATIVE_VIEW,
            HIDDEN_CEF_NATIVE_VIEWS, ACTIVE_CEF_NATIVE_VIEW) were cleaned up
            only by `CefBrowser::drop`, so a browser torn down by CEF itself
            (renderer crash, Chromium-destroyed window) left dangling entries.
            ACTIVE_CEF_NATIVE_VIEW is set on every mouseDown and later
            dereferenced as an NSView pointer by
            select_all_for_active_native_view, so a stale entry is a
            use-after-free. on_before_close is CEF's last callback before the
            browser window is destroyed and runs on the CEF UI thread, which
            is the main thread under the external message pump — the same
            thread that owns these thread_local registries.
            unregister_native_view_browser is idempotent, so the Drop path
            may run it again for app-initiated closes.
            */
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return;
            };
            unregister_native_view_browser(platform::native_view_ptr(host.window_handle()));
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            /*
            CDXC:GPUIBrowserPopups 2026-06-22-07:14:
            Browser-mode target=_blank and window.open requests must stay inside the GPUI Browser workspace. Intercept CEF popup creation through cef-rs LifeSpanHandler, forward only the requested target URL to the shell tab model, and return handled so Chromium does not create a separate native CEF window.

            CDXC:GPUIBrowserPopups 2026-06-23-11:43:
            Match native macOS CEF popup policy: empty target URLs are handled here without dispatching a shell popup callback because there is no transferable URL/content and no fallback transfer path. Non-empty targets remain shell-owned Browser tab requests.
            */
            if let Some(no_javascript_access) = no_javascript_access {
                *no_javascript_access = 1;
            }

            if let (Some(popup_open_handler), Some(requested_url)) = (
                self.popup_open_handler.as_ref(),
                browser_popup_target_url_for_shell(target_url),
            ) {
                (popup_open_handler)(requested_url, BrowserPopupPlacement::Selected);
            }
            1
        }
    }
}

wrap_display_handler! {
    pub(crate) struct GhostexGpuiDisplayHandler {
        page_metadata_handler: BrowserPageMetadataHandler,
        suppress_initial_about_blank: Cell<bool>,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            /*
            CDXC:GPUIBrowserMetadata 2026-06-22-07:23:
            Browser-tab URL state must be driven by CEF's DisplayHandler rather than synthetic shell guesses. Forward only main-frame address changes to the GPUI tab model, where raw runtime URLs can update the active address field while persistence remains guarded by the existing sanitizer.
            */
            if let Some(frame) = frame
                && frame.is_main() == 0
            {
                return;
            }

            let url = url.map(CefString::to_string).unwrap_or_default();
            if self.suppress_initial_about_blank.get() {
                if url.eq_ignore_ascii_case("about:blank") {
                    return;
                }
                self.suppress_initial_about_blank.set(false);
            }
            (self.page_metadata_handler)(BrowserPageMetadataEvent::AddressChanged(url));
        }

        fn on_title_change(&self, _browser: Option<&mut cef::Browser>, title: Option<&CefString>) {
            /*
            CDXC:GPUIBrowserMetadata 2026-06-22-07:23:
            Page titles may contain user-owned content, so CEF title callbacks may update only runtime tab-strip presentation. The GPUI shell-state writer must continue deriving restored titles from sanitized URLs instead of storing raw page titles.
            */
            let title = title.map(CefString::to_string).unwrap_or_default();
            (self.page_metadata_handler)(BrowserPageMetadataEvent::TitleChanged(title));
        }

        fn on_favicon_urlchange(
            &self,
            _browser: Option<&mut cef::Browser>,
            icon_urls: Option<&mut cef::CefStringList>,
        ) {
            /*
            CDXC:GPUIBrowserFavicons 2026-06-22-09:11:
            CEF favicon URL callbacks are runtime browser metadata only. Forward a single representative non-empty URL so GPUI browser chrome and sidebar sessions can show favicon presence, but keep bitmap download/cache and shell-state persistence of favicon URLs out of this slice.
            */
            let representative_url = icon_urls.and_then(|icon_urls| {
                // `CefStringList::clone` changes a mutable borrowed list into a
                // non-iterable immutable wrapper in cef-rs. Move the callback's
                // borrowed wrapper out instead so the URLs CEF supplied remain
                // visible to the iterator for the lifetime of this callback.
                let icon_urls = std::mem::take(icon_urls);
                icon_urls.into_iter().find_map(|url| {
                    let url = url.trim().to_string();
                    if url.is_empty() { None } else { Some(url) }
                })
            });
            (self.page_metadata_handler)(BrowserPageMetadataEvent::FaviconUrlChanged(
                representative_url,
            ));
        }
    }
}

wrap_drag_handler! {
    pub(crate) struct GhostexGpuiSessionChatDragHandler;

    impl DragHandler {
        fn on_drag_enter(
            &self,
            browser: Option<&mut cef::Browser>,
            drag_data: Option<&mut DragData>,
            _mask: DragOperationsMask,
        ) -> c_int {
            /*
            CDXC:GPUISessionChatDropPaths 2026-08-29:
            Chromium never exposes an OS file drag's absolute paths to the
            page, so the browser process is the only place a Session Chat
            drop can resolve to real local paths (folders included). Publish
            the drag's paths onto the bundled chat page's `ghostexGpui`
            namespace at drag-enter — a non-file drag publishes the empty
            list, clearing any earlier drag's paths — and let the drop itself
            proceed normally. Installed only for Session Chat surfaces; the
            page-side transport reads the paths only for a session running on
            this machine, so a remote chat keeps uploading bytes and never
            hands this machine's paths to an agent elsewhere.
            */
            let paths: Vec<String> = drag_data
                .filter(|drag_data| drag_data.is_file() == 1)
                .map(|drag_data| {
                    let mut file_paths = cef::CefStringList::default();
                    if drag_data.file_paths(Some(&mut file_paths)) == 1 {
                        file_paths
                            .into_iter()
                            .filter(|path| !path.trim().is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    }
                })
                .unwrap_or_default();
            if let Some(browser) = browser
                && let Some(frame) = browser.main_frame()
                && let Ok(paths_json) = serde_json::to_string(&paths)
            {
                let script = format!(
                    "window.ghostexGpui = window.ghostexGpui || {{}}; \
                     window.ghostexGpui.sessionChatDropPaths = {paths_json};"
                );
                frame.execute_java_script(
                    Some(&CefString::from(script.as_str())),
                    Some(&CefString::from(BROWSER_APP_OWNED_SCRIPT_URL)),
                    1,
                );
            }
            0
        }
    }
}

wrap_permission_handler! {
    pub(crate) struct GhostexGpuiPermissionHandler {
        allow_first_party_loopback_requests: bool,
        trusted_clipboard_origin: Option<String>,
        media_access_handler: Option<BrowserMediaAccessHandler>,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> c_int {
            /*
            CDXC:GPUIBrowserMediaPermissions 2026-07-27:
            Only device microphone/camera requests are answered by the shell;
            desktop capture bits keep CEF's default deny so a mixed request can
            never grant screen capture as a side effect of a microphone
            decision. Surfaces without a media handler (sidebar and editor)
            also keep default handling.
            */
            let Some(handler) = self.media_access_handler.clone() else {
                return 0;
            };
            let kinds = BrowserMediaAccessKinds {
                microphone: requested_permissions
                    & MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw() as u32
                    != 0,
                camera: requested_permissions
                    & MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw() as u32
                    != 0,
            };
            if kinds.is_empty() {
                return 0;
            }
            let Some(callback) = callback else {
                return 0;
            };
            handler(BrowserMediaAccessRequest {
                requesting_origin: requesting_origin
                    .map(CefString::to_string)
                    .unwrap_or_default(),
                kinds,
                callback: Some(callback.clone()),
            });
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut cef::Browser>,
            _prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> c_int {
            /*
            CDXC:GPUIWindowsLoopbackPermission 2026-08-04:
            Current Windows CEF asks for LOCAL_NETWORK_ACCESS, LOCAL_NETWORK,
            or LOOPBACK_NETWORK before a bundled file:// app surface may call
            the authenticated loopback gxserver API. Alloy has no permission
            UI for these hidden first-party surfaces, so leaving the prompt to
            default handling strands fetch (and therefore sleeping-session
            wake) indefinitely.
            Accept only a pure local-network request on surfaces that were
            explicitly constructed with the sidebar gxserver bridge/bootstrap;
            Browser, editor, project-workarea, and modal surfaces keep their
            existing permission behavior.
            */
            let local_network_permissions =
                PermissionRequestTypes::LOCAL_NETWORK_ACCESS.get_raw() as u32
                    | PermissionRequestTypes::LOCAL_NETWORK.get_raw() as u32
                    | PermissionRequestTypes::LOOPBACK_NETWORK.get_raw() as u32;
            if self.allow_first_party_loopback_requests
                && requested_permissions & local_network_permissions != 0
                && requested_permissions & !local_network_permissions == 0
            {
                let Some(callback) = callback else {
                    return 0;
                };
                crate::support_logs::append(
                    crate::support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.cef.firstPartyLoopbackPermissionAccepted",
                    serde_json::json!({
                        "requestedPermissions": requested_permissions,
                    }),
                );
                callback.cont(PermissionRequestResult::ACCEPT);
                return 1;
            }
            /*
            macOS `GhostexCEFBrowserClient::OnShowPermissionPrompt` parity: only
            clipboard prompts are decided here (anything else keeps CEF's
            default handling), and clipboard is granted only when the request
            carries no other permission bits and the requesting origin matches
            this surface's trusted code-server origin. Embedded VS Code runs in
            CEF Alloy, whose default permission handling ignores clipboard
            prompts, so without this the code-server clipboard silently fails.
            */
            let Some(trusted_clipboard_origin) = self.trusted_clipboard_origin.as_deref() else {
                return 0;
            };
            let clipboard_permission = PermissionRequestTypes::CLIPBOARD.get_raw() as u32;
            if requested_permissions & clipboard_permission == 0 {
                return 0;
            }
            let Some(callback) = callback else {
                return 0;
            };
            let requesting_origin = requesting_origin
                .map(CefString::to_string)
                .unwrap_or_default();
            let unsupported_permissions = requested_permissions & !clipboard_permission;
            let should_accept = unsupported_permissions == 0
                && cef_origins_match(&requesting_origin, trusted_clipboard_origin);
            callback.cont(if should_accept {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
            1
        }
    }
}
pub(crate) fn apply_browser_page_appearance(browser: &cef::Browser) {
    let Some(host) = browser.host() else {
        return;
    };
    /*
    CDXC:GPUIBrowserPageAppearanceParity 2026-07-10:
    Public Browser tabs match the production macOS CEF host: the page receives
    the current system prefers-color-scheme while Chromium's unspecified
    document canvas stays Chrome-like white. This is renderer state only; it
    does not persist per-origin appearance, inject page CSS, or add a fallback
    rendering path.
    */
    let mut media_params = match cef::dictionary_value_create() {
        Some(params) => params,
        None => return,
    };
    media_params.set_string(Some(&CefString::from("media")), Some(&CefString::from("")));
    let mut features = match cef::list_value_create() {
        Some(features) => features,
        None => return,
    };
    let mut feature = match cef::dictionary_value_create() {
        Some(feature) => feature,
        None => return,
    };
    feature.set_string(
        Some(&CefString::from("name")),
        Some(&CefString::from("prefers-color-scheme")),
    );
    feature.set_string(
        Some(&CefString::from("value")),
        Some(&CefString::from(
            if platform::system_uses_dark_page_appearance() {
                "dark"
            } else {
                "light"
            },
        )),
    );
    features.set_dictionary(0, Some(&mut feature));
    media_params.set_list(Some(&CefString::from("features")), Some(&mut features));
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from("Emulation.setEmulatedMedia")),
        Some(&mut media_params),
    );

    let mut background_params = match cef::dictionary_value_create() {
        Some(params) => params,
        None => return,
    };
    let mut color = match cef::dictionary_value_create() {
        Some(color) => color,
        None => return,
    };
    for (key, value) in [("r", 255), ("g", 255), ("b", 255)] {
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
    pub(crate) uses_system_page_appearance: bool,
    pub(crate) extension_bridge_installed: bool,
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
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
        media_access_handler: Option<BrowserMediaAccessHandler>,
        sidebar_runtime_settings: Option<SidebarRuntimeSettingsSnapshot>,
        sidebar_gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
        sidebar_bridge_event_handler: Option<SidebarBridgeEventHandler>,
        project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        manage_docs_resource_scope: Option<ManageDocsResourceScope>,
        app_modal_host_bridge_surface: Option<AppModalHostBridgeSurface>,
        app_modal_host_bridge_event_handler: Option<AppModalHostBridgeEventHandler>,
        extension_bridge_surface: Option<ExtensionBridgeSurfaceSpec>,
        extension_bridge_event_handler: Option<ExtensionBridgeEventHandler>,
        page_load_end_handler: Option<PageLoadEndHandler>,
    ) -> Result<Self, String> {
        let keyboard_zoom_enabled = page_metadata_handler.is_some()
            || project_workarea_bridge_event_handler.is_some()
            || app_modal_host_bridge_surface == Some(AppModalHostBridgeSurface::SessionChat);
        /*
        CDXC:GPUICefBrowserCreateFallible 2026-07-11:
        CreateBrowserSync returns null when the per-profile request context's
        asynchronous initialization has not completed yet (the same race the
        app-ui profiles dodge via the pre-initialized global context — see
        CDXC:GPUIAppUiPersistence 2026-07-09). This used to be an `.expect`
        that hard-crashed the whole app (five "failed to create cef-rs child
        browser" aborts on 2026-07-10, all from fresh browser
        profile contexts). Creation is now fallible; ensure-style callers skip
        the surface for this pass and naturally create it on their next
        reconcile once the context finishes initializing.
        */
        let initial_bounds = cef::Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
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
        let allow_first_party_loopback_requests =
            sidebar_bridge_installed_for_handler(sidebar_bridge_event_handler.is_some())
                || sidebar_gxserver_bootstrap.is_some();
        let mut browser_settings = cef::BrowserSettings::default();
        if trusted_clipboard_origin.is_some() {
            browser_settings.javascript_access_clipboard = State::ENABLED;
            browser_settings.javascript_dom_paste = State::ENABLED;
        }
        let requested_url = url.to_string();
        if let Some(expected_surface) = app_modal_host_bridge_surface
            && app_modal_host_bridge_surface_for_frame_url(&requested_url) != Some(expected_surface)
        {
            return Err("app-modal CEF surface does not match its first-party entry URL".into());
        }
        /*
        CDXC:GPUIExtensionRemoteUrlSurface 2026-08-28:
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
        CDXC:GPUIBrowserMediaPermissions 2026-07-27:
        The permission handler now serves independent surfaces: the
        code-server clipboard grant (trusted origin only) and Browser-pane
        microphone/camera prompts, plus bundled sidebar/session-chat loopback
        access. Install it when any is in play, and keep the decisions
        independent inside the handler.
        */
        let permission_handler = (allow_first_party_loopback_requests
            || trusted_clipboard_origin.is_some()
            || media_access_handler.is_some())
        .then(|| {
            GhostexGpuiPermissionHandler::new(
                allow_first_party_loopback_requests,
                trusted_clipboard_origin.clone(),
                media_access_handler,
            )
        });
        let context_menu_handler = GhostexGpuiContextMenuHandler::new(popup_open_handler.clone());
        let display_handler = page_metadata_handler.as_ref().map(|handler| {
            GhostexGpuiDisplayHandler::new(handler.clone(), Cell::new(uses_system_page_appearance))
        });
        let find_handler = page_metadata_handler
            .as_ref()
            .map(|handler| GhostexGpuiFindHandler::new(handler.clone()));
        let manage_docs_resource_base_url = manage_docs_resource_scope
            .as_ref()
            .map(|scope| scope.base_url().to_string());
        let is_shared_sidebar_surface =
            sidebar_bridge_installed_for_handler(sidebar_bridge_event_handler.is_some());
        let request_handler = manage_docs_resource_scope
            .as_ref()
            .map(ManageDocsResourceScope::request_handler)
            .or_else(|| {
                is_shared_sidebar_surface.then(GhostexGpuiSidebarRendererRequestHandler::new)
            })
            .or_else(|| {
                // Browser panes are the only surface with a shell popup path,
                // so they are the only ones that turn middle-click and
                // Cmd/Ctrl-click link opens into Browser tabs.
                popup_open_handler
                    .clone()
                    .map(GhostexGpuiBrowserRequestHandler::new)
            });
        let browser_lifecycle_handler = page_metadata_handler.clone();
        let load_handler = if let Some(surface) = extension_bridge_surface
            .clone()
            .filter(|_| extension_bridge_installed)
        {
            Some(GhostexGpuiExtensionBridgeLoadHandler::new(surface))
        } else if let Some(page_load_end_handler) = page_load_end_handler {
            /*
            CDXC:GPUITutorialVideoFullscreen 2026-08-18:
            Only bridge-less third-party surfaces (the tutorial video modal)
            pass this handler, so it can never displace the sidebar,
            session-chat, workarea, or Browser load handlers below.
            */
            Some(GhostexGpuiPageLoadEndHandler::new(page_load_end_handler))
        } else if sidebar_bridge_installed_for_handler(sidebar_bridge_event_handler.is_some()) {
            Some(GhostexGpuiSidebarProjectContextLoadHandler::new(
                sidebar_runtime_settings.unwrap_or_default(),
                sidebar_gxserver_bootstrap,
            ))
        } else if sidebar_gxserver_bootstrap.is_some() {
            /*
            CDXC:GPUISessionChatSurface 2026-07-31:
            A bootstrap without the sidebar bridge handler identifies the
            per-session Session Chat surface: it gets only the bootstrap
            install message so the bundled chat page can reach the local
            gxserver, while Browser, workarea, and modal clients keep passing
            no bootstrap at all.
            */
            Some(GhostexGpuiSessionChatGxserverBootstrapLoadHandler::new(
                sidebar_gxserver_bootstrap,
            ))
        } else if project_workarea_bridge_event_handler.is_some() {
            Some(GhostexGpuiProjectWorkareaBridgeLoadHandler::new(
                manage_docs_resource_base_url,
            ))
        } else {
            page_metadata_handler.map(GhostexGpuiBrowserPageLoadHandler::new)
        };
        // Session Chat pages resolve local drops to real absolute paths
        // published by the shell at drag-enter (Chromium hides them from the
        // page); every other surface keeps CEF's default drag behavior.
        let drag_handler = (app_modal_host_bridge_surface
            == Some(AppModalHostBridgeSurface::SessionChat))
        .then(GhostexGpuiSessionChatDragHandler::new);
        // Every GPUI CEF browser needs the client's life-span handler so
        // DoClose is always handled and CEF can never close the host GPUI
        // window when a browser is dropped.
        let mut client = Some(GhostexGpuiCefClient::new(
            Some(GhostexGpuiLifeSpanHandler::new(
                popup_open_handler,
                browser_lifecycle_handler,
                false,
            )),
            Some(context_menu_handler),
            display_handler,
            find_handler,
            load_handler,
            sidebar_bridge_event_handler,
            project_workarea_bridge_event_handler,
            app_modal_host_bridge_event_handler,
            extension_bridge_surface,
            extension_bridge_event_handler,
            request_handler,
            permission_handler,
            Some(GhostexGpuiCefFocusHandler::new()),
            keyboard_zoom_handler(keyboard_zoom_enabled),
            drag_handler,
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
        if let Some(host) = browser.host() {
            let native_view = platform::native_view_ptr(host.window_handle());
            platform::prepare_native_view_for_focus(native_view);
            /*
            CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
            The shared sidebar is chrome, not a work surface: clicking its
            background must never pull the keyboard away from the active
            terminal/pane. Mark exactly this surface mouse-focus passive so
            the AppKit focus subclass stops claiming first responder on its
            mouse-downs; keyboard focus arrives only through the fixed
            editable-focus bridge grant when the page focuses a text input.
            */
            if is_shared_sidebar_surface {
                platform::set_native_view_mouse_focus_passive(native_view, true);
            }
            register_native_view_browser(
                native_view,
                &browser,
                uses_system_page_appearance,
                keyboard_zoom_enabled,
            );
        }
        if uses_system_page_appearance {
            apply_browser_page_appearance(&browser);
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(requested_url.as_str())));
            }
        }

        Ok(Self {
            browser: RefCell::new(browser),
            _client: client,
            _request_context: request_context,
            last_bounds: RefCell::new(None),
            last_visible: Cell::new(None),
            uses_system_page_appearance,
            extension_bridge_installed,
        })
    }

    pub fn extension_bridge_installed(&self) -> bool {
        self.extension_bridge_installed
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
        CDXC:GPUICefNativeViewFrame 2026-06-14-15:25:
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

    #[cfg(target_os = "macos")]
    pub fn native_view(&self) -> Option<*mut c_void> {
        let browser = self.browser.borrow();
        browser
            .host()
            .map(|host| platform::native_view_ptr(host.window_handle()))
    }

    pub fn set_visible(&self, visible: bool) {
        if self.last_visible.get() == Some(visible) {
            return;
        }
        // CDXC:SessionChatFocusDiagnostics 2026-08-24: hiding a CEF surface
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

    pub fn order_front(&self) {
        /*
        CDXC:GPUITitlebarDropdownZOrder 2026-07-09:
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

    pub fn focus(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:GPUICefFocusRouting 2026-06-14-16:31:
        Web-page text fields inside CEF must regain both native focus ownership (AppKit first responder / Win32 keyboard focus) and Chromium browser focus after GPUI chrome has been focused. Without this handoff, command shortcuts such as Cmd+A can stay routed to GPUI instead of selecting text in the active page input.
        */
        platform::focus_native_view(platform::native_view_ptr(host.window_handle()));
        host.set_focus(1);
    }

    pub fn paste(&self) -> bool {
        self.focus();
        let browser = self.browser.borrow();
        edit_command_in_browser(&browser, CefEditCommand::Paste)
    }

    pub fn blur(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:GPUIBrowserLifecycle 2026-06-23-11:32:
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

    /*
    CDXC:GPUITutorialVideoFullscreen 2026-08-18:
    The tutorial modal loads the YouTube watch page as its own top-level CEF
    document, so the app cannot put its player in fullscreen from injected
    JavaScript: Chromium's Fullscreen API requires a transient user
    activation, and app-owned `execute_java_script` runs without one (the
    `requestFullscreen()` promise is rejected outright). Sending the key
    through the browser host instead feeds Chromium's real input pipeline, so
    the page sees a trusted keydown with user activation and runs its own "f"
    shortcut. "f" toggles, so callers must send this exactly once per loaded
    page. This carries no page data and does not persist or log anything.
    */
    pub fn send_fullscreen_toggle_key(&self) {
        // Windows virtual key code for F; Chromium derives DOM `code`/`key`
        // from this plus the platform-native code below.
        const VK_F: c_int = 0x46;
        #[cfg(target_os = "macos")]
        const NATIVE_F_KEY_CODE: c_int = 3; // kVK_ANSI_F
        #[cfg(target_os = "linux")]
        const NATIVE_F_KEY_CODE: c_int = 41; // X11 keycode for F
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        const NATIVE_F_KEY_CODE: c_int = 0;

        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        let mut event = cef::KeyEvent {
            size: std::mem::size_of::<cef::sys::cef_key_event_t>(),
            type_: cef::KeyEventType::RAWKEYDOWN,
            modifiers: 0,
            windows_key_code: VK_F,
            native_key_code: NATIVE_F_KEY_CODE,
            is_system_key: 0,
            character: b'f' as u16,
            unmodified_character: b'f' as u16,
            focus_on_editable_field: 0,
        };
        host.send_key_event(Some(&event));
        // CEF's char event carries the produced character in the key code.
        event.type_ = cef::KeyEventType::CHAR;
        event.windows_key_code = b'f' as c_int;
        host.send_key_event(Some(&event));
        event.type_ = cef::KeyEventType::KEYUP;
        event.windows_key_code = VK_F;
        host.send_key_event(Some(&event));
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
        CDXC:GPUIBrowserFeedback 2026-06-23-11:04:
        GPUI Browser feedback tools now use CEF's normal main-frame JavaScript execution path for app-owned injection scripts. Pass a synthetic script URL and return only main-frame availability so this backend does not log page URLs, titles, script bodies, user content, JS errors, cookies, tokens, paths, command text, or terminal content.

        CDXC:GPUICefAppOwnedScriptFocus 2026-07-15:
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

    pub fn refresh_sidebar_runtime_settings(
        &self,
        runtime_settings: SidebarRuntimeSettingsSnapshot,
    ) {
        let browser = self.browser.borrow();
        let Some(mut frame) = browser.main_frame() else {
            return;
        };
        send_sidebar_runtime_settings_process_message(
            &mut frame,
            SIDEBAR_RUNTIME_SETTINGS_UPDATE_MESSAGE_NAME,
            runtime_settings,
        );
    }

    pub fn refresh_sidebar_gxserver_bootstrap(
        &self,
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    ) {
        let browser = self.browser.borrow();
        let Some(mut frame) = browser.main_frame() else {
            return;
        };
        send_sidebar_gxserver_bootstrap_process_message(&mut frame, gxserver_bootstrap);
    }

    pub fn refresh_session_chat_gxserver_bootstrap(
        &self,
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:GPUISessionChatSurface 2026-07-31:
        Session Chat surfaces refresh through their dedicated bootstrap
        message because the sidebar update path refuses pages without the
        installed sidebar bridge. Same scope rules as the sidebar refresh:
        app-owned snapshot only, main frame only, never logged or persisted.
        */
        let browser = self.browser.borrow();
        let Some(mut frame) = browser.main_frame() else {
            return;
        };
        send_session_chat_gxserver_bootstrap_process_message(&mut frame, gxserver_bootstrap);
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
        CDXC:GPUIBrowserToolbar 2026-06-22-11:59:
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
        CDXC:GPUIBrowserToolbar 2026-06-22-11:50:
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
        if let Some(host) = self.browser.borrow().host() {
            let native_view = platform::native_view_ptr(host.window_handle());
            unregister_native_view_browser(native_view);
            platform::release_native_view(native_view);
            host.close_browser(1);
            /*
            CDXC:GPUICefDropPumpReentrancy 2026-07-11:
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

            CDXC:GPUICefCloseContract 2026-08-24:
            release_native_view above destroys the CEF child view/window
            (removeFromSuperview on macOS, DestroyWindow on Windows, embed-host
            destroy on Linux), which can complete the whole browser close
            synchronously before close_browser(1) even runs — in that case CEF
            skips DoClose and close_browser is a no-op backstop. Callbacks that
            can fire synchronously in that window (on_before_close) touch only
            thread-local registries, never the gpui App.
            */
            platform::schedule_message_pump_work(0);
        }
    }
}

pub(crate) fn cef_root_cache_path() -> Result<PathBuf> {
    /*
    CDXC:GPUIPrivacyAudit 2026-06-23-13:18:
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
    /*
    CDXC:GPUIBrowserProfilePersistence 2026-07-16:
    Browser profile ids are app-global rather than project- or tab-scoped. The
    built-in Default profile uses CEF's pre-initialized durable global context,
    so ordinary logins survive app restarts and are visible from every
    Default-profile tab/project. Generated profiles remain separate and
    memory-backed.

    CDXC:GPUIAppUiPersistence 2026-07-09-03:40:
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

pub(crate) fn remote_debugging_port() -> i32 {
    // Tooling (browser-use MCP, macOS app scripts) sets the shared
    // GHOSTEX_CEF_REMOTE_DEBUGGING_PORT; the GPUI-specific name stays as a
    // more-specific override so side-by-side runs can split ports. The
    // default 9334 stays inside the tooling's 9333-9343 scan range.
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
    .unwrap_or(9334)
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

    CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
        browsers.borrow_mut().remove(&(native_view as usize));
    });
    KEYBOARD_ZOOM_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    set_cef_native_view_hidden(native_view, false);
    clear_active_native_view_if_matching(native_view);
    let _ = SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.compare_exchange(
        native_view as usize,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}

pub(crate) fn refresh_system_page_appearance_for_native_view(native_view: *mut c_void) -> c_int {
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
