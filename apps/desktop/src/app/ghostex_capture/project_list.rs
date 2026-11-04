//! How the Running Agents panel and the prompt's Send to list show projects: projects as header
//! rows that open and close, and a filter that searches project and session names.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: "the list of sessions needs to be collapsed by default same way that the sidebar is
//! collapsed in the real sidebar", and "add ability to filter by typing on the sessions list and
//! let me select the project then look at the sessions in that project i dont want to see all
//! projects and all sessions at once in the list".
//!
//! CDXC:GhostexCapture 2026-10-28 DECISION:
//! User, about the Running Agents panel: "please expand the top 3 projects here". This supersedes
//! the 2026-09-30 "collapsed by default, one opened at a time" for the panel only: it opens with
//! the first 3 projects in its order expanded, and a header click opens or closes that project on
//! its own, so several can be open. The prompt's Send to list keeps the 2026-09-30 behaviour.
//!
//! The panel's remote machine sections (`machine_sections.rs`) open and close the same way: a
//! connected machine starts open, one that is not connected starts closed, and the filter also
//! matches machine names (a match shows everything the machine lists).

use std::collections::HashMap;

/// Which projects are open and what the filter says, for one open list.
#[derive(Default)]
pub(crate) struct ListState {
    /// How many projects, counted from the top of the list, are open until clicked.
    default_open: usize,
    /// Several projects may be open at once; otherwise opening one closes the others.
    many: bool,
    /// Projects the user opened or closed by hand (true = open), over the top-of-list default.
    toggled: HashMap<String, bool>,
    /// Remote machine sections the user opened or closed by hand (true = open).
    machines_toggled: HashMap<String, bool>,
    pub(crate) filter: String,
}

/// What one remote machine's section draws.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MachineShown {
    /// Filtered out.
    Hidden,
    /// Its header only.
    Collapsed,
    /// Its header and what it lists; `name_matched` = the filter matches the machine's name, so
    /// every project and session of it shows.
    Open { name_matched: bool },
}

/// What one project draws.
pub(crate) enum Shown {
    /// Filtered out.
    Hidden,
    /// Its header only.
    Collapsed,
    /// Its header and these sessions (indices into its session list).
    Open(Vec<usize>),
}

impl ListState {
    /// The Running Agents panel: the first `default_open` projects open, any number open at once.
    pub(crate) fn panel(default_open: usize) -> Self {
        Self {
            default_open,
            many: true,
            ..Self::default()
        }
    }

    pub(crate) fn filtering(&self) -> bool {
        !self.filter.trim().is_empty()
    }

    /// `rank` is the project's place in the whole list (0 = top), counted before any filtering.
    fn is_open(&self, rank: usize, project_id: &str) -> bool {
        self.toggled
            .get(project_id)
            .copied()
            .unwrap_or(rank < self.default_open)
    }

