//! A click on a row of THIS computer, and everything that behaves like one: the page's half and
//! the runtime's half of the old click, both performed by the store.
//!
//! CDXC:FocusRouting 2026-09-21 WHY:
//! Five senders in this crate post `{type:'selectSession', mode:'focus'}`: a row click
//! (`native_sidebar/sessions.rs`), the project slot hotkey and the session slot hotkey (both
//! through `gx_store_focus_and_reveal_slot_row`), the session walk's steps into another project
//! (`session_walk.rs`) and a held key's landing row (`burst.rs`). All five end here, in ONE route,
//! and it is deliberately one interception rather than five call-site changes, so a sixth sender
//! cannot miss it.
//!
//! CDXC:FocusRouting 2026-09-25 WHY:
//! The route used to send the runtime its `focusSession` after the page's half. The runtime's focus
//! is gone (focus_perform.rs), so the store performs that half too: right after the click's own
//! in-process reaction, which the row click, the slot hotkeys and the walk run in the same frame
//! AFTER posting this command. That is the order the runtime's asynchronous answer always had, and
//! it matters: the reaction selects a live tab or stages one in the click's frame, and the focus
//! then wakes or attaches the staged tab. Whether the click changes the project is judged NOW,
//! before the reaction moves the store, and handed to the focus as `keep_view`
//! (CDXC:Navigation 2026-09-11 DECISION in focus_perform.rs).
//!
//! **The held-key hot path is not on it.** A held previous/next session walk reaches
//! `NativeSidebarClickReaction::InProcess` and sends NOTHING (`session_walk.rs`): no
//! `selectSession`, so no work here.
//!
//! **Counters** ride `gxStore.sidebarActions.summary` as `localFocus`: `focuses`, `browserRows`,
//! `modalsClosed`, `declinedSource`. A run in which the user clicked a row and `focuses` is zero
//! means the click never reached here; `declinedSource` moving means a click arrived before the
//! list was ready, which is the launch window and nothing else.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/focus_perform.rs,
//! apps/desktop/src/app/gx_store/sidebar_remote_focus.rs.

use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

use super::focus_perform::RowFocusOptions;
use crate::GhostexGpuiApp;

/// What this app run did with local row focus. Rides `gxStore.sidebarActions.summary`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalFocusRouteCounters {
    /// Local `selectSession` clicks the store focused.
    pub(crate) focuses: u64,
    /// Of those, the ones naming a browser tab, which no focus takes any more.
    pub(crate) browser_rows: u64,
    /// Of those, the ones that closed an open app modal (the click's `closeAppModal`).
    pub(crate) modals_closed: u64,
    /// Clicks the store dropped because the list was not ready yet (the launch window).
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Answers a LOCAL row's `selectSession` with `mode: focus`. Returns whether it did, in which
    /// case the command goes no further.
    ///
    /// A remote row never gets here: `gx_store_plan_remote_row_focus` runs first and owns it.
    pub(crate) fn gx_store_focus_local_row(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = local_focus_session_id(command) else {
            return false;
        };
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.local_focus_route.declined_source += 1;
            return false;
        }
        // `selectNativeSidebarSession` cleared the multi-selection before it posted; the store's
        // own selection intent is that clear.
        self.gx_store_note_sidebar_command(command, cx);
        // `closeAppModal('SettingsDismissal:focusSession')`, through the same function the bridge's
        // own `close` arm reaches. Before the focus, as the TypeScript had it.
        let had_modal = self.app_modal_window.is_some() || self.native_app_modal.is_some();
        self.close_app_modal_from_bridge(cx);
        // A focus starts from the store's newest selection.
        self.gx_store_flush_local_selection(cx);
        // `focusChangesActiveProject`, judged before the click's reaction moves the store.
        let keep_view = SessionKey::parse_sidebar_session_id(&session_id).is_some_and(|session| {
            self.gx_store.core.focus().active_project.as_ref() != Some(&session.project_key())
        });
        let browser = session_id.starts_with("gpui-browser:");
        let counters = &mut self.gx_store.local_focus_route;
        counters.focuses += 1;
        if browser {
            counters.browser_rows += 1;
        }
        if had_modal {
            counters.modals_closed += 1;
        }
        // After the click's in-process reaction, which the senders run right after this command.
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                let options = RowFocusOptions {
                    keep_view,
                    ..RowFocusOptions::default()
                };
                app.gx_store_focus_session_row(&session_id, options, cx);
            });
        });
        true
    }

    /// The counters, for the periodic summary in `sidebar_remote.rs`.
    pub(super) fn gx_store_local_focus_route_counters(&self) -> LocalFocusRouteCounters {
        self.gx_store.local_focus_route
    }
}

/// The session a LOCAL row click names. `selectSession` is a RENDERER command and arrives at the
/// top level; only `mode: focus` posts anything, and a remote id is not this path's.
fn local_focus_session_id(command: &Value) -> Option<String> {
    if command.get("type").and_then(Value::as_str) != Some("selectSession") {
        return None;
    }
    if command.get("mode").and_then(Value::as_str) != Some("focus") {
        return None;
    }
    let session_id = command.get("sessionId").and_then(Value::as_str)?;
    if SessionKey::parse_remote_scoped_session_id(session_id).is_some() {
        return None;
    }
    Some(session_id.to_string())
}

/// The counters as the summary carries them: 4 keys at depth 2.
pub(super) fn local_focus_route_counters_json(counters: &LocalFocusRouteCounters) -> Value {
    json!({
        "focuses": counters.focuses,
        "browserRows": counters.browser_rows,
        "modalsClosed": counters.modals_closed,
        "declinedSource": counters.declined_source,
    })
}
