//! Trusting a session's folder in its agent's own records just before gxserver starts the agent.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::stores::{trust_folder, TrustingAgent};
use crate::domain::DomainRepository;

/// The Settings > Agents switch "Trust project folders automatically".
const TRUST_PROJECT_FOLDERS_SETTING: &str = "trustProjectFoldersAutomatically";

/// CDXC:AgentProviders 2026-10-17 DECISION:
/// User: auto trust folders by default, with no folder-trust notice. So Ghostex trusts the folder a session runs in (its project or worktree, never a parent such as home or `/`) in that agent's own records before starting it, whenever "Trust project folders automatically" is on (the default). With it off, only folders the user remembered with Trust and Remember, the Ghostex config folder and an orchestrator's own project and worktrees are trusted this way, and every other folder keeps the agent's prompt and the chat's folder-trust card.
/// SEE-ALSO: server/src/session_chat_trust_memory.rs (`session_folders_remembered`, Trust and Remember), server/src/agent_folder_trust/stores.rs (what each agent reads).
fn trust_project_folders_enabled() -> bool {
    let default = ghostex_settings_catalog::default_value(TRUST_PROJECT_FOLDERS_SETTING)
        .is_some_and(|value| *value == ghostex_settings_catalog::J::Bool(true));
    crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(None))
        .as_ref()
        .and_then(|settings| settings.get(TRUST_PROJECT_FOLDERS_SETTING))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

/// The folder to trust: the session's own folder as a real path. `None` for the filesystem root,
/// the home folder and any folder above it, since most agents also trust every folder inside a
/// trusted one.
fn trustable_folder(cwd: &str, home: &Path) -> Option<PathBuf> {
    let folder = std::fs::canonicalize(cwd).ok()?;
    let home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if folder.parent().is_none() || home.starts_with(&folder) {
        return None;
    }
    Some(folder)
}

/// Run right before the agent command reaches the session's terminal, at a fresh start and when an
/// account switch restarts the agent. Writes only for agents that have a folder-trust step, and
/// never for a box session, whose agent keeps its records inside the box.
pub(crate) fn trust_session_folder_before_launch(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    session: &Value,
    cwd: &str,
) {
    if session.pointer("/runtimeSettings/agentbox").is_some() {
        return;
    }
    let Some(agent) = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .as_deref()
        .and_then(TrustingAgent::from_agent_id)
    else {
        return;
    };
    if !trust_project_folders_enabled()
        && !crate::session_chat_trust_memory::session_folders_remembered(
            repository, project_id, session_id,
        )
    {
        return;
    }
    let home = crate::resume_lookup::home_dir();
    let Some(folder) = trustable_folder(cwd, &home) else {
        return;
    };
    for warning in trust_folder(agent, &home, &folder) {
        eprintln!(
            "Could not mark {} as a trusted folder for {warning}",
            folder.display()
        );
    }
}