    /// With no filter, the open projects show their sessions. With one, a project whose name
    /// matches shows all of them, another shows the sessions whose names match, and the rest are
    /// hidden.
    pub(crate) fn shown(
        &self,
        rank: usize,
        project_id: &str,
        title: &str,
        sessions: &[&str],
    ) -> Shown {
        let filter = self.filter.trim().to_lowercase();
        if filter.is_empty() {
            return if self.is_open(rank, project_id) {
                Shown::Open((0..sessions.len()).collect())
            } else {
                Shown::Collapsed
            };
        }
        if title.to_lowercase().contains(&filter) {
            return Shown::Open((0..sessions.len()).collect());
        }
        let matching: Vec<usize> = sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| session.to_lowercase().contains(&filter))
            .map(|(index, _)| index)
            .collect();
        if matching.is_empty() {
            Shown::Hidden
        } else {
            Shown::Open(matching)
        }
    }

    fn machine_is_open(&self, machine_id: &str, connected: bool) -> bool {
        self.machines_toggled
            .get(machine_id)
            .copied()
            .unwrap_or(connected)
    }

    /// With no filter, a machine is open or closed (connected ones start open). With one, a
    /// machine whose name matches shows everything, another shows only when one of its projects
    /// matches (`any_project_shown`).
    pub(crate) fn machine_shown(
        &self,
        machine_id: &str,
        name: &str,
        connected: bool,
        any_project_shown: bool,
    ) -> MachineShown {
        let filter = self.filter.trim().to_lowercase();
        if filter.is_empty() {
            return if self.machine_is_open(machine_id, connected) {
                MachineShown::Open {
                    name_matched: false,
                }
            } else {
                MachineShown::Collapsed
            };
        }
        if name.to_lowercase().contains(&filter) {
            MachineShown::Open { name_matched: true }
        } else if any_project_shown {
            MachineShown::Open {
                name_matched: false,
            }
        } else {
            MachineShown::Hidden
        }
    }

    /// A click on a machine's header: opens or closes it. While filtering, it clears the filter
    /// and opens that machine, like a project header.
    pub(crate) fn toggle_machine(&mut self, machine_id: &str, connected: bool) -> bool {
        let cleared = self.filtering();
        let open = cleared || !self.machine_is_open(machine_id, connected);
        if cleared {
            self.filter.clear();
        }
        self.machines_toggled.insert(machine_id.to_string(), open);
        cleared
    }

    /// A click on a project's header: opens or closes that project (and, when the list is not
    /// `many`, closes the others). While filtering, it clears the filter and opens that project;
    /// what was open before the filter is still open.
    pub(crate) fn toggle(&mut self, rank: usize, project_id: &str) -> bool {
        let cleared = self.filtering();
        let open = cleared || !self.is_open(rank, project_id);
        if cleared {
            self.filter.clear();
        }
        if !self.many && open {
            self.toggled.clear();
        }
        self.toggled.insert(project_id.to_string(), open);
        cleared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_open(state: &ListState, rank: usize, id: &str) -> bool {
        matches!(state.shown(rank, id, id, &["a", "b"]), Shown::Open(_))
    }

    fn open_ids(state: &ListState, ids: &[&str]) -> Vec<String> {
        ids.iter()
            .enumerate()
            .filter(|(rank, id)| is_open(state, *rank, id))
            .map(|(_, id)| id.to_string())
            .collect()
    }

    const IDS: [&str; 5] = ["p0", "p1", "p2", "p3", "p4"];

    #[test]
    fn panel_opens_the_top_three_projects() {
        let state = ListState::panel(3);
        assert_eq!(open_ids(&state, &IDS), ["p0", "p1", "p2"]);
        assert_eq!(open_ids(&state, &IDS[..2]), ["p0", "p1"]);
    }

    #[test]
    fn panel_headers_toggle_on_their_own() {
        let mut state = ListState::panel(3);
        state.toggle(1, "p1");
        assert_eq!(open_ids(&state, &IDS), ["p0", "p2"]);
        state.toggle(4, "p4");
        assert_eq!(open_ids(&state, &IDS), ["p0", "p2", "p4"]);
        state.toggle(1, "p1");
        assert_eq!(open_ids(&state, &IDS), ["p0", "p1", "p2", "p4"]);
    }

    #[test]
    fn panel_filter_matches_then_returns_to_what_was_open() {
        let mut state = ListState::panel(3);
        state.toggle(0, "p0");
        state.filter = "p3".into();
        assert_eq!(open_ids(&state, &IDS), ["p3"]);
        assert!(matches!(state.shown(0, "p0", "p0", &["a"]), Shown::Hidden));
        state.filter.clear();
        assert_eq!(open_ids(&state, &IDS), ["p1", "p2"]);
    }

    #[test]
    fn panel_header_click_while_filtering_clears_it_and_opens_that_project() {
        let mut state = ListState::panel(3);
        state.filter = "p4".into();
        assert!(state.toggle(4, "p4"));
        assert!(!state.filtering());
        assert_eq!(open_ids(&state, &IDS), ["p0", "p1", "p2", "p4"]);
    }

    #[test]
    fn connected_machines_start_open_and_the_others_closed() {
        let mut state = ListState::panel(3);
        let open = MachineShown::Open {
            name_matched: false,
        };
        assert_eq!(state.machine_shown("mini", "Mini", true, false), open);
        assert_eq!(
            state.machine_shown("omarchy", "omarchy", false, false),
            MachineShown::Collapsed
        );
        state.toggle_machine("omarchy", false);
        state.toggle_machine("mini", true);
        assert_eq!(
            state.machine_shown("omarchy", "omarchy", false, false),
            open
        );
        assert_eq!(
            state.machine_shown("mini", "Mini", true, false),
            MachineShown::Collapsed
        );
    }

    #[test]
    fn filter_matches_machine_names_and_their_projects() {
        let mut state = ListState::panel(3);
        state.filter = "omar".into();
        assert_eq!(
            state.machine_shown("omarchy", "Omarchy", false, false),
            MachineShown::Open { name_matched: true }
        );
        assert_eq!(
            state.machine_shown("mini", "Mini", true, true),
            MachineShown::Open {
                name_matched: false
            }
        );
        assert_eq!(
            state.machine_shown("mini", "Mini", true, false),
            MachineShown::Hidden
        );
    }

    #[test]
    fn machine_header_click_while_filtering_clears_it_and_opens_that_machine() {
        let mut state = ListState::panel(3);
        state.filter = "omar".into();
        assert!(state.toggle_machine("omarchy", false));
        assert!(!state.filtering());
        assert_eq!(
            state.machine_shown("omarchy", "Omarchy", false, false),
            MachineShown::Open {
                name_matched: false
            }
        );
    }

    #[test]
    fn send_to_list_starts_collapsed_and_opens_one_at_a_time() {
        let mut state = ListState::default();
        assert!(open_ids(&state, &IDS).is_empty());
        state.toggle(0, "p0");
        state.toggle(2, "p2");
        assert_eq!(open_ids(&state, &IDS), ["p2"]);
        state.toggle(2, "p2");
        assert!(open_ids(&state, &IDS).is_empty());
    }
}
