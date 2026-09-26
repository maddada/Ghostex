//! The two sleep sets that are not one project's: the titlebar and Resources "Sleep Inactive"
//! across every machine, and the Running Sessions stop control's "every running local session".
//!
//! Both end in one `setSessionsSleeping` payload the host posts through the bulk path, so the
//! pacing (350 ms apart), the declined-sleep handling and the replacement focus are the ones every
//! other sleep uses.
//!
//! CDXC:SessionSleep 2026-09-25 WHY:
//! The old runtime's titlebar sweep filtered with `isGpuiInactiveProjectPresentationSession`,
//! which predates the user's decision below and so slept a session whose agent still had a
//! background shell or monitor running. This set uses the same `is_inactive` rule a project's
//! Sleep Inactive uses, which follows that decision; it is the one intended difference from the
//! runtime's set.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/runtime_actions.rs,
//! apps/desktop/sidebar/gxserver-runtime/auto-sleep.ts (`sleepInactiveSessionsFromTitlebar`,
//! `sleepAllLocalDaemonSessions`, deleted with this port).

use ghostex_gx_protocol::{LifecycleState, PresentationSession};

use crate::core::Core;
use crate::keys::{MachineId, SessionKey};

use super::bulk::is_inactive;

/// Sidebar session ids of every inactive awake session: this computer's first, then each remote
/// machine this run's stream delivered. A machine drawn from its last-seen copy has no tunnel to
/// sleep through and is skipped, as the runtime's `remotePresentations` skipped it.
pub fn titlebar_sleep_inactive_ids(core: &Core) -> Vec<String> {
    let mut ids = machine_rows(core, &MachineId::Local, is_inactive);
    for (machine, _) in core.presentation().machines() {
        if !machine.is_local() {
            ids.extend(machine_rows(core, machine, is_inactive));
        }
    }
    ids
}

/// Sidebar session ids of every RUNNING session on this computer, whatever it is doing: the
/// Running Sessions list's daemon-stop control stops the local daemon's sessions, and remote
/// machines are untouched because the list shows local daemon state.
pub fn running_local_session_ids(core: &Core) -> Vec<String> {
    machine_rows(core, &MachineId::Local, |row| {
        row.lifecycle_state == LifecycleState::Running
    })
}

/// A machine's rows that pass `keep`, in the daemon's array order (sort key byte order), as sidebar
/// session ids.
fn machine_rows(
    core: &Core,
    machine: &MachineId,
    keep: impl Fn(&PresentationSession) -> bool,
) -> Vec<String> {
    let Some(loaded) = core.presentation().loaded_live(machine) else {
        return Vec::new();
    };
    let mut rows: Vec<&PresentationSession> =
        loaded.server_sessions().filter(|row| keep(row)).collect();
    rows.sort_by(|left, right| {
        (
            left.sort_key.as_str(),
            left.project_id.as_str(),
            left.session_id.as_str(),
        )
            .cmp(&(
                right.sort_key.as_str(),
                right.project_id.as_str(),
                right.session_id.as_str(),
            ))
    });
    rows.into_iter()
        .map(|row| {
            SessionKey {
                machine: machine.clone(),
                project_id: row.project_id.clone(),
                session_id: row.session_id.clone(),
            }
            .to_sidebar_session_id()
        })
        .collect()
}
