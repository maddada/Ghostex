// The gxserver half of the app-modal hydrate, held between opens so an open does not wait on gxserver.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:AppModal 2026-09-26 WHY:
    /// Every app-modal open used to read projects, agents, actions, Recent Projects, pinned prompts and session tags from gxserver synchronously on the UI thread before its window existed. That was about 60 ms on macOS, but on Windows gxserver usually runs inside WSL, so each fresh connection crossed WSL's localhost relay while the app, CEF's message pump included, stood still.
    /// An open now assembles the message from fresh local settings plus the gxserver data held from the previous read, and refreshes that data in the background; the open modal receives a corrected hydrate only when gxserver's data actually changed. Only the first open of a launch still reads synchronously, and the warm spare's preload normally makes that read first, in the background.
    pub(crate) fn gpui_app_modal_sidebar_state_message_from_held_hydrate(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> serde_json::Value {
        let active_project_id = self.gpui_app_modal_hydrate_active_project_id();
        let held = self
            .app_modal_gxserver_hydrate
            .as_ref()
            .filter(|hydrate| hydrate.active_project_id == active_project_id)
            .cloned();
        let hydrate = match held {
            Some(hydrate) => {
                self.refresh_gpui_app_modal_gxserver_hydrate(cx);
                hydrate
            }
            None => {
                let hydrate =
                    gpui_fetch_app_modal_gxserver_hydrate(None, active_project_id.as_deref());
                self.app_modal_gxserver_hydrate = Some(hydrate.clone());
                hydrate
            }
        };
        self.gpui_app_modal_sidebar_state_message_for_hydrate(&hydrate)
    }

    pub(crate) fn gpui_app_modal_sidebar_state_message_for_hydrate(
        &self,
        hydrate: &GpuiAppModalGxserverHydrate,
    ) -> serde_json::Value {
        self.with_gpui_command_pane_sidebar_indicators(
            gpui_app_modal_sidebar_state_message_from_gxserver_hydrate(
                &shared_settings::shared_sidebar_settings_snapshot(),
                hydrate,
            ),
        )
    }

    pub(crate) fn refresh_gpui_app_modal_gxserver_hydrate(&mut self, cx: &mut gpui::Context<Self>) {
        if self.app_modal_gxserver_hydrate_refreshing {
            return;
        }
        self.app_modal_gxserver_hydrate_refreshing = true;
        let active_project_id = self.gpui_app_modal_hydrate_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let hydrate = background
                .spawn(async move {
                    gpui_fetch_app_modal_gxserver_hydrate(None, active_project_id.as_deref())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.receive_gpui_app_modal_gxserver_hydrate(hydrate, cx);
            });
        })
        .detach();
    }

    fn receive_gpui_app_modal_gxserver_hydrate(
        &mut self,
        hydrate: GpuiAppModalGxserverHydrate,
        cx: &mut gpui::Context<Self>,
    ) {
        self.app_modal_gxserver_hydrate_refreshing = false;
        let changed = self.app_modal_gxserver_hydrate.as_ref() != Some(&hydrate);
        self.app_modal_gxserver_hydrate = Some(hydrate.clone());
        if changed && let Some(handle) = self.app_modal_window {
            let message = self.gpui_app_modal_sidebar_state_message_for_hydrate(&hydrate);
            let _ = handle.update(cx, |host, _window, cx| {
                if host.current_modal.requires_sidebar_state() {
                    host.refresh_sidebar_state_message(message, cx);
                }
            });
        }
        self.ensure_gpui_app_modal_spare_preloaded(cx);
    }

    fn gpui_app_modal_hydrate_active_project_id(&self) -> Option<String> {
        gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
            .map(str::to_string)
    }
}
