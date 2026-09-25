//! The desktop half of attention: every door that means "the user saw this session" or "Escape in
//! its terminal" becomes a store intent here, and the store's attention effects are performed here.
//!
//! Doors: a terminal the user typed or clicked in, a tab a held key stopped on and Split Right
//! (queued with the selection tell, gx_store/burst.rs), a remote row click
//! (gx_store/sidebar_remote_focus.rs), the store's own focus paths (gx_store/focus_perform.rs,
//! which replaced the old runtime's facts channel `attentionAcknowledge`), and Escape
//! (terminal_sync/workspace_terminal_dispatch.rs).
//!
//! CDXC:Notifications 2026-09-25 WHY:
//! These all reached the QuickJS runtime's attention tracker, which kept its own copy of the rows.
//! One tracker now, in the store, so the minimum visible window and the local clear are decided
//! once whichever door the user came through.
//!
//! SEE-ALSO: packages/gx-core/src/attention.rs.

use std::time::Duration;

use ghostex_gx_core::{Effect, Event, Intent, SessionKey};

use super::report::report_agent_activity;
use crate::GhostexGpuiApp;
use crate::app::gx_store::host::now_ms;

impl GhostexGpuiApp {
    /// The user saw the session: acknowledge its attention now, or once it has been on screen for
    /// the minimum time.
    pub(crate) fn gx_store_acknowledge_attention(
        &mut self,
        session: SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_attention_intent(Intent::AcknowledgeAttention { session }, cx);
    }

    /// Escape in the session's terminal: no completion sound for a while, the attention cleared,
    /// the daemon told.
    pub(crate) fn gx_store_terminal_escape(
        &mut self,
        session: SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_attention_intent(Intent::TerminalEscape { session }, cx);
    }

    fn gx_store_attention_intent(&mut self, intent: Intent, cx: &mut gpui::Context<Self>) {
        let output = self.gx_store.core.handle(Event::Intent(intent), now_ms());
        if !output.changes.is_empty() {
            self.gx_store.sidebar_list.note_changes(&output.changes);
            self.gx_store_update_sidebar_list(cx);
        }
        self.gx_store.run_effects(output.effects);
    }

    /// Performs one attention effect (effects.rs hands them over).
    pub(in crate::app::gx_store) fn gx_store_perform_attention_effect(
        &mut self,
        effect: Effect,
        cx: &mut gpui::Context<Self>,
    ) {
        match effect {
            Effect::ArmAttentionAcknowledge {
                session,
                delay_ms,
                entered_at_ms,
            } => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(delay_ms))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.gx_store_attention_intent(
                            Intent::AttentionAcknowledgeDue {
                                session,
                                entered_at_ms,
                            },
                            cx,
                        );
                    });
                })
                .detach();
            }
            Effect::ReportAgentActivity {
                session,
                report,
                agent_name,
            } => {
                // A remote session is reported down its machine's tunnel; a machine that is not
                // connected any more has nobody to tell, as the old runtime's request would have
                // failed there too.
                let remote = match session.machine.remote_id() {
                    None => None,
                    Some(machine_id) => match self.remote_gxserver_connections.get(machine_id) {
                        Some(connection) => Some(connection.request_target()),
                        None => return,
                    },
                };
                cx.background_executor()
                    .spawn(report_agent_activity(remote, session, report, agent_name))
                    .detach();
            }
            Effect::SessionAttentionRaised { session } => {
                let Some(sound) = completion_sound() else {
                    return;
                };
                self.play_session_completion(sound, Some(session.to_sidebar_session_id()), cx);
            }
            _ => {}
        }
    }
}

/// The completion sound the settings ask for, or `None` when it is off: `completionBellEnabled`
/// false, or `completionSound` set to `off` (`normalizeCompletionSoundPreference` in
/// packages/shared/ghostex-settings/normalize.ts).
fn completion_sound() -> Option<&'static str> {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let settings = settings.object();
    if settings.get("completionBellEnabled") == Some(&serde_json::Value::Bool(false)) {
        return None;
    }
    let sound = settings
        .get("completionSound")
        .and_then(serde_json::Value::as_str);
    if sound == Some("off") {
        return None;
    }
    Some(crate::app::helpers::gpui_normalize_completion_sound(sound))
}
