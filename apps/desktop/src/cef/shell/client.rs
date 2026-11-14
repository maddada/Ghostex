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
        project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        // The page the project bridge belongs to (`first_party_page_entry_identity`), `None` when
        // this browser gets no project bridge.
        project_workarea_bridge_page: Option<String>,
        extension_bridge_surface: Option<ExtensionBridgeSurfaceSpec>,
        extension_bridge_event_handler: Option<ExtensionBridgeEventHandler>,
        request_handler: Option<RequestHandler>,
        permission_handler: Option<PermissionHandler>,
        focus_handler: Option<FocusHandler>,
        keyboard_handler: Option<KeyboardHandler>,
        download_handler: Option<DownloadHandler>,
    }

    impl Client {
        fn focus_handler(&self) -> Option<FocusHandler> {
            self.focus_handler.clone()
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            self.keyboard_handler.clone()
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            self.download_handler.clone()
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
            if source_process != ProcessId::RENDERER {
                return 0;
            }

            let Some(message) = message else {
                return 0;
            };
            let message_name = CefString::from(&message.name()).to_string();
            let project_workarea_event_kind =
                project_workarea_bridge_event_kind_for_process_message(&message_name);
            let is_extension_bridge_message =
                message_name == EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME;
            if project_workarea_event_kind.is_none() && !is_extension_bridge_message {
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
            if let Some(event_kind) = project_workarea_event_kind {
                let Some(handler) = self.project_workarea_bridge_event_handler.clone() else {
                    return 0;
                };
                // Only the page the bridge was installed for may use it (CDXC:CefRuntime
                // 2026-11-14 in GhostexGpuiProjectWorkareaBridgeLoadHandler).
                let frame_url = frame
                    .as_ref()
                    .map(|frame| CefString::from(&frame.url()).to_string())
                    .unwrap_or_default();
                if !self
                    .project_workarea_bridge_page
                    .as_deref()
                    .is_some_and(|page| same_first_party_page(&frame_url, page))
                {
                    return 1;
                }
                /*
                CDXC:CefRuntime 2026-06-24-11:03:
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
                /*
                CDXC:Extensions 2026-08-28:
                A remote `server.url` surface never receives the bridge install
                message, so this shape can only arrive from a page that built
                the message itself. Drop it here as well, so the inbound path
                does not depend on the renderer shim staying uninstalled.
                */
                if !surface.bridge_enabled {
                    return 1;
                }
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

            0
        }
    }
}

/// CDXC:CefRuntime 2026-09-19 WHY:
/// A CEF client owns exactly one LoadHandler, so a surface that installs its bridge at load end
/// cannot also carry a separate load-end handler: whichever one is chosen silently replaces the
/// other. Reporting the edge from inside every handler is what keeps both on the same surface.
/// A standalone load-end handler put in front of the others on 2026-09-19 left Docs, Kanban, and
/// Automate with no `window.ghostexGpui.post*Request` at all, so every request they made timed out.
fn report_main_frame_load_end(handler: &Option<PageLoadEndHandler>) {
    if let Some(handler) = handler {
        handler();
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiBrowserPageLoadHandler {
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
        code_editor_origin: Option<String>,
        page_load_end_handler: Option<PageLoadEndHandler>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut cef::Browser>,
            is_loading: c_int,
            can_go_back: c_int,
            can_go_forward: c_int,
        ) {
            if let Some(handler) = &self.page_metadata_handler {
                handler(BrowserPageMetadataEvent::LoadingStateChanged {
                    is_loading: is_loading != 0,
                    can_go_back: can_go_back != 0,
                    can_go_forward: can_go_forward != 0,
                });
            }
        }

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
            report_main_frame_load_end(&self.page_load_end_handler);
            let Some(origin) = self.code_editor_origin.as_deref() else {
                return;
            };
            if !cef_origins_match(&CefString::from(&frame.url()).to_string(), origin) {
                return;
            }
            // CDXC:DesignSystem 2026-09-15 SEE-ALSO:
            // Code runs in a separate document, so share Monaco's floating 5px hover styling with app-scrollbars.css through the same stylesheet.
            let css = serde_json::to_string(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/components/ui/monaco-scrollbars.css"))).expect("static CSS is serializable");
            let script = format!("(() => {{ document.documentElement.classList.add('gx-code-scrollbars'); if (document.getElementById('gx-code-scrollbars')) return; const style = document.createElement('style'); style.id = 'gx-code-scrollbars'; style.textContent = {css}; document.head.append(style); }})()");
            frame.execute_java_script(Some(&CefString::from(script.as_str())), Some(&CefString::from(BROWSER_APP_OWNED_SCRIPT_URL)), 1);
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiExtensionBridgeLoadHandler {
        surface: ExtensionBridgeSurfaceSpec,
        page_load_end_handler: Option<PageLoadEndHandler>,
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
            report_main_frame_load_end(&self.page_load_end_handler);
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
            CDXC:Onboarding 2026-08-18:
            Report only the main-frame load-end edge to the app; sub-frames
            (ads, player iframes) must not retrigger the host action.
            */
            (self.load_end_handler)();
        }
    }
}

wrap_load_handler! {
    pub(crate) struct GhostexGpuiProjectWorkareaBridgeLoadHandler {
        manage_docs_resource_base_url: Option<String>,
        // The page the bridge is installed on (`first_party_page_entry_identity`); `None` installs
        // it nowhere (the Code view).
        bridge_page: Option<String>,
        page_load_end_handler: Option<PageLoadEndHandler>,
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
            report_main_frame_load_end(&self.page_load_end_handler);

            /*
            CDXC:CefRuntime 2026-06-24-11:03:
            Project workarea CEF clients install only the Kanban/Manage fixed bridge functions after the first-party CEF entry loads. Sidebar and ordinary Browser clients do not receive this handler, keeping project file/board messages out of generic Browser tabs and avoiding WKWebView/WebKit compatibility at the native runtime layer.

            CDXC:CefRuntime 2026-11-14 WHY:
            The bridge goes only to the page the surface was made for, checked by its address like the extension bridge's `matches_url`, so a document that somehow got into the main frame never receives it; the client refuses bridge messages from any other address as well. The Code view (code-server) is not Ghostex's page and never gets it.
            */
            let frame_url = CefString::from(&frame.url()).to_string();
            if !self
                .bridge_page
                .as_deref()
                .is_some_and(|page| same_first_party_page(&frame_url, page))
            {
                return;
            }
            let mut message =
                match cef::process_message_create(Some(&CefString::from(
                    PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME,
                ))) {
                    Some(message) => message,
                    None => return,
                };
            if let Some(arguments) = message.argument_list()
                && let Some(base_url) = self.manage_docs_resource_base_url.as_deref() {
                    arguments.set_size(1);
                    arguments.set_string(0, Some(&CefString::from(base_url)));
                }
            frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        }
    }
}

wrap_render_process_handler! {
    pub(crate) struct GhostexGpuiRenderProcessHandler;

    impl RenderProcessHandler {
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
            let is_project_workarea_install_message =
                message_name == PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME;
            let is_extension_bridge_install_message =
                message_name == EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME;
            if !is_project_workarea_install_message && !is_extension_bridge_install_message {
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
            } else {
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
