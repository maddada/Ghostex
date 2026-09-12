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
            CDXC:FocusRouting 2026-07-15:
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
            CDXC:ContextMenus 2026-07-10:
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
            CDXC:Resources 2026-07-09:
            All GPUI CEF browsers are child NSViews inside app-owned GPUI
            windows. CEF's default DoClose flow (returning 0) sends a native
            close to the browser's top-level host window, so dropping any
            short-lived browser (e.g. the fresh-per-open titlebar Resources
            panel) closed the MAIN window and the quit-on-last-window hook
            then terminated the whole app. Return handled: browser teardown
            is fully owned by `CefBrowser::drop`, and the host GPUI window
            must never receive a close from CEF.

            CDXC:CefRuntime 2026-08-24:
            Returning handled here does NOT end the close on its own — per
            cef_life_span_handler.h the app must still complete it by
            proceeding with window/view-hierarchy tear-down, or the browser is
            left partially closed and its renderer process never exits. That
            step is `CefBrowser::drop` calling `platform::release_native_view`,
            which removes the CEF child view from its superview (macOS) or
            destroys the embed-host window (Linux). Drop is only "fully owning"
            teardown because it performs that removal; do not turn
            release_native_view back into a no-op.

            CDXC:Browser 2026-08-21:
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
            CDXC:CefRuntime 2026-07-11:
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
            CDXC:Browser 2026-06-22-07:14:
            Browser-mode target=_blank and window.open requests must stay inside the GPUI Browser workspace. Intercept CEF popup creation through cef-rs LifeSpanHandler, forward only the requested target URL to the shell tab model, and return handled so Chromium does not create a separate native CEF window.

            CDXC:Browser 2026-06-23-11:43:
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
            CDXC:Browser 2026-06-22-07:23:
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
            CDXC:Browser 2026-06-22-07:23:
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
            CDXC:Browser 2026-06-22-09:11:
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
            CDXC:Clipboard 2026-08-29:
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
        trusted_loopback_entry_identity: Option<String>,
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
            CDXC:Browser 2026-07-27:
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
            browser: Option<&mut cef::Browser>,
            _prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> c_int {
            /*
            CDXC:PlatformSupport 2026-08-04:
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
            let requesting_origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let trusted_loopback_request = self.trusted_loopback_entry_identity.as_deref().is_some_and(|entry_identity| {
                browser.as_ref().and_then(|browser| browser.main_frame()).is_some_and(|frame| {
                    let frame_url = CefString::from(&frame.url()).to_string();
                    first_party_loopback_request_matches(entry_identity, &frame_url, &requesting_origin)
                })
            });
            if trusted_loopback_request
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
