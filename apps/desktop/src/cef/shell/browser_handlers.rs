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
        Some(GhostexGpuiLifeSpanHandler::new(
            None,
            None,
            true,
            StdRc::new(Cell::new(false)),
        )),
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

fn context_menu_link_url(params: &ContextMenuParams) -> Option<String> {
    let unfiltered = params.unfiltered_link_url();
    let mut url = CefString::from(&unfiltered).to_string();
    if url.trim().is_empty() {
        let filtered = params.link_url();
        url = CefString::from(&filtered).to_string();
    }
    let url = url.trim();
    (!url.is_empty()).then(|| url.to_string())
}

fn context_menu_image_url(params: &ContextMenuParams) -> Option<String> {
    if params.media_type() != ContextMenuMediaType::IMAGE {
        return None;
    }
    let source = params.source_url();
    let url = CefString::from(&source).to_string();
    let url = url.trim();
    (!url.is_empty()).then(|| url.to_string())
}

/*
CDXC:ContextMenus 2026-10-05 WHY:
Alloy-style CEF (the only style a child-view browser can use) builds a page menu of Back, Forward, Print and View page source, with no link or image entries, so Browser pages get the Chrome link/image commands here. They go at the top in Chrome's order and only on Browser pages (the surfaces with a page metadata handler), which also own the clipboard route and the download handler that Save needs.
*/
fn insert_browser_page_context_menu_items(
    params: &ContextMenuParams,
    model: &MenuModel,
    can_open_tabs: bool,
) {
    let mut items: Vec<Option<(c_int, &str)>> = Vec::new();
    if context_menu_link_url(params).is_some() {
        if can_open_tabs {
            items.push(Some((
                CEF_CONTEXT_MENU_APP_OPEN_LINK_NEW_TAB_COMMAND_ID,
                "Open link in new tab",
            )));
        }
        items.push(Some((
            CEF_CONTEXT_MENU_SAVE_LINK_AS_COMMAND_ID,
            "Save link as...",
        )));
        items.push(Some((
            CEF_CONTEXT_MENU_COPY_LINK_ADDRESS_COMMAND_ID,
            "Copy link address",
        )));
    }
    if context_menu_image_url(params).is_some() {
        if !items.is_empty() {
            items.push(None);
        }
        if can_open_tabs {
            items.push(Some((
                CEF_CONTEXT_MENU_OPEN_IMAGE_NEW_TAB_COMMAND_ID,
                "Open image in new tab",
            )));
        }
        items.push(Some((
            CEF_CONTEXT_MENU_SAVE_IMAGE_AS_COMMAND_ID,
            "Save image as...",
        )));
        items.push(Some((CEF_CONTEXT_MENU_COPY_IMAGE_COMMAND_ID, "Copy image")));
        items.push(Some((
            CEF_CONTEXT_MENU_COPY_IMAGE_ADDRESS_COMMAND_ID,
            "Copy image address",
        )));
    }
    if items.is_empty() {
        return;
    }
    let had_default_items = model.count() > 0;
    for (index, item) in items.iter().enumerate() {
        match item {
            Some((command_id, label)) => {
                model.insert_item_at(index, *command_id, Some(&CefString::from(*label)));
            }
            None => {
                model.insert_separator_at(index);
            }
        }
    }
    if had_default_items {
        model.insert_separator_at(items.len());
    }
}

fn start_browser_download(browser: Option<&mut cef::Browser>, url: &str) -> c_int {
    let Some(host) = browser.and_then(|browser| browser.host()) else {
        return 0;
    };
    host.start_download(Some(&CefString::from(url)));
    1
}

wrap_download_image_callback! {
    pub(crate) struct GhostexGpuiCopyImageCallback {
        page_metadata_handler: BrowserPageMetadataHandler,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            _http_status_code: c_int,
            image: Option<&mut cef::Image>,
        ) {
            let Some(png) = image.and_then(|image| image.as_png(1.0, 1, None, None)) else {
                return;
            };
            let mut bytes = vec![0; png.size()];
            if bytes.is_empty() || png.data(Some(&mut bytes), 0) != bytes.len() {
                return;
            }
            (self.page_metadata_handler)(BrowserPageMetadataEvent::CopyToClipboard(
                gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
                    gpui::ImageFormat::Png,
                    bytes,
                )),
            ));
        }
    }
}

