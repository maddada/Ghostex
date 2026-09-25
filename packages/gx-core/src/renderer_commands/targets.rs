//! Which session or project a renderer command names.
//!
//! These replace the old runtime's `resolveGxserverRendererCommandSession` and
//! `resolveEmbeddedBrowserRendererCommandProjectId` (in the deleted
//! `gxserver-runtime/app-shot-and-misc.ts`).
//!
//! SEE-ALSO: server/src/ghostex_cli/actions.rs (`with_renderer_session_target`).

use serde_json::Value;

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, MachineId, ProjectKey, SessionKey};
use crate::sidebar_view::text::js_trim;

const COMBINED_SESSION_PREFIX: &str = "combined-session:";

/// A local session a renderer command resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererSession {
    pub key: SessionKey,
    /// The sidebar id the old runtime focused by: the payload's own `combined-session:` id when it
    /// sent one, else the one built from the raw ids.
    pub sidebar_session_id: String,
}

fn record_string<'a>(record: Option<&'a Value>, key: &str) -> Option<&'a str> {
    record?.get(key)?.as_str()
}

/// `readGpuiRecordString(record, key)?.trim()`, where an empty result is falsy for the `||` chain.
fn trimmed(record: Option<&Value>, key: &str) -> Option<String> {
    let value = js_trim(record_string(record, key)?);
    (!value.is_empty()).then(|| value.to_string())
}

/// `parseGpuiRendererCommandGlobalSessionRef`: `<machine>:<project>:<session>`.
fn parse_global_ref(global_ref: Option<&str>) -> Option<(String, String)> {
    let parts: Vec<&str> = js_trim(global_ref?).split(':').collect();
    if parts.len() != 3 || parts[1].is_empty() || parts[2].is_empty() {
        return None;
    }
    Some((parts[1].to_string(), parts[2].to_string()))
}

fn local_session_exists(core: &Core, project_id: &str, session_id: &str) -> bool {
    core.presentation()
        .loaded(&MachineId::Local)
        .is_some_and(|loaded| loaded.server_session(project_id, session_id).is_some())
}

/// CDXC:CefRuntime 2026-06-27-02:05:
/// gxserver renderer commands can target local sessions with raw project/session ids in
/// `sessionTarget`, while the sidebar focuses by combined `combined-session:<project>:<session>`
/// ids. Resolve those raw ids to the same combined id before focusing, and keep the command result
/// bounded to ids and status rather than paths, titles, command text, URLs, tokens, terminal
/// output or payload echoes.
///
/// `resolveGxserverRendererCommandSession`: a session of THIS computer named by `sessionTarget`
/// (`projectId`, `sessionId`, `globalRef`) or the same keys at the top level. `None` is "No
/// matching session was found."
///
/// `loose` is for the verbs the desktop answers for the first time in this port (Full Reload,
/// Restart, Close After Done): the phone sends them a bare session id with no project, so a raw id
/// that names exactly one session of this computer is accepted. `ghostex focus` and
/// `rename-command` keep the old rule, where a raw id needs its project.
pub fn resolve_renderer_session(
    core: &Core,
    payload: &Value,
    loose: bool,
) -> Option<RendererSession> {
    let target = payload
        .get("sessionTarget")
        .filter(|value| value.is_object());
    let global = parse_global_ref(
        record_string(target, "globalRef").or_else(|| record_string(Some(payload), "globalRef")),
    );
    let project_id = trimmed(target, "projectId")
        .or_else(|| trimmed(Some(payload), "projectId"))
        .or_else(|| global.as_ref().map(|(project_id, _)| project_id.clone()));
    let session_id = trimmed(target, "sessionId")
        .or_else(|| trimmed(Some(payload), "sessionId"))
        .or_else(|| global.as_ref().map(|(_, session_id)| session_id.clone()))?;
    if session_id.starts_with(COMBINED_SESSION_PREFIX) {
        if let Some(key) = SessionKey::parse_sidebar_session_id(&session_id) {
            if project_id
                .as_deref()
                .is_some_and(|project_id| project_id != key.project_id)
            {
                return None;
            }
            if !local_session_exists(core, &key.project_id, &key.session_id) {
                return None;
            }
            return Some(RendererSession {
                key,
                sidebar_session_id: session_id,
            });
        }
    }
    let project_id = match project_id {
        Some(project_id) => project_id,
        None if loose => unique_local_project_of(core, &session_id)?,
        None => return None,
    };
    if !local_session_exists(core, &project_id, &session_id) {
        return None;
    }
    let key = SessionKey::local(project_id, session_id);
    let sidebar_session_id = key.to_sidebar_session_id();
    Some(RendererSession {
        key,
        sidebar_session_id,
    })
}

