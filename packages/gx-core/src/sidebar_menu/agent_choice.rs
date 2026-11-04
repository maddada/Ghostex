//! Which agent a project's "Create <agent>" button, New Agent Session and the New Thread picker
//! start: the agent last picked in that project, in its Space, or anywhere, as Settings > Agents
//! "Remember the picked agent" says.
//!
//! CDXC:AgentLauncher 2026-10-30 DECISION:
//! User (for a Discord user who works on some projects with Claude only and others with Codex only): remember the agent picked on the project header's agent button per project by default, with a setting to remember it per project, per Space, or everywhere (one choice for every project, the behaviour before). The header button's logo shows the agent that project will start, and its tooltip stays as it was. A project with no choice yet starts the agent picked last anywhere, which is also where an existing user's one stored choice carries over. The middle option is the sidebar's Spaces and is called "Per Space" (user: "call it Per Space please"), never "per workspace". The phone follows the same setting (apps/mobile/app/src/components/sessions/agentChoice.ts).
//!
//! WHY: every pick is written to all three memories whatever the setting says, so switching the
//! setting later starts from what the user actually picked in each project and Space. A worktree
//! and a session group share their project's choice. With Spaces turned off there is one Space, so
//! "per Space" reads as "everywhere".
//! SEE-ALSO: apps/desktop/src/app/gx_store/primary_launcher.rs (storage), packages/client-storage/catalog.ts (`launcherChoices`).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_view::{GroupView, SidebarView, LOCAL_MACHINE_ID};

/// The setting that picks how the choice is remembered.
pub const AGENT_CHOICE_SCOPE_SETTING_KEY: &str = "newSessionAgentScope";
/// The client-storage key the per-project and per-Space choices live under. The agent picked last
/// anywhere stays under its historic key, `ghostex-sidebar-project-terminal-launcher`.
pub const AGENT_CHOICES_STORAGE_KEY: &str = "ghostex.sidebar.agentLauncherChoices.v1";

/// How many projects (and, separately, Spaces) keep a choice; the oldest-named one gives way, which
/// keeps the document well inside the store's 64 KiB entry bound.
const MAX_REMEMBERED: usize = 250;
const MAX_KEY_CHARS: usize = 256;
const MAX_AGENT_ID_CHARS: usize = 128;

/// How the agent a project starts is remembered (`newSessionAgentScope`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AgentChoiceScope {
    /// Each project starts the agent last picked in it (the default).
    #[default]
    Project,
    /// Every project in a Space starts the agent last picked in that Space.
    Space,
    /// One choice for every project: the agent picked last anywhere.
    Everywhere,
}

impl AgentChoiceScope {
    /// The saved setting; anything unknown reads as the default.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("space") => Self::Space,
            Some("everywhere") => Self::Everywhere,
            _ => Self::Project,
        }
    }

    /// The scope in a settings object.
    pub fn from_settings_json(settings: &Value) -> Self {
        Self::from_setting(
            settings
                .get(AGENT_CHOICE_SCOPE_SETTING_KEY)
                .and_then(Value::as_str),
        )
    }
}

/// Where a launch happens: the keys its project and its Space are remembered under. A key is
/// absent when there is nothing to remember it by (no project resolved, or Spaces turned off).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AgentChoicePlace {
    pub project: Option<String>,
    pub space: Option<String>,
}

impl AgentChoicePlace {
    /// The place of a drawn group. Every drawn group belongs to the selected Space (the list draws
    /// only that Space's projects), and a worktree or a session group counts as its project.
    pub fn for_group(view: &SidebarView, group: &GroupView) -> Self {
        let machine = view.selected_machine_id.as_str();
        let project = group
            .space_project
            .as_ref()
            .map(|project| {
                project
                    .parent_project_id
                    .as_deref()
                    .unwrap_or(&project.project_id)
            })
            .or_else(|| {
                group
                    .core
                    .project_context
                    .as_ref()
                    .map(|project| project.project_id.as_str())
            })
            .map(|project_id| format!("{machine}|{project_id}"));
        let space = view
            .spaces_enabled
            .then(|| view.spaces.iter().find(|space| space.selected))
            .flatten()
            .map(|space| format!("{machine}|{}", space.id));
        Self { project, space }
    }

