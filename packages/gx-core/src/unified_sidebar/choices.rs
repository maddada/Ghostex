//! Where a new session starts: the projects the New thread picker and the new chat's tray offer,
//! the machine each starts on, the project New session aims at, and the filter after it starts.

use std::collections::{BTreeMap, BTreeSet};

use crate::keys::MachineId;

use super::build::{machine_key, UnifiedFilter};
use super::repo::repo_identity;

/// One project of one machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectChoiceInput {
    pub project_id: String,
    pub title: String,
    pub path: String,
    pub git_remote_origin_url: Option<String>,
    /// A worktree's parent checkout, which marks it as a worktree.
    pub worktree_parent_path: Option<String>,
    /// The project has an open session on this machine.
    pub has_sessions: bool,
}

/// One machine and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedChoiceMachineInput {
    pub machine: MachineId,
    pub label: String,
    /// A session can start there now. A machine that isn't connected keeps its chip, dimmed.
    pub connected: bool,
    pub projects: Vec<ProjectChoiceInput>,
}

/// One chip of a repository's row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedChoiceMachine {
    /// `local` for this computer.
    pub machine_id: String,
    pub label: String,
    pub connected: bool,
    pub project_id: String,
}

/// One row of the project list: a repository, with the machines that have it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedProjectChoice {
    pub repo_key: String,
    pub label: String,
    pub machines: Vec<UnifiedChoiceMachine>,
    /// Which machine the row starts on ([`default_machine`]).
    pub selected: usize,
    /// One of its machines has an open session; the list says "No sessions" otherwise.
    pub has_sessions: bool,
    /// One of the last projects a session was started in, listed first under Recent.
    pub recent: bool,
}

impl UnifiedProjectChoice {
    pub fn selected_machine(&self) -> Option<&UnifiedChoiceMachine> {
        self.machines.get(self.selected)
    }
}

/// The project list's rows: recent projects first, in the order they were last used, then the
/// rest A to Z.
///
/// CDXC:Sidebar 2026-11-03 DECISION:
/// User (Q1163 "we need A and also need C", Q1164, Q1165 "recent", Q1166 "listed", Q1167): "i want to go towards not separating projects by machines. i want to pick the project then pick which machine on the "New thread" page itself (default to the machine used last). right now we make the user pick the machine too much". So a repository on several machines is one row with no machine chips on it: the machine is picked afterwards, on the New thread picker's Run on row or the tray's machine chip. Recent projects (the last ones a session was started in) come first, then every other project A to Z, including projects with no open session, which say "No sessions" and are listed without touching the sidebar's filter. On each machine the row points at its main checkout: a worktree stands in only when that machine has nothing else of the repository. This supersedes Q848's machine chips on every row.
pub fn unified_project_choices(
    machines: &[UnifiedChoiceMachineInput],
    last_machines: &BTreeMap<String, String>,
    recent_repos: &[String],
    current_machine: &str,
) -> Vec<UnifiedProjectChoice> {
    let mut by_repo: BTreeMap<String, UnifiedProjectChoice> = BTreeMap::new();
    for machine in machines {
        let machine_id = machine_key(&machine.machine);
        // Per repository: the first main checkout, else the first worktree, and whether any of
        // its projects on this machine has a session.
        let mut picks: BTreeMap<String, (&ProjectChoiceInput, String)> = BTreeMap::new();
        let mut with_sessions: BTreeSet<String> = BTreeSet::new();
        for project in &machine.projects {
            let repo = repo_identity(
                project.git_remote_origin_url.as_deref(),
                &project.path,
                project.worktree_parent_path.as_deref(),
                &project.title,
            );
            if project.has_sessions {
                with_sessions.insert(repo.key.clone());
            }
            let replaces = picks.get(&repo.key).is_some_and(|(pick, _)| {
                pick.worktree_parent_path.is_some() && project.worktree_parent_path.is_none()
            });
            if replaces || !picks.contains_key(&repo.key) {
                picks.insert(repo.key, (project, repo.label));
            }
        }
        for (repo_key, (project, label)) in picks {
            let has_sessions = with_sessions.contains(&repo_key);
            let choice = by_repo
                .entry(repo_key.clone())
                .or_insert_with(|| UnifiedProjectChoice {
                    repo_key,
                    label,
                    ..UnifiedProjectChoice::default()
                });
            choice.has_sessions |= has_sessions;
            choice.machines.push(UnifiedChoiceMachine {
                machine_id: machine_id.to_string(),
                label: machine.label.clone(),
                connected: machine.connected,
                project_id: project.project_id.clone(),
            });
        }
    }
    let mut choices: Vec<UnifiedProjectChoice> = by_repo.into_values().collect();
    for choice in &mut choices {
        choice.selected = default_machine(choice, last_machines, current_machine);
        choice.recent = recent_repos.contains(&choice.repo_key);
    }
    let recent_rank = |choice: &UnifiedProjectChoice| {
        recent_repos
            .iter()
            .position(|repo| *repo == choice.repo_key)
            .unwrap_or(usize::MAX)
    };
    choices.sort_by(|left, right| {
        recent_rank(left)
            .cmp(&recent_rank(right))
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then_with(|| left.repo_key.cmp(&right.repo_key))
    });
    choices
}

