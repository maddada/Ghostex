use crate::app::helpers::*;
use crate::*;
use gpui_component::{WindowExt as _, notification::Notification};
use serde_json::json;

/// CDXC:Workarea 2026-09-22 WHY:
/// Keep the former extension's persisted tab identity while using the built-in project-view lifecycle. No installed manifest or extension process participates in this view.
pub(crate) fn storybook_view() -> GpuiCustomView {
    GpuiCustomView {
        id: ExtensionId::new("storybook").expect("built-in view ID"),
        title: "Storybook".into(),
        enabled: !shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("storybookViewTabHidden")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        url: String::new(),
        definition: json!({
            "id": "storybook", "name": "Storybook", "availability": "matching",
            "source": {"kind": "report", "discovery": "storybook"}
        }),
    }
}

impl TitlebarMode {
    pub(crate) fn is_storybook(self) -> bool {
        matches!(self, Self::Extension(id) if id.as_str() == "storybook")
    }

    pub(crate) fn is_addon_view(self) -> bool {
        matches!(self, Self::Extension(_))
            && !self.is_storybook()
            && self.website_provider().is_none()
    }
}

impl GhostexGpuiApp {
    pub(crate) fn annotate_storybook(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let mode = TitlebarMode::Extension(storybook_view().id);
        if self.active_mode != mode || !self.titlebar_mode_available(mode) {
            return;
        }
        let script = browser_agentation_feedback_injection_script();
        let surface = ProjectWorkareaCefSurfaceSlotKey::for_titlebar_mode(mode)
            .and_then(|slot| self.project_workarea_runtime_cef_surfaces.get(&slot))
            .map(|owned| owned.surface.clone());
        if let Some(surface) = &surface {
            self.listen_for_storybook_annotations(surface, cx);
        }
        let injected = surface.is_some_and(|surface| {
            surface.update(cx, |surface, _| {
                surface.inject_feedback_tool_script(&script)
            })
        });
        if !injected {
            window.push_notification(
                Notification::warning("Wait for Storybook to finish loading before annotating."),
                cx,
            );
        }
        cx.notify();
    }

    /// Gives the Storybook page the page bridge that Annotate's Send posts to (Browser tabs get
    /// theirs with their notifications), once per page, and hands what it posts to
    /// `receive_page_annotation` for as long as the page lives.
    fn listen_for_storybook_annotations(
        &mut self,
        surface: &Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .storybook_annotation_bridge
            .as_ref()
            .and_then(|listening| listening.upgrade())
            .is_some_and(|listening| listening.entity_id() == surface.entity_id())
        {
            return;
        }
        let Some(mut bridge) = CefSurface::install_page_bridge(surface, "", cx) else {
            return;
        };
        self.storybook_annotation_bridge = Some(surface.downgrade());
        cx.spawn(async move |this, cx| {
            while let Some(message) = bridge.next().await {
                let Ok(message) = serde_json::from_str::<serde_json::Value>(&message) else {
                    continue;
                };
                if this
                    .update(cx, |this, cx| {
                        this.receive_page_annotation(&message, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
