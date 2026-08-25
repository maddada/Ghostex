// C1 wave-2 extraction: the GpuiAppModalHostWindow entity moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) struct GpuiAppModalHostWindow {
    pub(crate) current_modal: GpuiAppModalKind,
    is_ready: bool,
    latest_sidebar_state_message: serde_json::Value,
    pending_messages: Vec<serde_json::Value>,
    presented_modal: Option<GpuiAppModalKind>,
    /*
    CDXC:GPUITutorialVideoFullscreen 2026-08-18:
    "f" toggles the YouTube player's fullscreen state, so the host-side key
    press is sent at most once per window. Page-internal navigations can raise
    several main-frame load-end edges, and the modal host outlives them.
    */
    tutorial_video_fullscreen_key_sent: bool,
    // None when CEF browser creation failed; the host window then never
    // reports ready and the existing app-modal ready-timeout retry/close
    // flow recovers (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
    pub(crate) surface: Option<Entity<CefSurface>>,
}

impl GpuiAppModalHostWindow {
    pub(crate) fn new(
        window: &mut Window,
        url: String,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        sidebar_gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
        event_handler: cef::AppModalHostBridgeEventHandler,
        extension_bridge: Option<(
            cef::ExtensionBridgeSurfaceSpec,
            cef::ExtensionBridgeEventHandler,
        )>,
        page_load_end_handler: Option<cef::PageLoadEndHandler>,
        cx: &mut App,
    ) -> Entity<Self> {
        let parent_ns_view = cef_parent_native_view(window)
            .expect("GPUI app-modal host requires a native parent view");
        let uses_react_modal_host = modal.uses_react_modal_host();
        let is_find_prompts = modal == GpuiAppModalKind::FindPrompts;
        let (bridge_surface, event_handler) = if uses_react_modal_host {
            (
                Some(cef::AppModalHostBridgeSurface::NativeWindow),
                Some(event_handler),
            )
        } else if is_find_prompts {
            (
                Some(cef::AppModalHostBridgeSurface::FindPrompts),
                Some(event_handler),
            )
        } else {
            (None, None)
        };
        /*
        CDXC:GPUIFirstLaunchTutorialVideo 2026-08-19:
        The React modal host serves one app-owned page from the synthetic https
        origin: the first-launch tutorial player. The handler only answers URLs
        under that origin, so every other request on this surface is untouched.
        */
        let app_served_resource_scope =
            uses_react_modal_host.then(gpui_app_modal_host_resource_scope);
        let find_theme = is_find_prompts.then(|| {
            gpui_session_chat_theme_from_settings(
                shared_settings::shared_sidebar_settings_snapshot().object(),
            )
        });
        let (prepaint_background, background) = match find_theme.as_deref() {
            Some("light") => (CEF_LIGHT_PREPAINT_BACKGROUND_COLOR, rgb(0xfdfdfd).into()),
            Some(_) => (
                CEF_FIND_PROMPTS_DARK_PREPAINT_BACKGROUND_COLOR,
                rgb(0x111111).into(),
            ),
            None => (CEF_DARK_PREPAINT_BACKGROUND_COLOR, titlebar_background()),
        };
        let surface = if let Some((extension_bridge_surface, extension_bridge_event_handler)) =
            extension_bridge
        {
            CefSurface::try_new_extension(
                APP_MODAL_HOST_ID.to_string(),
                parent_ns_view,
                url,
                APP_MODAL_HOST_CEF_PROFILE_ID.to_string(),
                prepaint_background,
                false,
                background,
                true,
                extension_bridge_surface,
                extension_bridge_event_handler,
                cx,
            )
        } else {
            CefSurface::try_new(
                APP_MODAL_HOST_ID.to_string(),
                parent_ns_view,
                url,
                APP_MODAL_HOST_CEF_PROFILE_ID.to_string(),
                prepaint_background,
                false,
                background,
                None,
                true,
                None,
                None,
                None,
                None,
                matches!(
                    modal,
                    GpuiAppModalKind::FindPrompts | GpuiAppModalKind::ExtensionsBrowser
                )
                .then_some(sidebar_gxserver_bootstrap)
                .flatten(),
                None,
                None,
                app_served_resource_scope,
                bridge_surface,
                event_handler,
                page_load_end_handler,
                cx,
            )
        }
        .map_err(|error| {
            support_logs::append(
                support_logs::GpuiSupportLog::CrashReports,
                "gpui.cefSurface.createFailed",
                serde_json::json!({ "surface": "appModalHost", "error": error }),
            );
        })
        .ok();
        let pending_messages = if uses_react_modal_host {
            vec![open_message]
        } else {
            Vec::new()
        };
        cx.new(move |_cx| Self {
            current_modal: modal,
            is_ready: !uses_react_modal_host,
            latest_sidebar_state_message: sidebar_state_message,
            pending_messages,
            presented_modal: None,
            tutorial_video_fullscreen_key_sent: false,
            surface,
        })
    }

