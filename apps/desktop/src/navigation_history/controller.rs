//! Back/Forward's conversation with gxserver: it records the stop the sidebar is on, walks the
//! trail when an arrow, a hotkey or a swipe asks, and focuses the stop it lands on.
//!
//! CDXC:Navigation 2026-09-25 WHY:
//! This was the runtime's `NavigationHistoryController`, fed from `postActiveProjectContext` and
//! reached through a `ghostex-gpui-sidebar-navigation-history-command` page event; the titlebar
//! only painted the availability it posted back. The app runtime port (family F6) runs it here on
//! the store's own sidebar model, with the same rules: an unchanged stop costs one comparison, a
//! changed one waits 150 ms and only one visit is ever in flight, a navigation first flushes the
//! pending visit so the daemon walks back from where the user really is, a stop that no longer
//! exists is forgotten and the walk continues (at most 12 tries), a second click while one hop is
//! still activating is queued once, and for 4 s after a landing the intermediate focus changes
//! are not recorded. The focus itself still goes out as the `focusSession` / `focusGroup` a
//! sidebar click sends. Only `gx_rpc`, gx-core and GPUI are used, so the web build can compile it.
//!
//! SEE-ALSO: packages/gx-core/src/navigation_history.rs (the bookkeeping and the wire contract),
//! apps/desktop/src/app/gx_store/activation_focus.rs (the focus commands).

use std::time::Duration;

use ghostex_gx_core::navigation_history::{
    MAX_NAVIGATION_ATTEMPTS, NAVIGATION_HISTORY_NAVIGATE_ENDPOINT,
    NAVIGATION_HISTORY_READ_ENDPOINT, NAVIGATION_HISTORY_SCOPE_GPUI,
    NAVIGATION_HISTORY_VISIT_ENDPOINT, NAVIGATION_VISIT_DEBOUNCE_MS, NavigationHistoryButtons,
    NavigationHistoryEntry, NavigationTrail, navigation_history_buttons, navigation_history_target,
};
use serde_json::{Value, json};

use super::GpuiNavigationHistoryState;
use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;

