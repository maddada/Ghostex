//! When the host must read again what the event stream only announces.
//!
//! The sidebar HUD (agents and Actions), a machine's recent projects and the notification feed are
//! plain HTTP reads; the stream says only that one of them may have moved. These rules are the old
//! app runtime's, moved here so every client reads again at the same moments: when a machine's
//! stream goes live (a first connect, or back after a loss, when announcements may have been
//! missed), and when a delta carries a project's domain row or removes a project, because agents
//! and project Actions are project metadata.
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! Every desktop read of these three used to start inside the QuickJS runtime (`startFromBootstrap`,
//! the stream recovery ladder, `applyDomainProjectDelta`); the app runtime port moves the reads to
//! the host, and the triggers belong to the store that sees the frames.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/effects.rs (the desktop performs them),
//! the deleted `applyDomainProjectDelta` in apps/desktop/sidebar/gxserver-runtime/sidebar-groups.ts.

use ghostex_gx_protocol::PresentationDelta;

use crate::core::Effect;
use crate::keys::MachineId;

/// The effect a delta that the store APPLIED asks for, if any. A stale delta asks for nothing,
/// as in the runtime, which dropped it before looking at it.
pub(crate) fn delta_refetch(machine: &MachineId, delta: &PresentationDelta) -> Option<Effect> {
    match delta {
        PresentationDelta::ProjectAdded {
            project,
            domain_project: Some(domain_project),
        }
        | PresentationDelta::ProjectUpdated {
            project,
            domain_project: Some(domain_project),
        } => Some(Effect::DomainProjectChanged {
            machine: machine.clone(),
            project_id: project.project_id.clone(),
            is_recent_project: domain_project.get("isRecentProject")
                == Some(&serde_json::Value::Bool(true)),
            removed: false,
        }),
        PresentationDelta::ProjectRemoved { project_id } => Some(Effect::DomainProjectChanged {
            machine: machine.clone(),
            project_id: project_id.clone(),
            is_recent_project: false,
            removed: true,
        }),
        _ => None,
    }
}