    /// The place of the drawn group `group_id`; nothing when the list does not draw it.
    pub fn for_group_id(view: &SidebarView, group_id: &str) -> Self {
        view.groups
            .iter()
            .find(|group| group.core.group_id == group_id)
            .map(|group| Self::for_group(view, group))
            .unwrap_or_default()
    }

    /// The place of a project: its drawn group when the list shows it, otherwise the project alone
    /// (a project outside the selected Space, or on another machine, has no Space to read).
    pub fn for_project(view: &SidebarView, project: &ProjectKey) -> Self {
        let machine = match &project.machine {
            MachineId::Local => LOCAL_MACHINE_ID,
            MachineId::Remote(machine) => machine.as_str(),
        };
        let drawn = (machine == view.selected_machine_id)
            .then(|| {
                view.groups.iter().find(|group| {
                    group
                        .core
                        .project_context
                        .as_ref()
                        .is_some_and(|context| context.project_id == project.project_id)
                })
            })
            .flatten();
        match drawn {
            Some(group) => Self::for_group(view, group),
            None => Self {
                project: Some(format!("{machine}|{}", project.project_id)),
                space: None,
            },
        }
    }
}

/// The remembered choices: the agent picked last anywhere and the one picked last in each project
/// and each Space, with the setting that says which one a launch reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AgentChoices {
    pub scope: AgentChoiceScope,
    /// The agent picked last anywhere (`ghostex-sidebar-project-terminal-launcher`).
    pub last: Option<String>,
    pub by_project: BTreeMap<String, String>,
    pub by_space: BTreeMap<String, String>,
}

impl AgentChoices {
    /// The choices read from storage: the last-picked agent's own key and the choices document
    /// (`AGENT_CHOICES_STORAGE_KEY`). Anything unreadable is left out.
    pub fn from_storage(
        scope: AgentChoiceScope,
        last: Option<&str>,
        document: Option<&str>,
    ) -> Self {
        let mut choices = Self {
            scope,
            last: last.and_then(clean_agent_id),
            ..Self::default()
        };
        let Some(document) = document.and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        else {
            return choices;
        };
        choices.by_project = read_map(document.get("projects"));
        choices.by_space = read_map(document.get("spaces"));
        choices
    }

    /// The choices document to store under `AGENT_CHOICES_STORAGE_KEY`.
    pub fn document_json(&self) -> String {
        let map = |entries: &BTreeMap<String, String>| {
            Value::Object(
                entries
                    .iter()
                    .map(|(key, agent_id)| (key.clone(), Value::String(agent_id.clone())))
                    .collect::<Map<String, Value>>(),
            )
        };
        let mut document = Map::new();
        document.insert("projects".to_string(), map(&self.by_project));
        document.insert("spaces".to_string(), map(&self.by_space));
        Value::Object(document).to_string()
    }

    /// The agents a launch at `place` tries, in order: the one remembered for its project or Space
    /// (as the setting says), then the one picked last anywhere. A caller takes the first that is
    /// still offered.
    pub fn candidates(&self, place: &AgentChoicePlace) -> Vec<&str> {
        let remembered = match self.scope {
            AgentChoiceScope::Project => place
                .project
                .as_ref()
                .and_then(|key| self.by_project.get(key)),
            AgentChoiceScope::Space => place.space.as_ref().and_then(|key| self.by_space.get(key)),
            AgentChoiceScope::Everywhere => None,
        };
        let mut candidates: Vec<&str> = remembered.map(String::as_str).into_iter().collect();
        if let Some(last) = self
            .last
            .as_deref()
            .filter(|last| !candidates.contains(last))
        {
            candidates.push(last);
        }
        candidates
    }

    /// The agent a launch at `place` starts, before checking it is still offered.
    pub fn agent_for(&self, place: &AgentChoicePlace) -> Option<&str> {
        self.candidates(place).into_iter().next()
    }

    /// Remembers a pick at `place`: as the last agent anywhere, for its project and for its Space.
    pub fn remember(&mut self, agent_id: &str, place: &AgentChoicePlace) {
        let Some(agent_id) = clean_agent_id(agent_id) else {
            return;
        };
        if let Some(key) = place.project.as_deref() {
            insert_bounded(&mut self.by_project, key, &agent_id);
        }
        if let Some(key) = place.space.as_deref() {
            insert_bounded(&mut self.by_space, key, &agent_id);
        }
        self.last = Some(agent_id);
    }
}