/// The project of the one local session with this raw id, `None` when there is none or several.
fn unique_local_project_of(core: &Core, session_id: &str) -> Option<String> {
    let loaded = core.presentation().loaded(&MachineId::Local)?;
    let mut matches = loaded
        .server_sessions()
        .filter(|session| session.session_id == session_id);
    let first = matches.next()?;
    matches.next().is_none().then(|| first.project_id.clone())
}

/// `resolveWorkspaceGroupProjectId`: the workspace project id a sidebar group id names (a
/// user-made session group's project, a remote group's scoped project id, or a local project).
pub fn resolve_renderer_group_project(group_id: &str) -> Option<String> {
    if let Some((project, _)) = parse_workspace_subgroup_id(group_id) {
        return Some(project.to_workspace_project_id());
    }
    ProjectKey::parse_sidebar_group_id(group_id).map(|project| project.to_workspace_project_id())
}

/// `resolveEmbeddedBrowserKnownProjectId`: a remote project the machine's live stream holds, or a
/// local project the daemon lists. Returns the workspace project id.
pub(crate) fn known_project_id(core: &Core, workspace_project_id: &str) -> Option<String> {
    let key = ProjectKey::parse_workspace_project_id(workspace_project_id)?;
    let store = core.presentation();
    let known = match &key.machine {
        MachineId::Remote(_) => store
            .loaded_live(&key.machine)
            .is_some_and(|loaded| loaded.project(&key.project_id).is_some()),
        MachineId::Local => store
            .machine(&MachineId::Local)
            .is_some_and(|machine| machine.domain_project(&key.project_id).is_some()),
    };
    known.then(|| key.to_workspace_project_id())
}

/// `normalizeGpuiProjectPath`: trimmed, trailing slashes removed, `None` when empty.
/// A path that is only slashes normalizes to the empty string, which the old runtime then treated as
/// no path at all (`if (!normalizedPath) return undefined`).
fn normalize_project_path(value: &str) -> Option<String> {
    let trimmed = js_trim(value);
    let path = trimmed.trim_end_matches('/');
    (!trimmed.is_empty() && !path.is_empty()).then(|| path.to_string())
}

/// `resolveDomainProjectScope({ projectPath })`: the local project the daemon lists at this path.
pub(crate) fn local_project_at_path(core: &Core, project_path: &str) -> Option<String> {
    let wanted = normalize_project_path(project_path)?;
    let machine = core.presentation().machine(&MachineId::Local)?;
    machine
        .domain_projects()
        .find(|(_, project)| {
            project
                .get("path")
                .and_then(Value::as_str)
                .and_then(normalize_project_path)
                .is_some_and(|path| path == wanted)
        })
        .map(|(project_id, _)| project_id.to_string())
}

/// A local project by its sidebar name: an exact match first, then the only case-insensitive one.
pub(crate) fn local_project_named(core: &Core, name: &str) -> Option<String> {
    let wanted = js_trim(name);
    if wanted.is_empty() {
        return None;
    }
    let loaded = core.presentation().loaded(&MachineId::Local)?;
    if let Some(project) = loaded
        .projects()
        .iter()
        .find(|project| js_trim(&project.title) == wanted)
    {
        return Some(project.project_id.clone());
    }
    let lowered = wanted.to_lowercase();
    let mut matches = loaded
        .projects()
        .iter()
        .filter(|project| js_trim(&project.title).to_lowercase() == lowered);
    let first = matches.next()?;
    matches.next().is_none().then(|| first.project_id.clone())
}
