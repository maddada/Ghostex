use crate::*;

pub(crate) struct GpuiTitlebarExtensionPopupState {
    pub(crate) id: ExtensionId,
    pub(crate) trigger_bounds: Bounds<Pixels>,
    pub(crate) size: GpuiExtensionPopupSize,
    pub(crate) generation: u64,
    pub(crate) panel: Option<Entity<GpuiTitlebarExtensionPanel>>,
    pub(crate) error: Option<String>,
}

pub(crate) struct GpuiTitlebarExtensionPanel {
    pub(crate) surface: Entity<CefSurface>,
}

impl GpuiTitlebarExtensionPanel {
    pub(crate) fn create_browser(
        parent_ns_view: *mut std::ffi::c_void,
        extension_id: ExtensionId,
        url: &str,
        bridge_surface: cef::ExtensionBridgeSurfaceSpec,
        bridge_event_handler: cef::ExtensionBridgeEventHandler,
    ) -> Result<Rc<CefBrowser>, String> {
        let id = extension_id.as_str();
        let browser = Rc::new(CefBrowser::new(
            parent_ns_view,
            url,
            &format!("titlebar-extension-{id}"),
            CEF_DARK_PREPAINT_BACKGROUND_COLOR,
            false,
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
        browser.set_visible(false);
        Ok(browser)
    }

    pub(crate) fn from_browser(
        extension_id: ExtensionId,
        browser: Rc<CefBrowser>,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) -> Entity<Self> {
        let surface = cx.new(move |cx| {
            CefSurface::from_browser(
                format!("ghostex-gpui-titlebar-extension-{}", extension_id.as_str()),
                titlebar_popup_menu_background(),
                true,
                browser,
                cx,
            )
        });
        cx.new(move |_| Self { surface })
    }

    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut gpui::Context<Self>) {
        self.surface.update(cx, |surface, _| {
            if visible {
                surface.order_front();
            }
            surface.set_visible(visible);
        });
    }

    pub(crate) fn dispatch_bridge_message(
        &mut self,
        payload: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.surface.update(cx, |surface, _| {
            surface.dispatch_extension_bridge_message(payload);
        });
    }
}

impl Render for GpuiTitlebarExtensionPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .overflow_hidden()
            .bg(titlebar_popup_menu_background())
            .child(self.surface.clone())
    }
}