/// The trail's client state. The buttons themselves are `navigation_history_state`.
#[derive(Default)]
pub(crate) struct NavigationHistoryHost {
    trail: NavigationTrail,
    /// Bumped to cancel an armed debounce.
    visit_timer: u64,
    visit_timer_armed: bool,
    visit_in_flight: bool,
    navigating: bool,
    queued: Option<&'static str>,
    /// A navigation waiting for the visits in flight to land first.
    after_visit: Option<&'static str>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

impl GhostexGpuiApp {
    /// Adopts the trail this scope already has, so Back works across an app restart. Run when
    /// this computer's stream goes live.
    pub(crate) fn navigation_history_refresh(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let response = gx_rpc(
                None,
                NAVIGATION_HISTORY_READ_ENDPOINT,
                json!({ "scopeId": NAVIGATION_HISTORY_SCOPE_GPUI }),
            )
            .await;
            if let Ok(response) = response {
                let _ = this.update(cx, |app, cx| app.apply_navigation_history(&response, cx));
            }
        })
        .detach();
    }

    /// The sidebar list was rebuilt: record the stop it is on.
    pub(crate) fn navigation_history_sidebar_changed(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(entry) = self.gx_store_navigation_entry() else {
            return;
        };
        if self.navigation_history.trail.record_visit(entry, now_ms()) {
            self.arm_navigation_visit(cx);
        }
    }

    /// One arrow click, hotkey or swipe.
    pub(crate) fn navigate_history(
        &mut self,
        direction: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = &mut self.navigation_history;
        if host.navigating {
            // One queued click covers a double click while the first hop activates.
            host.queued = Some(direction);
            return;
        }
        host.navigating = true;
        host.visit_timer += 1;
        host.visit_timer_armed = false;
        if host.visit_in_flight || host.trail.has_pending() {
            host.after_visit = Some(direction);
            if !host.visit_in_flight {
                self.send_navigation_visit(cx);
            }
            return;
        }
        self.navigation_history_step(direction, Vec::new(), cx);
    }

    fn arm_navigation_visit(&mut self, cx: &mut gpui::Context<Self>) {
        let host = &mut self.navigation_history;
        if host.visit_timer_armed {
            return;
        }
        host.visit_timer_armed = true;
        let token = host.visit_timer;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(NAVIGATION_VISIT_DEBOUNCE_MS))
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.navigation_history.visit_timer != token {
                    return;
                }
                app.navigation_history.visit_timer_armed = false;
                if !app.navigation_history.visit_in_flight {
                    app.send_navigation_visit(cx);
                }
            });
        })
        .detach();
    }

    fn send_navigation_visit(&mut self, cx: &mut gpui::Context<Self>) {
        let Some((entry, replace_current)) = self.navigation_history.trail.take_pending() else {
            self.navigation_history_visits_settled(cx);
            return;
        };
        self.navigation_history.visit_in_flight = true;
        let mut params = json!({
            "entry": entry.to_json(),
            "scopeId": NAVIGATION_HISTORY_SCOPE_GPUI,
        });
        if replace_current {
            params["replaceCurrent"] = json!(true);
        }
        cx.spawn(async move |this, cx| {
            let response = gx_rpc(None, NAVIGATION_HISTORY_VISIT_ENDPOINT, params).await;
            let _ = this.update(cx, |app, cx| {
                app.navigation_history.visit_in_flight = false;
                match response {
                    Ok(response) => app.apply_navigation_history(&response, cx),
                    Err(_) => app.navigation_history.trail.visit_failed(),
                }
                // A stop that changed while this one was in flight goes next.
                if app.navigation_history.trail.has_pending() {
                    app.send_navigation_visit(cx);
                } else {
                    app.navigation_history_visits_settled(cx);
                }
            });
        })
        .detach();
    }

    /// No visit is pending or in flight: a navigation that waited for them goes now.
    fn navigation_history_visits_settled(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(direction) = self.navigation_history.after_visit.take() {
            self.navigation_history_step(direction, Vec::new(), cx);
        }
    }

    /// One `navigateHistory` call, and the activation of the stop it answers with.
    fn navigation_history_step(
        &mut self,
        direction: &'static str,
        forget_keys: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params =
            json!({ "direction": direction, "scopeId": NAVIGATION_HISTORY_SCOPE_GPUI });
        if !forget_keys.is_empty() {
            params["forgetKeys"] = json!(forget_keys);
        }
        cx.spawn(async move |this, cx| {
            let response = gx_rpc(None, NAVIGATION_HISTORY_NAVIGATE_ENDPOINT, params).await;
            let _ = this.update(cx, |app, cx| {
                let Ok(response) = response else {
                    app.finish_navigation_history(cx);
                    return;
                };
                app.apply_navigation_history(&response, cx);
                let Some(target) = navigation_history_target(&response) else {
                    app.finish_navigation_history(cx);
                    return;
                };
                if app.activate_navigation_history_entry(&target, cx) {
                    let host = &mut app.navigation_history;
                    host.trail.landed(target, now_ms());
                    host.visit_timer += 1;
                    host.visit_timer_armed = false;
                    app.finish_navigation_history(cx);
                    return;
                }
                let mut forget_keys = forget_keys;
                forget_keys.push(target.key());
                if forget_keys.len() < MAX_NAVIGATION_ATTEMPTS {
                    app.navigation_history_step(direction, forget_keys, cx);
                } else {
                    app.finish_navigation_history(cx);
                }
            });
        })
        .detach();
    }

    fn finish_navigation_history(&mut self, cx: &mut gpui::Context<Self>) {
        self.navigation_history.navigating = false;
        if let Some(direction) = self.navigation_history.queued.take() {
            self.navigate_history(direction, cx);
        }
    }

    /// Focuses a stop, or reports it gone so the daemon drops it. A stop's session wins over its
    /// group: the session is what the user was looking at.
    fn activate_navigation_history_entry(
        &mut self,
        entry: &NavigationHistoryEntry,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let groups = self.gx_store.sidebar_list.model().built_groups();
        if let Some(session_id) = entry.session_id.as_deref() {
            let exists = groups
                .iter()
                .any(|(_, rows)| rows.iter().any(|row| row.sidebar_session_id == session_id));
            drop(groups);
            return exists && self.gx_store_focus_activated_session(session_id, cx);
        }
        let Some(group_id) = entry.group_id.as_deref() else {
            return false;
        };
        let exists = groups.iter().any(|(group, _)| group.group_id == group_id);
        drop(groups);
        exists && self.gx_store_focus_activated_group(group_id, cx)
    }

    fn apply_navigation_history(&mut self, response: &Value, cx: &mut gpui::Context<Self>) {
        let NavigationHistoryButtons {
            can_go_back,
            can_go_forward,
        } = navigation_history_buttons(response);
        let state = GpuiNavigationHistoryState {
            can_go_back,
            can_go_forward,
        };
        if self.navigation_history_state != state {
            self.navigation_history_state = state;
            cx.notify();
        }
    }
}
