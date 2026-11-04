//! This computer's agentbox run locations (`/api/agentbox` `{"action":"status"}`), read for the New
//! Thread picker's Run on row and the sidebar launcher's Run in a Box pages, and the store facts the
//! desktop asks about box sessions.
//!
//! CDXC:AgentBox 2026-10-01 WHY:
//! The status is read in the background when the picker or the launcher opens (reused for 30
//! seconds; gxserver caches `agentbox doctor` for about as long), once when the sidebar first
//! ticks, and then every ten minutes only while agentbox is installed, so a computer without it
//! pays for one cheap read. A failed read, including a gxserver that does not know the endpoint
//! yet, keeps the last answer and does not count as fresh, so the next open reads again; with no
//! answer at all only this computer is offered. The Settings > Cloud Boxes page's own status
//! reads update this cache too, so a provider set up there shows in the next launcher.
//!
//! SEE-ALSO: packages/gx-core/src/agentbox.rs, apps/desktop/src/app/new_thread_picker_lifecycle.rs.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ghostex_gx_core::{
    AgentboxLocations, SessionKey, agentbox_locations_from_status,
    session_chat_view_unavailable_showing_cloud_pages,
};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_gxserver_rpc_result;

/// gxserver runs `agentbox doctor --json` for an uncached status, which takes a few seconds.
const STATUS_TIMEOUT: Duration = Duration::from_secs(25);
/// A surface that opens reuses a read this recent.
pub(crate) const AGENTBOX_STATUS_ON_OPEN: Duration = Duration::from_secs(30);
/// The sidebar tick re-reads it this rarely, and only while agentbox is installed.
const AGENTBOX_STATUS_IN_BACKGROUND: Duration = Duration::from_secs(600);

/// Something waiting for the status (New Agent Session with a box default, before any answer).
type StatusWaiter = Box<
    dyn FnOnce(&mut GhostexGpuiApp, Option<AgentboxLocations>, &mut gpui::Context<GhostexGpuiApp>)
        + Send,
>;

struct StatusCache {
    /// The last status gxserver answered; `None` until one arrives.
    locations: Option<AgentboxLocations>,
    /// When the last answered read started.
    read_at: Option<Instant>,
    /// When the last read started, answered or not: the background tick's clock.
    attempted_at: Option<Instant>,
    in_flight: bool,
    waiters: Vec<StatusWaiter>,
}

static STATUS: Mutex<StatusCache> = Mutex::new(StatusCache {
    locations: None,
    read_at: None,
    attempted_at: None,
    in_flight: false,
    waiters: Vec::new(),
});

/// Whether the Cloud Boxes built-in extension is on. While it is off nothing reads the status and
/// every surface sees no answer, so no Run on row or Run in a Box page appears.
fn cloud_boxes_enabled() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot().cloud_boxes_enabled()
}

/// The last status gxserver answered, if any; none while Cloud Boxes is off.
pub(crate) fn cached_agentbox_locations() -> Option<AgentboxLocations> {
    if !cloud_boxes_enabled() {
        return None;
    }
    STATUS.lock().ok().and_then(|cache| cache.locations.clone())
}

/// Takes an `/api/agentbox` status answer read elsewhere (the Settings > Cloud Boxes page).
/// Returns whether the ready locations changed.
fn store_status_answer(status: &Value, started: Instant) -> bool {
    let Ok(mut cache) = STATUS.lock() else {
        return false;
    };
    let locations = Some(agentbox_locations_from_status(status));
    let changed = cache.locations != locations;
    cache.locations = locations;
    cache.read_at = Some(started);
    changed
}

/// Makes the next open read the status again (a Cloud Boxes setup step was started).
pub(crate) fn invalidate_agentbox_status() {
    if let Ok(mut cache) = STATUS.lock() {
        cache.read_at = None;
    }
}