wrap_download_handler! {
    pub(crate) struct GhostexGpuiDownloadHandler {}

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut cef::Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> c_int {
            1
        }

        fn on_before_download(
            &self,
            _browser: Option<&mut cef::Browser>,
            _download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> c_int {
            let Some(callback) = callback else {
                return 0;
            };
            /*
            CDXC:Browser 2026-10-05 WHY:
            Alloy-style CEF cancels every download when the client has no download handler, so Save image/link as... and page downloads never reached disk. Every download asks where to save, starting in the user's Downloads folder with the page's suggested name.
            */
            let suggested_name = suggested_name
                .map(|name| name.to_string())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "download".to_string());
            let download_path = std::env::home_dir()
                .map(|home| home.join("Downloads").join(&suggested_name))
                .unwrap_or_else(|| PathBuf::from(&suggested_name));
            callback.cont(
                Some(&CefString::from(download_path.to_string_lossy().as_ref())),
                1,
            );
            1
        }
    }
}

wrap_context_menu_handler! {
    pub(crate) struct GhostexGpuiContextMenuHandler {
        popup_open_handler: Option<BrowserPopupOpenHandler>,
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let Some(model) = model else {
                return;
            };
            if self.page_metadata_handler.is_some()
                && let Some(params) = params.as_deref()
            {
                insert_browser_page_context_menu_items(
                    params,
                    model,
                    self.popup_open_handler.is_some(),
                );
            }
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
            let Some(params) = params.as_deref() else {
                return 0;
            };
            let copy_text = |text: String| -> c_int {
                let Some(handler) = self.page_metadata_handler.as_ref() else {
                    return 0;
                };
                handler(BrowserPageMetadataEvent::CopyToClipboard(
                    gpui::ClipboardItem::new_string(text),
                ));
                1
            };
            match command_id {
                CEF_CONTEXT_MENU_OPEN_LINK_NEW_TAB_COMMAND_ID
                | CEF_CONTEXT_MENU_OPEN_LINK_NEW_WINDOW_COMMAND_ID
                | CEF_CONTEXT_MENU_APP_OPEN_LINK_NEW_TAB_COMMAND_ID
                | CEF_CONTEXT_MENU_OPEN_IMAGE_NEW_TAB_COMMAND_ID => {
                    let url = if command_id == CEF_CONTEXT_MENU_OPEN_IMAGE_NEW_TAB_COMMAND_ID {
                        context_menu_image_url(params)
                    } else {
                        context_menu_link_url(params)
                    };
                    let (Some(popup_open_handler), Some(url)) =
                        (self.popup_open_handler.as_ref(), url)
                    else {
                        return 0;
                    };
                    popup_open_handler(url, BrowserPopupPlacement::Selected);
                    1
                }
                CEF_CONTEXT_MENU_SAVE_LINK_AS_COMMAND_ID => context_menu_link_url(params)
                    .map_or(0, |url| start_browser_download(browser, &url)),
                CEF_CONTEXT_MENU_SAVE_IMAGE_AS_COMMAND_ID => context_menu_image_url(params)
                    .map_or(0, |url| start_browser_download(browser, &url)),
                CEF_CONTEXT_MENU_COPY_LINK_ADDRESS_COMMAND_ID => {
                    context_menu_link_url(params).map_or(0, copy_text)
                }
                CEF_CONTEXT_MENU_COPY_IMAGE_ADDRESS_COMMAND_ID => {
                    context_menu_image_url(params).map_or(0, copy_text)
                }
                CEF_CONTEXT_MENU_COPY_IMAGE_COMMAND_ID => {
                    let (Some(url), Some(handler), Some(host)) = (
                        context_menu_image_url(params),
                        self.page_metadata_handler.clone(),
                        browser.and_then(|browser| browser.host()),
                    ) else {
                        return 0;
                    };
                    let mut callback = GhostexGpuiCopyImageCallback::new(handler);
                    host.download_image(
                        Some(&CefString::from(url.as_str())),
                        0,
                        0,
                        0,
                        Some(&mut callback),
                    );
                    1
                }
                _ => 0,
            }
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
        app_initiated_close: StdRc<Cell<bool>>,
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
            step on macOS/Windows is `CefBrowser::drop` calling
            `platform::release_native_view`. Linux instead lets CEF close its
            own X11 child before releasing the embed host in on_before_close.

            CDXC:Browser 2026-08-21:
            DevTools Target.closeTarget and /json/close enter through this
            same CEF close request. Browser panes must hand that request back
            to the GPUI tab model before returning handled; otherwise CEF
            accepts the request but the app-owned pane remains. App-owned
            surface disposal is identified before native teardown and does
            not request a model close.
            */
            if !self.app_initiated_close.get()
                && let Some(handler) = self.page_metadata_handler.as_ref()
            {
                handler(BrowserPageMetadataEvent::CloseRequested);
            }
            // CEF's Linux CloseHostWindow sends WM_DELETE_WINDOW to its own
            // CefWindowX11, not the GPUI ancestor. Let that native close tear
            // down Chromium's compositor before removing our embed host.
            #[cfg(target_os = "linux")]
            if self.app_initiated_close.get() || self.register_created_native_view {
                return 0;
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
            let Some(browser) = browser else {
                return;
            };
            local_network_prompts_browser_closed(browser.identifier());
            let Some(host) = browser.host() else {
                return;
            };
            let native_view = platform::native_view_ptr(host.window_handle());
            unregister_native_view_browser(native_view);
            #[cfg(target_os = "linux")]
            platform::browser_native_close_finished(native_view);
        }

        fn on_before_popup(
            &self,
            browser: Option<&mut cef::Browser>,
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
            if dispatch_external_app_popup(browser, target_url) {
                return 1;
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
                let urls = icon_urls
                    .into_iter()
                    .map(|url| url.trim().to_string())
                    .filter(|url| !url.is_empty())
                    .collect::<Vec<_>>();
                // The tab icon decodes bitmaps only, so a page that offers both (GitHub lists a
                // PNG beside its SVG) is represented by the one that can be drawn.
                let is_svg = |url: &String| {
                    url.split(['?', '#'])
                        .next()
                        .is_some_and(|path| path.to_ascii_lowercase().ends_with(".svg"))
                };
                urls.iter()
                    .find(|url| !is_svg(url))
                    .or_else(|| urls.first())
                    .cloned()
            });
            (self.page_metadata_handler)(BrowserPageMetadataEvent::FaviconUrlChanged(
                representative_url,
            ));
        }
    }
}

