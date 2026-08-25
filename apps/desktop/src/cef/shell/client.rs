// C4 light split: the CEF Client dispatch table, LoadHandlers, and
// RenderProcessHandler. Pure move out of `cef/shell.rs`. See
// docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

wrap_client! {
    pub(crate) struct GhostexGpuiCefClient {
        life_span_handler: Option<LifeSpanHandler>,
        context_menu_handler: Option<ContextMenuHandler>,
        display_handler: Option<DisplayHandler>,
        find_handler: Option<FindHandler>,
        load_handler: Option<LoadHandler>,
        sidebar_bridge_event_handler: Option<SidebarBridgeEventHandler>,
        project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        app_modal_host_bridge_event_handler: Option<AppModalHostBridgeEventHandler>,
        extension_bridge_surface: Option<ExtensionBridgeSurfaceSpec>,
        extension_bridge_event_handler: Option<ExtensionBridgeEventHandler>,
        request_handler: Option<RequestHandler>,
        permission_handler: Option<PermissionHandler>,
        focus_handler: Option<FocusHandler>,
        keyboard_handler: Option<KeyboardHandler>,
    }

    impl Client {
        fn focus_handler(&self) -> Option<FocusHandler> {
            self.focus_handler.clone()
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            self.keyboard_handler.clone()
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            self.life_span_handler.clone()
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            self.context_menu_handler.clone()
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            self.display_handler.clone()
        }

        fn find_handler(&self) -> Option<FindHandler> {
            self.find_handler.clone()
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            self.load_handler.clone()
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            self.permission_handler.clone()
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            self.request_handler.clone()
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> c_int {
            /*
            CDXC:GPUIProjectSidebarBridge 2026-06-23-18:29:
            The GPUI sidebar bridge may carry only the allowlisted typed sidebar events from `window.ghostexGpui`, each as one bounded string payload. Ordinary Browser CEF surfaces construct clients without this handler, and CEF only classifies the private event kind; strict JSON parsing and stale/private-shape rejection stay in the GPUI app stores with no logging or persistence at this boundary.

            CDXC:GPUISidebarProjectPathActions 2026-06-24-14:18:
            Sidebar-native project path actions use the same fixed-function CEF bridge as project-context/readiness events. CEF forwards only a bounded string from the bundled sidebar main frame; Rust app code must parse the small action/project-id JSON and resolve project paths through gxserver, not from renderer-provided absolute path data.

            CDXC:GPUISidebarGit 2026-06-24-15:43:
            Existing-PR browser open and changed-file IDE open are still sidebar-only native side effects on this fixed bridge. CEF does not trust or inspect URLs or paths; app-side Rust must re-query gxserver and treat any file path as a relative candidate only.

            CDXC:GPUICommandPane 2026-06-24-23:17:
            Sidebar command actions use their own fixed sidebar bridge function so the shared SidebarApp and command palette can ask GPUI to run the gxserver-projected action through Rust-owned Browser or command-pane paths. CEF still forwards only one bounded string from the sidebar main frame and does not log, persist, inspect, or execute command text.

            CDXC:GPUIAppShots 2026-06-25-23:28:
            App Shot prompt insertion uses its own fixed sidebar bridge function. CEF forwards only one bounded JSON string from the bundled sidebar; app-side Rust must parse the gxserver presentation session id and prompt, then verify the exact mounted Agents surface before writing terminal bytes.

            CDXC:GPUIAppShots 2026-06-26-04:27:
            The same bridge may carry a machine-scoped remote presentation session id for App Shots, but CEF remains a string forwarder only; Rust must decline unless the exact remote attach Agents terminal is already mounted.

            CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
            GPUI status indicators and pet overlay state use their own fixed sidebar bridge functions. CEF forwards only bounded first-party strings; app-side Rust must strictly parse counts/settings/candidate ids and never treat renderer paths, URLs, command text, terminal output, tokens, or generic message names as presentation authority.
            */
            if source_process != ProcessId::RENDERER {
                return 0;
            }

            let Some(message) = message else {
                return 0;
            };
            let message_name = CefString::from(&message.name()).to_string();
            let sidebar_event_kind = sidebar_bridge_event_kind_for_process_message(&message_name);
            let is_sidebar_editable_focus_message =
                message_name == SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME;
            let project_workarea_event_kind =
                project_workarea_bridge_event_kind_for_process_message(&message_name);
            let is_app_modal_host_message =
                message_name == APP_MODAL_HOST_BRIDGE_PROCESS_MESSAGE_NAME;
            let is_native_host_message = message_name == NATIVE_HOST_BRIDGE_PROCESS_MESSAGE_NAME;
            let is_extension_bridge_message =
                message_name == EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME;
            if sidebar_event_kind.is_none()
                && !is_sidebar_editable_focus_message
                && project_workarea_event_kind.is_none()
                && !is_app_modal_host_message
                && !is_native_host_message
                && !is_extension_bridge_message
            {
                return 0;
            }
            if frame.as_ref().map(|frame| frame.is_main() == 0).unwrap_or(true) {
                return 1;
            }

            let Some(arguments) = message.argument_list() else {
                return 1;
            };
            if arguments.size() != 1 || arguments.get_type(0) != ValueType::STRING {
                return 1;
            }

            let payload = CefString::from(&arguments.string(0)).to_string();
            if is_sidebar_editable_focus_message {
                /*
                CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
                The shared sidebar surface is mouse-focus passive: clicking its
                background never moves AppKit first responder away from the
                active terminal. The only way the sidebar may take keyboard
                focus is this fixed bridge message, sent when its page focuses
                a real editable element (search, rename). It is consumed here
                as a native focus transfer for the sending browser; it carries
                no app data and never reaches the app event handler.
                */
                handle_sidebar_editable_focus(browser, &payload);
                return 1;
            }
            if let Some(event_kind) = sidebar_event_kind {
                let Some(handler) = self.sidebar_bridge_event_handler.clone() else {
                    return 0;
                };
                if payload.chars().count() > SIDEBAR_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(event_kind.with_payload(payload));
                return 1;
            }

            if let Some(event_kind) = project_workarea_event_kind {
                let Some(handler) = self.project_workarea_bridge_event_handler.clone() else {
                    return 0;
                };
                /*
                CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
                Project-workarea CEF process messages are fixed-function and main-frame-only like the sidebar bridge, but their payload budget is larger because Manage save requests carry bounded file contents. The CEF boundary forwards only in-memory strings to the app handler and does not log, persist, inspect URL/title state, expose generic IPC, or create a WKWebView/WebKit path.
                */
                if payload.chars().count() > PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(event_kind.with_payload(payload));
                return 1;
            }

            if is_extension_bridge_message {
                let Some(surface) = self.extension_bridge_surface.as_ref() else {
                    return 0;
                };
                let frame_url = frame
                    .as_ref()
                    .map(|frame| CefString::from(&frame.url()).to_string())
                    .unwrap_or_default();
                if !surface.matches_url(&frame_url) {
                    return 1;
                }
                let Some(handler) = self.extension_bridge_event_handler.clone() else {
                    return 0;
                };
                if payload.chars().count() > EXTENSION_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }
                handler(ExtensionBridgeEvent {
                    extension_id: surface.id.clone(),
                    payload,
                });
                return 1;
            }

            if is_app_modal_host_message {
                let Some(handler) = self.app_modal_host_bridge_event_handler.clone() else {
                    return 0;
                };
                /*
                CDXC:GPUITitlebarAppModalHost 2026-06-24-10:42:
                The GPUI app-modal host and titlebar Tips panel reuse the macOS React bridge shape, but CEF forwards each message as a single bounded JSON string from first-party bundled pages only. Keep this main-frame-only and handler-scoped so Browser tabs, workarea pages, logs, persistence, raw URLs, page titles, and generic IPC never receive app-modal payloads.
                */
                if payload.chars().count() > APP_MODAL_HOST_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(AppModalHostBridgeEvent::Message(payload));
                return 1;
            }

            if is_native_host_message {
                let Some(handler) = self.app_modal_host_bridge_event_handler.clone() else {
                    return 0;
                };
                /*
                CDXC:GPUITitlebarNativeHost 2026-07-08:
                The bundled titlebar-host Resources document uses macOS's `ghostexNativeHost` bridge for process sampling and titlebar actions. CEF forwards only a bounded main-frame JSON string from first-party modal/sidebar/titlebar surfaces and tags it as native-host; app-side Rust owns the fixed process allowlist and action validation.
                */
                if payload.chars().count() > NATIVE_HOST_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(AppModalHostBridgeEvent::NativeHostMessage(payload));
                return 1;
            }

            0
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiBrowserPageLoadHandler {
        page_metadata_handler: BrowserPageMetadataHandler,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut cef::Browser>,
            is_loading: c_int,
            can_go_back: c_int,
            can_go_forward: c_int,
        ) {
            (self.page_metadata_handler)(BrowserPageMetadataEvent::LoadingStateChanged {
                is_loading: is_loading != 0,
                can_go_back: can_go_back != 0,
                can_go_forward: can_go_forward != 0,
            });
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiExtensionBridgeLoadHandler {
        surface: ExtensionBridgeSurfaceSpec,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            let frame_url = CefString::from(&frame.url()).to_string();
            if !self.surface.matches_url(&frame_url) {
                return;
            }
            let mut message = match cef::process_message_create(Some(&CefString::from(
                EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME,
            ))) {
                Some(message) => message,
                None => return,
            };
            frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiPageLoadEndHandler {
        load_end_handler: PageLoadEndHandler,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUITutorialVideoFullscreen 2026-08-18:
            Report only the main-frame load-end edge to the app; sub-frames
            (ads, player iframes) must not retrigger the host action.
            */
            (self.load_end_handler)();
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiSidebarProjectContextLoadHandler {
        runtime_settings: SidebarRuntimeSettingsSnapshot,
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUIProjectSidebarBridge 2026-06-24-11:17:
            Install renderer-side `window.ghostexGpui` only for sidebar CEF clients with fixed allowlisted post functions, strict debug/beta booleans, saved shared Settings, and the real gxserver bootstrap when the local token helper can construct it. The private install message may carry the loopback base URL, bearer token, protocol version, stable client id, and only explicit gxserver ids from app state; ordinary Browser, workarea, and modal CEF clients never attach this load handler or receive the bootstrap.

            CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
            The same sidebar-only runtime message must carry the saved shared Settings object so the mounted React SidebarApp can normalize real user preferences instead of booting from hardcoded GPUI defaults plus debug/beta flags. Keep this as a bounded first-party CEF payload scoped to the sidebar renderer; Browser, workarea, and modal-host clients must not receive it.
            */
            send_sidebar_install_process_message(
                frame,
                self.runtime_settings.clone(),
                self.gxserver_bootstrap.clone(),
            );
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiSessionChatGxserverBootstrapLoadHandler {
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUISessionChatSurface 2026-07-31:
            Session Chat CEF clients receive only the gxserver bootstrap so the
            bundled chat.html page can call the session-chat endpoints and open
            /api/events directly, matching the sidebar's loopback token scope.
            No sidebar post functions, runtime settings, or workarea bridges are
            installed for this surface, and ordinary Browser/workarea/modal
            clients never attach this load handler. The page polls for the
            installed object, so load-end delivery cannot strand it.
            */
            send_session_chat_gxserver_bootstrap_process_message(
                frame,
                self.gxserver_bootstrap.clone(),
            );
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiProjectWorkareaBridgeLoadHandler {
        manage_docs_resource_base_url: Option<String>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
            Project workarea CEF clients install only the Kanban/Manage fixed bridge functions after the first-party CEF entry loads. Sidebar and ordinary Browser clients do not receive this handler, keeping project file/board messages out of generic Browser tabs and avoiding WKWebView/WebKit compatibility at the native runtime layer.
            */
            let mut message =
                match cef::process_message_create(Some(&CefString::from(
                    PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME,
                ))) {
                    Some(message) => message,
                    None => return,
                };
            if let Some(arguments) = message.argument_list() {
                if let Some(base_url) = self.manage_docs_resource_base_url.as_deref() {
                    arguments.set_size(1);
                    arguments.set_string(0, Some(&CefString::from(base_url)));
                }
            }
            frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        }
    }
}

wrap_render_process_handler! {
    pub(crate) struct GhostexGpuiRenderProcessHandler;

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut cef::V8Context>,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            let frame_url = CefString::from(&frame.url()).to_string();
            let surface = app_modal_host_bridge_surface_for_frame_url(&frame_url);
            let Some(surface) = surface else {
                return;
            };
            let Some(context) = context else {
                return;
            };
            /*
            CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
            Install the CEF-compatible `window.webkit.messageHandlers.ghostexAppModalHost` shim at V8 context creation for only bundled modal-host.html, titlebar-host.html, and sidebar index.html entries. Install `ghostexNativeHost` for titlebar-host and the first-party sidebar so either surface can invoke Rust's fixed, validated native actions, including gxserver lifecycle controls. The shared React modal host posts `ready` during mount, the titlebar panels post dropdown/process messages during hydration, and the shared sidebar can emit Settings/Hotkeys/Command Palette opens after hydration, so waiting for load-end would race real presentation. Only modal-host.html receives the native-window identity fields; Browser tabs, project workareas, arbitrary pages, raw URLs, titles, logs, persistence, and generic IPC do not receive these bridges.

            CDXC:GPUILoggingRemoval 2026-06-28-17:06:
            App-modal CEF setup keeps only the functional host message bridge. Do not emit lifecycle diagnostic IPC or renderer logging events from bridge installation while GPUI logging is intentionally removed.
            */
            install_app_modal_host_v8_bridge(Some(&mut *context), surface);
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> c_int {
            if source_process != ProcessId::BROWSER {
                return 0;
            }
            let Some(message) = message else {
                return 0;
            };
            let message_name = CefString::from(&message.name()).to_string();
            let is_install_message = message_name == SIDEBAR_PROJECT_CONTEXT_INSTALL_MESSAGE_NAME;
            let is_runtime_settings_update =
                message_name == SIDEBAR_RUNTIME_SETTINGS_UPDATE_MESSAGE_NAME;
            let is_gxserver_bootstrap_update =
                message_name == SIDEBAR_GXSERVER_BOOTSTRAP_UPDATE_MESSAGE_NAME;
            let is_session_chat_gxserver_bootstrap_message =
                message_name == SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME;
            let is_project_workarea_install_message =
                message_name == PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME;
            let is_extension_bridge_install_message =
                message_name == EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME;
            if !is_install_message
                && !is_runtime_settings_update
                && !is_gxserver_bootstrap_update
                && !is_session_chat_gxserver_bootstrap_message
                && !is_project_workarea_install_message
                && !is_extension_bridge_install_message
            {
                return 0;
            }
            let Some(frame) = frame else {
                return 1;
            };
            if frame.is_main() == 0 {
                return 1;
            }
            let Some(mut context) = frame.v8_context() else {
                return 1;
            };
            if context.enter() == 0 {
                return 1;
            }
            if is_extension_bridge_install_message {
                install_extension_v8_bridge(Some(&mut context));
            } else if is_project_workarea_install_message {
                let manage_docs_resource_base_url = message
                    .argument_list()
                    .filter(|arguments| {
                        arguments.size() == 1 && arguments.get_type(0) == ValueType::STRING
                    })
                    .map(|arguments| CefString::from(&arguments.string(0)).to_string())
                    .filter(|value| value == MANAGE_DOCS_RESOURCE_BASE_URL);
                install_project_workarea_v8_bridge(
                    Some(&mut context),
                    manage_docs_resource_base_url.as_deref(),
                );
            } else if is_install_message {
                let runtime_settings = sidebar_runtime_settings_from_install_message(message);
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(
                    message,
                    SIDEBAR_RUNTIME_SETTINGS_ARGUMENT_COUNT,
                );
                install_sidebar_project_context_v8_bridge(
                    Some(&mut context),
                    runtime_settings,
                    gxserver_bootstrap,
                );
            } else if is_runtime_settings_update {
                let runtime_settings = sidebar_runtime_settings_from_install_message(message);
                update_sidebar_runtime_settings_v8_bridge(Some(&mut context), runtime_settings);
            } else if is_session_chat_gxserver_bootstrap_message {
                /*
                CDXC:GPUISessionChatSurface 2026-07-31:
                Session Chat bootstrap install creates the ghostexGpui
                namespace when missing and sets only the gxserverBootstrap
                object plus the fixed changed callback; it must not install
                sidebar post functions or relax the sidebar update path's
                installed-bridge integrity gate.
                */
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(message, 0);
                install_session_chat_gxserver_bootstrap_v8_bridge(
                    Some(&mut context),
                    gxserver_bootstrap,
                );
            } else {
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(message, 0);
                update_sidebar_gxserver_bootstrap_v8_bridge(
                    Some(&mut context),
                    gxserver_bootstrap,
                );
            }
            context.exit();
            if is_extension_bridge_install_message {
                frame.execute_java_script(
                    Some(&CefString::from(EXTENSION_BRIDGE_RUNTIME_SHIM)),
                    Some(&CefString::from("ghostex://gpui/extension-bridge")),
                    1,
                );
            }
            1
        }
    }
}