impl GhostexGpuiApp {
    /// Reads the status in the background unless an answered read younger than `max_age` exists
    /// or one is under way; the picker gets the answer pushed in, and the sidebar's next tick
    /// picks it up.
    pub(crate) fn refresh_agentbox_locations(
        &mut self,
        max_age: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        if !cloud_boxes_enabled() {
            return;
        }
        let started = Instant::now();
        {
            let Ok(mut cache) = STATUS.lock() else {
                return;
            };
            let fresh = cache
                .read_at
                .is_some_and(|read_at| read_at.elapsed() < max_age);
            if cache.in_flight || fresh {
                return;
            }
            cache.in_flight = true;
            cache.attempted_at = Some(started);
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let answer = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/agentbox",
                        &json!({ "action": "status" }),
                        STATUS_TIMEOUT,
                    )
                })
                .await;
            let changed = match &answer {
                Ok(status) => store_status_answer(status, started),
                Err(_) => false,
            };
            let (locations, waiters) = {
                let Ok(mut cache) = STATUS.lock() else {
                    return;
                };
                cache.in_flight = false;
                (cache.locations.clone(), std::mem::take(&mut cache.waiters))
            };
            let _ = this.update(cx, |this, cx| {
                if changed {
                    this.push_gpui_new_thread_picker_run_locations(cx);
                }
                for waiter in waiters {
                    waiter(this, locations.clone(), cx);
                }
            });
        })
        .detach();
    }

    /// Runs `then` with the status: at once when one has been answered, otherwise once the read
    /// under way (or one started here) finishes, answered or not. Never blocks the main thread.
    pub(crate) fn with_agentbox_locations(
        &mut self,
        then: impl FnOnce(&mut Self, Option<AgentboxLocations>, &mut gpui::Context<Self>)
        + Send
        + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        if !cloud_boxes_enabled() {
            return then(self, None, cx);
        }
        let cached = {
            let Ok(mut cache) = STATUS.lock() else {
                return then(self, None, cx);
            };
            if cache.locations.is_none() {
                cache.waiters.push(Box::new(then));
                None
            } else {
                Some((cache.locations.clone(), then))
            }
        };
        match cached {
            Some((locations, then)) => then(self, locations, cx),
            None => self.refresh_agentbox_locations(Duration::ZERO, cx),
        }
    }

    /// A status the Settings > Cloud Boxes page read (settings_modal_lifecycle.rs).
    pub(crate) fn note_agentbox_status_answer(
        &mut self,
        status: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if store_status_answer(status, Instant::now()) {
            self.push_gpui_new_thread_picker_run_locations(cx);
        }
    }

    /// The sidebar's one-second tick: the launcher's Run in a Box page reads the cached status.
    /// After the first read, only a computer with agentbox installed keeps reading.
    pub(super) fn gx_store_poll_agentbox_locations(&mut self, cx: &mut gpui::Context<Self>) {
        let due = STATUS.lock().is_ok_and(|cache| {
            let installed = cache
                .locations
                .as_ref()
                .is_none_or(|locations| locations.supported && locations.installed);
            installed
                && cache
                    .attempted_at
                    .is_none_or(|attempted| attempted.elapsed() >= AGENTBOX_STATUS_IN_BACKGROUND)
        });
        if due {
            self.refresh_agentbox_locations(AGENTBOX_STATUS_IN_BACKGROUND, cx);
        }
    }

    /// gx-core's Chat View rule (`session_chat_view_unavailable`) for a workspace tab, local or
    /// on a remote machine.
    pub(crate) fn agents_session_chat_view_unavailable(
        &self,
        session_id: crate::TerminalSessionId,
    ) -> bool {
        let key = if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            SessionKey::local(key.project_id, key.session_id)
        } else if let Some(key) = self.agents_chat_remote_key_for_session(session_id) {
            SessionKey::remote(key.remote_machine_id, key.project_id, key.session_id)
        } else {
            return false;
        };
        // The desktop shows a Claude cloud session's claude.ai page in its chat pane.
        session_chat_view_unavailable_showing_cloud_pages(&self.gx_store.core, &key)
    }

    /// Whether the project a create without a group lands in is on another machine, whose boxes
    /// this computer's status does not describe.
    pub(crate) fn gx_store_active_project_is_remote(&self) -> bool {
        self.gx_store
            .core
            .focus()
            .active_project
            .as_ref()
            .is_some_and(|project| !project.machine.is_local())
    }
}