fn clean_agent_id(agent_id: &str) -> Option<String> {
    let agent_id = agent_id.trim();
    (!agent_id.is_empty() && agent_id.len() <= MAX_AGENT_ID_CHARS).then(|| agent_id.to_string())
}

fn read_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(key, _)| !key.is_empty() && key.len() <= MAX_KEY_CHARS)
        .filter_map(|(key, agent_id)| Some((key.clone(), clean_agent_id(agent_id.as_str()?)?)))
        .take(MAX_REMEMBERED)
        .collect()
}

fn insert_bounded(map: &mut BTreeMap<String, String>, key: &str, agent_id: &str) {
    if key.is_empty() || key.len() > MAX_KEY_CHARS {
        return;
    }
    if !map.contains_key(key) && map.len() >= MAX_REMEMBERED {
        map.pop_first();
    }
    map.insert(key.to_string(), agent_id.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(project: &str, space: Option<&str>) -> AgentChoicePlace {
        AgentChoicePlace {
            project: Some(project.to_string()),
            space: space.map(str::to_string),
        }
    }

    fn picked(scope: AgentChoiceScope) -> AgentChoices {
        let mut choices = AgentChoices::from_storage(scope, Some("claude"), None);
        // Sam's case: Regula and Harbor are both in the Work Space; Codex is picked in Regula.
        choices.remember("codex", &place("local|regula", Some("local|work")));
        choices
    }

    #[test]
    fn per_project_keeps_each_projects_own_pick() {
        let mut choices = picked(AgentChoiceScope::Project);
        choices.remember("claude", &place("local|harbor", Some("local|work")));
        assert_eq!(
            choices.agent_for(&place("local|regula", Some("local|work"))),
            Some("codex")
        );
        assert_eq!(
            choices.agent_for(&place("local|harbor", Some("local|work"))),
            Some("claude")
        );
    }

    #[test]
    fn per_project_new_project_starts_the_last_agent_picked_anywhere() {
        let choices = picked(AgentChoiceScope::Project);
        assert_eq!(
            choices.agent_for(&place("local|legal", Some("local|work"))),
            Some("codex")
        );
        assert_eq!(
            choices.agent_for(&AgentChoicePlace::default()),
            Some("codex")
        );
    }

    #[test]
    fn per_space_shares_the_pick_across_the_space() {
        let mut choices = picked(AgentChoiceScope::Space);
        assert_eq!(
            choices.agent_for(&place("local|harbor", Some("local|work"))),
            Some("codex")
        );
        choices.remember("claude", &place("local|home", Some("local|personal")));
        assert_eq!(
            choices.agent_for(&place("local|harbor", Some("local|work"))),
            Some("codex")
        );
        assert_eq!(
            choices.agent_for(&place("local|notes", Some("local|personal"))),
            Some("claude")
        );
        // With Spaces off there is one Space: the last pick anywhere.
        assert_eq!(
            choices.agent_for(&place("local|harbor", None)),
            Some("claude")
        );
    }

    #[test]
    fn everywhere_is_one_choice_for_every_project() {
        let mut choices = picked(AgentChoiceScope::Everywhere);
        choices.remember("claude", &place("local|harbor", Some("local|work")));
        assert_eq!(
            choices.agent_for(&place("local|regula", Some("local|work"))),
            Some("claude")
        );
        assert_eq!(
            choices.agent_for(&place("local|harbor", Some("local|work"))),
            Some("claude")
        );
    }

    #[test]
    fn existing_choice_is_the_starting_value() {
        for scope in [
            AgentChoiceScope::Project,
            AgentChoiceScope::Space,
            AgentChoiceScope::Everywhere,
        ] {
            let choices = AgentChoices::from_storage(scope, Some(" codex "), None);
            assert_eq!(
                choices.agent_for(&place("local|harbor", Some("local|work"))),
                Some("codex")
            );
        }
    }

    #[test]
    fn candidates_fall_back_to_the_last_pick() {
        let mut choices = picked(AgentChoiceScope::Project);
        choices.remember("claude", &place("local|harbor", None));
        // Regula remembers Codex; if Codex is turned off the caller tries the last pick next.
        assert_eq!(
            choices.candidates(&place("local|regula", None)),
            vec!["codex", "claude"]
        );
        assert_eq!(
            choices.candidates(&place("local|harbor", None)),
            vec!["claude"]
        );
    }

    #[test]
    fn document_round_trips_and_switching_scope_keeps_every_pick() {
        let mut choices = picked(AgentChoiceScope::Project);
        choices.remember("claude", &place("local|harbor", Some("local|personal")));
        let stored = choices.document_json();
        let space =
            AgentChoices::from_storage(AgentChoiceScope::Space, Some("claude"), Some(&stored));
        assert_eq!(space.by_project, choices.by_project);
        assert_eq!(
            space.agent_for(&place("local|other", Some("local|work"))),
            Some("codex")
        );
        let project =
            AgentChoices::from_storage(AgentChoiceScope::Project, Some("claude"), Some(&stored));
        assert_eq!(
            project.agent_for(&place("local|regula", None)),
            Some("codex")
        );
    }

    #[test]
    fn unreadable_storage_and_unknown_setting_read_as_defaults() {
        let choices = AgentChoices::from_storage(
            AgentChoiceScope::from_setting(Some("galaxy")),
            Some("  "),
            Some(r#"{"projects":{"local|a":7,"":"codex","local|b":"pi"},"spaces":"no"}"#),
        );
        assert_eq!(choices.scope, AgentChoiceScope::Project);
        assert_eq!(choices.last, None);
        assert_eq!(
            choices.by_project,
            BTreeMap::from([("local|b".to_string(), "pi".to_string())])
        );
        assert!(choices.by_space.is_empty());
        assert_eq!(
            AgentChoices::from_storage(AgentChoiceScope::Project, None, Some("not json"))
                .by_project
                .len(),
            0
        );
        assert_eq!(
            AgentChoiceScope::from_settings_json(
                &serde_json::json!({ "newSessionAgentScope": "space" })
            ),
            AgentChoiceScope::Space
        );
        assert_eq!(
            AgentChoiceScope::from_settings_json(
                &serde_json::json!({ "newSessionAgentScope": "everywhere" })
            ),
            AgentChoiceScope::Everywhere
        );
        assert_eq!(
            AgentChoiceScope::from_settings_json(&serde_json::json!({})),
            AgentChoiceScope::Project
        );
    }

    #[test]
    fn header_button_starts_the_projects_agent_while_it_is_offered() {
        use crate::sidebar_menu::{LauncherAgent, MenuHost};
        let agent = |agent_id: &str| LauncherAgent {
            agent_id: agent_id.to_string(),
            name: agent_id.to_string(),
            ..LauncherAgent::default()
        };
        let mut host = MenuHost {
            agents: vec![agent("claude"), agent("codex")],
            agent_choices: picked(AgentChoiceScope::Project),
            ..MenuHost::default()
        };
        host.agent_choices
            .remember("claude", &place("local|harbor", None));
        let started = |host: &MenuHost, project: &str| {
            host.primary_agent_at(&place(project, None))
                .map(|agent| agent.agent_id.clone())
        };
        assert_eq!(started(&host, "local|regula").as_deref(), Some("codex"));
        assert_eq!(started(&host, "local|harbor").as_deref(), Some("claude"));
        // Codex turned off: Regula starts the last pick anywhere, then the first agent offered.
        host.agents = vec![agent("claude")];
        assert_eq!(started(&host, "local|regula").as_deref(), Some("claude"));
        host.agents = vec![agent("pi")];
        assert_eq!(started(&host, "local|regula").as_deref(), Some("pi"));
    }

    #[test]
    fn memory_stays_bounded() {
        let mut choices = AgentChoices::default();
        for index in 0..(MAX_REMEMBERED + 20) {
            choices.remember("codex", &place(&format!("local|p{index:04}"), None));
        }
        assert_eq!(choices.by_project.len(), MAX_REMEMBERED);
        assert!(choices.document_json().len() < 32 * 1024);
    }
}
