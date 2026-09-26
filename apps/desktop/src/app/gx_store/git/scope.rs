//! Which project a Git request is about, on which computer, and the project facts the Git flows
//! read (preferences, worktree metadata, path, title), taken from the store's presentation.

use ghostex_gx_core::git_menu::{GitPreferences, GitStateProject};
use ghostex_gx_core::{MachineId, ProjectKey, group_project, is_chat_project_path};
use serde_json::Value;

use super::calls::Remote;
use crate::GhostexGpuiApp;

/// The worktree metadata a project row carries (`normalizeGpuiWorktreeMetadata`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorktreeFacts {
    pub(crate) parent_project_id: String,
    pub(crate) branch: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) parent_project_name: Option<String>,
}

/// One project a Git request is about.
#[derive(Clone)]
pub(crate) struct GitScope {
    pub(crate) key: ProjectKey,
    pub(crate) remote: Remote,
    pub(crate) title: String,
    pub(crate) path: Option<String>,
    pub(crate) preferences: GitPreferences,
    pub(crate) worktree: Option<WorktreeFacts>,
    /// A Chats container or another project with no repository of its own.
    pub(crate) projectless: bool,
}

/// Why no scope could be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScopeMiss {
    /// The id names nothing this app can read.
    Unknown,
    /// A remote machine that is not streaming, or has no tunnel right now.
    RemoteUnavailable,
}

impl GitScope {
    pub(crate) fn project_id(&self) -> String {
        self.key.project_id.clone()
    }

    /// The id the sidebar and the titlebar key this project by: the raw id on this computer,
    /// `remote:<machine>:project:<id>` on another.
    pub(crate) fn scoped_id(&self) -> String {
        self.key.to_workspace_project_id()
    }

    pub(crate) fn is_remote(&self) -> bool {
        !self.key.machine.is_local()
    }

    pub(crate) fn machine_id(&self) -> Option<&str> {
        self.key.machine.remote_id()
    }

    pub(crate) fn is_worktree(&self) -> bool {
        self.worktree.is_some()
    }

    /// The worktree name a state carries: the row's worktree name, and on a remote machine the
    /// project title when the row has none (`readRemoteSidebarGitState`).
    pub(crate) fn worktree_name(&self) -> Option<String> {
        let name = self
            .worktree
            .as_ref()
            .and_then(|worktree| worktree.name.clone());
        match self.is_remote() {
            true => name.or_else(|| Some(self.title.clone()).filter(|title| !title.is_empty())),
            false => name,
        }
    }

    pub(crate) fn state_project(&self) -> GitStateProject {
        GitStateProject {
            preferences: self.preferences,
            is_worktree: self.is_worktree(),
            worktree_name: self.worktree_name(),
        }
    }
}

fn text(value: Option<&Value>, key: &str) -> Option<String> {
    value?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// `normalizeGpuiWorktreeMetadata`: present only with a parent project id.
pub(crate) fn worktree_facts(worktree: Option<&Value>) -> Option<WorktreeFacts> {
    Some(WorktreeFacts {
        parent_project_id: text(worktree, "parentProjectId")?,
        branch: text(worktree, "branch"),
        name: text(worktree, "name"),
        parent_project_name: text(worktree, "parentProjectName"),
    })
}

impl GhostexGpuiApp {
    /// The scope of a project named by its scoped id.
    pub(crate) fn git_scope_for_project(&self, scoped_id: &str) -> Result<GitScope, ScopeMiss> {
        let key = ProjectKey::parse_workspace_project_id(scoped_id).ok_or(ScopeMiss::Unknown)?;
        self.git_scope_for_key(key)
    }

    /// The scope of a sidebar group: a project's group, a user-made group inside one, or a remote
    /// machine's project group (`resolveProjectIdForGroup`, `resolveRemotePresentationProjectScope`).
    pub(crate) fn git_scope_for_group(&self, group_id: &str) -> Result<GitScope, ScopeMiss> {
        let key = group_project(group_id).ok_or(ScopeMiss::Unknown)?;
        self.git_scope_for_key(key)
    }

    pub(crate) fn git_scope_for_key(&self, key: ProjectKey) -> Result<GitScope, ScopeMiss> {
        let presentation = self.gx_store.core.presentation();
        let remote = match &key.machine {
            MachineId::Local => None,
            MachineId::Remote(machine_id) => {
                // A remote machine answers only while its stream is live: the old runtime read
                // these from what the stream delivered, never from the last-seen copy.
                if presentation.loaded_live(&key.machine).is_none() {
                    return Err(ScopeMiss::RemoteUnavailable);
                }
                Some(
                    self.gpui_remote_gxserver_request_target(machine_id)
                        .ok_or(ScopeMiss::RemoteUnavailable)?,
                )
            }
        };
        let loaded = presentation
            .loaded(&key.machine)
            .ok_or(ScopeMiss::Unknown)?;
        let project = loaded.project(&key.project_id).ok_or(ScopeMiss::Unknown)?;
        let git_config = project
            .git_config
            .as_ref()
            .and_then(|config| serde_json::to_value(config).ok());
        let path = project
            .path
            .as_deref()
            .map(str::trim)
            .map(|path| path.trim_end_matches('/'))
            .filter(|path| !path.is_empty())
            .map(str::to_string);
        let projectless =
            path.as_deref().is_none_or(is_chat_project_path) || self.git_project_is_quick(&key);
        Ok(GitScope {
            remote,
            title: project.title.clone(),
            path,
            preferences: GitPreferences::from_git_config(git_config.as_ref()),
            worktree: worktree_facts(project.worktree.as_ref()),
            projectless,
            key,
        })
    }

    /// `isGpuiPresentationQuickDomainProject`: the machine's domain row says Quick.
    fn git_project_is_quick(&self, key: &ProjectKey) -> bool {
        self.gx_store
            .core
            .presentation()
            .machine(&key.machine)
            .and_then(|machine| machine.domain_project(&key.project_id))
            .is_some_and(|project| {
                project.get("isQuick").and_then(Value::as_bool) == Some(true)
                    || project
                        .get("launchSettings")
                        .and_then(|settings| settings.get("isQuick"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
    }

    /// The project the titlebar shows, as the store's scoped id.
    pub(crate) fn git_active_scoped_project_id(&self) -> Option<String> {
        self.gpui_app_modal_active_project_id()
            .filter(|project_id| !project_id.trim().is_empty())
    }
}
