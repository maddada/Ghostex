// The gxserver half of the app-modal hydrate, held between opens so an open does not wait on gxserver.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:AppModal 2026-09-26 WHY:
    /// Every app-modal open used to read projects, agents, actions, Recent Projects, pinned prompts and session tags from gxserver synchronously on the UI thread before its window existed. That was about 60 ms on macOS, but on Windows gxserver usually runs inside WSL, so each fresh connection crossed WSL's localhost relay while the app, CEF's message pump included, stood still.
    /// An open now assembles the message from fresh local settings plus the gxserver data held from the previous read, and refreshes that data in the background. Only the first open of a launch still reads synchronously, and the warm spare's preload normally makes that read first, in the background.
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
                let hydrate = gpui_fetch_app_modal_gxserver_hydrate(active_project_id.as_deref());
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

    /// CDXC:Settings 2026-10-23 DECISION:
    /// User (Q35, "Should saving a setting stop freezing the window while it asks gxserver for fresh data?"): "Yes, read in the background". A settings change used to read this hydrate from gxserver synchronously on the main thread, up to five reads with a 2 s timeout each, in every open window. The open Settings and Git Commit windows now get the new settings at once with the gxserver data held from the last read, and that data again when the background read lands if it moved, so their agent or Actions lists can lag a save until then.
    pub(crate) fn refresh_open_gpui_app_modals_after_settings_change(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_hydrate_active_project_id();
        let held = self
            .app_modal_gxserver_hydrate
            .as_ref()
            .filter(|hydrate| hydrate.active_project_id == active_project_id)
            .cloned();
        if let Some(hydrate) = &held {
            let message = self.with_gpui_command_pane_sidebar_indicators(
                gpui_app_modal_sidebar_state_message_from_gxserver_hydrate(
                    settings_snapshot,
                    hydrate,
                ),
            );
            self.send_gpui_app_modal_sidebar_state(message, cx);
        }
        self.app_modal_gxserver_hydrate_resend = GpuiAppModalHydrateResend::Waiting {
            sent: held,
            stale: self.app_modal_gxserver_hydrate_refreshing,
        };
        self.refresh_gpui_app_modal_gxserver_hydrate(cx);
    }

    fn send_gpui_app_modal_sidebar_state(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.refresh_open_gpui_app_modal_sidebar_state(message, cx);
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
                    gpui_fetch_app_modal_gxserver_hydrate(active_project_id.as_deref())
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
        self.app_modal_gxserver_hydrate = Some(hydrate.clone());
        match std::mem::take(&mut self.app_modal_gxserver_hydrate_resend) {
            GpuiAppModalHydrateResend::Idle => {}
            GpuiAppModalHydrateResend::Waiting { sent, stale: true } => {
                self.app_modal_gxserver_hydrate_resend =
                    GpuiAppModalHydrateResend::Waiting { sent, stale: false };
                self.refresh_gpui_app_modal_gxserver_hydrate(cx);
            }
            GpuiAppModalHydrateResend::Waiting { sent, stale: false } => {
                // The settings are read again here: a later save may have landed since the change.
                if sent.as_ref() != Some(&hydrate)
                    && hydrate.active_project_id == self.gpui_app_modal_hydrate_active_project_id()
                {
                    let message = self.gpui_app_modal_sidebar_state_message_for_hydrate(&hydrate);
                    self.send_gpui_app_modal_sidebar_state(message, cx);
                }
            }
        }
    }

    fn gpui_app_modal_hydrate_active_project_id(&self) -> Option<String> {
        gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
            .map(str::to_string)
    }
}
