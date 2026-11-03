//! How the Running Agents panel splits its projects by computer: this computer's projects first,
//! then one section per saved remote machine, in the order the sidebar's machine tabs keep them.
//!
//! CDXC:GhostexCapture 2026-11-03 DECISION:
//! User, about the floating button's panel: "pls show remote machines too here". Every remote
//! machine the sidebar has a tab for gets its own section under this computer's projects, named by
//! the machine. A connected machine lists its projects and running agents with the same rows as
//! this computer's; one that is not connected is a single quiet row saying so with a Connect
//! button, closed by default. Its last-seen rows are not listed: they are not what runs there now.
//! The machines and their states are the sidebar's own (`gx_store.remote.tabs()`), so the panel
//! adds no polling.

use ghostex_gx_core::{MachineId, ProjectKey};

use super::panel_window::PanelProject;

/// One saved remote machine, as the panel draws its header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PanelMachine {
    pub(crate) machine_id: String,
    pub(crate) name: String,
    pub(crate) connected: bool,
    /// A connect attempt is running.
    pub(crate) busy: bool,
    /// The last connect attempt failed.
    pub(crate) failed: bool,
    /// What its row says while it is not connected ("Not connected", "Cannot reach machine", …).
    pub(crate) status: String,
    /// Why, when the sidebar knows.
    pub(crate) detail: Option<String>,
}

/// One remote machine's part of the list.
pub(crate) struct MachineSection {
    pub(crate) machine: PanelMachine,
    /// Its projects with running agents; always empty while it is not connected.
    pub(crate) projects: Vec<PanelProject>,
}

/// This computer's projects, then each remote machine with its own projects. A remote project
/// whose machine has no tab, or is not connected, is left out.
pub(crate) fn split_by_machine(
    projects: Vec<PanelProject>,
    machines: &[PanelMachine],
) -> (Vec<PanelProject>, Vec<MachineSection>) {
    let mut local = Vec::new();
    let mut sections: Vec<MachineSection> = machines
        .iter()
        .map(|machine| MachineSection {
            machine: machine.clone(),
            projects: Vec::new(),
        })
        .collect();
    for project in projects {
        match ProjectKey::parse_workspace_project_id(&project.project_id).map(|key| key.machine) {
            Some(MachineId::Remote(machine_id)) => {
                if let Some(section) = sections.iter_mut().find(|section| {
                    section.machine.machine_id == machine_id && section.machine.connected
                }) {
                    section.projects.push(project);
                }
            }
            _ => local.push(project),
        }
    }
    (local, sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ghostex_capture::panel_window::PanelSession;
    use crate::app::helpers::GpuiStatusIndicatorStatus;

    fn project(id: &str) -> PanelProject {
        PanelProject {
            project_id: id.to_string(),
            title: id.to_string(),
            sessions: vec![PanelSession {
                session_id: format!("{id}-s"),
                title: "s".into(),
                status: GpuiStatusIndicatorStatus::Working,
                question: false,
                trailing: String::new(),
            }],
        }
    }

    fn machine(id: &str, connected: bool) -> PanelMachine {
        PanelMachine {
            machine_id: id.to_string(),
            name: id.to_uppercase(),
            connected,
            busy: false,
            failed: false,
            status: if connected {
                String::new()
            } else {
                "Not connected".into()
            },
            detail: None,
        }
    }

    fn ids(projects: &[PanelProject]) -> Vec<&str> {
        projects
            .iter()
            .map(|project| project.project_id.as_str())
            .collect()
    }

    #[test]
    fn local_projects_stay_first_and_remote_ones_go_to_their_machine() {
        let (local, sections) = split_by_machine(
            vec![
                project("remote:omarchy:project:p1"),
                project("local-a"),
                project("remote:mini:project:p2"),
                project("local-b"),
                project("remote:omarchy:project:p3"),
            ],
            &[machine("mini", true), machine("omarchy", true)],
        );
        assert_eq!(ids(&local), ["local-a", "local-b"]);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].machine.machine_id, "mini");
        assert_eq!(ids(&sections[0].projects), ["remote:mini:project:p2"]);
        assert_eq!(
            ids(&sections[1].projects),
            ["remote:omarchy:project:p1", "remote:omarchy:project:p3"]
        );
    }

    #[test]
    fn every_machine_gets_a_section_even_with_nothing_running() {
        let (local, sections) = split_by_machine(
            vec![project("local-a")],
            &[machine("omarchy", false), machine("mini", true)],
        );
        assert_eq!(ids(&local), ["local-a"]);
        let names: Vec<&str> = sections
            .iter()
            .map(|section| section.machine.name.as_str())
            .collect();
        assert_eq!(names, ["OMARCHY", "MINI"]);
        assert!(sections.iter().all(|section| section.projects.is_empty()));
    }

    #[test]
    fn a_machine_that_is_not_connected_lists_none_of_its_last_seen_rows() {
        let (_, sections) = split_by_machine(
            vec![project("remote:omarchy:project:p1")],
            &[machine("omarchy", false)],
        );
        assert!(sections[0].projects.is_empty());
    }

    #[test]
    fn a_remote_project_without_a_machine_tab_is_left_out() {
        let (local, sections) = split_by_machine(vec![project("remote:gone:project:p1")], &[]);
        assert!(local.is_empty());
        assert!(sections.is_empty());
    }
}
