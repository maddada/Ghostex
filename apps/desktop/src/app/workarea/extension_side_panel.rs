//! A sidebar extension's side panel view: a tab of the view panel (`TitlebarMode::Extension(id)`)
//! drawn natively from the extension's side panel document, the computer's one or, for
//! `scope: session`, the focused session's (gx-core `extension_side_panel_view`). It is never a
//! web page: a sidebar extension has no `view` placement, so `extension_view_runtime_url` gives
//! it none.
//!
//! CDXC:Extensions 2026-11-14 DECISION:
//! User: "I want to be able to use the same thing for the side panel views please (use the extensions api to get stuff from agent etc)". The side panel draws the same blocks with the same renderer as the sidebar (native_sidebar/extension_panel/), fed by the same extension API.

use ghostex_gx_core::extension_panel::{extension_side_panel_view, extension_side_panels};
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use gpui_component::v_flex;

use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::*;

impl GhostexGpuiApp {
    /// Whether `id` is a sidebar extension that offers a side panel view right now.
    pub(crate) fn extension_side_panel_offered(&self, id: ExtensionId) -> bool {
        self.native_extension_panels()
            .and_then(|panels| panels.0.get(id.as_str()))
            .is_some_and(|entry| entry.side_panel.is_some())
    }

    /// The side panel views the sidebar extensions offer, by title.
    pub(crate) fn extension_side_panel_modes(&self) -> Vec<TitlebarMode> {
        extension_side_panels(self.native_extension_panels())
            .into_iter()
            .filter_map(|(id, _, _)| ExtensionId::new(&id).map(TitlebarMode::Extension))
            .collect()
    }

    pub(crate) fn render_native_extension_side_panel(
        &mut self,
        id: ExtensionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(hud) = self
            .native_sidebar
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.hud.clone())
        else {
            return div().size_full().into_any_element();
        };
        let appearance = SidebarAppearance::from_hud(&hud, window);
        // A per-session panel follows the session open in the Agents workspace.
        let session = self
            .local_workspace_latest_focus_key
            .as_ref()
            .map(|key| format!("{}:{}", key.project_id, key.session_id));
        // Until the first frame measures it, the panel counts as wide.
        let width = match self.native_sidebar.extension_side_panel_width {
            width if width > 0.0 => width,
            _ => 480.0,
        };
        let view = extension_side_panel_view(
            self.native_extension_panels(),
            id.as_str(),
            session.as_deref(),
            &self.native_sidebar.extension_panel_ui,
            width,
        );
        let measure = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(first) = bounds.first() else {
                    return;
                };
                let measured = f32::from(first.size.width);
                measure.update(cx, |app, cx| {
                    if (app.native_sidebar.extension_side_panel_width - measured).abs() > 1.0 {
                        app.native_sidebar.extension_side_panel_width = measured;
                        cx.notify();
                    }
                });
            })
            .id(gpui::SharedString::from(format!(
                "extension-side-panel-{}",
                id.as_str()
            )))
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .bg(titlebar_background())
            .text_color(appearance.foreground)
            .text_size(px(
                crate::app::native_sidebar::appearance::SIDEBAR_TEXT_SIZE * appearance.scale,
            ))
            .font_family(crate::ui_fonts::UI_FONT)
            .p(px(10.0 * appearance.scale))
            .child(match view {
                Some(view) => {
                    self.render_native_extension_surface(&view, "sidePanel", true, &appearance, cx)
                }
                None => div()
                    .w_full()
                    .text_color(appearance.muted)
                    .child("Open a session to see its panel.")
                    .into_any_element(),
            })
            .into_any_element()
    }
}