/// The machine a project starts on: the last one used for it, else the one you are on, else this
/// computer, else its first machine, always skipping a machine that isn't connected while a
/// connected one has the project.
///
/// CDXC:Sidebar 2026-11-03 DECISION:
/// User (Q1181 "per-project", after "default to the machine used last"): the machine is the last one a session of this project was started on (`last_machines`, kept per repository), not the last one used for any session. A machine that isn't connected keeps its chip, dimmed, but is never the starting pick while another machine that has the project is connected.
pub fn default_machine(
    choice: &UnifiedProjectChoice,
    last_machines: &BTreeMap<String, String>,
    current_machine: &str,
) -> usize {
    let connected_chip = |machine_id: &str| {
        choice
            .machines
            .iter()
            .position(|machine| machine.machine_id == machine_id && machine.connected)
    };
    last_machines
        .get(&choice.repo_key)
        .and_then(|machine_id| connected_chip(machine_id))
        .or_else(|| connected_chip(current_machine))
        .or_else(|| connected_chip(crate::sidebar_view::LOCAL_MACHINE_ID))
        .or_else(|| choice.machines.iter().position(|machine| machine.connected))
        .unwrap_or(0)
}

/// The rows whose name, or a machine's name, holds every typed word.
pub fn filter_project_choices(
    choices: &[UnifiedProjectChoice],
    query: &str,
) -> Vec<UnifiedProjectChoice> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    choices
        .iter()
        .filter(|choice| {
            let mut haystack = choice.label.to_lowercase();
            for machine in &choice.machines {
                haystack.push('\n');
                haystack.push_str(&machine.label.to_lowercase());
            }
            words.iter().all(|word| haystack.contains(word))
        })
        .cloned()
        .collect()
}

/// A left or right arrow on the machine chips: the next connected machine, stopping at the ends.
pub fn move_project_choice_machine(choice: &UnifiedProjectChoice, forward: bool) -> usize {
    let connected = |index: &usize| {
        choice
            .machines
            .get(*index)
            .is_some_and(|machine| machine.connected)
    };
    let next = if forward {
        (choice.selected + 1..choice.machines.len()).find(connected)
    } else {
        (0..choice.selected).rev().find(connected)
    };
    next.unwrap_or(choice.selected)
}

/// Where a new session starts: a project of a repository on one machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedNewSessionTarget {
    pub repo_key: String,
    pub machine_id: String,
    pub project_id: String,
}