wrap_permission_handler! {
    pub(crate) struct GhostexGpuiPermissionHandler {
        trusted_clipboard_origin: Option<String>,
        media_access_handler: Option<BrowserMediaAccessHandler>,
        // The browser's CEF profile, where a Local Network Access answer is kept.
        profile: String,
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
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> c_int {
            let requesting_origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            // Local Network Access (CDXC:Browser 2026-10-03 in site_requests.rs): asked in the app
            // when the prompt is for nothing else.
            let loopback_network = PermissionRequestTypes::LOOPBACK_NETWORK.get_raw() as u32;
            let local_network_current = PermissionRequestTypes::LOCAL_NETWORK.get_raw() as u32;
            let local_network_deprecated =
                PermissionRequestTypes::LOCAL_NETWORK_ACCESS_DEPRECATED.get_raw() as u32;
            let local_network = local_network_current | local_network_deprecated;
            if requested_permissions != 0
                && requested_permissions & !(loopback_network | local_network) == 0
            {
                let Some(callback) = callback else {
                    return 0;
                };
                let browser_id = browser.map(|browser| browser.identifier()).unwrap_or_default();
                dispatch_browser_site_request(BrowserSiteRequest::LocalNetworkAccess(
                    BrowserLocalNetworkAccessRequest {
                        origin: requesting_origin,
                        local_network: requested_permissions & local_network != 0,
                        profile: self.profile.clone(),
                        content_types: local_network_access_content_types(
                            requested_permissions & loopback_network != 0,
                            requested_permissions & local_network_current != 0,
                            requested_permissions & local_network_deprecated != 0,
                        ),
                        prompt_id,
                        callback: Some(callback.clone()),
                        page_gone: Some(register_local_network_prompt(prompt_id, browser_id)),
                    },
                ));
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

        fn on_dismiss_permission_prompt(
            &self,
            _browser: Option<&mut cef::Browser>,
            prompt_id: u64,
            _result: PermissionRequestResult,
        ) {
            local_network_prompt_dismissed(prompt_id);
        }
    }
}
