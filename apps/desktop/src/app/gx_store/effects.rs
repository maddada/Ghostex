use futures::StreamExt as _;
use futures::channel::mpsc;
use ghostex_gx_core::{Effect, MachineId};

use super::host::GxStoreHost;
use crate::GhostexGpuiApp;

/// The effects that need the app, not only the store: reads through `gx_rpc` whose answers land in
/// app state. `run_effects` has no `cx`, so it hands them to one task the app runs for its whole
/// life, in the order the core asked for them.
///
/// CDXC:Sidebar 2026-09-25 WHY:
/// Before the app runtime port these effects were dropped here because the QuickJS runtime made the
/// same reads on its own socket; performing them as well would have read everything twice. Each
/// one is performed here from the commit that deleted the runtime's copy.
#[derive(Default)]
pub(crate) struct AppEffectQueue {
    sender: Option<mpsc::UnboundedSender<Effect>>,
    /// What the core asked for before the task started: the first snapshot can land before it.
    held: Vec<Effect>,
}

impl AppEffectQueue {
    fn push(&mut self, effect: Effect) {
        match &self.sender {
            Some(sender) => {
                if let Err(error) = sender.unbounded_send(effect) {
                    self.held.push(error.into_inner());
                }
            }
            None => self.held.push(effect),
        }
    }
}

impl GxStoreHost {
    /// Performs what the core asked for: the requests that keep the store itself correct, and
    /// what focus ownership needs. The reads whose answers land outside the store go to the app's
    /// effect task (`AppEffectQueue`).
    pub(super) fn run_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                // Routed by machine: a remote machine's snapshot has to be asked of that
                // machine's client, and asking the local one would resubscribe the wrong daemon
                // while the machine that needs a snapshot waits for ever.
                Effect::ResubscribePresentation { machine, reason } => {
                    self.counters.resubscribes_requested += 1;
                    self.diagnostics.resubscribe_requested(&reason);
                    let client = match machine.remote_id() {
                        None => self.client.as_ref(),
                        Some(machine_id) => self.remote.client(machine_id),
                    };
                    if let Some(client) = client {
                        client.request_resubscribe();
                    }
                }
                Effect::ReportSkippedRows {
                    projects,
                    groups,
                    sessions,
                    first_error,
                    ..
                } => {
                    self.counters.skipped_row_reports += 1;
                    self.diagnostics
                        .skipped_rows(projects, groups, sessions, &first_error);
                }
                // Kept newest per project and written to client storage (`projectLastSession`) when
                // the selection finishes (gx_store/focus_perform.rs), local and remote alike.
                Effect::RememberProjectSession { session, .. } => {
                    self.local_focus.remember(session);
                }
                // The notification feed is this computer's daemon's: the old runtime read it on
                // its local socket only, and a remote machine's feed is not shown.
                Effect::RefetchNotificationFeed { machine } if machine.is_local() => {
                    self.app_effects
                        .push(Effect::RefetchNotificationFeed { machine });
                }
                // A stream that went live reads the feed (this computer's only) and the HUD
                // (gx_store/hud/); the HUD is read again when its announcements say so.
                effect @ (Effect::MachineLive { .. }
                | Effect::RefetchSidebarHud { .. }
                | Effect::DomainProjectChanged { .. }) => self.app_effects.push(effect),
                // Attention: the acknowledgement timer, the report to the session's daemon and the
                // completion sound (gx_store/attention/).
                effect @ (Effect::ArmAttentionAcknowledge { .. }
                | Effect::ReportAgentActivity { .. }
                | Effect::SessionAttentionRaised { .. }) => self.app_effects.push(effect),
                // The core's effect list grows with each milestone; a new one is wired when the
                // milestone that introduces it lands.
                _ => {}
            }
        }
    }
}

impl GhostexGpuiApp {
    /// Starts the task that performs the app-level effects. Idempotent: the task lives as long as
    /// the app.
    pub(crate) fn gx_store_start_app_effects(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.app_effects.sender.is_some() {
            return;
        }
        // The first HUD, from the settings alone, before any read answers: the runtime posted the
        // same at its start, so the list never waits for one (gx_store/hud/).
        self.gx_store_compose_hud(cx);
        let (sender, mut effects) = mpsc::unbounded::<Effect>();
        for effect in std::mem::take(&mut self.gx_store.app_effects.held) {
            let _ = sender.unbounded_send(effect);
        }
        self.gx_store.app_effects.sender = Some(sender);
        cx.spawn(async move |this, cx| {
            while let Some(effect) = effects.next().await {
                if this
                    .update(cx, |this, cx| this.gx_store_perform_app_effect(effect, cx))
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    }

    fn gx_store_perform_app_effect(&mut self, effect: Effect, cx: &mut gpui::Context<Self>) {
        match effect {
            Effect::RefetchNotificationFeed {
                machine: MachineId::Local,
            } => self.gx_store_refresh_notification_feed(cx),
            effect @ (Effect::ArmAttentionAcknowledge { .. }
            | Effect::ReportAgentActivity { .. }
            | Effect::SessionAttentionRaised { .. }) => {
                self.gx_store_perform_attention_effect(effect, cx)
            }
            Effect::MachineLive { machine } => {
                if machine.is_local() {
                    self.gx_store_refresh_notification_feed(cx);
                    self.navigation_history_refresh(cx);
                }
                self.gx_store_perform_hud_effect(Effect::MachineLive { machine }, cx);
            }
            effect @ (Effect::RefetchSidebarHud { .. } | Effect::DomainProjectChanged { .. }) => {
                self.gx_store_perform_hud_effect(effect, cx)
            }
            _ => {}
        }
    }
}