/// The project you have open, on its machine, with its repository ([`repo_identity`]'s key). It
/// can be a worktree the repository's row doesn't point at, and it is still where "here" is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveProject<'a> {
    pub repo_key: &'a str,
    pub machine_id: &'a str,
    pub project_id: &'a str,
}

/// Where the New thread picker starts: the filtered repository when the filter is exactly one,
/// else the project you have open, else the first project of the list.
///
/// CDXC:Sidebar 2026-11-03 DECISION:
/// User (Q1182 "open", Q1181 "per-project"): the picker starts on the project of the session you have open, on that session's machine, so another session here stays one press. With the filter on exactly one repository it starts there instead: on the machine you have open when that project is yours, else on the repository's own start (`default_machine`). Nothing is activated on the way (CDXC:Sidebar 2026-10-31 WHY below), and the picker's top row and Run on row change the project and the machine before anything starts. This supersedes Q847's rule that only the filtered repository was aimed at and that the machine was changed after the draft opened.
///
/// CDXC:Sidebar 2026-10-31 WHY:
/// User: "Fix the stray-session bug". The button used to activate the filtered project first, and activating a project with no sessions starts its default agent (gx_store_activate_project), so one press could leave a stray session in that project beside the one the picker then started.
pub fn unified_new_session_target(
    filter: &UnifiedFilter,
    choices: &[UnifiedProjectChoice],
    active: Option<ActiveProject<'_>>,
) -> Option<UnifiedNewSessionTarget> {
    let target =
        |choice: &UnifiedProjectChoice, machine: &UnifiedChoiceMachine| UnifiedNewSessionTarget {
            repo_key: choice.repo_key.clone(),
            machine_id: machine.machine_id.clone(),
            project_id: machine.project_id.clone(),
        };
    // The open project, when its repository is listed on that machine.
    let here = active.and_then(|active| {
        let choice = choices
            .iter()
            .find(|choice| choice.repo_key == active.repo_key)?;
        choice
            .machines
            .iter()
            .any(|machine| machine.machine_id == active.machine_id)
            .then(|| UnifiedNewSessionTarget {
                repo_key: active.repo_key.to_string(),
                machine_id: active.machine_id.to_string(),
                project_id: active.project_id.to_string(),
            })
    });
    if filter.repos.len() == 1 {
        let repo_key = filter.repos.first()?;
        if let Some(choice) = choices.iter().find(|choice| &choice.repo_key == repo_key) {
            if let Some(here) = here.as_ref().filter(|here| &here.repo_key == repo_key) {
                return Some(here.clone());
            }
            return Some(target(choice, choice.selected_machine()?));
        }
    }
    if here.is_some() {
        return here;
    }
    let first = choices.first()?;
    Some(target(first, first.selected_machine()?))
}

/// The filter once a session has started in `repo_key` on `machine_id` (in the Space
/// `space_key`, when that machine has Spaces on): every kind of pick that would hide it gains it,
/// and a kind with no pick stays empty, because adding one would narrow the list.
///
/// CDXC:Sidebar 2026-11-03 DECISION:
/// User (Q1187 "add-chip"): when a new session starts in a project the filter hides, its project joins the filter as a chip with its X, so you see where it went and one click undoes it; the filter is never cleared. A machine or Space pick that would still hide it gains that machine or Space the same way. The text search is left as it is.
pub fn widen_filter_for_new_session(
    filter: &UnifiedFilter,
    repo_key: &str,
    machine_id: &str,
    space_key: Option<&str>,
) -> UnifiedFilter {
    let mut widened = filter.clone();
    if !widened.repos.is_empty() {
        widened.repos.insert(repo_key.to_string());
    }
    if !widened.machines.is_empty() {
        widened.machines.insert(machine_id.to_string());
    }
    if let Some(space_key) = space_key.filter(|_| !widened.spaces.is_empty()) {
        widened.spaces.insert(space_key.to_string());
    }
    widened
}
