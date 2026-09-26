//! The sessions this client shows, reported to the daemons that own them as keep-awake leases,
//! so their Auto Sleep sweep leaves them alone. Web-ready: `gx_rpc` only.
//!
//! CDXC:SessionSleep 2026-09-25 WHY:
//! Auto Sleep runs in gxserver (CDXC:SessionSleep 2026-09-25 DECISION in `server/src/session_auto_sleep.rs`), which cannot see a client's screen. Every client reports what it shows through `/api/holdSessionsAwake` (the phone already did), renewing before the lease runs out and releasing what it stopped showing, and a session is shown while any client holds it. A remote machine's session is held on that machine.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/shown_sessions.rs,
//! server/src/session_keep_awake.rs.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// How long one report holds a session (the daemon's default lease).
pub(crate) const SHOWN_SESSIONS_TTL_MS: i64 = 180_000;
/// How often the whole set is reported again, well inside the lease.
pub(crate) const SHOWN_SESSIONS_RENEW_MS: u64 = 60_000;

/// The shown sessions per machine (`None` is this computer), as `(projectId, sessionId)`.
pub(crate) type ShownSessions = BTreeMap<Option<String>, BTreeSet<(String, String)>>;

/// One `/api/holdSessionsAwake` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HoldCall {
    pub(crate) machine: Option<String>,
    pub(crate) sessions: Vec<(String, String)>,
    pub(crate) release: bool,
}

/// What was last reported, so a change sends only the machines it touched.
#[derive(Default)]
pub(crate) struct ShownSessionsReport {
    reported: ShownSessions,
}

impl ShownSessionsReport {
    /// The calls that bring the daemons from what was reported to `next`: a machine whose set
    /// changed reports its whole new set, and the sessions it no longer shows are released.
    pub(crate) fn changes(&mut self, next: ShownSessions) -> Vec<HoldCall> {
        let mut calls = Vec::new();
        let machines: BTreeSet<Option<String>> =
            self.reported.keys().chain(next.keys()).cloned().collect();
        for machine in machines {
            let before = self.reported.get(&machine);
            let after = next.get(&machine);
            if before == after {
                continue;
            }
            let released: Vec<(String, String)> = before
                .map(|before| {
                    before
                        .iter()
                        .filter(|session| after.is_none_or(|after| !after.contains(*session)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if !released.is_empty() {
                calls.push(HoldCall {
                    machine: machine.clone(),
                    sessions: released,
                    release: true,
                });
            }
            if let Some(after) = after.filter(|after| !after.is_empty()) {
                calls.push(HoldCall {
                    machine: machine.clone(),
                    sessions: after.iter().cloned().collect(),
                    release: false,
                });
            }
        }
        self.reported = next;
        calls
    }

    /// The calls that renew every lease this client holds.
    pub(crate) fn renewal(&self) -> Vec<HoldCall> {
        self.reported
            .iter()
            .filter(|(_, sessions)| !sessions.is_empty())
            .map(|(machine, sessions)| HoldCall {
                machine: machine.clone(),
                sessions: sessions.iter().cloned().collect(),
                release: false,
            })
            .collect()
    }
}

/// Sends one call. A daemon that is not reachable just misses a renewal; the next one repeats it.
pub(crate) async fn send_hold_call(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    holder_id: String,
    call: HoldCall,
) {
    let sessions: Vec<Value> = call
        .sessions
        .into_iter()
        .map(|(project_id, session_id)| json!({ "projectId": project_id, "sessionId": session_id }))
        .collect();
    let mut params = json!({
        "holderId": holder_id,
        "sessions": sessions,
        "ttlMs": SHOWN_SESSIONS_TTL_MS,
    });
    if call.release {
        params["release"] = Value::Bool(true);
    }
    let _ = gx_rpc(remote, "/api/holdSessionsAwake", params).await;
}
