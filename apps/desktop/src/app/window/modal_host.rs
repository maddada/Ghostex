// C1 wave-2 extraction: the GpuiAppModalHostWindow entity moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

fn app_modal_host_background() -> Hsla {
    if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
        rgb(0xffffff).into()
    } else if window_glass_active() {
        // The page's lighter window colour over glass (packages/core-ui/styles/modals-glass.css (deleted 2026-11-06)).
        titlebar_background().blend(gpui::white().opacity(0.06))
    } else {
        titlebar_background()
    }
}

/// The CEF child window of an extension modal: it loads the extension's own page with the
/// extension bridge. Every built-in app modal is a native GPUI window
/// (`native_app_modal_lifecycle.rs`).
pub(crate) struct GpuiAppModalHostWindow {
    #[cfg(target_os = "windows")]
    _mouse_navigation_release: gpui::Subscription,
    pub(crate) current_modal: GpuiAppModalKind,
    pub(crate) initial_window_size: Size<Pixels>,
    // None when CEF browser creation failed; the window then stays empty until
    // the user closes it (CDXC:CefRuntime 2026-07-11).
    pub(crate) surface: Option<Entity<CefSurface>>,
}

impl GpuiAppModalHostWindow {
    pub(crate) fn new(
        window: &mut Window,
        url: String,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        extension_bridge: (
            cef::ExtensionBridgeSurfaceSpec,
            cef::ExtensionBridgeEventHandler,
        ),
        cx: &mut App,
    ) -> Entity<Self> {
        let parent_ns_view = cef_parent_native_view(window)
            .expect("GPUI app-modal host requires a native parent view");
        let (extension_bridge_surface, extension_bridge_event_handler) = extension_bridge;
        let surface = CefSurface::try_new_extension(
            APP_MODAL_HOST_ID.to_string(),
            parent_ns_view,
            url,
            APP_MODAL_HOST_CEF_PROFILE_ID.to_string(),
            pane_prepaint_background_color(),
            false,
            app_modal_host_background(),
            true,
            extension_bridge_surface,
            extension_bridge_event_handler,
            None,
            cx,
        )
        .map_err(|error| {
            support_logs::append(
                support_logs::GpuiSupportLog::CrashReports,
                "gpui.cefSurface.createFailed",
                serde_json::json!({ "surface": "appModalHost", "error": error }),
            );
        })
        .ok();
        let initial_window_size = modal.window_size_for_open(&open_message);
        // CDXC:AppModal 2026-09-27 WHY: A CEF page is created 1×1 and only gets its frame when the window paints, but the warm spare stays hidden while Settings renders into it, so the page laid out in a 14px viewport. The Strength slider's track collapsed to zero width there, and Base UI measures an edge-aligned thumb only when it mounts or its value changes, so the thumb and fill stayed hidden after the window grew. The page gets the window's full frame from the start instead.
        if let Some(surface) = &surface {
            surface.read(cx).set_initial_bounds(
                gpui::Bounds::new(gpui::Point::default(), initial_window_size),
                window.scale_factor(),
            );
        }
        cx.new(move |_cx| Self {
            #[cfg(target_os = "windows")]
            _mouse_navigation_release: {
                crate::navigation_history::windows_mouse::register_modal(
                    parent_ns_view,
                    _cx.weak_entity(),
                    _cx.to_async(),
                );
                _cx.on_release(move |_, _| {
                    crate::navigation_history::windows_mouse::unregister(parent_ns_view);
                })
            },
            current_modal: modal,
            initial_window_size,
            surface,
        })
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
}

impl Render for GpuiAppModalHostWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(app_modal_host_background())
            .children(self.surface.clone())
    }
}