    /// CDXC:GPUITutorialVideoFullscreen 2026-08-18: entering fullscreen needs a
    /// trusted key press from the host (see `CefBrowser::send_fullscreen_toggle_key`),
    /// and it must happen once, only while the tutorial video is the presented
    /// modal.
    pub(crate) fn send_tutorial_video_fullscreen_key(&mut self, cx: &mut gpui::Context<Self>) {
        if self.current_modal != GpuiAppModalKind::WatchGhostexVideo
            || self.tutorial_video_fullscreen_key_sent
        {
            return;
        }
        let Some(surface) = self.surface.clone() else {
            return;
        };
        self.tutorial_video_fullscreen_key_sent = true;
        surface.update(cx, |surface, _cx| {
            // The player only reacts to its shortcut when the page owns
            // Chromium keyboard focus inside this child window.
            surface.focus();
            surface.send_fullscreen_toggle_key();
        });
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.is_ready
    }

    pub(crate) fn open_modal(
        &mut self,
        open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        modal: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) {
        self.current_modal = modal;
        self.presented_modal = None;
        self.latest_sidebar_state_message = sidebar_state_message;
        if !self.current_modal.uses_react_modal_host() {
            self.is_ready = true;
            self.pending_messages.clear();
            cx.notify();
            return;
        }
        if !self.is_ready {
            self.pending_messages.push(open_message);
            cx.notify();
            return;
        }
        if self.current_modal.requires_sidebar_state() {
            self.dispatch_sidebar_state(cx);
        }
        self.dispatch_message(open_message, cx);
        cx.notify();
    }

    pub(crate) fn receive_bridge_message(
        &mut self,
        message: serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match message.get("type").and_then(serde_json::Value::as_str) {
            Some("ready") => {
                self.is_ready = true;
                if self.current_modal.requires_sidebar_state() {
                    self.dispatch_sidebar_state(cx);
                }
                let pending_messages = std::mem::take(&mut self.pending_messages);
                for pending_message in pending_messages {
                    self.dispatch_message(pending_message, cx);
                }
            }
            Some("presented") => {
                self.presented_modal = message
                    .get("modal")
                    .and_then(serde_json::Value::as_str)
                    .and_then(GpuiAppModalKind::from_modal_id);
                window.activate_window();
                if let Some(surface) = &self.surface {
                    surface.update(cx, |surface, _| {
                        surface.focus();
                    });
                }
            }
            Some("contentHeightMeasured") => {
                /*
                CDXC:GPUIAppModalFitHeight 2026-07-28:
                Compact modal-host dialogs measure their rendered React dialog
                once per open and report it (macOS resizes its child window to
                that height; GPUI previously ignored the message, leaving large
                dead gutters above and below dialogs like Rename Session inside
                their worst-case fixed frame). Fit the child window's content
                height to that one-shot measurement, keeping the modal's fixed
                width. The measurement is a bounded number only; post-open
                content growth still scrolls inside the dialog via the
                fixed-window stylesheet caps.
                */
                let measured_modal = message
                    .get("modal")
                    .and_then(serde_json::Value::as_str)
                    .and_then(GpuiAppModalKind::from_modal_id);
                let measured_height = message
                    .get("height")
                    .and_then(serde_json::Value::as_f64)
                    .map(|height| height as f32)
                    .filter(|height| height.is_finite() && *height > 0.0);
                if measured_modal == Some(self.current_modal) {
                    if let Some(height) = measured_height {
                        let fitted_height = height.clamp(
                            APP_MODAL_HOST_FIT_CONTENT_MIN_WINDOW_HEIGHT,
                            APP_MODAL_HOST_FIT_CONTENT_MAX_WINDOW_HEIGHT,
                        );
                        window.resize(size(
                            self.current_modal.window_size().width,
                            px(fitted_height),
                        ));
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn dispatch_sidebar_state(&mut self, cx: &mut gpui::Context<Self>) {
        self.dispatch_message(
            serde_json::json!({
                "message": self.latest_sidebar_state_message,
                "type": "sidebarState",
            }),
            cx,
        );
    }

    pub(crate) fn refresh_sidebar_state_message(
        &mut self,
        sidebar_state_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsPersistence 2026-06-24-11:14:
        An open GPUI app-modal host must receive the saved sidebar hydrate snapshot after `updateSettings` succeeds, matching macOS publish-to-modal behavior. Update the stored latest snapshot and dispatch `sidebarState` only through the modal host's existing app-owned CEF script channel; do not create overlays, hidden views, global input routing, or a second Settings state channel.
        */
        self.latest_sidebar_state_message = sidebar_state_message;
        if self.is_ready {
            self.dispatch_sidebar_state(cx);
        }
    }

    pub(crate) fn dispatch_transient_sidebar_state_message(
        &mut self,
        payload: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_transient_message(
            serde_json::json!({
                "message": payload,
                "type": "sidebarState",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_transient_message(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.is_ready {
            self.pending_messages.push(message);
            cx.notify();
            return;
        }
        self.dispatch_message(message, cx);
        cx.notify();
    }

    pub(crate) fn dispatch_extension_bridge_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = &self.surface {
            surface.update(cx, |surface, _| {
                surface.dispatch_extension_bridge_message(message);
            });
        }
    }

    fn dispatch_message(&mut self, message: serde_json::Value, cx: &mut gpui::Context<Self>) {
        let script = format!(
            "window.dispatchEvent(new CustomEvent('ghostex-app-modal-host-message', {{ detail: {} }})); undefined;",
            message
        );
        if let Some(surface) = &self.surface {
            surface.update(cx, |surface, _| {
                surface.execute_app_owned_script(&script);
            });
        }
    }
}

impl Render for GpuiAppModalHostWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(titlebar_background())
            .children(self.surface.clone())
    }
}
