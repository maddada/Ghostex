//! Trusting a session's folder in its agent's own records just before gxserver starts the agent.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::stores::{trust_folder, TrustingAgent};
use crate::domain::DomainRepository;

/// CDXC:AgentProviders 2026-11-14 DECISION:
/// Ghostex trusts the folder a session runs in (its project or worktree, never a parent such as home or `/`) in that agent's own records before starting it only while "Trust project folders automatically" is on and was confirmed on this computer (the user's rule, packages/settings-catalog/src/folder_trust.rs). It is off by default, which supersedes the 2026-10-17 "auto trust folders by default". With it off, only folders the user remembered with Trust and Remember, the Ghostex config folder and an orchestrator's own project and worktrees are trusted this way, and every other folder keeps the agent's own question, which the chat shows as the folder-trust card.
/// SEE-ALSO: server/src/session_chat_trust_memory.rs (`session_folders_remembered`, Trust and Remember), server/src/agent_folder_trust/stores.rs (what each agent reads).
fn trust_project_folders_enabled() -> bool {
    crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(None))
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(ghostex_settings_catalog::folder_trust::trust_project_folders_on)
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

/// CDXC:AgentProviders 2026-11-17 WHY:
/// Every Claude account is its own profile with its own trusted folders, so an account switch restarted a session the user had already let into its folder on Claude's "Trust this folder?" question again, and the switch sat on "Resume conversation" until it failed with "its input box is not ready" (S90:P1953:G3cfn on 2026-11-16, moved from account 4, which trusted ~/dev/Kiwimake, to account 5, which never ran there). The user's yes in one profile is carried to the others when a switch restarts the session; a folder no profile trusts keeps Claude's own question, as the 2026-11-14 decision above wants with the setting off.
/// SEE-ALSO: server/src/accounts/restart.rs (the in-place switch), server/src/accounts/restart_verification.rs (the readiness wait).
pub(crate) fn carry_claude_folder_trust_for_switch(session: &Value, cwd: &str) {
    if session.pointer("/runtimeSettings/agentbox").is_some()
        || crate::session_chat_composer::session_chat_composer_agent_id(session)
            .as_deref()
            .and_then(TrustingAgent::from_agent_id)
            != Some(TrustingAgent::Claude)
    {
        return;
    }
    let home = crate::resume_lookup::home_dir();
    let Some(folder) = trustable_folder(cwd, &home) else {
        return;
    };
    if !super::stores::claude_folder_trusted_in_any_profile(&home, &folder) {
        return;
    }
    for warning in trust_folder(TrustingAgent::Claude, &home, &folder) {
        eprintln!(
            "Could not mark {} as a trusted folder for {warning}",
            folder.display()
        );
    }
}

/// The agents whose folder trust a moved folder carries, and whether any of their profiles trusted
/// `folder` (read before a folder move, while the folder exists): Claude and Codex.
pub(crate) fn folder_trusted_by(folder: &Path) -> Vec<String> {
    let home = crate::resume_lookup::home_dir();
    let mut agents = Vec::new();
    if super::stores::claude_folder_trusted_in_any_profile(&home, folder) {
        agents.push("claude".to_string());
    }
    if super::stores::codex_folder_trusted_in_any_profile(&home, folder) {
        agents.push("codex".to_string());
    }
    agents
}

/// CDXC:Projects 2026-11-19 WHY: a moved project folder keeps the user's yes to the agent's "Trust this folder?" (risk 11 of docs/2026-11-19/project-move-folder/PLAN.md): when Claude or Codex trusted the old folder in some profile, every profile of that agent trusts the new one, written through the same per-agent step as a launch rather than by editing its settings by hand. Their other per-folder records (Claude's allowed tools and MCP servers) stay under the old path, and the move's result says so. Returns the warnings.
pub(crate) fn carry_folder_trust_for_move(agents: &[String], new_folder: &Path) -> Vec<String> {
    let home = crate::resume_lookup::home_dir();
    let Some(folder) = trustable_folder(&new_folder.to_string_lossy(), &home) else {
        return Vec::new();
    };
    agents
        .iter()
        .filter_map(|agent| match agent.as_str() {
            "claude" => Some(TrustingAgent::Claude),
            "codex" => Some(TrustingAgent::Codex),
            _ => None,
        })
        .flat_map(|agent| trust_folder(agent, &home, &folder))
        .map(|warning| {
            format!(
                "Could not mark {} as a trusted folder for {warning}",
                folder.display()
            )
        })
        .collect()
}

/// CDXC:AgentProviders 2026-11-11 DECISION:
/// User, on the first-run page (step 16, trust the folder): "from now on always trust folders added automatically i hate this trust thing". With "Trust project folders automatically" on (confirmed on this computer, see above), a session in a folder the user added to Ghostex (the project's own folder, a folder inside it, or a worktree Ghostex made for it) gets its agent's folder-trust question answered for it, like a coordinator's thread already did (CDXC:Coordinators 2026-09-30 in coordinators/state.rs). The record written before launch (`trust_session_folder_before_launch`) usually stops the question, but an agent that had never run on the computer has no record to write to yet, and some agents ask anyway; those sessions now go on without the user. Never the home folder or anything above it, which a project at `~` would be (Main Hub has its own rule). The agent's first approval of a command or an edit stays the agent's own question.
/// SEE-ALSO: server/src/session_chat_trust_memory.rs (`session_folders_remembered`, the funnel that answers), server/src/coordinators/state.rs (`coordinator_thread_folder_trusted`).
pub(crate) fn added_project_folder_trusted(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> bool {
    if !trust_project_folders_enabled() {
        return false;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let Some(project_path) = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| {
            project
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|path| !path.trim().is_empty())
    else {
        return false;
    };
    let home = crate::resume_lookup::home_dir();
    let Some(project_folder) = trustable_folder(project_path.trim(), &home) else {
        return false;
    };
    let cwd = session
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty());
    let Some(cwd) = cwd else {
        // A session with no folder of its own runs in its project's.
        return true;
    };
    if crate::worktree_sessions::read_worktree_session_marker(&session)
        .is_some_and(|marker| marker.path.trim() == cwd)
    {
        return trustable_folder(cwd, &home).is_some();
    }
    trustable_folder(cwd, &home).is_some_and(|folder| folder.starts_with(&project_folder))
}
