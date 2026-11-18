//! Autopilot's own "Autopilot" project, which each sidebar hides once (`SidebarUiStore::hide_once`,
//! CDXC:Autopilot 2026-11-18 there).

use crate::keys::ProjectKey;
use crate::presentation_store::PresentationStore;

/// The sidebar group of every Autopilot project gxserver marked (`autopilotSupervisorProject`), on
/// every machine this sidebar holds.
pub fn autopilot_project_group_ids(presentation: &PresentationStore) -> Vec<String> {
    let mut group_ids = Vec::new();
    for (machine, entry) in presentation.machines() {
        let Some(loaded) = entry.loaded() else {
            continue;
        };
        for project in loaded.projects() {
            if project.autopilot_supervisor_project {
                group_ids.push(
                    ProjectKey {
                        machine: machine.clone(),
                        project_id: project.project_id.clone(),
                    }
                    .to_sidebar_group_id(),
                );
            }
        }
    }
    group_ids
}
