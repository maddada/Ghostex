//! The app side of the native Automate view: which project it shows, rendering it in the view
//! panel, and handing it the store's answers (gx_store/create/board.rs) to its session-list requests.

use super::requests::AutomateScope;
use super::view::{AutomateHostInfo, NATIVE_AUTOMATE_SESSIONS_REQUEST_PREFIX, NativeAutomateView};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::model::*;
use gpui::{
    AnyElement, AnyView, AppContext as _, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, StyleRefinement, Styled as _, Window, div,
};

impl GhostexGpuiApp {
    /// The Automate scope for the sidebar's active project, with the same gates the CEF page's
    /// URL had (`automate_workarea_runtime_url_from_project_snapshot`): no Automate feature, a
    /// projectless context, or missing identity leaves the static placeholder.
    fn native_automate_scope(&self) -> Option<AutomateScope> {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        if !snapshot.feature_availability.automate {
            return None;
        }
        let project_id = snapshot.active_project_id.as_ref()?.0.clone();
        let editor_id = snapshot.surface_ids.automate_board_id.clone()?;
        if project_id == GPUI_QUICK_AUTOMATIONS_PROJECT_ID {
            return Some(AutomateScope {
                all_projects: true,
                project_id,
                project_path: String::new(),
                project_name: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE.to_string(),
                editor_id,
                coming_soon: !self.sidebar_runtime_settings_snapshot.show_beta_features,
            });
        }
        if snapshot.is_quick_projectless {
            return None;
        }
        let project_path = snapshot
            .in_memory_project_path
            .as_ref()?
            .to_string_lossy()
            .to_string();
        Some(AutomateScope {
            all_projects: false,
            project_id,
            project_path,
            project_name: snapshot.display_name.clone(),
            editor_id,
            coming_soon: false,
        })
    }

    /// The view panel's Automate page. See the DECISION on `NativeAutomateView`'s `Render`.
    ///
    /// CDXC:Automations 2026-09-23 WHY:
    /// The page is a cached view, so it is rebuilt only when it notifies itself, its project or the window glass and light appearance changed (checked here), or the settings it draws from changed (`native_automate_notify_appearance`), not on every app redraw (terminal output, status indicators). Anything else it reads from outside must notify it.
    pub(crate) fn render_native_automate_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(scope) = self.native_automate_scope() else {
            let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Automate)
                .expect("Automate placeholder signature must exist");
            return self.render_project_editor_placeholder(signature, cx);
        };
        let view = match self.native_automate.clone() {
            Some(view) => view,
            None => {
                let app = cx.weak_entity();
                let view = cx.new(|cx| NativeAutomateView::new(app, cx));
                self.native_automate = Some(view.clone());
                view
            }
        };
        // Same bridge context the CEF page's automation requests ran with (workspace_events.rs).
        let mut context = project_board_bridge_runtime_context_from_snapshot(
            self.latest_sidebar_project_snapshot.as_ref(),
        );
        if let Some(context) = context.as_mut()
            && let Some(remote_machine_id) = context.remote_machine_id.as_deref()
        {
            context.remote_target = self.gpui_remote_gxserver_request_target(remote_machine_id);
        }
        let host = AutomateHostInfo {
            #[cfg(target_os = "linux")]
            window: window.window_handle(),
            main_window_bounds: self.main_window_bounds,
            display_id: self.main_window_display_id,
            palette: self.gpui_native_modal_palette(),
        };
        let appearance = (
            window_glass_active_in(window),
            CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed),
        );
        let changed = view.update(cx, |view, cx| {
            view.sync(scope, context, host, appearance, cx)
        });
        if changed {
            // A notify from inside this draw would only reach the next frame.
            window.render_view_this_frame(view.entity_id());
        }
        div()
            .id("ghostex-gpui-native-automate-host")
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.focus_project_editor_surface(TitlebarMode::Automate, window, cx);
                }),
            )
            .child(AnyView::from(view).cached(StyleRefinement::default().size_full()))
            .into_any_element()
    }

    /// The settings or theme the Automate page draws from changed.
    pub(crate) fn native_automate_notify_appearance(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(view) = self.native_automate.as_ref() {
            gpui::App::notify(cx, view.entity_id());
        }
    }

    /// Hands a sidebar-runtime board response to the Automate view when it answers one of the
    /// view's own session-list requests. False for every other response.
    pub(crate) fn route_native_automate_board_response(
        &mut self,
        response: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let ours = response
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id.starts_with(NATIVE_AUTOMATE_SESSIONS_REQUEST_PREFIX));
        if !ours {
            return false;
        }
        if let Some(view) = self.native_automate.clone() {
            view.update(cx, |view, cx| view.receive_sessions_response(response, cx));
        }
        true
    }
}
